//! The shell-independent checks of `pre-bash`, each against 1.x on the same inputs: every
//! expected value was printed by 1.x's own function (`shellWrites` and `fromFirst` evaluated
//! from `hooks.mjs`, the exported `blankProse`, `baselineRaise`, `claudeRestore` and
//! `cargoDependencyCommands`). The handler's order and texts are ported in
//! `temper-app`'s `hooks::pre_bash::tests`.

use super::deps::DepPatterns;
use super::flags::FlagPatterns;
use super::prose::ProsePatterns;
use super::raise::{RaiseFlag, RaisePatterns};
use super::writes::{BASELINE, LOCKFILE, MANIFEST, RUST_SOURCE, ShellWrites, Target, VENDORED};

/// Each command of `table` against `target`'s patterns.
fn writes(target: Target, table: &[(&str, bool)]) {
    let patterns = ShellWrites::new(target).expect("the patterns compile");
    for &(cmd, want) in table {
        assert_eq!(patterns.test(cmd), want, "{cmd:?}");
    }
}

#[test]
fn shell_writes_to_rust_sources_follow_1x() {
    writes(
        RUST_SOURCE,
        &[
            ("echo x > src/lib.rs", true),
            ("echo x >src/a.rs.bak", false),
            ("echo x > a.rs/b", true),
            ("cat a.rs > b.txt", false),
            ("2>a.rs x", false),
            ("x &>> lib.rs", true),
            ("x >| lib.rs", true),
            ("x 1>lib.rs", false),
            ("tee -a 'lib.rs'", true),
            ("sed -n p lib.rs", false),
            ("sed -i s/a/b/ lib.rs", true),
            ("sed s/a/b/ lib.rs -Ei", true),
            ("sed --in-place s lib.rs", true),
            ("sed -i s/a/b/ x | cat lib.rs", false),
            ("perl -pi -e s lib.rs", true),
            ("perl -e s lib.rs", false),
            ("python -c \"open('lib.rs', 'w')\"", true),
            ("python -c \"open('lib.rs')\"", false),
            ("python -c \"open(x) ; f('lib.rs', 'w')\"", false),
            ("Set-Content lib.rs x", true),
            ("cp a lib.rs", false),
            ("cp lib.rs /tmp/x", false),
            ("cp a Cargo.lock", false),
            ("cp a 'Cargo.lock'", false),
            ("cp Cargo.lock x", false),
            ("mv x .claude/temper/a.mjs", false),
            ("cp x .claude/temper/.claude/temper/y", false),
            ("echo > .claude/temper/x.mjs", false),
            ("echo > a/.claude/temper", false),
            ("cat .claude/temper/temper.mjs", false),
            ("python3 -c \"open('.claude/temper/x', 'w')\"", false),
            ("cp x Cargo.toml\n", false),
            ("cp x (Cargo.toml)", false),
            ("install -m 644 x Cargo.toml", false),
            ("Copy-Item x Cargo.toml", false),
            ("x > temper-baseline.json.tmp", false),
            ("echo > .claude/temper-baseline.json", false),
        ],
    );
}

#[test]
fn shell_writes_to_lock_follow_1x_with_copies() {
    writes(
        LOCKFILE,
        &[
            ("echo x > src/lib.rs", false),
            ("echo x >src/a.rs.bak", false),
            ("echo x > a.rs/b", false),
            ("cat a.rs > b.txt", false),
            ("2>a.rs x", false),
            ("x &>> lib.rs", false),
            ("x >| lib.rs", false),
            ("x 1>lib.rs", false),
            ("tee -a 'lib.rs'", false),
            ("sed -n p lib.rs", false),
            ("sed -i s/a/b/ lib.rs", false),
            ("sed s/a/b/ lib.rs -Ei", false),
            ("sed --in-place s lib.rs", false),
            ("sed -i s/a/b/ x | cat lib.rs", false),
            ("perl -pi -e s lib.rs", false),
            ("perl -e s lib.rs", false),
            ("python -c \"open('lib.rs', 'w')\"", false),
            ("python -c \"open('lib.rs')\"", false),
            ("python -c \"open(x) ; f('lib.rs', 'w')\"", false),
            ("Set-Content lib.rs x", false),
            ("cp a lib.rs", false),
            ("cp lib.rs /tmp/x", false),
            ("cp a Cargo.lock", true),
            ("cp a 'Cargo.lock'", true),
            ("cp Cargo.lock x", false),
            ("mv x .claude/temper/a.mjs", false),
            ("cp x .claude/temper/.claude/temper/y", false),
            ("echo > .claude/temper/x.mjs", false),
            ("echo > a/.claude/temper", false),
            ("cat .claude/temper/temper.mjs", false),
            ("python3 -c \"open('.claude/temper/x', 'w')\"", false),
            ("cp x Cargo.toml\n", false),
            ("cp x (Cargo.toml)", false),
            ("install -m 644 x Cargo.toml", false),
            ("Copy-Item x Cargo.toml", false),
            ("x > temper-baseline.json.tmp", false),
            ("echo > .claude/temper-baseline.json", false),
        ],
    );
}

#[test]
fn shell_writes_to_toml_follow_1x_with_copies() {
    writes(
        MANIFEST,
        &[
            ("echo x > src/lib.rs", false),
            ("echo x >src/a.rs.bak", false),
            ("echo x > a.rs/b", false),
            ("cat a.rs > b.txt", false),
            ("2>a.rs x", false),
            ("x &>> lib.rs", false),
            ("x >| lib.rs", false),
            ("x 1>lib.rs", false),
            ("tee -a 'lib.rs'", false),
            ("sed -n p lib.rs", false),
            ("sed -i s/a/b/ lib.rs", false),
            ("sed s/a/b/ lib.rs -Ei", false),
            ("sed --in-place s lib.rs", false),
            ("sed -i s/a/b/ x | cat lib.rs", false),
            ("perl -pi -e s lib.rs", false),
            ("perl -e s lib.rs", false),
            ("python -c \"open('lib.rs', 'w')\"", false),
            ("python -c \"open('lib.rs')\"", false),
            ("python -c \"open(x) ; f('lib.rs', 'w')\"", false),
            ("Set-Content lib.rs x", false),
            ("cp a lib.rs", false),
            ("cp lib.rs /tmp/x", false),
            ("cp a Cargo.lock", false),
            ("cp a 'Cargo.lock'", false),
            ("cp Cargo.lock x", false),
            ("mv x .claude/temper/a.mjs", false),
            ("cp x .claude/temper/.claude/temper/y", false),
            ("echo > .claude/temper/x.mjs", false),
            ("echo > a/.claude/temper", false),
            ("cat .claude/temper/temper.mjs", false),
            ("python3 -c \"open('.claude/temper/x', 'w')\"", false),
            ("cp x Cargo.toml\n", true),
            ("cp x (Cargo.toml)", true),
            ("install -m 644 x Cargo.toml", true),
            ("Copy-Item x Cargo.toml", true),
            ("x > temper-baseline.json.tmp", false),
            ("echo > .claude/temper-baseline.json", false),
        ],
    );
}

#[test]
fn shell_writes_to_baseline_follow_1x_with_copies() {
    writes(
        BASELINE,
        &[
            ("echo x > src/lib.rs", false),
            ("echo x >src/a.rs.bak", false),
            ("echo x > a.rs/b", false),
            ("cat a.rs > b.txt", false),
            ("2>a.rs x", false),
            ("x &>> lib.rs", false),
            ("x >| lib.rs", false),
            ("x 1>lib.rs", false),
            ("tee -a 'lib.rs'", false),
            ("sed -n p lib.rs", false),
            ("sed -i s/a/b/ lib.rs", false),
            ("sed s/a/b/ lib.rs -Ei", false),
            ("sed --in-place s lib.rs", false),
            ("sed -i s/a/b/ x | cat lib.rs", false),
            ("perl -pi -e s lib.rs", false),
            ("perl -e s lib.rs", false),
            ("python -c \"open('lib.rs', 'w')\"", false),
            ("python -c \"open('lib.rs')\"", false),
            ("python -c \"open(x) ; f('lib.rs', 'w')\"", false),
            ("Set-Content lib.rs x", false),
            ("cp a lib.rs", false),
            ("cp lib.rs /tmp/x", false),
            ("cp a Cargo.lock", false),
            ("cp a 'Cargo.lock'", false),
            ("cp Cargo.lock x", false),
            ("mv x .claude/temper/a.mjs", false),
            ("cp x .claude/temper/.claude/temper/y", false),
            ("echo > .claude/temper/x.mjs", false),
            ("echo > a/.claude/temper", false),
            ("cat .claude/temper/temper.mjs", false),
            ("python3 -c \"open('.claude/temper/x', 'w')\"", false),
            ("cp x Cargo.toml\n", false),
            ("cp x (Cargo.toml)", false),
            ("install -m 644 x Cargo.toml", false),
            ("Copy-Item x Cargo.toml", false),
            ("x > temper-baseline.json.tmp", false),
            ("echo > .claude/temper-baseline.json", true),
        ],
    );
}

#[test]
fn shell_writes_to_vendored_follow_1x_with_copies() {
    writes(
        VENDORED,
        &[
            ("echo x > src/lib.rs", false),
            ("echo x >src/a.rs.bak", false),
            ("echo x > a.rs/b", false),
            ("cat a.rs > b.txt", false),
            ("2>a.rs x", false),
            ("x &>> lib.rs", false),
            ("x >| lib.rs", false),
            ("x 1>lib.rs", false),
            ("tee -a 'lib.rs'", false),
            ("sed -n p lib.rs", false),
            ("sed -i s/a/b/ lib.rs", false),
            ("sed s/a/b/ lib.rs -Ei", false),
            ("sed --in-place s lib.rs", false),
            ("sed -i s/a/b/ x | cat lib.rs", false),
            ("perl -pi -e s lib.rs", false),
            ("perl -e s lib.rs", false),
            ("python -c \"open('lib.rs', 'w')\"", false),
            ("python -c \"open('lib.rs')\"", false),
            ("python -c \"open(x) ; f('lib.rs', 'w')\"", false),
            ("Set-Content lib.rs x", false),
            ("cp a lib.rs", false),
            ("cp lib.rs /tmp/x", false),
            ("cp a Cargo.lock", false),
            ("cp a 'Cargo.lock'", false),
            ("cp Cargo.lock x", false),
            ("mv x .claude/temper/a.mjs", true),
            ("cp x .claude/temper/.claude/temper/y", true),
            ("echo > .claude/temper/x.mjs", true),
            ("echo > a/.claude/temper", false),
            ("cat .claude/temper/temper.mjs", false),
            ("python3 -c \"open('.claude/temper/x', 'w')\"", true),
            ("cp x Cargo.toml\n", false),
            ("cp x (Cargo.toml)", false),
            ("install -m 644 x Cargo.toml", false),
            ("Copy-Item x Cargo.toml", false),
            ("x > temper-baseline.json.tmp", false),
            ("echo > .claude/temper-baseline.json", false),
        ],
    );
}

/// `fromFirst` reads each segment once from its first key, so a long run of keys stays linear
/// (#155); the decision is the one a search from any key would give.
#[test]
fn from_first_decides_long_runs_of_keys() {
    let rs = ShellWrites::new(RUST_SOURCE).expect("the patterns compile");
    let vendored = ShellWrites::new(VENDORED).expect("the patterns compile");

    let got = [
        rs.test(&format!("{}lib.rs", "sed ".repeat(32_768))),
        rs.test(&format!("{}-i lib.rs", "sed ".repeat(32_768))),
        rs.test(&format!("{}x", "write_text ".repeat(16_384))),
        rs.test(&format!("{}lib.rs", "write_text ".repeat(16_384))),
        vendored.test(&format!("cp {}<", ".claude/temper/".repeat(8192))),
        vendored.test(&format!("mv x {}", ".claude/temper/".repeat(8192))),
    ];

    assert_eq!(got, [false, true, false, true, false, true]);
}

#[test]
fn blank_prose_follows_1x() {
    let prose = ProsePatterns::new().expect("the patterns compile");
    for (cmd, want) in [
        ("git commit -m --x y", "git commit -m --x y"),
        ("git commit --message= --x", "git commit --message'' --x"),
        ("git commit --message=--x", "git commit --message''"),
        ("git commit -mm --x", "git commit -m'' --x"),
        ("git commit -m - -m x", "git commit -m '' -m ''"),
        ("git commit -m -", "git commit -m ''"),
        ("git commit -m --", "git commit -m --"),
        ("git commit -bm x", "git commit -bm ''"),
        ("cat <<EOFX\nbody\nEOF\nnext", "cat <<EOFX\nEOF\nnext"),
        ("cat <<-EOF\n\tbody\n\tEOF", "cat <<EOF\nEOF"),
        ("cat <<-EOF\nbody\n-EOF", "cat <<-EOF\n-EOF"),
        ("cat << 'E F'\nx\nE F", "cat << 'E F'\nx\nE F"),
        ("cat <<'EOF'\nx\nEOF", "cat <<'EOF'\nEOF"),
        ("bash <<EOF\nx\nEOF", "bash <<EOF\nx\nEOF"),
        ("x <<<EOF\nb\nEOF", "x <<<EOF\nb\nEOF"),
        ("cat <<EOF\nEOF", "cat <<EOF\nEOF"),
        ("cat <<EOF\n\nEOF", "cat <<EOF\nEOF"),
        (
            "git commit -F- <<EOF\n$(rm x)\nEOF",
            "git commit -F- <<EOF\n$(rm x)\nEOF",
        ),
        (
            "git commit -F- <<'EOF'\n$(rm x)\nEOF",
            "git commit -F- <<'EOF'\nEOF",
        ),
        ("echo $(cat <<'EOF'\nEOF\n)", "echo $(cat <<'EOF'\nEOF\n)"),
        (
            "git commit -m \"$(cat <<'EOF'\nmsg --no-verify\nEOF\n)\"",
            "git commit -m ''",
        ),
        ("grep -- --no-verify f", "grep -- "),
        ("grep.exe x -- y z", "grep.exe x -- "),
        ("rgx --no-verify", "rgx --no-verify"),
        ("a/rg/x --no-verify", "a/rg/x --no-verify"),
        ("a=b rg 'x' -- --no-verify", "a=b rg '' -- "),
        ("rg\n--no-verify", "rg\n--no-verify"),
        ("x; sudo env grep \"-A\" | y", "x; sudo env grep '' | y"),
        ("foo\\\nbar", "foo bar"),
        ("git commit -m x\\\r\n -n", "git commit -m ''  -n"),
        ("printf x | rg 'a' -- b c", "printf x | rg '' -- "),
        ("(grep 'q')", "(grep '')"),
    ] {
        assert_eq!(prose.blank(cmd), want, "{cmd:?}");
    }
}

/// 1.x "blankProse: a megabyte of starts, assignments, paths or newlines is blanked in well under
/// a second": 1.x returns each input unchanged. The wall-clock bound is retired into the hook
/// parity harness's per-step budget (OC-4 A, wi-3ea323).
#[test]
fn blank_prose_leaves_a_megabyte_of_starts_assignments_paths_or_newlines_unchanged() {
    let prose = ProsePatterns::new().expect("the patterns compile");
    for (word, n) in [
        (";a=", 1 << 18),
        ("(/", 1 << 19),
        ("(", 1 << 20),
        ("\n", 1 << 20),
        (" \n", 1 << 19),
        ("a=b ", 1 << 18),
    ] {
        let cmd = format!("{}x", word.repeat(n));
        assert!(prose.blank(&cmd) == cmd, "{word:?}");
    }
}

#[test]
fn baseline_raise_reads_every_spelling_of_the_cli() {
    let raise = RaisePatterns::new().expect("the patterns compile");
    let flag = |text: &str| match text {
        "--init" => RaiseFlag::Init,
        _ => RaiseFlag::Update,
    };
    for (cmd, want) in [
        ("node \"$TEMPER_CLI\" baseline --init", Some(flag("--init"))),
        (
            "node .claude/temper/temper.mjs baseline \"--update\"",
            Some(flag("--update")),
        ),
        ("temper baseline --init", Some(flag("--init"))),
        ("bin/temper baseline --init", None),
        ("node $T -a=$0 -b baseline --update", Some(flag("--update"))),
        ("node $T baseline --tighten", None),
        ("temper baseline --initx", None),
        ("temper  baseline\r --init", None),
        ("x=temper baseline --update", Some(flag("--update"))),
        (
            "node \"$TEMPER_CLI\" baseline --update; temper gate",
            Some(flag("--update")),
        ),
        ("echo temper baseline --init", Some(flag("--init"))),
        (
            "node $T --no-cache baseline -update",
            Some(flag("--update")),
        ),
        // D16(g): `temper.exe`, and `temper` by an absolute path; 1.x finds neither.
        ("temper.exe baseline --init", Some(RaiseFlag::Init)),
        (
            "/usr/local/bin/temper baseline --update",
            Some(RaiseFlag::Update),
        ),
        (
            "C:\\tools\\temper.exe baseline --init",
            Some(RaiseFlag::Init),
        ),
    ] {
        assert_eq!(raise.baseline_raise(cmd), want, "{cmd:?}");
    }
}

#[test]
fn claude_restore_names_the_restoring_segment() {
    let raise = RaisePatterns::new().expect("the patterns compile");
    for (cmd, want) in [
        (
            "git checkout HEAD~3 -- .claude/temper-baseline.json",
            Some("git checkout HEAD~3 -- .claude/temper-baseline.json".to_owned()),
        ),
        (
            "git restore --source=HEAD~3 .claude/x",
            Some("git restore --source=HEAD~3 .claude/x".to_owned()),
        ),
        (
            "git restore -sHEAD~1 .claude/x",
            Some("git restore -sHEAD~1 .claude/x".to_owned()),
        ),
        (
            "git restore -s=main .claude/x",
            Some("git restore -s=main .claude/x".to_owned()),
        ),
        (
            "git restore .claude/x -s",
            Some("git restore .claude/x -s".to_owned()),
        ),
        ("git restore -s HEAD .claude/x", None),
        ("git restore --source @ .claude/x", None),
        (
            "git stash pop .claude",
            Some("git stash pop .claude".to_owned()),
        ),
        ("git checkout -- .claude/x", None),
        (
            "git checkout main .claude/settings.json",
            Some("git checkout main .claude/settings.json".to_owned()),
        ),
        ("git checkout .claude/settings.json", None),
        (
            "git -C a -c b=c checkout main -- ./.claude/x",
            Some("git -C a -c b=c checkout main -- ./.claude/x".to_owned()),
        ),
        (
            "git show HEAD:x > .claude/settings.json",
            Some("git show HEAD:x > .claude/settings.json".to_owned()),
        ),
        (
            "git show HEAD:x >.claude/a",
            Some("git show HEAD:x >.claude/a".to_owned()),
        ),
        (
            "  git stash && git checkout old -- .claude  ",
            Some("git checkout old -- .claude".to_owned()),
        ),
        (
            "git checkout v1 -- x:/.claude/y",
            Some("git checkout v1 -- x:/.claude/y".to_owned()),
        ),
    ] {
        assert_eq!(raise.claude_restore(cmd), want, "{cmd:?}");
    }
}

#[test]
fn cargo_dependency_commands_name_their_crates() {
    let flags = FlagPatterns::new().expect("the patterns compile");
    let deps = DepPatterns::new().expect("the patterns compile");
    for (cmd, want) in [
        ("cargo add -p ledger-domain serde_yaml", "add:serde_yaml"),
        (
            "cargo add -p ledger-domain --features derive serde thiserror@2",
            "add:serde,thiserror",
        ),
        ("cargo +nightly rm a b", "remove:a,b"),
        ("cargo remove", "remove:"),
        ("cargo addr2line", ""),
        ("x && cargo add --git https://github.com/x/y", "add:"),
        ("cargo add 'a b' c", "add:'',c"),
        ("cargo   add\tx --rename y z", "add:x,z"),
        ("echo cargo add x", "add:x"),
        ("cargo add --dev proptest && cargo test", "add:proptest"),
    ] {
        let blanked = flags.blanked(cmd);
        let got: Vec<String> = deps
            .commands(flags.segments(&blanked))
            .iter()
            .map(|c| format!("{}:{}", c.op.name(), c.crates.join(",")))
            .collect();
        assert_eq!(got.join(";"), want, "{cmd:?}");
    }
}

/// Long runs of the starts each hand-written matcher resumes from: `git` words for
/// `GIT_COMMIT_N`, `git commit` for its tail, CLI spellings for `baselineCall`, and
/// here-document openers with long tags or many lines. Each decision is 1.x's; a matcher that
/// rescans from every start makes this test hang instead of fail (review of #200, F1 to F3).
#[test]
fn long_runs_of_matcher_starts_are_decided() {
    let flags = FlagPatterns::new().expect("the patterns compile");
    let raise = RaisePatterns::new().expect("the patterns compile");
    let prose = ProsePatterns::new().expect("the patterns compile");

    assert!(!flags.skips_git_hooks(&format!("git{}", " -git".repeat(1 << 16))));
    assert!(!flags.skips_git_hooks(&"git commit ".repeat(1 << 15)));
    assert_eq!(raise.baseline_raise(&"x=temper ".repeat(1 << 17)), None);
    for cmd in [
        format!("<<{}{}", "a".repeat(1 << 15), "\n".repeat((1 << 15) + 1)),
        "<<a\n".repeat(1 << 15),
        "<<a\nb\n".repeat(1 << 14),
    ] {
        assert!(prose.blank(&cmd) == cmd, "{:?}", cmd.get(..8));
    }
}

/// Rows that a naive translation of a rewritten look-around or back-reference answers
/// differently (2-16), each with 1.x's answer: the here-document's quote back-reference, and
/// `--ignore-rust-version`'s `(?![\w-])`.
#[test]
fn the_rewritten_look_arounds_answer_as_1x() {
    let prose = ProsePatterns::new().expect("the patterns compile");
    let flags = FlagPatterns::new().expect("the patterns compile");
    for cmd in [
        "cat <<'EOF\nx\nEOF",
        "cat <<\"EOF\nx\nEOF",
        "cat <<'EOF\"\nx\nEOF",
        // `SEARCH`'s `(?=\s|$)`: `rg` inside a longer word is no search command.
        "rgx 'a' -- --no-verify",
        "ager 'x' -- y",
    ] {
        assert_eq!(prose.blank(cmd), cmd, "{cmd:?}");
    }
    for (cmd, want) in [
        ("cargo test --ignore-rust-version", true),
        ("cargo test --ignore-rust-version=1", true),
        ("cargo test --ignore-rust-version.x", true),
        ("cargo test --ignore-rust-version-x", false),
        ("cargo test --ignore-rust-version_x", false),
    ] {
        assert_eq!(flags.ignores_rust_version(cmd), want, "{cmd:?}");
    }
}
