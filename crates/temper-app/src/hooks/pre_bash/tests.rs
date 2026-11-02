//! `pre-bash` through the app with fakes: the ports of `tests/hooks.test.mjs`'s Bash cases that
//! the shell-independent checks decide, in its order, each command with the answer 1.x's
//! `preBash` gave it on the same workspace (captured by running the 1.x test file), plus
//! `tests/telemetry.test.mjs`'s "shell deny rows". The wall-clock bound of "the flag checks
//! stay fast on long commands" is retired into the hook parity harness's per-step budget
//! (OC-4 A, wi-3ea323); its decisions are kept.

mod protected;
mod rows;
mod state_dir;

use temper_domain::{HookEvent, HookOutcome, PathStyle::Posix, Scanner};

use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts};
use crate::hooks::HookInput;

/// What 1.x answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer<'a> {
    Pass,
    Deny(&'a str),
    Ask(&'a str),
}

use Answer::{Ask, Deny};

const PASS: Answer<'static> = Answer::Pass;

const MSRV: &str = "temper: --ignore-rust-version bypasses the MSRV check, so green would not mean the declared rust-version builds (MOD-01, H-12). If a dependency needs a newer Rust, raising rust-version is a human decision (H-16): say so and ask.";
const LOCKFILE: &str = "temper: Cargo.lock is maintained by Cargo and is never edited by hand (PROC-13). Change it with cargo: `cargo update -p <crate> --precise <version>` to pin or roll back one dependency, `cargo update --workspace` after a manifest edit, `cargo update --dry-run` to preview. For a merge conflict, check out one side's Cargo.lock and re-run a cargo command.";
const NO_VERIFY: &str =
    "temper: skipping git hooks (--no-verify) is not allowed (H-12). Fix what the hook reports.";
const LOWERS_LINTS: &str = "temper: lowering lint levels on the command line is not allowed (H-05). Fix the code the lint points at.";
const MANIFEST: &str = "temper: edit Cargo.toml with the Edit or Write tools, not the shell, so the manifest checks run on it (MOD-09, PERF-09, H-12, H-16). Use the Edit tool for this change.";
const RAISE_INIT: &str = "temper: `temper baseline --init` records every current finding as accepted legacy debt (H-12), so it is the human's decision, and the CLI refuses it inside Claude Code. To request it, run the same command prefixed with `TEMPER_ALLOW_RAISE=1`: the human will be asked to approve it. (`baseline --tighten` and `--check` never raise it and run freely.)";
const RAISE_UPDATE: &str = "temper: `temper baseline --update` records every current finding as accepted legacy debt (H-12), so it is the human's decision, and the CLI refuses it inside Claude Code. To request it, run the same command prefixed with `TEMPER_ALLOW_RAISE=1`: the human will be asked to approve it. (`baseline --tighten` and `--check` never raise it and run freely.)";
const ALLOW_RAISE: &str = "temper: this command sets TEMPER_ALLOW_RAISE, which lets `temper baseline --init`/`--update` record every current finding as accepted legacy debt, so the gate and CI stop failing on it. Raising the baseline weakens verification (H-12): approve only if you asked for it.";
const BASELINE: &str = "temper: .claude/temper-baseline.json records the accepted lint debt and is written only by `temper baseline` (H-12). After fixing findings, lower it with `node \"$TEMPER_CLI\" baseline --tighten`. Raising it (`baseline --update`) is a human decision: ask the human.";
const VENDORED: &str = "temper: .claude/temper/ is the vendored temper CLI that CI and the pre-commit hook run (the lint ratchet and the baseline-raise check live in it). It is written only by `temper setup`, never by hand (H-12). Leave it unchanged, or ask the human to re-vendor it.";

/// A workspace like `hooks.test.mjs`'s, at `/ws`, without git: what the Bash checks read is the
/// project and its `.claude/temper.json`.
fn files() -> FakeFiles {
    FakeFiles::new(Posix)
        .with_file(
            "/ws/Cargo.toml",
            "[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n",
        )
        .with_file(
            "/ws/crates/ledger-domain/Cargo.toml",
            "[package]\nname = \"ledger-domain\"\n\n[lints]\nworkspace = true\n",
        )
        .with_file("/ws/crates/ledger-domain/src/lib.rs", "//! Domain.\n")
        .with_file(
            "/ws/.claude/temper.json",
            r#"{"manifests":{"ownLints":["crates/foo-sys"]}}"#,
        )
}

/// The workspace and the hook run on it.
struct Ws {
    ports: FakePorts,
    env: RunEnv,
    scanner: Scanner,
}

impl Ws {
    fn new() -> Self {
        Self::with(files())
    }

    fn with(files: FakeFiles) -> Self {
        Self {
            ports: FakePorts::new(files),
            env: RunEnv::at("/ws", Posix),
            scanner: Scanner::new().expect("every built-in pattern compiles"),
        }
    }

    /// The handler's answer to `json`, which it must give.
    fn run(&self, event: HookEvent, json: &str) -> HookOutcome {
        let input = HookInput::parse(json).expect("the test's input is JSON");
        let call = crate::hooks::HookCall {
            ports: &self.ports,
            env: &self.env,
            input: &input,
        };
        crate::hooks::run(&call, &self.scanner, event).expect("the handler answers")
    }

    /// 1.x `bash(command)`: `preBash` with `hooks.test.mjs`'s input.
    fn bash(&self, cmd: &str) -> HookOutcome {
        let json = format!(
            r#"{{"tool_name":"Bash","cwd":"/ws","session_id":"hooks-test","tool_input":{{"command":{}}}}}"#,
            temper_domain::JsValue::String(cmd.to_owned()).to_json()
        );
        self.run(HookEvent::PreBash, &json)
    }

    /// 1.x `writeTo(rel, content)`: `preEdit` of a Write as the preloaded implementer.
    fn write_to(&self, rel: &str, content: &str) -> HookOutcome {
        let json = format!(
            r#"{{"tool_name":"Write","cwd":"/ws","session_id":"hooks-test","agent_type":"temper:rust-implementer","tool_input":{{"file_path":"/ws/{rel}","content":{}}}}}"#,
            temper_domain::JsValue::String(content.to_owned()).to_json()
        );
        self.run(HookEvent::PreEdit, &json)
    }

    /// The state-dir rule's and the protected-config rule's answers to `cmd`, each within a
    /// budget of `budget` bytes of directory text: whether it writes the state dir, and the
    /// protected files it writes.
    fn rules_within(&self, cmd: &str, budget: usize) -> (bool, Vec<String>) {
        let json = format!(
            r#"{{"tool_name":"Bash","cwd":"/ws","tool_input":{{"command":{}}}}}"#,
            temper_domain::JsValue::String(cmd.to_owned()).to_json()
        );
        let input = HookInput::parse(&json).expect("the test's input is JSON");
        let call = crate::hooks::HookCall {
            ports: &self.ports,
            env: &self.env,
            input: &input,
        };
        let ws = call.workspace().expect("the fake workspace is found");
        let code = temper_domain::InterpreterCode::new().expect("the built-in patterns compile");
        let guard = crate::hooks::shell::Guard::with_budget(budget);
        let state = super::state_dir::state_dir_write_in(&call, &ws, (cmd, &code), &guard);
        let guard = crate::hooks::shell::Guard::with_budget(budget);
        let line = (cmd, &code);
        let changes = super::protected::protected_shell_writes_in(&call, &ws, line, &guard);
        (state, changes.as_domain().written.to_vec())
    }

    /// Each command of `table` gets 1.x's answer.
    fn answers(&self, table: &[(&str, Answer<'static>)]) {
        for &(cmd, want) in table {
            let got = self.bash(cmd);
            assert_eq!(answer(&got), want, "{cmd:?}");
        }
    }
}

/// The answer of `outcome`, which carries no context on these checks.
fn answer(outcome: &HookOutcome) -> Answer<'_> {
    match outcome {
        HookOutcome::Pass => Answer::Pass,
        HookOutcome::Deny {
            reason,
            context: None,
        } => Deny(reason),
        HookOutcome::Ask {
            reason,
            context: None,
        } => Ask(reason),
        other => panic!("not an answer of the Bash checks: {other:?}"),
    }
}

const IGNORE_RUST_VERSION: &[(&str, Answer<'static>)] = &[
    ("cargo check --ignore-rust-version", Deny(MSRV)),
    (
        "cargo +1.96 test --workspace --ignore-rust-version --locked",
        Deny(MSRV),
    ),
    ("cargo +1.96 check --locked", PASS),
];

/// 1.x "Bash: --ignore-rust-version is denied (MOD-01)".
#[test]
fn ignore_rust_version_is_denied_mod_01() {
    let ws = Ws::new();
    ws.answers(IGNORE_RUST_VERSION);
}

const LOCKFILE_WRITES: &[(&str, Answer<'static>)] = &[
    (
        "sed -i 's/version = \"1.0.1\"/version = \"1.0.2\"/' Cargo.lock",
        Deny(LOCKFILE),
    ),
    ("echo \"\" >> Cargo.lock", Deny(LOCKFILE)),
    ("cat new.lock > Cargo.lock", Deny(LOCKFILE)),
    ("perl -pi -e s/a/b/ Cargo.lock", Deny(LOCKFILE)),
    ("tee Cargo.lock < x", Deny(LOCKFILE)),
    ("cargo update -p serde --precise 1.0.200", PASS),
    ("git checkout origin/main -- Cargo.lock", PASS),
    ("grep -n serde Cargo.lock > /tmp/serde.txt", PASS),
    ("git diff Cargo.lock", PASS),
    ("cargo generate-lockfile", PASS),
    (
        "sed -i 's/unwrap()/?/' src/lib.rs",
        Deny(
            "temper: write Rust sources with the Edit or Write tools, not the shell, so the standard's checks run on them (H-12). Use the Edit tool for this change.",
        ),
    ),
    ("echo x >| Cargo.lock", Deny(LOCKFILE)),
    ("echo x &> Cargo.lock", Deny(LOCKFILE)),
    ("cp ../other/Cargo.lock Cargo.lock", Deny(LOCKFILE)),
    ("sed -Ei s/a/b/ Cargo.lock", Deny(LOCKFILE)),
    (
        "python3 -c \"open('Cargo.lock','w').write('')\"",
        Deny(LOCKFILE),
    ),
    ("sed s/a/b/ Cargo.lock -i", Deny(LOCKFILE)),
    ("echo; sed -n p x; sed -i s/a/b/ Cargo.lock", Deny(LOCKFILE)),
    ("python3 -c \"open(*('Cargo.lock', 'w'))\"", Deny(LOCKFILE)),
    (
        "python3 -c \"open(('Cargo.lock','w')[0], 'w')\"",
        Deny(LOCKFILE),
    ),
    (
        "python3 -c \"open('(' and 'Cargo.lock', 'w')\"",
        Deny(LOCKFILE),
    ),
    ("perl -ne 'print if /serde-derive/' Cargo.lock", PASS),
    ("cargo metadata --format-version 1 > Cargo.lock.json", PASS),
    ("cp Cargo.lock /tmp/Cargo.lock.bak", PASS),
    ("git show HEAD:Cargo.lock > /tmp/old.lock", PASS),
];

/// 1.x "Bash: shell writes to Cargo.lock are denied; cargo and git are not".
#[test]
fn shell_writes_to_cargo_lock_are_denied_cargo_and_git_are_not() {
    let ws = Ws::new();
    ws.answers(LOCKFILE_WRITES);
}

const FLAGS_IN_PROSE: &[(&str, Answer<'static>)] = &[
    ("git commit -m \"docs: mention --no-verify flag\"", PASS),
    ("git commit -m 'hooks: deny git commit -n'", PASS),
    (
        "git commit -m \"docs: explain why cargo clippy -- -A clippy::x is refused\"",
        PASS,
    ),
    (
        "git commit -m \"ci: RUSTFLAGS=-A warnings is gone; no --cap-lints either\"",
        PASS,
    ),
    ("git commit --message=\"docs: --no-verify\" -a", PASS),
    ("git commit --message \"docs: --no-verify\"", PASS),
    ("git commit -m\"docs: --no-verify\"", PASS),
    ("git commit -m -n", PASS),
    ("git commit -am \"fix: -n flag\"", PASS),
    ("git commit -m x -- -n.txt", PASS),
    (
        "git commit -F- <<'EOF'\nhooks: refuse --no-verify and cargo clippy -- -A x\nEOF",
        PASS,
    ),
    ("git commit -F - <<EOF\nmention --no-verify\nEOF", PASS),
    ("git commit -F- <<\"EOF\"\nmention --no-verify\nEOF", PASS),
    ("git commit -F- <<-EOF\n\tmention --no-verify\n\tEOF", PASS),
    (
        "git commit -F- <<EOF\nmention --no-verify and cargo clippy -- -A x\nEOF\ngit push",
        PASS,
    ),
    (
        "cargo fmt && bash tests/ci.sh --quick && git commit -F- <<'EOF'\nhooks: allow quoted --no-verify\nEOF",
        PASS,
    ),
    (
        "git commit -m \"$(cat <<'EOF'\ndocs: --no-verify and git commit -n are refused\nEOF\n)\"",
        PASS,
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nhooks: allow quoted flags\n\n1) quote the flag\n2) `--no-verify` in a message is text\nEOF\n)\"",
        PASS,
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nfix: don't crash on --no-verify\nEOF\n)\"",
        PASS,
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nfix: handle a) and b) cases; mention `cargo clippy -- -A x`\nEOF\n)\"",
        PASS,
    ),
    (
        "gh pr create --title \"fix: -A flag\" --body \"cargo clippy -- -A x and --no-verify are refused\"",
        PASS,
    ),
    (
        "gh pr create --body=\"RUSTFLAGS=-A warnings stays denied\" -t x",
        PASS,
    ),
    (
        "gh pr create --title x --body \"$(cat <<'EOF'\n- Point 1) done\n- `RUSTFLAGS=-A warnings` stays denied\nEOF\n)\"",
        PASS,
    ),
    ("gh release create v1 --notes 'no --no-verify'", PASS),
    (
        "glab mr create --description \"cargo clippy -- -A x\" --title y",
        PASS,
    ),
    ("grep -- \"--no-verify\" file", PASS),
    ("grep -- --no-verify file", PASS),
    ("grep -rn -e '--no-verify' scripts/", PASS),
    ("grep -rn \"cargo clippy -- -A\" docs", PASS),
    ("egrep '-A warnings|--no-verify' ci.log", PASS),
    ("rg -n 'git commit --no-verify' docs", PASS),
    ("rg -n -- '--no-verify' docs", PASS),
    ("rg \"--cap-lints\"", PASS),
    ("git grep -n -- --no-verify", PASS),
    ("git push -n", PASS),
    ("git push -n origin main", PASS),
    ("git checkout -b fix-no-verify", PASS),
    ("git add -A", PASS),
    ("ls -A", PASS),
    ("cargo clippy -- -D warnings", PASS),
    ("cargo clippy --all-targets -- -W clippy::pedantic", PASS),
    (
        "cargo clippy --workspace --all-targets --locked -- -D warnings",
        PASS,
    ),
    ("cargo clippy --fix --allow-dirty", PASS),
    ("cargo clippy --fix --allow-staged", PASS),
    ("cargo clippy --fix --allow-no-vcs", PASS),
    (
        "cargo clippy --fix --lib -p ledger-domain --allow-dirty",
        PASS,
    ),
    ("cargo clippy --fix --allow-dirty -- -D warnings", PASS),
    ("cargo -q clippy --all-targets -- -D warnings", PASS),
    ("RUSTFLAGS=\"-D warnings\" cargo build", PASS),
    ("RUSTFLAGS=-Dwarnings cargo test", PASS),
    ("RUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps", PASS),
    (
        "RUSTFLAGS=\"--remap-path-prefix /Users/me/src/Foo-API=.\" cargo build --release",
        PASS,
    ),
    (
        "RUSTFLAGS=\"-L native=/opt/Intel-AMX/lib\" cargo build",
        PASS,
    ),
    (
        "RUSTFLAGS=\"-C link-arg=-Wl,-rpath,/opt/My-App/lib\" cargo build",
        PASS,
    ),
    (
        "RUSTFLAGS='-L /home/u/Project-Alpha/target' cargo test",
        PASS,
    ),
    (
        "RUSTDOCFLAGS=\"--html-in-header /home/u/Docs-Assets/katex.html\" cargo doc",
        PASS,
    ),
    ("git -c commit.gpgsign=false log -n 5 --oneline", PASS),
    ("git -c commit.gpgsign=false cherry-pick -n abc123", PASS),
    ("git -C commit-tools log -n 1", PASS),
    ("git commit -m x \\\n  --author \"A <a@b>\"", PASS),
    (
        "cargo clippy --workspace \\\n  --all-targets -- -D warnings",
        PASS,
    ),
    ("cargo clippy -p grep -- -D warnings", PASS),
    (
        "git commit -m \"docs: the -n and --no-verify flags are refused\"",
        PASS,
    ),
    (
        "gh pr create --title \"fix: -A flag\" --body \"run cargo clippy -- -A x? no\"",
        PASS,
    ),
    ("git -C \"dir with space\" commit -m \"mention -n\"", PASS),
    ("git -C \"dir with space\" log -n 3", PASS),
    ("\"bash\" tests/ci.sh --quick", PASS),
];

/// 1.x "Bash: --no-verify and lint lowering in a message, here-document or search pattern are allowed (H-12, H-05)".
#[test]
fn no_verify_and_lint_lowering_in_a_message_here_document_or_search_pattern_are_allowed_h_12_h_05()
{
    let ws = Ws::new();
    ws.answers(FLAGS_IN_PROSE);
}

const SKIPPED_HOOKS: &[(&str, Answer<'static>)] = &[
    ("git commit --no-verify -m x", Deny(NO_VERIFY)),
    ("git commit \"--no-verify\" -m x", Deny(NO_VERIFY)),
    ("git commit '--no-verify' -m x", Deny(NO_VERIFY)),
    ("git commit -m \"x\" --no-verify", Deny(NO_VERIFY)),
    ("git push --no-verify", Deny(NO_VERIFY)),
    ("git merge --no-verify feature", Deny(NO_VERIFY)),
    ("git commit -n", Deny(NO_VERIFY)),
    ("git commit -n -m x", Deny(NO_VERIFY)),
    ("git commit -m x -n", Deny(NO_VERIFY)),
    ("git commit -nm msg", Deny(NO_VERIFY)),
    ("git commit -anm msg", Deny(NO_VERIFY)),
    ("git commit \"-n\" -m x", Deny(NO_VERIFY)),
    ("git -C crates/a commit -n -m x", Deny(NO_VERIFY)),
    ("cargo fmt && git commit -n -m x", Deny(NO_VERIFY)),
    ("grep x f && git commit --no-verify -m x", Deny(NO_VERIFY)),
    (
        "git rebase -x \"git commit --amend --no-edit --no-verify\" HEAD~3",
        Deny(NO_VERIFY),
    ),
    (
        "git rebase --exec \"git commit --amend --no-edit -n\" HEAD~3",
        Deny(NO_VERIFY),
    ),
    ("bash -c \"git commit --no-verify -m x\"", Deny(NO_VERIFY)),
    ("bash -c -- \"git commit --no-verify\"", Deny(NO_VERIFY)),
    ("sh -c 'git commit -nm x'", Deny(NO_VERIFY)),
    (
        "\"C:\\Git\\bin\\bash.exe\" -c \"git commit --no-verify -m x\"",
        Deny(NO_VERIFY),
    ),
    (
        "powershell -Command \"git commit --no-verify\"",
        Deny(NO_VERIFY),
    ),
    ("cmd /c \"git commit --no-verify -m x\"", Deny(NO_VERIFY)),
    ("echo \"$(git commit --no-verify -m x)\"", Deny(NO_VERIFY)),
    ("eval \"git commit --no-verify -m x\"", Deny(NO_VERIFY)),
    ("echo \"git commit -n\" | bash", Deny(NO_VERIFY)),
    ("printf 'git commit -n\\n' | sh", Deny(NO_VERIFY)),
    ("bash <<<\"git commit -n -m x\"", Deny(NO_VERIFY)),
    ("echo \"--no-verify\"", Deny(NO_VERIFY)),
    (
        "cat <<EOF | bash\ngit commit --no-verify -m x\nEOF",
        Deny(NO_VERIFY),
    ),
    (
        "bash <<EOF\ngit commit --no-verify -m x\nEOF",
        Deny(NO_VERIFY),
    ),
    (
        "cat <<EOF\n$(git commit --no-verify -m x)\nEOF",
        Deny(NO_VERIFY),
    ),
    (
        "git commit -F- <<< \"msg\"\ngit commit --no-verify -m x",
        Deny(NO_VERIFY),
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nfix: don't crash\nEOF\n)\" --no-verify",
        Deny(NO_VERIFY),
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nfix: don't crash\nEOF\n)\" -n",
        Deny(NO_VERIFY),
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nfix: don't crash\nEOF\n)\" && git push --no-verify",
        Deny(NO_VERIFY),
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nhandle (a case\nEOF\n)\" --no-verify",
        Deny(NO_VERIFY),
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nit's 1) fine\nEOF\n)\" --no-verify",
        Deny(NO_VERIFY),
    ),
    (
        "cat <<\"E\"OF\nhi\nEOF\ngit commit --no-verify -m x",
        Deny(NO_VERIFY),
    ),
    (
        "cat <<EOF\nnever closed\ngit commit --no-verify -m x",
        Deny(NO_VERIFY),
    ),
    ("git commit -m \"unterminated --no-verify", Deny(NO_VERIFY)),
    ("git commit -m \"wip\" \\\n  -n", Deny(NO_VERIFY)),
    ("git commit -a \\\n  -n -m wip", Deny(NO_VERIFY)),
    (
        "git -c commit.gpgsign=false commit -n -m x",
        Deny(NO_VERIFY),
    ),
    ("git rebase -m --no-verify main", Deny(NO_VERIFY)),
    (
        "python3 - <<'EOF'\nimport subprocess\nsubprocess.run(['git', 'commit', '--no-verify', '-m', 'x'])\nEOF",
        Deny(NO_VERIFY),
    ),
    (
        "node <<'EOF'\nrequire('child_process').execSync('git commit --no-verify -m x')\nEOF",
        Deny(NO_VERIFY),
    ),
    (
        "git commit -m \"$(git commit --no-verify -m x)\"",
        Deny(NO_VERIFY),
    ),
    (
        "git commit -m \"wip `git commit --no-verify -m x`\"",
        Deny(NO_VERIFY),
    ),
    (
        "gh pr create --body \"$(git commit -n -m x)\" --title t",
        Deny(NO_VERIFY),
    ),
    (
        "grep -- $(git commit --no-verify -m x) file",
        Deny(NO_VERIFY),
    ),
    ("rg \"$(git commit --no-verify -m x)\"", Deny(NO_VERIFY)),
    ("grep -e \"`git commit -n -m x`\" f", Deny(NO_VERIFY)),
    (
        "\"bash\" <<'EOF'\ngit commit --no-verify -m x\nEOF",
        Deny(NO_VERIFY),
    ),
    (
        "'/bin/sh' <<EOF\ngit commit --no-verify -m x\nEOF",
        Deny(NO_VERIFY),
    ),
    ("git -C \"dir with space\" commit -n -m x", Deny(NO_VERIFY)),
    ("git -c 'user.name=A B' commit -nm x", Deny(NO_VERIFY)),
    ("git --git-dir=\"a b/.git\" commit -n -m x", Deny(NO_VERIFY)),
];

/// 1.x "Bash: skipping git hooks stays denied however it is quoted or handed on (H-12)".
#[test]
fn skipping_git_hooks_stays_denied_however_it_is_quoted_or_handed_on_h_12() {
    let ws = Ws::new();
    ws.answers(SKIPPED_HOOKS);
}

const LOWERED_LINTS: &[(&str, Answer<'static>)] = &[
    ("cargo clippy -- -A clippy::unwrap_used", Deny(LOWERS_LINTS)),
    ("cargo clippy -- -A warnings", Deny(LOWERS_LINTS)),
    ("cargo clippy -- -Awarnings", Deny(LOWERS_LINTS)),
    ("cargo clippy -- -Aclippy::all", Deny(LOWERS_LINTS)),
    (
        "cargo clippy -- \"-A\" clippy::unwrap_used",
        Deny(LOWERS_LINTS),
    ),
    (
        "cargo clippy -- --allow clippy::unwrap_used",
        Deny(LOWERS_LINTS),
    ),
    ("cargo clippy -- --allow=warnings", Deny(LOWERS_LINTS)),
    ("cargo clippy -- --cap-lints warn", Deny(LOWERS_LINTS)),
    (
        "cargo +1.96 clippy --workspace -- -A warnings",
        Deny(LOWERS_LINTS),
    ),
    ("cargo -q clippy -- -A x", Deny(LOWERS_LINTS)),
    ("cargo +nightly --locked clippy -- -A x", Deny(LOWERS_LINTS)),
    (
        "cargo --config net.offline=true clippy -- -A x",
        Deny(LOWERS_LINTS),
    ),
    ("cargo rustc -- -A warnings", Deny(LOWERS_LINTS)),
    ("bash -c 'cargo clippy -- -A warnings'", Deny(LOWERS_LINTS)),
    (
        "watchexec -- \"cargo clippy -- -A warnings\"",
        Deny(LOWERS_LINTS),
    ),
    (
        "git commit -m \"$(cat <<'EOF'\nfix: don't crash\nEOF\n)\"\ncargo clippy -- -A warnings",
        Deny(LOWERS_LINTS),
    ),
    ("RUSTFLAGS=\"-A warnings\" cargo build", Deny(LOWERS_LINTS)),
    (
        "RUSTFLAGS='--cap-lints warn' cargo build",
        Deny(LOWERS_LINTS),
    ),
    ("RUSTFLAGS=-Awarnings cargo build", Deny(LOWERS_LINTS)),
    (
        "RUSTFLAGS=\"-D warnings -A x\" cargo build",
        Deny(LOWERS_LINTS),
    ),
    ("RUSTDOCFLAGS=\"-A warnings\" cargo doc", Deny(LOWERS_LINTS)),
    (
        "CARGO_ENCODED_RUSTFLAGS=$'-Dwarnings\\x1f-Awarnings' cargo build",
        Deny(LOWERS_LINTS),
    ),
    (
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=\"-A warnings\" cargo build",
        Deny(LOWERS_LINTS),
    ),
    (
        "env \"RUSTFLAGS=-A warnings\" cargo build",
        Deny(LOWERS_LINTS),
    ),
    ("export 'RUSTFLAGS=-Awarnings'", Deny(LOWERS_LINTS)),
    (
        "export RUSTFLAGS=\"-A warnings\" && cargo build",
        Deny(LOWERS_LINTS),
    ),
    (
        "RUSTFLAGS=\"-L /opt/My-App/lib -A warnings\" cargo build",
        Deny(LOWERS_LINTS),
    ),
    (
        "cargo clippy --workspace --all-targets \\\n  -- -A clippy::pedantic",
        Deny(LOWERS_LINTS),
    ),
    (
        "cargo clippy --all-targets -- \\\n  -D warnings \\\n  -A dead_code",
        Deny(LOWERS_LINTS),
    ),
    ("cargo clippy -p grep -- -A clippy::all", Deny(LOWERS_LINTS)),
    ("cargo clippy -p rg -- -A dead_code", Deny(LOWERS_LINTS)),
    ("cargo clippy --bin ack -- -A dead_code", Deny(LOWERS_LINTS)),
    (
        "git commit -m x && cargo clippy --features ag -- -A x",
        Deny(LOWERS_LINTS),
    ),
];

/// 1.x "Bash: lowering lint levels stays denied however it is quoted or handed on (H-05)".
#[test]
fn lowering_lint_levels_stays_denied_however_it_is_quoted_or_handed_on_h_05() {
    let ws = Ws::new();
    ws.answers(LOWERED_LINTS);
}

const RUST_VERSION_FLAG_ONLY: &[(&str, Answer<'static>)] = &[
    (
        "git commit -m 'ci: stop passing --ignore-rust-version'",
        PASS,
    ),
    ("rg -n -- '--ignore-rust-version' .github", PASS),
    (
        "cargo fmt && RUSTFLAGS=-Dwarnings cargo test --ignore-rust-version",
        Deny(MSRV),
    ),
];

/// 1.x "Bash: --ignore-rust-version only counts as a cargo flag".
#[test]
fn ignore_rust_version_only_counts_as_a_cargo_flag() {
    let ws = Ws::new();
    ws.answers(RUST_VERSION_FLAG_ONLY);
}

const MANIFEST_WRITES: &[(&str, Answer<'static>)] = &[
    (
        "printf '[profile.release]\\nlto = true\\n' >> crates/a/Cargo.toml",
        Deny(MANIFEST),
    ),
    ("sed -i 's/1.96/1.97/' Cargo.toml", Deny(MANIFEST)),
    (
        "cp template/Cargo.toml crates/new/Cargo.toml",
        Deny(MANIFEST),
    ),
    (
        "cat > crates/x/Cargo.toml <<EOF\n[package]\nEOF",
        Deny(MANIFEST),
    ),
    ("cat Cargo.toml", PASS),
    ("grep -n workspace crates/*/Cargo.toml", PASS),
    ("cargo metadata --no-deps > meta.json", PASS),
];

/// 1.x "Bash: shell writes to Cargo.toml are refused so the manifest checks run".
#[test]
fn shell_writes_to_cargo_toml_are_refused_so_the_manifest_checks_run() {
    let ws = Ws::new();
    ws.answers(MANIFEST_WRITES);
}

const CARGO_DEPS: &[(&str, Answer<'static>)] = &[
    (
        "cargo add -p ledger-domain serde_yaml",
        Ask(
            "temper: `cargo add serde_yaml` changes the dependency graph without the manifest checks. Adding or removing a dependency is a human decision (H-16, ARC-12). Approve only if you asked for it.",
        ),
    ),
    (
        "cargo add -p ledger-domain --features derive serde thiserror@2",
        PASS,
    ),
    ("cargo add --dev proptest && cargo test", PASS),
    (
        "cargo remove regex",
        Ask(
            "temper: `cargo remove regex` changes the dependency graph without the manifest checks. Adding or removing a dependency is a human decision (H-16, ARC-12). Approve only if you asked for it.",
        ),
    ),
    (
        "cargo add --git https://github.com/x/y",
        Ask(
            "temper: `cargo add (no crate named)` changes the dependency graph without the manifest checks. Adding or removing a dependency is a human decision (H-16, ARC-12). Approve only if you asked for it.",
        ),
    ),
    ("cargo addr2line", PASS),
];

/// 1.x "Bash: cargo add/remove asks (H-16) unless every crate is pre-approved".
#[test]
fn cargo_add_remove_asks_h_16_unless_every_crate_is_pre_approved() {
    let ws = Ws::new();
    ws.answers(CARGO_DEPS);
}

const BASELINE_RAISES: &[(&str, Answer<'static>)] = &[
    ("node \"$TEMPER_CLI\" baseline --init", Deny(RAISE_INIT)),
    (
        "node \"${TEMPER_CLI}\" baseline --update",
        Deny(RAISE_UPDATE),
    ),
    (
        "node .claude/temper/temper.mjs baseline --update",
        Deny(RAISE_UPDATE),
    ),
    (
        "node \"/opt/plugins/temper/scripts/temper.mjs\" baseline --init --no-cache",
        Deny(RAISE_INIT),
    ),
    (
        "cargo fmt --all && node \"$TEMPER_CLI\" baseline --update",
        Deny(RAISE_UPDATE),
    ),
    ("temper baseline --init", Deny(RAISE_INIT)),
    (
        "node .claude/temper/temper.mjs baseline \"--update\"",
        Deny(RAISE_UPDATE),
    ),
    (
        "node .claude/temper/temper.mjs baseline '--init'",
        Deny(RAISE_INIT),
    ),
    (
        "T=.claude/temper/temper.mjs; node $T baseline --update",
        Deny(RAISE_UPDATE),
    ),
    (
        "TEMPER_ALLOW_RAISE=1 node \"$TEMPER_CLI\" baseline --init",
        Ask(ALLOW_RAISE),
    ),
    ("export TEMPER_ALLOW_RAISE=1", Ask(ALLOW_RAISE)),
    (
        "env TEMPER_ALLOW_RAISE=1 node .claude/temper/temper.mjs baseline --update",
        Ask(ALLOW_RAISE),
    ),
    ("node \"$TEMPER_CLI\" baseline --tighten", PASS),
    (
        "node .claude/temper/temper.mjs baseline --check --base \"origin/main\" --findings-out target/temper/findings.json",
        PASS,
    ),
    ("node \"$TEMPER_CLI\" baseline", PASS),
    ("node \"$TEMPER_CLI\" gate", PASS),
    ("git commit -m 'record baseline --init output'", PASS),
    ("grep -- --update notes.md", PASS),
];

/// 1.x "Bash: temper baseline --init/--update are refused unless `TEMPER_ALLOW_RAISE` asks the human (H-12); --tighten, --check and status run freely".
#[test]
fn temper_baseline_init_update_are_refused_unless_temper_allow_raise_asks_the_human_h_12_tighten_check_and_status_run_freely()
 {
    let ws = Ws::new();
    ws.answers(BASELINE_RAISES);
}

/// D16(g), new in 2.0: `temper.exe`, and `temper` by an absolute path, are the CLI too, so their
/// baseline raises are refused like the others; 1.x let them through.
#[test]
fn temper_exe_and_temper_by_an_absolute_path_raise_the_baseline_too_d16_g() {
    let ws = Ws::new();
    ws.answers(&[
        ("temper.exe baseline --update", Deny(RAISE_UPDATE)),
        (
            "/opt/temper/bin/temper.exe baseline --update",
            Deny(RAISE_UPDATE),
        ),
        ("/usr/local/bin/temper baseline --init", Deny(RAISE_INIT)),
        ("C:\\tools\\temper.exe baseline --init", Deny(RAISE_INIT)),
        ("/usr/local/bin/temper baseline --tighten", PASS),
        ("/usr/local/bin/xtemper baseline --init", PASS),
    ]);
}

const CLAUDE_RESTORES: &[(&str, Answer<'static>)] = &[
    (
        "git checkout HEAD~3 -- .claude/temper-baseline.json",
        Ask(
            "temper: `git checkout HEAD~3 -- .claude/temper-baseline.json` puts older verification configuration back into .claude/ (the lint baseline, settings or the vendored CLI). An older baseline can accept more debt (H-12): approve only if you asked for it.",
        ),
    ),
    (
        "git restore --source=HEAD~3 .claude/temper-baseline.json",
        Ask(
            "temper: `git restore --source=HEAD~3 .claude/temper-baseline.json` puts older verification configuration back into .claude/ (the lint baseline, settings or the vendored CLI). An older baseline can accept more debt (H-12): approve only if you asked for it.",
        ),
    ),
    (
        "git stash && git checkout old-branch -- .claude",
        Ask(
            "temper: `git checkout old-branch -- .claude` puts older verification configuration back into .claude/ (the lint baseline, settings or the vendored CLI). An older baseline can accept more debt (H-12): approve only if you asked for it.",
        ),
    ),
    (
        "git restore --source=main -- ./.claude/settings.json",
        Ask(
            "temper: `git restore --source=main -- ./.claude/settings.json` puts older verification configuration back into .claude/ (the lint baseline, settings or the vendored CLI). An older baseline can accept more debt (H-12): approve only if you asked for it.",
        ),
    ),
    (
        "git show HEAD~2:.claude/settings.json > .claude/settings.json",
        Ask(
            "temper: `git show HEAD~2:.claude/settings.json > .claude/settings.json` puts older verification configuration back into .claude/ (the lint baseline, settings or the vendored CLI). An older baseline can accept more debt (H-12): approve only if you asked for it.",
        ),
    ),
    ("git checkout -b feature", PASS),
    ("git stash", PASS),
    ("git restore src/lib.rs", PASS),
    ("git show HEAD:Cargo.lock > /tmp/old.lock", PASS),
    ("git checkout origin/main -- Cargo.lock", PASS),
    ("git checkout -- .claude/temper-baseline.json", PASS),
    ("git checkout HEAD -- .claude/temper-baseline.json", PASS),
    ("git restore .claude/temper-baseline.json", PASS),
    ("git restore --staged .claude/temper-baseline.json", PASS),
];

/// 1.x "Bash: git checkout/restore/stash of a .claude path, and git show into .claude/, ask (H-12)".
#[test]
fn git_checkout_restore_stash_of_a_claude_path_and_git_show_into_claude_ask_h_12() {
    let ws = Ws::new();
    ws.answers(CLAUDE_RESTORES);
}

const BASELINE_WRITES: &[(&str, Answer<'static>)] = &[
    ("echo {} > .claude/temper-baseline.json", Deny(BASELINE)),
    (
        "sed -i 's/: 3/: 4/' .claude/temper-baseline.json",
        Deny(BASELINE),
    ),
    (
        "jq . x.json | tee .claude/temper-baseline.json",
        Deny(BASELINE),
    ),
    (
        "cp /tmp/old.json .claude/temper-baseline.json",
        Deny(BASELINE),
    ),
    ("mv new.json ./.claude/temper-baseline.json", Deny(BASELINE)),
    (
        "python3 -c \"open('.claude/temper-baseline.json','w').write('{}')\"",
        Deny(BASELINE),
    ),
    ("cat .claude/temper-baseline.json", PASS),
    ("git diff .claude/temper-baseline.json", PASS),
    ("cp .claude/temper-baseline.json /tmp/bl.json", PASS),
    (
        "jq .totals .claude/temper-baseline.json > /tmp/t.json",
        PASS,
    ),
];

/// 1.x "Bash: shell writes to the baseline file are denied; Edit/Write to it asks (protected .claude/**)".
#[test]
fn shell_writes_to_the_baseline_file_are_denied_edit_write_to_it_asks_protected_claude() {
    let ws = Ws::new();
    ws.answers(BASELINE_WRITES);
    let write = ws.write_to(
        ".claude/temper-baseline.json",
        "{\"temper_baseline\":1,\"totals\":{}}\n",
    );
    let HookOutcome::Ask { reason, .. } = &write else {
        panic!("Edit/Write to the baseline file asks: {write:?}");
    };
    assert!(
        reason.contains(".claude/temper-baseline.json is verification/agent configuration"),
        "{reason}"
    );
}

const VENDORED_WRITES: &[(&str, Answer<'static>)] = &[
    (
        "sed -i \"s/unwrap_used/x/\" .claude/temper/lib/baseline.mjs",
        Deny(VENDORED),
    ),
    ("echo x > .claude/temper/temper.mjs", Deny(VENDORED)),
    (
        "cp /tmp/evil.mjs .claude/temper/lib/baseline.mjs",
        Deny(VENDORED),
    ),
    ("tee .claude/temper/VERSION < x", Deny(VENDORED)),
    (
        "perl -pi -e 's/a/b/' .claude/temper/lib/gate.mjs",
        Deny(VENDORED),
    ),
    ("cp .claude/temper/temper.mjs /tmp/x", PASS),
    ("node .claude/temper/temper.mjs baseline --check", PASS),
    ("cat .claude/temper/VERSION", PASS),
    ("diff -r .claude/temper scripts", PASS),
];

/// 1.x "Bash: shell writes to the vendored CLI (.claude/temper/) are denied; reading and running it are not".
#[test]
fn shell_writes_to_the_vendored_cli_claude_temper_are_denied_reading_and_running_it_are_not() {
    let ws = Ws::new();
    ws.answers(VENDORED_WRITES);
}

/// 1.x "Bash: the flag checks stay fast on long commands": the shapes that would backtrack
/// without the rewritten patterns all pass. The wall-clock bound is retired (OC-4 A,
/// wi-3ea323): the hook parity harness bounds each Rust step instead.
#[test]
fn the_flag_checks_stay_fast_on_long_commands() {
    let ws = Ws::new();
    let shapes = [
        "cargo clippy ".repeat(800),
        format!("{}{}", "cargo ".repeat(400), "clippy ".repeat(430)),
        "git commit ".repeat(800),
        format!("git {}", "--a=b ".repeat(400)),
        format!("git {}", "--a=\"b c\" ".repeat(400)),
        format!("git {}", "-c ".repeat(400)),
    ];
    for cmd in &shapes {
        assert_eq!(
            answer(&ws.bash(cmd)),
            PASS,
            "{}",
            cmd.get(..40).unwrap_or(cmd)
        );
    }
}
