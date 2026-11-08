use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use temper_domain::PathStyle::Posix;
use temper_domain::{HookEvent, HookOutcome, Scanner};

use crate::engine::{HookError, RunEnv};
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::hooks::{HookCall, HookInput};
use crate::ports::{ExitKind, Files};

const HEAD: &str = "1111111111111111111111111111111111111111";

fn ran(stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    }
}

/// A one-crate repository at `/r` with HEAD at [`HEAD`], an untracked `src/lib.rs`, and every
/// other command (the rest of git, `cargo fmt`) exiting 0 with nothing to say.
fn repository() -> FakePorts {
    let git = |args: &[&'static str]| [&["git"][..], args].concat();
    let head_commit = format!("{HEAD}^{{commit}}");
    let process = FakeProcess::new()
        .answering(&git(&["rev-parse", "--show-toplevel"]), "/r", ran("/r\n"))
        .answering(
            &git(&["rev-parse", "--git-common-dir"]),
            "/r",
            ran(".git\n"),
        )
        .answering(
            &git(&["rev-parse", "--git-dir", "--git-common-dir"]),
            "/r",
            ran(".git\n.git\n"),
        )
        .answering(
            &git(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]),
            "/r",
            ran(&format!("{HEAD}\n")),
        )
        .answering(
            &["git", "rev-parse", "--verify", "--quiet", &head_commit],
            "/r",
            ran(&format!("{HEAD}\n")),
        )
        .answering(
            &git(&["ls-files", "--others", "--exclude-standard", "-z"]),
            "/r",
            ran("src/lib.rs\0"),
        )
        .otherwise(ran(""));
    let files = FakeFiles::new(Posix)
        .with_file("/r/Cargo.toml", "[package]\nname = \"r\"\n")
        .with_file("/r/src/lib.rs", "//! R.\n");
    FakePorts {
        process,
        ..FakePorts::new(files)
    }
}

/// A reviewer's `subagent-start` in `/r` on `ports`; the outer result fails only when the
/// test's own input or patterns do.
fn reviewer_starts(
    ports: &FakePorts,
) -> Result<Result<HookOutcome, HookError>, Box<dyn std::error::Error>> {
    let env = RunEnv::at("/r", Posix);
    let input = HookInput::parse(
        r#"{"session_id":"s1","cwd":"/r","agent_type":"temper:rust-reviewer","agent_id":"rv1"}"#,
    )?;
    let call = HookCall {
        ports,
        env: &env,
        input: &input,
    };
    let scanner = Scanner::new()?;
    Ok(crate::hooks::run(&call, &scanner, HookEvent::SubagentStart))
}

/// 3e-16: a reviewer's start asks git each question about refs and objects once, however
/// many trees, fingerprints and change lists read the answer, and still formats the tree.
#[test]
fn a_reviewer_asks_git_each_ref_question_once() {
    let ports = repository();

    let outcome = reviewer_starts(&ports).expect("the test's input and patterns parse");

    assert!(
        matches!(outcome, Ok(HookOutcome::Context { ref text, .. }) if text.contains("src/lib.rs")),
        "the reviewer's context names the change: {outcome:?}"
    );
    let requests = ports.process.requests();
    let lasting = [
        "rev-parse",
        "symbolic-ref",
        "merge-base",
        "rev-list",
        "show",
    ];
    let mut seen: HashSet<(&[OsString], &PathBuf)> = HashSet::new();
    let repeated: Vec<_> = requests
        .iter()
        .filter(|(program, args, _)| {
            program == "git"
                && args
                    .first()
                    .is_some_and(|sub| lasting.iter().any(|l| sub == l))
        })
        .filter(|(_, args, cwd)| !seen.insert((args.as_slice(), cwd)))
        .collect();
    assert_eq!(
        repeated,
        Vec::<&(String, Vec<OsString>, PathBuf)>::new(),
        "asked again"
    );
    let fmt: Vec<OsString> = ["fmt", "--all"].map(OsString::from).to_vec();
    assert_eq!(
        requests
            .iter()
            .filter(|(program, args, _)| program == "cargo" && *args == fmt)
            .count(),
        1,
        "cargo fmt --all ran in the tree"
    );
}

/// #210 review F3: 1.x's `g.steps.filter(…)` throws on a red gate record while it builds the
/// gate line, before it reads the other worktrees, the earlier verdict and the diff stat; so
/// the run fails without asking git for the diff stat.
#[test]
fn a_red_gate_record_1x_cannot_filter_fails_before_the_diff_stat() {
    let ports = repository();
    let first = reviewer_starts(&ports).expect("the test's input and patterns parse");
    let Ok(HookOutcome::Context { text, .. }) = &first else {
        panic!("the first start answers: {first:?}");
    };
    let fp = text
        .lines()
        .find_map(|line| line.strip_prefix("- Current tree fingerprint: "))
        .expect("the context names the fingerprint");
    ports
        .files
        .write_atomic(
            Path::new(&format!("/r/.git/temper/gate/{fp}.json")),
            r#"{"ok":false,"steps":"x"}"#,
        )
        .expect("the fake writes");
    let before = ports.process.requests().len();

    let second = reviewer_starts(&ports).expect("the test's input and patterns parse");

    assert!(
        matches!(second, Err(HookError::InputType { .. })),
        "{second:?}"
    );
    let stat = ports.process.requests()[before..]
        .iter()
        .filter(|(_, args, _)| {
            args.first().is_some_and(|a| a == "diff")
                && args.get(1).is_some_and(|a| a == "--stat=120")
        })
        .count();
    assert_eq!(stat, 0, "no diff stat after the throw");
}
