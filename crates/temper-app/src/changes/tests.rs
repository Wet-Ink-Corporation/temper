use std::io::Write as _;

use temper_domain::PathStyle::Posix;
use temper_domain::{
    Config, EXCLUDED_DIRS, FINGERPRINT_PATHSPECS, Fingerprint, FingerprintHasher, gate_config,
};

use super::{changed_files, fingerprint, rust_changes};
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::ports::{EntryKind, ExitKind, Stream};
use crate::project::Workspace;

const ROOT: &str = "/w";
const HEAD: &str = "1111111111111111111111111111111111111111";
const BASE: &str = "2222222222222222222222222222222222222222";

fn workspace(git: bool) -> Workspace {
    Workspace {
        root: ROOT.to_owned(),
        state_dir: "/w/.git/temper".to_owned(),
        config: Config::defaults(),
        config_problem: None,
        git,
    }
}

fn env() -> RunEnv {
    RunEnv::at(ROOT, Posix)
}

fn ran(stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    }
}

fn failed(code: i32, stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(code),
        stdout: stdout.to_owned(),
    }
}

fn resolves(rev: &str) -> Vec<String> {
    [
        "git",
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{rev}^{{commit}}"),
    ]
    .map(str::to_owned)
    .to_vec()
}

/// 1.x's `git diff` argv for the fingerprint, verbatim.
fn diff(rev: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = ["git", "diff", "--no-color", "--binary", "--no-ext-diff"]
        .map(str::to_owned)
        .to_vec();
    args.extend(rev.map(str::to_owned));
    args.push("--".to_owned());
    args.extend(FINGERPRINT_PATHSPECS.map(str::to_owned));
    args.extend(EXCLUDED_DIRS.map(|dir| format!(":(exclude){dir}")));
    args
}

const LS_FILES: [&str; 5] = ["git", "ls-files", "--others", "--exclude-standard", "-z"];

fn show(rev: &str) -> Vec<String> {
    vec![
        "git".to_owned(),
        "show".to_owned(),
        format!("{rev}:.claude/temper.json"),
    ]
}

/// A process table from `(argv, reply)` rows, all run at the root; anything else fails to start.
fn git_table(rows: Vec<(Vec<String>, Reply)>) -> FakeProcess {
    rows.into_iter()
        .fold(FakeProcess::new(), |process, (argv, reply)| {
            let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
            process.answering(&argv, ROOT, reply)
        })
}

fn ports(files: FakeFiles, process: FakeProcess) -> FakePorts {
    FakePorts {
        process,
        ..FakePorts::new(files)
    }
}

/// The fingerprint of the frames, fed to the domain's hasher directly.
struct Expect(FingerprintHasher);

impl Expect {
    fn new(head: Option<&str>, rev: Option<&str>, diff: &str) -> Self {
        let mut hasher = FingerprintHasher::new();
        hasher.head_and_rev(head, rev);
        let mut sink = hasher.diff();
        sink.write_all(diff.as_bytes())
            .expect("hashing never fails");
        sink.finish();
        Self(hasher)
    }

    fn walked() -> Self {
        Self(FingerprintHasher::new())
    }

    fn untracked(mut self, rel: &str, content: Option<&str>) -> Self {
        let mut sink = self.0.untracked(rel);
        if let Some(content) = content {
            sink.write_all(content.as_bytes())
                .expect("hashing never fails");
            self.0.accept(sink);
        }
        self
    }

    fn file(mut self, rel: &str, content: Option<&str>) -> Self {
        let mut sink = self.0.file(rel);
        if let Some(content) = content {
            sink.write_all(content.as_bytes())
                .expect("hashing never fails");
            self.0.accept(sink);
        }
        self
    }

    fn config(mut self, json: &str) -> Self {
        self.0.gate_config(json);
        self
    }

    fn fp(self) -> Fingerprint {
        self.0.finish()
    }
}

/// Item 13: the argv verbatim (a wrong one gets no reply and fails); the head and revision;
/// the diff's stdout; untracked files filtered by `inFingerprint`, without `.temper` and
/// `target`, in UTF-16 order (U+1F600 before U+E000); a vanished one hashes its frame only.
#[test]
fn git_mode_hashes_head_diff_and_untracked_files_in_utf16_order() {
    let files = FakeFiles::new(Posix)
        .with_file("/w/tests/\u{e000}.rs", "e")
        .with_file("/w/tests/\u{1f600}.rs", "smile")
        .with_file("/w/crates/a/AGENTS.md", "charter")
        .with_file("/w/notes.md", "n");
    let process = git_table(vec![
        (resolves("HEAD"), ran(&format!("{HEAD}\n"))),
        (diff(Some(HEAD)), ran("diff --git a/src/lib.rs\n")),
        (
            LS_FILES.map(str::to_owned).to_vec(),
            ran(
                "tests/\u{e000}.rs\0tests/\u{1f600}.rs\0notes.md\0target/x.rs\0.temper/y.rs\0src/gone.rs\0crates/a/AGENTS.md\0",
            ),
        ),
        (show(HEAD), failed(128, "")),
    ]);
    let ports = ports(files, process);

    let fp = fingerprint(&ports, &env(), &workspace(true), None);

    let expected = Expect::new(Some(HEAD), Some(HEAD), "diff --git a/src/lib.rs\n")
        .untracked("crates/a/AGENTS.md", Some("charter"))
        .untracked("src/gone.rs", None)
        .untracked("tests/\u{1f600}.rs", Some("smile"))
        .untracked("tests/\u{e000}.rs", Some("e"))
        .fp();
    assert_eq!(fp, expected);
}

/// A base that resolves is the revision; one that does not (another repository's) is HEAD.
#[test]
fn a_base_that_does_not_resolve_is_replaced_by_head() {
    let table = |base_reply: Reply, rev: &str| {
        git_table(vec![
            (resolves("HEAD"), ran(&format!("{HEAD}\n"))),
            (resolves("b"), base_reply),
            (diff(Some(rev)), ran("D")),
            (LS_FILES.map(str::to_owned).to_vec(), ran("")),
            (show(rev), failed(128, "")),
        ])
    };
    let good = ports(
        FakeFiles::new(Posix),
        table(ran(&format!("{BASE}\n")), BASE),
    );
    let bad = ports(FakeFiles::new(Posix), table(failed(1, ""), HEAD));

    let fps = [
        fingerprint(&good, &env(), &workspace(true), Some("b")),
        fingerprint(&bad, &env(), &workspace(true), Some("b")),
    ];

    assert_eq!(
        fps,
        [
            Expect::new(Some(HEAD), Some(BASE), "D").fp(),
            Expect::new(Some(HEAD), Some(HEAD), "D").fp()
        ]
    );
}

/// No HEAD (a fresh repository): `head:\nrev:\n`, no revision argument, and no `git show`.
#[test]
fn without_head_the_diff_has_no_revision() {
    let process = git_table(vec![
        (resolves("HEAD"), failed(128, "")),
        (diff(None), ran("D")),
        (LS_FILES.map(str::to_owned).to_vec(), ran("")),
    ]);
    let ports = ports(FakeFiles::new(Posix), process);

    let fp = fingerprint(&ports, &env(), &workspace(true), None);

    assert_eq!(fp, Expect::new(None, None, "D").fp());
}

/// 1.x hashes `git diff`'s stdout whatever its exit code; a failed or overflowing `ls-files`
/// lists no untracked file.
#[test]
fn a_failing_diff_still_counts_and_a_failing_ls_files_lists_nothing() {
    let table = |ls_files: Reply| {
        git_table(vec![
            (resolves("HEAD"), ran(HEAD)),
            (diff(Some(HEAD)), failed(1, "partial")),
            (LS_FILES.map(str::to_owned).to_vec(), ls_files),
            (show(HEAD), failed(128, "")),
        ])
    };
    let files = || FakeFiles::new(Posix).with_file("/w/a.rs", "x");
    let fps = [
        fingerprint(
            &ports(files(), table(failed(128, "a.rs\0"))),
            &env(),
            &workspace(true),
            None,
        ),
        fingerprint(
            &ports(files(), table(Reply::Overflows(Stream::Stdout))),
            &env(),
            &workspace(true),
            None,
        ),
    ];

    let expected = Expect::new(Some(HEAD), Some(HEAD), "partial").fp();
    assert_eq!(fps, [expected, expected]);
}

/// OC-1 A (b): an untracked entry that is not a regular file hashes its frame only.
#[test]
fn an_untracked_entry_that_is_not_a_file_hashes_its_frame_only() {
    let files = FakeFiles::new(Posix).with_special("/w/tests/fifo");
    let process = git_table(vec![
        (resolves("HEAD"), ran(HEAD)),
        (diff(Some(HEAD)), ran("")),
        (LS_FILES.map(str::to_owned).to_vec(), ran("tests/fifo\0")),
        (show(HEAD), failed(128, "")),
    ]);

    let fp = fingerprint(&ports(files, process), &env(), &workspace(true), None);

    assert_eq!(
        fp,
        Expect::new(Some(HEAD), Some(HEAD), "")
            .untracked("tests/fifo", None)
            .fp()
    );
}

/// Item 16: the config frame is added only when what the gate reads differs from the config
/// at the revision; a hooks-only key does not count; outside git the defaults are the base.
#[test]
fn the_gate_config_counts_only_when_a_key_the_gate_reads_changed() {
    let table = |at_rev: Reply| {
        git_table(vec![
            (resolves("HEAD"), ran(HEAD)),
            (diff(Some(HEAD)), ran("")),
            (LS_FILES.map(str::to_owned).to_vec(), ran("")),
            (show(HEAD), at_rev),
        ])
    };
    let now = r#"{"gate":{"doc":false},"review":"off"}"#;
    let files = || FakeFiles::new(Posix).with_file("/w/.claude/temper.json", now);
    let changed = fingerprint(
        &ports(files(), table(ran("{}"))),
        &env(),
        &workspace(true),
        None,
    );
    let hooks_only = fingerprint(
        &ports(files(), table(ran(r#"{"gate":{"doc":false}}"#))),
        &env(),
        &workspace(true),
        None,
    );
    let outside_git = fingerprint(
        &ports(files(), FakeProcess::new()),
        &env(),
        &workspace(false),
        None,
    );

    let config = gate_config(Some(now));
    assert_eq!(
        [changed, hooks_only, outside_git],
        [
            Expect::new(Some(HEAD), Some(HEAD), "").config(&config).fp(),
            Expect::new(Some(HEAD), Some(HEAD), "").fp(),
            Expect::walked().config(&config).fp(),
        ]
    );
}

/// Item 15: outside git the walk skips `target`, `.git`, `node_modules` and `.temper` at any
/// depth, lists a linked directory as an entry without entering it, keeps Rust-relevant paths
/// and charters, appends the baseline, and hashes sorted absolute paths as `\n<rel>\n` frames.
#[test]
fn outside_git_the_walk_hashes_sorted_relevant_files() {
    let files = FakeFiles::new(Posix)
        .with_file("/w/Cargo.toml", "[workspace]")
        .with_file("/w/src/lib.rs", "lib")
        .with_file("/w/src/target/x.rs", "skipped")
        .with_file("/w/a/node_modules/x.rs", "skipped")
        .with_file("/w/.temper/x.rs", "skipped")
        .with_file("/w/AGENTS.md", "charter")
        .with_file("/w/notes.md", "n")
        .with_file("/w/.claude/temper-baseline.json", "{}")
        .with_listing(
            "/w",
            &[
                ("src", EntryKind::Dir),
                ("Cargo.toml", EntryKind::File),
                ("a", EntryKind::Dir),
                (".temper", EntryKind::Dir),
                ("AGENTS.md", EntryKind::File),
                ("notes.md", EntryKind::File),
                ("linked.rs", EntryKind::Symlink),
                ("lib", EntryKind::Symlink),
                (".claude", EntryKind::Dir),
            ],
        )
        .with_link("/w/linked.rs", "/w/src/lib.rs")
        .with_link("/w/lib", "/w/src");

    let fp = fingerprint(
        &ports(files, FakeProcess::new()),
        &env(),
        &workspace(false),
        None,
    );

    let expected = Expect::walked()
        .file(".claude/temper-baseline.json", Some("{}"))
        .file("AGENTS.md", Some("charter"))
        .file("Cargo.toml", Some("[workspace]"))
        .file("linked.rs", Some("lib"))
        .file("src/lib.rs", Some("lib"))
        .fp();
    assert_eq!(fp, expected);
}

/// 1.x tests `RUST_RELEVANT` on the absolute path, so a root under a `tests/` directory takes
/// every file (and the baseline twice: walked, then appended).
#[test]
fn a_root_under_a_tests_directory_takes_every_file() {
    let root = "/x/tests/w";
    let files = FakeFiles::new(Posix)
        .with_file("/x/tests/w/Cargo.toml", "m")
        .with_file("/x/tests/w/notes.md", "n")
        .with_file("/x/tests/w/.claude/temper-baseline.json", "b");
    let ws = Workspace {
        root: root.to_owned(),
        ..workspace(false)
    };

    let fp = fingerprint(
        &ports(files, FakeProcess::new()),
        &RunEnv::at(root, Posix),
        &ws,
        None,
    );

    let expected = Expect::walked()
        .file(".claude/temper-baseline.json", Some("b"))
        .file(".claude/temper-baseline.json", Some("b"))
        .file("Cargo.toml", Some("m"))
        .file("notes.md", Some("n"))
        .fp();
    assert_eq!(fp, expected);
}

/// Item 17: tracked names, then untracked ones, deduplicated in first-seen order, filtered by
/// `RUST_RELEVANT`, then `.claude/temper.json` when the gate config changed.
#[test]
fn rust_changes_are_tracked_then_untracked_then_the_config() {
    let names = |base: Option<&str>| {
        let mut argv: Vec<String> = ["git", "diff", "--name-only", "-z"]
            .map(str::to_owned)
            .to_vec();
        argv.extend(base.map(str::to_owned));
        argv.extend(["--", ".", ":(exclude).temper", ":(exclude)target"].map(str::to_owned));
        argv
    };
    let process = git_table(vec![
        (names(Some("b")), ran("src/b.rs\0notes.md\0src/a.rs\0")),
        (
            LS_FILES.map(str::to_owned).to_vec(),
            ran("src/a.rs\0tests/t.txt\0x.md\0"),
        ),
        (resolves("b"), ran(BASE)),
        (show(BASE), ran("{}")),
    ]);
    let files =
        FakeFiles::new(Posix).with_file("/w/.claude/temper.json", r#"{"gate":{"scan":false}}"#);
    let ports = ports(files, process);

    let all = changed_files(&ports, &env(), &workspace(true), Some("b"));
    let rust = rust_changes(&ports, &env(), &workspace(true), Some("b"));

    assert_eq!(
        all,
        ["src/b.rs", "notes.md", "src/a.rs", "tests/t.txt", "x.md"]
    );
    assert_eq!(
        rust,
        ["src/b.rs", "src/a.rs", "tests/t.txt", ".claude/temper.json"]
    );
}

/// Outside git the changed files are the walk's, in its depth-first readdir order (unsorted),
/// relative to the root.
#[test]
fn outside_git_the_changed_files_are_the_walks_in_its_order() {
    let files = FakeFiles::new(Posix)
        .with_file("/w/b.rs", "")
        .with_file("/w/a/z.rs", "")
        .with_file("/w/a/notes.md", "")
        .with_file("/w/target/t.rs", "")
        .with_listing(
            "/w",
            &[
                ("b.rs", EntryKind::File),
                ("a", EntryKind::Dir),
                ("target", EntryKind::Dir),
            ],
        );
    let ports = ports(files, FakeProcess::new());

    let changed = changed_files(&ports, &env(), &workspace(false), None);

    assert_eq!(changed, ["b.rs", "a/z.rs"]);
}
