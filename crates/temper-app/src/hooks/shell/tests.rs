//! Ports of `tests/shell.test.mjs`, in its order. Tests of the readers that arrive in port
//! Phase 2f (`opaquePatch`, `codeWriteTargets`, the PowerShell expressions) are ported then;
//! a test that also asserts them keeps its other assertions here, marked. The wall-clock
//! bound of "a long chain of wrappers or reserved words parses in linear time" is retired
//! into the hook parity harness's per-step budget (OC-4 A, wi-3ea323); its decisions are
//! kept, and `long_inputs` adds 2^15 to 2^17 repeats of each start shape the reader sees.

mod long_inputs;

use temper_domain::PathStyle;

use super::words::word_path;
use super::{
    Command, CopyDest, ParseOpts, PatchCommand, PatchTool, PathCx, Target, Verb, command_deletes,
    command_writes, landing, parse_commands, patch_command, patch_files, pathspec_pattern,
    resolve_path,
};
use crate::paths::PathEnv;

/// 1.x `path.resolve(os.tmpdir(), 'shell-test-root')`.
const ROOT: &str = "/tmp/shell-test-root";
const HOME: &str = "/home/u";

fn cx() -> PathCx<'static> {
    PathCx {
        at: PathEnv {
            style: PathStyle::Posix,
            cwd: "/work",
        },
        os_tmp: "/tmp",
    }
}

/// 1.x `at(...p)`: a path under the root.
fn at(rel: &str) -> String {
    [ROOT, "/", rel].concat()
}

/// The options 1.x's tests pass: `{ cwd: root }`, with the machine's home and temp dir.
fn opts() -> ParseOpts<'static> {
    ParseOpts {
        paths: cx(),
        cwd: Some(ROOT),
        home: HOME,
        top: None,
        git_dir: ".git",
        tmp: Some("/tmp"),
    }
}

fn parse(cmd: &str) -> Vec<Command> {
    parse_with(cmd, opts())
}

/// `parse_commands` on a line whose words stay shorter than V8's longest string.
fn parse_with(cmd: &str, opts: ParseOpts<'_>) -> Vec<Command> {
    parse_commands(cmd, opts).expect("no word grows past V8's longest string")
}

/// 1.x's verb names.
fn verb(t: &Target<'_>) -> String {
    match &t.verb {
        Verb::Redirect => "redirect".to_owned(),
        Verb::Named(name) => (*name).to_owned(),
        Verb::InPlace(name) => format!("{name} -i"),
        Verb::GitMv => "git mv".to_owned(),
        Verb::GitRestore { checkout: true, .. } => "git checkout".to_owned(),
        Verb::GitRestore {
            checkout: false, ..
        } => "git restore".to_owned(),
    }
}

/// 1.x `t.path`: `wordPath(w, t.cwd)`.
fn path_of(t: &Target<'_>) -> Option<String> {
    word_path(cx(), &t.word, t.cwd.as_deref())
}

/// 1.x `writeTargets(cmd, opts)` as `[verb, path]` pairs.
fn targets_with(cmd: &str, opts: ParseOpts<'_>) -> Vec<(String, Option<String>)> {
    let commands = parse_with(cmd, opts);
    commands
        .iter()
        .flat_map(|c| command_writes(c, cx()))
        .map(|t| (verb(&t), path_of(&t)))
        .collect()
}

/// 1.x `targets(cmd)`.
fn targets(cmd: &str) -> Vec<(String, Option<String>)> {
    targets_with(cmd, opts())
}

fn target(verb: &str, rel: &str) -> (String, Option<String>) {
    (verb.to_owned(), Some(at(rel)))
}

/// The paths alone.
fn paths(cmd: &str) -> Vec<Option<String>> {
    targets(cmd).into_iter().map(|(_, p)| p).collect()
}

#[test]
fn quotes_escapes_comments_and_heredoc_bodies_never_make_a_write() {
    for cmd in [
        "echo 'x > clippy.toml'",
        "echo \"x > clippy.toml\"",
        "echo x \\> clippy.toml",
        "echo hi # > clippy.toml",
        "cat <<EOF > /dev/null\necho x > clippy.toml\nEOF",
        "echo $((1<<2))",
        "git commit -m \"cp a b > c\"",
    ] {
        let real: Vec<_> = targets(cmd)
            .into_iter()
            .filter(|(_, p)| p.as_deref() != Some("/dev/null"))
            .collect();
        assert_eq!(real, [], "{cmd:?}");
    }
}

#[test]
fn redirects_and_write_verbs_are_reported_with_absolute_paths() {
    assert_eq!(
        targets("echo x>a 2>&1 >>b &>c"),
        [
            target("redirect", "a"),
            target("redirect", "b"),
            target("redirect", "c")
        ]
    );
    assert_eq!(
        targets("jq . x | tee -a t1 t2"),
        [target("tee", "t1"), target("tee", "t2")]
    );
    assert_eq!(targets("sed -i.bak 's/a/b/' s1"), [target("sed -i", "s1")]);
    assert_eq!(
        targets("dd if=x of=d1 && curl -sSLo c1 u && wget -O w1 u"),
        [
            target("dd", "d1"),
            target("curl", "c1"),
            target("wget", "w1")
        ]
    );
    // Port Phase 2f: the python and node code targets.
    assert_eq!(
        targets("Set-Content -Path ps1 -Value x"),
        [target("set-content", "ps1")]
    );
    let checkout: Vec<String> = targets("git checkout main -- g1")
        .into_iter()
        .map(|t| t.0)
        .collect();
    assert_eq!(checkout, ["git checkout"]);
    let restore = parse("git restore --source=v1 g1");
    let restore = restore.first().map(|c| command_writes(c, cx()));
    let source = restore
        .as_ref()
        .and_then(|t| t.first())
        .map(|t| t.verb.clone());
    assert_eq!(
        source,
        Some(Verb::GitRestore {
            checkout: false,
            source: Some("v1".to_owned())
        })
    );
}

#[test]
fn compound_keywords_wrappers_bash_c_eval_find_exec_and_substitutions_reach_the_command_inside() {
    for cmd in [
        "if true; then cp x k1; fi",
        "for f in a; do cp x k1; done",
        "while :; do cp x k1; done",
        "{ cp x k1; }",
        "! cp x k1",
        "sudo -u root cp x k1",
        "env A=1 cp x k1",
        "timeout 5 cp x k1",
        "nice -n 5 cp x k1",
        "bash -c \"cp x k1\"",
        "eval 'cp x k1'",
        "find . -exec cp {} k1 \\;",
        "echo \"$(cp x k1)\"",
        "echo `cp x k1`",
    ] {
        assert_eq!(paths(cmd), [Some(at("k1"))], "{cmd:?}");
    }
    assert_eq!(targets("command -v cp"), []);
}

#[test]
fn locale_quoting_eval_dashdash_trap_and_awk_system_reach_the_command_inside() {
    for cmd in [
        "bash -c $\"cp x k1\"",
        "$\"cp\" x k1",
        "eval $\"cp x k1\"",
        "eval -- \"cp x k1\"",
        "trap 'cp x k1' EXIT",
        "trap -- 'cp x k1' EXIT INT",
        "awk 'BEGIN{system(\"cp x k1\")}'",
        "gawk -e 'BEGIN { system(\"cp x k1\") }'",
        "awk -F: 'NR==1 { system(\"cp x k1\"); print }' f",
        "awk --source='BEGIN{system(\"cp x k1\")}'",
        "awk 'BEGIN{system(\"cp\\040x\\ k1\")}'",
        "gawk -i lib.awk 'BEGIN{system(\"cp x k1\")}'",
        "gawk '-eBEGIN{system(\"cp x k1\")}'",
    ] {
        assert_eq!(paths(cmd), [Some(at("k1"))], "{cmd:?}");
    }
    for cmd in [
        "trap - EXIT",
        "trap -p",
        "trap -l",
        "trap 'rm -f k1' EXIT",
        "echo $\"x\"",
        "awk '{print $1}' f",
        "awk -f prog.awk f",
        "gawk -E prog.awk 'BEGIN{system(\"cp x k1\")}'",
    ] {
        let writes: Vec<_> = targets(cmd).into_iter().filter(|t| t.0 != "rm").collect();
        assert_eq!(writes, [], "{cmd:?}");
    }
}

#[test]
fn cd_pushd_popd_subshells_and_pipelines_set_the_directory_each_command_runs_in() {
    let dir_of = |cmd: &str| -> Vec<Option<String>> {
        parse_with(cmd, opts_home(&at("home")))
            .into_iter()
            .filter(|c| c.name.as_deref() == Some("touch"))
            .map(|c| c.cwd.as_deref().map(str::to_owned))
            .collect()
    };
    assert_eq!(
        dir_of("cd a && touch x; cd .. && touch y"),
        [Some(at("a")), Some(ROOT.to_owned())]
    );
    assert_eq!(
        dir_of("(cd a && touch x); touch y"),
        [Some(at("a")), Some(ROOT.to_owned())]
    );
    assert_eq!(
        dir_of("pushd a; touch x; popd; touch y"),
        [Some(at("a")), Some(ROOT.to_owned())]
    );
    assert_eq!(dir_of("cd a | cat; touch y"), [Some(ROOT.to_owned())]);
    assert_eq!(
        dir_of("cd ~ && touch x; cd - && touch y"),
        [Some(at("home")), Some(ROOT.to_owned())]
    );
    assert_eq!(dir_of("if true; then cd a; fi; touch x"), [Some(at("a"))]);
    assert_eq!(dir_of("cd \"$UNKNOWN\" && touch x"), [None]);
    assert_eq!(dir_of("bash -c \"cd a\"; touch x"), [Some(ROOT.to_owned())]);
}

/// Not in 1.x's tests: a `cd` of one segment from a resolved directory appends it (see
/// `walk::cd_step`), and lands where Node's `path.resolve` does.
#[test]
fn a_cd_of_one_segment_lands_where_the_full_resolve_does() {
    let posix = |cmd: &str| -> Vec<Option<String>> {
        let opts = ParseOpts {
            cwd: Some("/r/s"),
            ..opts()
        };
        targets_with(cmd, opts).into_iter().map(|t| t.1).collect()
    };
    let one = |p: &str| [Some(p.to_owned())];
    assert_eq!(
        posix("cd a; cd ..; cd ..; cd ..; cd .; cd b; > o"),
        one("/b/o")
    );
    assert_eq!(posix("cd /; cd a; > o"), one("/a/o"));
    assert_eq!(posix("cd /; cd ..; cd x; > o"), one("/x/o"));
    assert_eq!(
        posix("cd a; cd \"b c\"; cd ...; > o"),
        one("/r/s/a/b c/.../o")
    );
    assert_eq!(posix("cd a; (cd b); > o"), one("/r/s/a/o"));
    assert_eq!(posix("cd a; cd -; cd b; > o"), one("/r/s/b/o"));
    let windows = PathCx {
        at: PathEnv {
            style: PathStyle::Windows,
            cwd: r"C:\work",
        },
        os_tmp: r"C:\Temp",
    };
    let win = |cmd: &str| -> Vec<Option<String>> {
        let opts = ParseOpts {
            paths: windows,
            cwd: Some(r"C:\r"),
            ..opts()
        };
        parse_with(cmd, opts)
            .iter()
            .flat_map(|c| command_writes(c, windows))
            .map(|t| word_path(windows, &t.word, t.cwd.as_deref()))
            .collect()
    };
    // `path.win32.resolve('C:\\r', 'a', '..', '..', 'b', 'o')` and the like.
    assert_eq!(win("cd a; cd ..; cd ..; cd b; > o"), one(r"C:\b\o"));
    assert_eq!(win("cd a; cd b; > o"), one(r"C:\r\a\b\o"));
    assert_eq!(win("cd D:; cd x; > o"), one(r"D:\x\o"));
}

fn opts_home(home: &str) -> ParseOpts<'_> {
    ParseOpts { home, ..opts() }
}

#[test]
fn variables_tilde_home_and_git_rev_parse_expand_and_unknown_values_stay_none() {
    let home = at("home");
    let git = at(".git");
    let top = at("top");
    let t = |cmd: &str| -> Vec<Option<String>> {
        let opts = ParseOpts {
            home: &home,
            git_dir: &git,
            top: Some(&top),
            ..opts()
        };
        targets_with(cmd, opts).into_iter().map(|t| t.1).collect()
    };
    assert_eq!(t("D=a/b; echo x > $D/c"), [Some(at("a/b/c"))]);
    assert_eq!(t("echo x > ~/h"), [Some(at("home/h"))]);
    assert_eq!(t("echo x > \"$HOME/h\""), [Some(at("home/h"))]);
    assert_eq!(
        t("echo x > \"$(git rev-parse --git-common-dir)/temper/v\""),
        [Some(at(".git/temper/v"))]
    );
    assert_eq!(
        t("cd \"$(git rev-parse --show-toplevel)\" && echo > t"),
        [Some(at("top/t"))]
    );
    // A prefix assignment only sets the command's environment.
    assert_eq!(t("X=a cmd; echo > $X"), [None]);
    assert_eq!(t("echo > *.toml"), [None]);
}

#[test]
fn copies_report_sources_and_landing_places_each_one() {
    let first = |cmd: &str| -> (Vec<Command>, usize) {
        let commands = parse(cmd);
        let index = commands
            .iter()
            .position(|c| !command_writes(c, cx()).is_empty())
            .unwrap_or(0);
        (commands, index)
    };
    let lands = |cmd: &str, dest_is_dir: bool| -> Vec<Option<String>> {
        let (commands, k) = first(cmd);
        let c = commands.get(k).expect("the command writes");
        let writes = command_writes(c, cx());
        let t = writes.first().expect("one target");
        let dest = path_of(t).expect("the destination is known");
        let op = t.copy.as_ref().expect("a copy");
        op.sources
            .iter()
            .map(|s| {
                landing(
                    cx(),
                    t,
                    CopyDest {
                        path: &dest,
                        is_dir: dest_is_dir,
                    },
                    s,
                )
            })
            .collect()
    };
    let (commands, _) = first("cp -r a/x b/y c/");
    let writes = command_writes(commands.first().expect("cp"), cx());
    let op = writes
        .first()
        .and_then(|t| t.copy.as_ref())
        .expect("a copy");
    assert_eq!((op.into_dir, op.recursive), (Some(true), true));
    assert_eq!(
        lands("cp -r a/x b/y c/", false),
        [Some(at("c/x")), Some(at("c/y"))]
    );
    assert_eq!(lands("mv a b", false), [Some(at("b"))]);
    assert_eq!(lands("mv a b", true), [Some(at("b/a"))]);
    assert_eq!(lands("cp -t d a", false), [Some(at("d/a"))]);
    assert_eq!(lands("rsync -a src/ dst", true), [Some(at("dst"))]);
    assert_eq!(targets("rsync -a x host:/y"), []);
    assert_eq!(targets("install -d a b"), []);
    let (commands, _) = first("Copy-Item -Path a -Destination b -Recurse");
    let writes = command_writes(commands.first().expect("copy"), cx());
    assert!(
        writes
            .first()
            .and_then(|t| t.copy.as_ref())
            .is_some_and(|op| op.recursive)
    );
}

/// 1.x `patchCommand(c)` of the first command that is one.
fn patch_of(cmd: &str) -> Option<Patched> {
    let commands = parse(cmd);
    commands
        .iter()
        .find_map(|c| patch_command(c, cx()).map(Patched::of))
}

/// What a patch command does with its patch.
#[derive(Debug, PartialEq, Eq)]
enum Does {
    Apply,
    Inspect,
}

/// What the tests read of a patch command.
#[derive(Debug, PartialEq, Eq)]
struct Patched {
    /// Whether it applies a patch or only inspects.
    does: Does,
    tool: PatchTool,
    inputs: Vec<Option<String>>,
    cwd: Option<String>,
    stored: bool,
    guessed: bool,
}

impl Patched {
    /// It applies a patch.
    fn apply(&self) -> bool {
        self.does == Does::Apply
    }

    fn of(p: PatchCommand<'_>) -> Self {
        match p {
            PatchCommand::Applies(p) => Self {
                does: Does::Apply,
                tool: p.tool,
                inputs: p
                    .inputs
                    .iter()
                    .map(|w| w.value().map(str::to_owned))
                    .collect(),
                cwd: p.cwd.as_deref().map(str::to_owned),
                stored: p.stored,
                guessed: p.guessed,
            },
            PatchCommand::Inspects(tool) => Self {
                does: Does::Inspect,
                tool,
                inputs: Vec::new(),
                cwd: None,
                stored: false,
                guessed: false,
            },
        }
    }
}

fn inputs(names: &[&str]) -> Vec<Option<String>> {
    names.iter().map(|n| Some((*n).to_owned())).collect()
}

#[test]
fn patch_command_reads_git_apply_git_am_and_patch_and_inspecting_forms_apply_nothing() {
    let first = |cmd: &str| {
        parse(cmd)
            .first()
            .and_then(|c| patch_command(c, cx()).map(Patched::of))
    };
    assert_eq!(first("cargo test"), None);
    for cmd in [
        "git apply --check x",
        "git apply --stat x",
        "git apply -h",
        "git apply --help",
        "patch --dry-run -p1",
        "patch --version",
        "patch -v",
        "git am --abort",
    ] {
        assert_eq!(first(cmd).map(|p| p.apply()), Some(false), "{cmd:?}");
    }
    assert_eq!(
        first("git apply -p1 a.diff").map(|p| p.inputs),
        Some(inputs(&["a.diff"]))
    );
    let am = first("git -C sub am a.mbox").expect("git am");
    assert_eq!((am.inputs, am.cwd), (inputs(&["a.mbox"]), Some(at("sub"))));
    assert_eq!(first("git am --continue").map(|p| p.stored), Some(true));
    let pt = first("patch -d sub -p1 -i x.diff").expect("patch");
    assert_eq!((pt.cwd, pt.inputs), (Some(at("sub")), inputs(&["x.diff"])));
    assert_eq!(
        patch_files("--- a/x.rs\t2026-01-01 00:00:00\n+++ b/x.rs\n"),
        ["a/x.rs", "b/x.rs"]
    );
}

#[test]
fn patch_command_reads_gpatch_busybox_patch_watch_and_git_c_aliases() {
    for cmd in [
        "gpatch -p1 -i x.diff",
        "busybox patch -p1 -i x.diff",
        "/usr/bin/gpatch -p1 -i x.diff",
        "watch git apply x.diff",
        "watch -n 1 -d git apply x.diff",
        "watch -n1 \"git apply x.diff\"",
        "watch --interval=2 -x git apply x.diff",
        "git -c alias.ap=apply ap x.diff",
        "git -c 'alias.ap=apply --index' ap x.diff",
        "git -c alias.a=b -c alias.b=apply a x.diff",
        "git -c alias.AP=apply ap x.diff",
    ] {
        let p = patch_of(cmd).expect(cmd);
        assert!(p.apply(), "{cmd:?}");
        assert_eq!(p.inputs, inputs(&["x.diff"]), "{cmd:?}");
    }
    assert_eq!(
        patch_of("git -c alias.x=am x a.mbox").map(|p| p.tool),
        Some(PatchTool::GitAm)
    );
    for cmd in [
        "gpatch --dry-run -p1 -i x.diff",
        "busybox patch --dry-run -i x.diff",
        "watch git apply --check x.diff",
        "git -c 'alias.ck=apply --check' ck x.diff",
    ] {
        assert_eq!(patch_of(cmd).map(|p| p.apply()), Some(false), "{cmd:?}");
    }
    for cmd in [
        "git -c alias.st=status st",
        "watch -n 5 git status",
        "busybox ls",
        "git -c alias.ap=apply status",
        "git ap x.diff",
    ] {
        assert_eq!(patch_of(cmd), None, "{cmd:?}");
    }
}

#[test]
fn a_subcommand_git_does_not_have_under_unread_configuration_reads_as_a_guessed_git_apply() {
    for cmd in [
        "X=apply git --config-env=alias.ap=X ap x.diff",
        "git --config-env alias.ap=X ap x.diff",
        "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=alias.ap GIT_CONFIG_VALUE_0=apply git ap x.diff",
        "export GIT_CONFIG_COUNT=1; git ap x.diff",
        "env GIT_CONFIG_PARAMETERS=\"'alias.ap=apply'\" git ap x.diff",
        "git -c include.path=/tmp/c ap x.diff",
        "git -c includeIf.onbranch:main.path=/tmp/c ap x.diff",
    ] {
        let p = patch_of(cmd).expect(cmd);
        assert!(p.apply() && p.guessed, "{cmd:?}");
        assert_eq!(p.inputs, inputs(&["x.diff"]), "{cmd:?}");
    }
    for cmd in [
        "GIT_CONFIG_COUNT=1 git status",
        "git --config-env=alias.ap=X log",
        "git -c alias.status=apply status",
        "git ap x.diff",
    ] {
        assert_eq!(patch_of(cmd), None, "{cmd:?}");
    }
    assert_eq!(
        patch_of("git -c alias.ap=apply ap x.diff").map(|p| p.guessed),
        Some(false)
    );
}

#[test]
fn the_dashed_forms_git_apply_and_git_am_at_any_path_read_as_git_apply_and_git_am() {
    for cmd in [
        "git-apply x.diff",
        "/usr/lib/git-core/git-apply x.diff",
        "\"$(git --exec-path)/git-apply\" x.diff",
        "sudo git-apply -p1 x.diff",
    ] {
        let p = patch_of(cmd).expect(cmd);
        assert_eq!((p.tool, p.apply()), (PatchTool::GitApply, true), "{cmd:?}");
        assert_eq!(p.inputs, inputs(&["x.diff"]), "{cmd:?}");
    }
    assert_eq!(
        patch_of("git-am x.mbox").map(|p| p.tool),
        Some(PatchTool::GitAm)
    );
    assert_eq!(
        patch_of("/usr/lib/git-core/git-apply --check x.diff").map(|p| p.apply()),
        Some(false)
    );
    assert_eq!(patch_of("git-diff HEAD"), None);
}

#[test]
fn piped_shell_scripts_heredocs_and_here_strings_as_the_patch_reader_sees_them() {
    // 1.x "echo or printf piped into a shell is not read as the script": its `patchCommand`
    // half. The `opaquePatch` half arrives in port Phase 2f.
    for cmd in [
        "echo 'git apply x.diff' | bash",
        "printf 'git apply x.diff\\n' | sh",
        "echo git apply x.diff | bash -s",
        "echo 'git apply x.diff' | bash -s -- a b",
        "echo -e 'true\\ngit apply x.diff' | sh",
        "printf '%s\\n' 'git apply x.diff' | bash",
        "echo 'git apply x.diff' |& sh",
        "X=x.diff; echo \"git apply $X\" | bash",
        "echo 'git apply x.diff' > out.txt | bash",
    ] {
        assert_eq!(patch_of(cmd), None, "{cmd:?}");
    }
    assert_eq!(patch_of("echo 'git apply x.diff' | grep apply"), None);
    for cmd in [
        "bash <<'EOF'\ngit apply x.diff\nEOF",
        "bash <<< 'git apply x.diff'",
        "cat <<'EOF' | bash\ngit apply x.diff\nEOF",
    ] {
        assert_eq!(
            patch_of(cmd).map(|p| p.inputs),
            Some(inputs(&["x.diff"])),
            "{cmd:?}"
        );
    }
}

#[test]
fn a_command_name_built_from_an_expansion_reads_as_its_last_segment_here() {
    // 1.x "a command name built from an expansion is not read, whatever its last segment":
    // the halves without `untracked` and `opaquePatch` (port Phase 2f).
    for cmd in ["\"$X/git-apply\" x.diff", "\"$X\"/../git-apply x.diff"] {
        assert_eq!(
            patch_of(cmd).map(|p| p.tool),
            Some(PatchTool::GitApply),
            "{cmd:?}"
        );
    }
    for cmd in ["\"$X/git\" apply x.diff", "\"$X/patch\" -p1 < x.diff"] {
        assert_eq!(patch_of(cmd), None, "{cmd:?}");
    }
}

#[test]
fn a_command_name_with_a_slash_in_it_is_never_a_builtin() {
    let cwd_of = |cmd: &str| patch_of(cmd).and_then(|p| p.cwd);
    assert_eq!(cwd_of("cd sub; git apply x.diff"), Some(at("sub")));
    for cmd in [
        "/usr/bin/cd sub; git apply x.diff",
        "\"$HOME/cd\" sub; git apply x.diff",
        "./pushd sub; git apply x.diff",
    ] {
        assert_eq!(cwd_of(cmd), Some(ROOT.to_owned()), "{cmd:?}");
    }
    let names =
        |cmd: &str| -> Vec<Option<String>> { parse(cmd).into_iter().map(|c| c.name).collect() };
    assert_eq!(
        names("/x/eval \"git apply x.diff\""),
        [Some("/x/eval".to_owned())]
    );
    assert_eq!(
        names("/x/export X=git; $X apply x.diff"),
        [Some("/x/export".to_owned()), None]
    );
    assert_eq!(names("/x/source ./set.sh"), [Some("/x/source".to_owned())]);
}

#[test]
fn env_split_string_in_every_spelling() {
    // The `patchCommand` half; the `untracked` `opaquePatch` half arrives in port Phase 2f.
    for cmd in [
        "env -S 'git apply x.diff'",
        "env -iS 'git apply x.diff'",
        "env -vS 'git apply x.diff'",
        "env -iS'git apply x.diff'",
        "env -0vS 'git apply x.diff'",
        "env --s 'git apply x.diff'",
        "env --sp='git apply x.diff'",
        "env --split 'git apply x.diff'",
        "env --split-string='git apply x.diff'",
        "env -S 'git\\_apply x.diff'",
        "env -S 'git\\_apply\\_x.diff'",
        "env -u X -S 'git apply x.diff'",
        "env -iu X -S 'git apply x.diff'",
        "env -S 'git\\tapply x.diff'",
        "env -S 'git\\napply x.diff'",
        "env -S 'git\\vapply\\fx.diff'",
        "env -S 'git\\rapply x.diff'",
        "env --unse X git apply x.diff",
        "env - git apply x.diff",
        "env -iu X git apply x.diff",
        "env --ch=/tmp git apply x.diff",
        "env -C /tmp git apply x.diff",
    ] {
        assert_eq!(
            patch_of(cmd).map(|p| p.tool),
            Some(PatchTool::GitApply),
            "{cmd:?}"
        );
    }
}

#[test]
fn an_unquoted_empty_expansion_is_dropped_from_the_command() {
    assert_eq!(
        patch_of("X=; $X git apply x.diff").map(|p| p.tool),
        Some(PatchTool::GitApply)
    );
    assert_eq!(
        patch_of("X=''; $X git apply x.diff").map(|p| p.tool),
        Some(PatchTool::GitApply)
    );
    assert_eq!(patch_of("X=; git \"$X\" apply x.diff"), None);
}

#[test]
fn ansi_c_quoting_is_decoded_exactly_or_its_word_is_unknown() {
    let values = |cmd: &str| -> Vec<Option<String>> {
        parse(cmd)
            .first()
            .map(|c| {
                c.args
                    .iter()
                    .map(|a| a.value().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let known =
        |s: &[&str]| -> Vec<Option<String>> { s.iter().map(|v| Some((*v).to_owned())).collect() };
    assert_eq!(
        values(
            r#"echo $'git\x20apply' $'a\tb' $'\101\x41\u0041\U00000041' $'\'\"\\\?' $'\a\b\e\E\f\v' $'\q'"#
        ),
        known(&[
            "git apply",
            "a\tb",
            "AAAA",
            "'\"\\?",
            "\x07\x08\x1b\x1b\x0c\x0b",
            "\\q"
        ])
    );
    for word in [
        r"$'\cA'",
        r"$'\u00e9'",
        r"$'\xff'",
        r"$'a\0b'",
        r"$'\x'",
        r"$'\x0'",
    ] {
        assert_eq!(values(&format!("echo {word}")), [None], "{word:?}");
    }
    assert_eq!(
        patch_of(r"bash -c $'git\x20apply x.diff'").map(|p| p.tool),
        Some(PatchTool::GitApply)
    );
    // Port Phase 2f: `opaquePatch` of `bash -c $'git\cAapply x.diff'`.
}

#[test]
fn the_c_script_is_the_first_operand_after_the_options() {
    for cmd in [
        "bash -c -- 'git apply x.diff'",
        "bash -c -x 'git apply x.diff'",
        "sh -c -- 'patch -p1 < x.diff'",
        "bash -c - 'git apply x.diff'",
        "bash -c -o pipefail 'git apply x.diff'",
        "bash -co pipefail 'git apply x.diff'",
        "bash -euo pipefail -c 'git apply x.diff'",
        "bash -c -O extglob -- 'git apply x.diff' arg0",
        "bash -xc 'git apply x.diff'",
        "dash -c -e 'git apply x.diff'",
    ] {
        assert_eq!(patch_of(cmd).map(|p| p.apply()), Some(true), "{cmd:?}");
    }
    assert_eq!(
        targets("bash -c -- 'cp /tmp/x crates/a/src/lib.rs'"),
        [target("cp", "crates/a/src/lib.rs")]
    );
    assert_eq!(
        targets("bash -euo pipefail -c 'cp /tmp/x out.rs'"),
        [target("cp", "out.rs")]
    );
    // Without -c, the first operand is a script file, which temper does not read.
    assert_eq!(patch_of("bash -o pipefail 'git apply x.diff'"), None);
    // Port Phase 2f: `opaquePatch` of `bash -c -- 'ls'`.
}

#[test]
fn options_at_the_start_of_a_git_alias_are_read_as_git_options() {
    for cmd in [
        "git -c alias.ap='-p apply' ap x.diff",
        "git -c alias.ap='--paginate apply' ap x.diff",
        "git -c alias.ap='-c core.pager=cat apply' ap x.diff",
        "git -c alias.ap='--exec-path=/usr/lib/git-core apply' ap x.diff",
        "git -c alias.ap='-p' -c alias.b='ap apply' b x.diff",
    ] {
        let p = patch_of(cmd).expect(cmd);
        assert_eq!((p.apply(), p.tool), (true, PatchTool::GitApply), "{cmd:?}");
        assert_eq!(p.inputs, inputs(&["x.diff"]), "{cmd:?}");
    }
    assert_eq!(patch_of("git -c alias.st='-p status' st"), None);
    assert_eq!(
        patch_of("git -c alias.ck='-p apply --check' ck x.diff").map(|p| p.apply()),
        Some(false)
    );
}

#[test]
fn an_alias_chain_longer_than_temper_follows_reads_as_a_guessed_git_apply() {
    let chain: Vec<String> = (0..10)
        .map(|k| format!("-c alias.a{k}=a{}", k + 1))
        .collect();
    let cmd = format!("git {} -c alias.a10=apply a0 x.diff", chain.join(" "));
    let p = patch_of(&cmd).expect("a guessed git apply");
    assert_eq!(
        (p.apply(), p.guessed, p.tool),
        (true, true, PatchTool::GitApply)
    );
    assert_eq!(p.inputs, inputs(&["x.diff"]));
}

#[test]
fn a_long_chain_of_wrappers_or_reserved_words_parses_to_one_command() {
    for cmd in [
        format!("{}ls", "busybox ".repeat(20_000)),
        format!("{}ls", "! ".repeat(20_000)),
        format!("{}git apply x.diff", "nohup ".repeat(20_000)),
    ] {
        assert_eq!(parse(&cmd).len(), 1, "{:?}", cmd.get(..20));
    }
}

#[test]
fn resolve_path_reads_git_bash_paths_on_windows_only() {
    assert_eq!(
        resolve_path(cx(), Some("/c/x"), Some(ROOT)),
        Some("/c/x".to_owned())
    );
    assert_eq!(resolve_path(cx(), Some("rel"), None), None);
    assert_eq!(resolve_path(cx(), None, Some(ROOT)), None);
    let windows = PathCx {
        at: PathEnv {
            style: PathStyle::Windows,
            cwd: r"C:\work",
        },
        os_tmp: r"C:\Users\u\AppData\Local\Temp",
    };
    assert_eq!(
        resolve_path(windows, Some("/c/x"), Some(r"D:\r")),
        Some(r"C:\x".to_owned())
    );
    assert_eq!(
        resolve_path(windows, Some("/d"), None),
        Some(r"D:\".to_owned())
    );
    assert_eq!(
        resolve_path(windows, Some("/tmp/a"), None),
        Some(r"C:\Users\u\AppData\Local\Temp\a".to_owned())
    );
    assert_eq!(
        resolve_path(windows, Some("x/y"), Some(r"D:\r")),
        Some(r"D:\r\x\y".to_owned())
    );
    // A drive-relative path on another drive resolves from that drive's root, not the base.
    assert_eq!(
        resolve_path(windows, Some("E:y"), Some(r"D:\r")),
        Some(r"E:\y".to_owned())
    );
}

#[test]
fn temp_directories_resolve_under_the_temp_dir_and_a_mktemp_in_the_current_directory_does_not() {
    let tmp = "/tmp/shell-test-tmp";
    let t = |cmd: &str| -> Vec<Option<String>> {
        let opts = ParseOpts {
            tmp: Some(tmp),
            ..opts()
        };
        targets_with(cmd, opts).into_iter().map(|t| t.1).collect()
    };
    for cmd in [
        "echo x > \"$TMPDIR/u\"",
        "echo x > ${TMP}/u",
        "echo x > $TEMP/u",
        "echo x > $env:TEMP/u",
        "echo x > $Env:TMP/u",
        "echo x > \"$(mktemp)\"",
        "echo x > \"$(mktemp -d)/u\"",
        "D=$(mktemp -d); echo x > $D/u",
        "cd \"$(mktemp -d)\" && echo x > u",
        "echo x > \"$(mktemp -t temper.XXXX)\"",
        "echo x > `mktemp -d`/u",
    ] {
        let got = t(cmd);
        let under = got
            .first()
            .and_then(Option::as_deref)
            .is_some_and(|p| p.starts_with(&format!("{tmp}/")));
        assert!(under, "{cmd:?}: {got:?}");
    }
    assert_eq!(
        t("TMPDIR=/elsewhere; echo x > $TMPDIR/u"),
        [Some("/elsewhere/u".to_owned())]
    );
    for cmd in [
        "echo x > \"$(mktemp ./x.XXXX)\"",
        "echo x > \"$(mktemp -p . x.XXXX)\"",
        "echo x > $env:FOO/u",
        "echo x > $OUT_DIR/u",
    ] {
        assert_eq!(t(cmd), [None], "{cmd:?}");
    }
}

#[test]
fn git_rev_parse_path_format_names_the_git_dir_too() {
    let git = at(".git");
    let t = |cmd: &str| -> Vec<Option<String>> {
        let opts = ParseOpts {
            git_dir: &git,
            ..opts()
        };
        targets_with(cmd, opts).into_iter().map(|t| t.1).collect()
    };
    assert_eq!(
        t("echo x > \"$(git rev-parse --path-format=absolute --git-dir)/temper/v\""),
        [Some(at(".git/temper/v"))]
    );
    assert_eq!(
        t("echo x > \"$(git rev-parse --path-format=relative --git-common-dir)/temper/v\""),
        [Some(at(".git/temper/v"))]
    );
    assert_eq!(
        t("echo x > \"$(git rev-parse --path-format=absolute --show-prefix)/v\""),
        [None]
    );
}

#[test]
fn deeply_nested_find_exec_stops_at_the_depth_limit() {
    let cmd = format!("{}cp x k1 \\;", "find . -exec ".repeat(5000));
    assert!(!parse(&cmd).is_empty());
    assert_eq!(
        paths("find . -exec find . -exec cp x k1 \\; \\;"),
        [Some(at("k1"))]
    );
}

#[test]
fn a_test_command_is_not_powershell_and_heredoc_bodies_are_not_code_of_the_line() {
    // The no-target half; the PowerShell expressions arrive in port Phase 2f.
    assert_eq!(
        targets(
            "[ -d docs ] || mkdir docs; cat <<'EOF'\nPath(\".cargo/config.toml\").write_text(x)\nEOF"
        ),
        []
    );
    assert_eq!(targets("[[ -n x ]] && echo \"open('w.txt', 'w')\""), []);
    assert_eq!(targets("$x = 1\ncat <<'EOF'\nopen('w.txt', 'w')\nEOF"), []);
}

#[test]
fn pathspec_pattern_spots_wildcards_and_magic_in_git_checkout_and_restore_paths() {
    for raw in ["clippy.toml", ".cargo/config.toml", "docs"] {
        assert!(!pathspec_pattern(raw), "{raw:?}");
    }
    for raw in [
        "*.toml",
        ".cargo/conf?g.toml",
        "[a-c]lippy.toml",
        ":(glob)**/clippy.toml",
        ":/clippy.toml",
        ":!docs",
        ":^docs",
    ] {
        assert!(pathspec_pattern(raw), "{raw:?}");
    }
}

#[test]
fn bash_drops_a_backslash_before_any_char_outside_quotes() {
    assert_eq!(
        targets("echo {} > .git/tem\\per/verdicts/a.json"),
        [target("redirect", ".git/temper/verdicts/a.json")]
    );
    assert_eq!(targets("c\\p x k\\1"), [target("cp", "k1")]);
    assert_eq!(targets("\\cp x k1"), [target("cp", "k1")]);
    assert_eq!(
        parse("g\\it status").first().and_then(|c| c.name.clone()),
        Some("git".to_owned())
    );
    // Inside double quotes a backslash before a letter stays, as bash keeps it.
    let texts = |cmd: &str| -> Vec<String> {
        parse(cmd)
            .iter()
            .flat_map(|c| command_writes(c, cx()))
            .map(|t| t.word.text().to_owned())
            .collect()
    };
    assert_eq!(texts("echo x > \"a\\b\""), ["a\\b"]);
    // `<<\EOF` is a quoted delimiter: the body ends at EOF and is not expanded.
    assert_eq!(
        targets("cat <<\\EOF\necho x > k9\nEOF\ncp a k8"),
        [target("cp", "k8")]
    );
    let stdin = parse("cat <<\\EOF\n$x\nEOF")
        .first()
        .and_then(|c| c.stdin.clone());
    assert_eq!(
        stdin,
        Some(super::walk::Stdin::Heredoc {
            body: Some(std::rc::Rc::from("$x\n")),
            literal: true
        })
    );
}

#[test]
fn a_powershell_command_keeps_its_backslashes_as_path_separators() {
    for cmd in [
        "Copy-Item v.json -Destination .git\\temper\\a.json",
        "pwsh -Command \"Copy-Item v.json -Destination .git\\temper\\a.json\"",
        "powershell -Command 'Set-Content -Path .git\\temper\\a.json -Value x'",
        "'{}' | Out-File .git\\temper\\a.json",
    ] {
        let texts: Vec<String> = parse(cmd)
            .iter()
            .flat_map(|c| command_writes(c, cx()))
            .map(|t| t.word.text().to_owned())
            .collect();
        assert_eq!(texts, [".git\\temper\\a.json"], "{cmd:?}");
    }
}

#[test]
fn curl_and_wget_outputs_attached_to_a_cluster_are_writes_and_other_clustered_values_are_not() {
    assert_eq!(targets("curl -sSLoc2 u"), [target("curl", "c2")]);
    assert_eq!(
        targets("curl -sSLo.claude/settings.json u"),
        [target("curl", ".claude/settings.json")]
    );
    assert_eq!(targets("wget -qOw2 u"), [target("wget", "w2")]);
    assert_eq!(
        targets("curl -sSLo c3 u && curl -o c4 u && curl -oc5 u && curl --output=c6 u"),
        [
            target("curl", "c3"),
            target("curl", "c4"),
            target("curl", "c5"),
            target("curl", "c6")
        ]
    );
    assert_eq!(
        targets("curl -dfoobar u && curl -Hx-o: u && wget -Uo u"),
        []
    );
}

fn verbs(cmd: &str) -> Vec<String> {
    targets(cmd).into_iter().map(|t| t.0).collect()
}

#[test]
fn whatif_false_runs_the_powershell_command_and_whatif_and_whatif_true_do_not() {
    assert_eq!(verbs("Copy-Item a.rs b.rs -WhatIf:$false"), ["copy-item"]);
    assert_eq!(
        verbs("Set-Content -Path s.txt -Value x -WhatIf:$false"),
        ["set-content"]
    );
    for cmd in [
        "Copy-Item a.rs b.rs -WhatIf",
        "Copy-Item a.rs b.rs -WhatIf:$true",
        "Set-Content -Path s.txt -Value x -WhatIf",
    ] {
        assert_eq!(targets(cmd), [], "{cmd:?}");
    }
}

#[test]
fn only_a_bare_whatif_or_whatif_true_is_a_powershell_dry_run() {
    for v in ["0", "$f", "(1 -eq 0)", "$False", "$x", "1"] {
        assert_eq!(
            verbs(&format!("Copy-Item a.rs b.rs -WhatIf:{v}")),
            ["copy-item"],
            "{v:?}"
        );
        assert_eq!(
            verbs(&format!("Set-Content -Path s.txt -Value x -WhatIf:{v}")),
            ["set-content"],
            "{v:?}"
        );
    }
    for cmd in [
        "Copy-Item a.rs b.rs -whatif",
        "Copy-Item a.rs b.rs -WHATIF:$TRUE",
        "Set-Content -Path s.txt -Value x -WhatIf:$true",
    ] {
        assert_eq!(targets(cmd), [], "{cmd:?}");
    }
}

/// 1.x `commandDeletes` of each command, as `[verb, path, recursive]`.
fn deletes(cmd: &str) -> Vec<(String, Option<String>, bool)> {
    parse(cmd)
        .iter()
        .flat_map(|c| {
            let verb = match c.name.as_deref() {
                Some("git") => "git rm".to_owned(),
                name => name.unwrap_or("").to_owned(),
            };
            command_deletes(c, cx()).into_iter().map(move |d| {
                let path = word_path(cx(), &d.word, d.cwd.as_deref());
                (verb.clone(), path, d.recursive)
            })
        })
        .collect()
}

fn delete(verb: &str, rel: &str, recursive: bool) -> (String, Option<String>, bool) {
    (verb.to_owned(), Some(at(rel)), recursive)
}

#[test]
fn command_deletes_reads_rm_unlink_remove_item_and_its_aliases_and_git_rm() {
    assert_eq!(
        deletes("rm -f a -- -b && unlink c"),
        [
            delete("rm", "a", false),
            delete("rm", "-b", false),
            delete("unlink", "c", false)
        ]
    );
    assert_eq!(
        deletes("rm -rf d && rm --recursive e && rm -Recurse -Force f"),
        [
            delete("rm", "d", true),
            delete("rm", "e", true),
            delete("rm", "f", true)
        ]
    );
    assert_eq!(
        deletes("Remove-Item -Path g,h -Force; ri i -Recurse; del j/k; erase l; rd m"),
        [
            delete("remove-item", "g", false),
            delete("remove-item", "h", false),
            delete("ri", "i", true),
            delete("del", "j/k", false),
            delete("erase", "l", false),
            delete("rd", "m", false),
        ]
    );
    assert_eq!(
        deletes("del -r j && Remove-Item -Rec -Force k"),
        [delete("del", "j", true), delete("remove-item", "k", true)]
    );
    // The aliases are PowerShell, so a backslash stays a path separator.
    let text = parse("del .claude\\settings.json")
        .iter()
        .flat_map(|c| {
            command_deletes(c, cx())
                .into_iter()
                .map(|d| d.word.text().to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(text, [".claude\\settings.json"]);
}

/// The rest of 1.x "commandDeletes reads rm, unlink, Remove-Item and its aliases, and git rm;
/// dry runs and mentions delete nothing".
#[test]
fn command_deletes_reads_git_rm_and_inner_commands_and_dry_runs_and_mentions_delete_nothing() {
    assert_eq!(
        deletes("git rm n && git rm --cached -r o && git -C sub rm -- p"),
        [
            delete("git rm", "n", false),
            delete("git rm", "o", true),
            delete("git rm", "sub/p", false)
        ]
    );
    assert_eq!(
        deletes("cd sub && bash -c \"rm q\""),
        [delete("rm", "sub/q", false)]
    );
    for cmd in [
        "git rm -n a",
        "git rm --dry-run a",
        "git rm --pathspec-from-file=list",
        "Remove-Item a -WhatIf",
        "git commit -m \"rm a\"",
        "echo 'rm a'",
        "rmdir a",
        "git status",
    ] {
        assert_eq!(deletes(cmd), [], "{cmd:?}");
    }
    assert_eq!(
        deletes("Remove-Item -Path a -Filter *.toml")
            .first()
            .map(|d| d.1.clone()),
        Some(None)
    );
    let globbed = parse("rm *.toml");
    let globbed: Vec<bool> = globbed
        .iter()
        .flat_map(|c| command_deletes(c, cx()))
        .map(|d| d.word.glob())
        .collect();
    assert_eq!(globbed, [true]);
}
