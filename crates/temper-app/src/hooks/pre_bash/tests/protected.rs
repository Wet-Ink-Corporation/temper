//! `tests/hooks.test.mjs`'s protected-configuration Bash cases that need no git, one test
//! each, in its order. `protected_cases.json` holds every `preBash` call those tests make,
//! with the answer 1.x gave (captured by running the 1.x test file with `preBash` wrapped, its
//! temporary directories renamed to `/ws`, `/own`, `/tmp/cfg-patch` and `/tmp/cfg-tree`).
//! The workspace holds what the 1.x test's holds at that point; git answers as outside a
//! repository. The cases with git (nested checkouts, restores from a revision) are in
//! `temper-cli`'s `tests/it/hooks.rs`.
//!
//! A row whose write is interpreter code (`python3 -c`, `node -e`, a Python here-document,
//! `[IO.File]::WriteAllText`) or that 1.x's `.rs` rule denies first arrives with port Phase
//! 2f: such a row carries `until_2f`, the answer this build gives, which 2f removes.

use temper_domain::{HookEvent, HookOutcome, JsValue, PathStyle::Posix};

use super::{Ws, files};
use crate::fakes::{FakeFiles, FakeProcess, Reply};
use crate::ports::ExitKind;

const CASES: &str = include_str!("protected_cases.json");

const CFG_PATCH: &str = "diff --git a/.cargo/config.toml b/.cargo/config.toml\n--- a/.cargo/config.toml\n+++ b/.cargo/config.toml\n@@ -1 +1,2 @@\n+[build]\n";
const CLIPPY_PATCH: &str = "--- a/clippy.toml\n+++ b/clippy.toml\n@@ -1 +1 @@\n-a\n+b\n";
const CFG_DOC_PATCH: &str = "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old\n+new\n";

/// `hooks.test.mjs`'s workspace with the files its later tests add, and the patch and tree
/// fixtures they write outside it.
fn hooks_ws() -> FakeFiles {
    files()
        .with_file("/ws/Cargo.lock", "version = 4\n")
        .with_file(
            "/ws/crates/profiled/Cargo.toml",
            "[package]\nname = \"profiled\"\n",
        )
        .with_file("/ws/crates/profiled/src/lib.rs", "//! Profiled.\n")
        .with_file(
            "/ws/crates/legacy/Cargo.toml",
            "[package]\nname = \"legacy\"\n",
        )
        .with_file("/ws/crates/legacy/src/lib.rs", "//! Legacy.\n")
        .with_file("/ws/cfgdel/sub/clippy.toml", "x\n")
        .with_file("/tmp/cfg-patch/doc.diff", CFG_DOC_PATCH)
        .with_file("/tmp/cfg-patch/cfg.patch", CFG_PATCH)
        .with_file("/tmp/cfg-patch/clippy.patch", CLIPPY_PATCH)
        .with_file("/tmp/cfg-tree/cfg/sub/clippy.toml", "a = 1\n")
        .with_file("/tmp/cfg-tree/cfg/notes.md", "x\n")
        .with_file("/tmp/cfg-tree/plain/notes.md", "x\n")
}

/// "the shell check uses the project's own protected list"'s workspace.
fn own_ws() -> FakeFiles {
    FakeFiles::new(Posix)
        .with_file("/own/Cargo.toml", "[workspace]\nresolver = \"3\"\n")
        .with_file(
            "/own/.claude/temper.json",
            r#"{"protected":["docs/policy/**"]}"#,
        )
}

/// `files` where git runs and answers as outside a repository (exit 128).
fn without_repository(files: FakeFiles) -> Ws {
    let mut ws = Ws::with(files);
    ws.ports.process = FakeProcess::new().otherwise(Reply::Exit {
        status: ExitKind::Code(128),
        stdout: String::new(),
    });
    ws
}

/// A field of a case, as a string.
fn text<'a>(case: &'a JsValue, key: &str) -> &'a str {
    case.get(key).and_then(JsValue::as_str).unwrap_or_default()
}

/// The answer as the capture writes it: `none`, or `ask`/`deny` with the reason.
fn answer(outcome: &HookOutcome) -> (&'static str, &str) {
    match outcome {
        HookOutcome::Pass => ("none", ""),
        HookOutcome::Ask { reason, .. } => ("ask", reason),
        HookOutcome::Deny { reason, .. } => ("deny", reason),
        _ => ("context", ""),
    }
}

/// Every captured call of 1.x's test `test` gets 1.x's answer, or its `until_2f` one.
fn replays(test: &str, ws: &Ws) {
    let groups = JsValue::from_json(CASES).expect("the cases are JSON");
    let JsValue::Array(groups) = groups else {
        panic!("the cases are a list");
    };
    let group = groups
        .iter()
        .find(|g| text(g, "test") == test)
        .expect("the test has captured cases");
    let Some(JsValue::Array(cases)) = group.get("cases") else {
        panic!("{test} has cases");
    };
    assert!(!cases.is_empty(), "{test}");
    let mut wrong = Vec::new();
    for case in cases {
        let (cwd, cmd) = (text(case, "cwd"), text(case, "cmd"));
        let json = format!(
            r#"{{"tool_name":"Bash","cwd":{},"session_id":"hooks-test","tool_input":{{"command":{}}}}}"#,
            JsValue::String(cwd.to_owned()).to_json(),
            JsValue::String(cmd.to_owned()).to_json()
        );
        let got = ws.run(HookEvent::PreBash, &json);
        let want = case.get("until_2f").unwrap_or(case);
        let (decision, reason) = answer(&got);
        if (decision, reason) != (text(want, "decision"), text(want, "reason")) {
            wrong.push(format!("{cmd:?} in {cwd}: {decision} {reason:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{test}: 1.x answers differently:\n{}",
        wrong.join("\n")
    );
}

/// The rows 1.x logs: the protected ask's first, then the dependency ask's.
#[test]
fn the_protected_ask_logs_its_row_before_the_dependency_ask_row() {
    let ws = without_repository(hooks_ws());
    for cmd in ["echo x > clippy.toml; cargo add serde_yaml", "rm deny.toml"] {
        let json = format!(
            r#"{{"tool_name":"Bash","cwd":"/ws","session_id":"s","tool_input":{{"command":{}}}}}"#,
            JsValue::String(cmd.to_owned()).to_json()
        );
        ws.run(HookEvent::PreBash, &json);
    }
    let log = ws
        .ports
        .files
        .text("/ws/target/temper/telemetry.jsonl")
        .expect("the asks are logged");
    let rows: Vec<&str> = log
        .lines()
        .map(|row| {
            row.split_once(r#""agent":"main","#)
                .map_or(row, |(_, rest)| rest)
        })
        .collect();
    assert_eq!(
        rows,
        [
            r#""event":"ask","via":"shell-write-protected","file":"clippy.toml"}"#,
            r#""event":"ask","via":"cargo-deps"}"#,
            r#""event":"ask","via":"shell-delete-protected","file":"deny.toml"}"#,
        ]
    );
}

#[test]
fn shell_writes_to_protected_configuration_ask_like_edit_write_to_the_same_file_h_05_h_12() {
    replays(
        "shell_writes_to_protected_configuration_ask_like_edit_write_to_the_same_file_h_05_h_12",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn reading_copying_out_of_and_running_protected_configuration_stays_allowed() {
    replays(
        "reading_copying_out_of_and_running_protected_configuration_stays_allowed",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn every_existing_deny_still_wins_over_the_protected_config_ask() {
    replays(
        "every_existing_deny_still_wins_over_the_protected_config_ask",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn the_shell_check_uses_the_projects_own_protected_list() {
    replays(
        "the_shell_check_uses_the_projects_own_protected_list",
        &without_repository(own_ws()),
    );
}

#[test]
fn text_that_only_mentions_a_write_read_only_one_liners_and_writes_outside_the_project_stay_allowed()
 {
    replays(
        "text_that_only_mentions_a_write_read_only_one_liners_and_writes_outside_the_project_stay_allowed",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn protected_config_writes_behind_cd_subshells_wrappers_interpreters_and_other_write_verbs_ask() {
    replays(
        "protected_config_writes_behind_cd_subshells_wrappers_interpreters_and_other_write_verbs_ask",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn protected_config_writes_inside_if_for_while_brace_groups_ask_and_a_cd_inside_them_is_followed() {
    replays(
        "protected_config_writes_inside_if_for_while_brace_groups_ask_and_a_cd_inside_them_is_followed",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn a_whole_directory_written_over_protected_configuration_asks_restoring_from_head_or_the_index_does_not()
 {
    replays(
        "a_whole_directory_written_over_protected_configuration_asks_restoring_from_head_or_the_index_does_not",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn deleting_protected_configuration_from_the_shell_asks_like_a_write_to_it_wi_2000a3() {
    replays(
        "deleting_protected_configuration_from_the_shell_asks_like_a_write_to_it_wi_2000a3",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn a_heredoc_printed_after_a_test_is_not_code_powershell_expressions_still_are() {
    replays(
        "a_heredoc_printed_after_a_test_is_not_code_powershell_expressions_still_are",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn a_patch_that_changes_protected_configuration_asks_other_patches_and_inspecting_forms_do_not() {
    replays(
        "a_patch_that_changes_protected_configuration_asks_other_patches_and_inspecting_forms_do_not",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn a_patch_temper_cannot_read_asks_inspecting_forms_do_not() {
    replays(
        "a_patch_temper_cannot_read_asks_inspecting_forms_do_not",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn a_command_the_shell_reader_cannot_finish_still_gets_the_other_bash_denies() {
    replays(
        "a_command_the_shell_reader_cannot_finish_still_gets_the_other_bash_denies",
        &without_repository(hooks_ws()),
    );
}

#[test]
fn protected_config_written_by_a_clustered_curl_wget_output_option_or_by_a_recursive_copy_of_protected_files_asks()
 {
    replays(
        "protected_config_written_by_a_clustered_curl_wget_output_option_or_by_a_recursive_copy_of_protected_files_asks",
        &without_repository(hooks_ws()),
    );
}

/// Not in 1.x's tests: a restore git is asked about once per line, however often the line
/// repeats it (#201 F4: two git calls per restore, 9.2 s for 2,000 of them).
#[test]
fn a_restore_repeated_on_one_line_asks_git_about_it_once() {
    let ran = |stdout: &str| Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    };
    let diff = [
        "git",
        "diff",
        "--name-only",
        "--no-renames",
        "-z",
        "v1",
        "--",
        "*.toml",
    ];
    let calls = |n: usize| {
        let mut ws = Ws::with(hooks_ws());
        ws.ports.process = FakeProcess::new()
            .answering(
                &["git", "rev-parse", "--show-toplevel"],
                "/ws",
                ran("/ws\n"),
            )
            .answering(&diff, "/ws", ran("clippy.toml\0"))
            .otherwise(Reply::Exit {
                status: ExitKind::Code(128),
                stdout: String::new(),
            });
        let cmd = "git checkout v1 -- '*.toml'; ".repeat(n);
        let json = format!(
            r#"{{"tool_name":"Bash","cwd":"/ws","session_id":"s","tool_input":{{"command":{}}}}}"#,
            JsValue::String(cmd).to_json()
        );
        let got = ws.run(HookEvent::PreBash, &json);
        assert_eq!(answer(&got).0, "ask", "{n} restores");
        ws.ports.process.calls()
    };
    assert_eq!(calls(3), calls(1));
}

/// Not in 1.x's tests (#202 re-review m2): the owner of the names in a directory is looked up
/// once per directory, and a directory whose parent's names belong to the project may still
/// hold a checkout of its own. Answers probed from 1.x on disk, with git.
#[test]
fn a_nested_checkout_under_a_directory_shared_by_many_names_owns_only_the_names_below_it() {
    let files = files()
        .with_file("/ws/a/sub/.git/HEAD", "ref: refs/heads/main\n")
        .with_file("/ws/a/sub/Cargo.toml", "[workspace]\nresolver = \"3\"\n")
        .with_file(
            "/ws/a/sub/.claude/temper.json",
            r#"{"protected":["docs/policy/**"]}"#,
        );
    let mut ws = Ws::with(files);
    ws.ports.process = FakeProcess::new()
        .answering(
            &["git", "rev-parse", "--show-toplevel"],
            "/ws/a/sub",
            Reply::Exit {
                status: ExitKind::Code(0),
                stdout: "/ws/a/sub\n".to_owned(),
            },
        )
        .otherwise(Reply::Exit {
            status: ExitKind::Code(128),
            stdout: String::new(),
        });
    let ask = |names: &str| {
        format!(
            "temper: this shell command writes {names}. {names} are verification/agent configuration. Changing them can weaken the checks (H-05, H-12). Approve only if you asked for this change."
        )
    };
    let cases = [
        (
            "> a/o; > a/p; > a/sub/clippy.toml; > a/sub/docs/policy/x; > a/q/clippy.toml",
            ask("a/sub/docs/policy/x, a/q/clippy.toml"),
        ),
        (
            "cd a; > o; > sub/docs/policy/y; cd sub; > clippy.toml; > docs/policy/z",
            ask("a/sub/docs/policy/y, a/sub/docs/policy/z"),
        ),
    ];
    for (cmd, want) in cases {
        let json = format!(
            r#"{{"tool_name":"Bash","cwd":"/ws","session_id":"s","tool_input":{{"command":{}}}}}"#,
            JsValue::String(cmd.to_owned()).to_json()
        );
        let got = ws.run(HookEvent::PreBash, &json);
        assert_eq!(answer(&got), ("ask", want.as_str()), "{cmd}");
    }
}

#[test]
fn a_protected_config_ask_does_not_hide_the_dependency_or_snapshot_ask_in_the_same_command() {
    replays(
        "a_protected_config_ask_does_not_hide_the_dependency_or_snapshot_ask_in_the_same_command",
        &without_repository(hooks_ws()),
    );
}
