//! `temper hook <event>`: the protocol, `post-tool`, `user-prompt` and `pre-edit` against the
//! real file system and git, and `hooks/hooks.json`.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use temper_domain::JsValue;

use crate::support::{TestResult, git, git_init, hook_in, write_tree};

const WORKSPACE: &str = "[workspace]\nmembers = [\"crates/*\"]\n";
const NUDGE: &str = "{\"hookSpecificOutput\":{\"hookEventName\":\"UserPromptSubmit\",\"additionalContext\":\"temper: this is a Rust workspace and the rust-craft standard (skill `temper:rust-craft`) is not loaded in this session yet; Rust edits are refused until it is. Planning Rust work benefits from loading it first.\"}}";
const SKILL: &str =
    r#"{"tool_name":"Skill","session_id":"s1","tool_input":{"skill":"temper:rust-craft"}}"#;

/// A temporary workspace without git.
fn workspace() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    write_tree(dir.path(), &[("Cargo.toml", WORKSPACE)])?;
    Ok(dir)
}

fn data(dir: &Path) -> [(&'static str, &OsStr); 1] {
    [("CLAUDE_PLUGIN_DATA", dir.as_os_str())]
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn an_unknown_or_missing_event_is_reported_on_stderr_and_exits_0() -> TestResult {
    let dir = tempfile::tempdir()?;
    let runs = [
        hook_in(dir.path(), "bogus", "{}", &[])?,
        crate::support::temper_in(dir.path(), &["hook"])?,
    ];

    let got: Vec<_> = runs
        .iter()
        .map(|r| (r.code, r.stdout.as_str(), r.stderr.as_str()))
        .collect();

    assert_eq!(
        got,
        [
            (Some(0), "", "temper: unknown hook event bogus\n"),
            (Some(0), "", "temper: unknown hook event undefined\n"),
        ],
        "scripts/temper.mjs hook()"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn input_that_is_not_json_says_nothing_was_checked_unless_disabled() -> TestResult {
    let dir = workspace()?;
    let notice = "{\"systemMessage\":\"temper hook user-prompt: could not parse the hook input as JSON; nothing was checked\"}";
    let off = [("TEMPER_DISABLE", OsStr::new("1"))];

    let runs = [
        hook_in(dir.path(), "user-prompt", "{\"tool_name\": \"Write\",", &[])?,
        hook_in(dir.path(), "user-prompt", " \n", &[])?,
        hook_in(dir.path(), "user-prompt", "\u{feff}{}", &[])?,
        hook_in(dir.path(), "user-prompt", "not json", &off)?,
        hook_in(dir.path(), "user-prompt", "{}", &off)?,
    ];

    let got: Vec<_> = runs.iter().map(|r| (r.code, r.stdout.as_str())).collect();
    assert_eq!(
        got,
        [
            (Some(0), notice),
            (Some(0), notice),
            (Some(0), notice),
            (Some(0), ""),
            (Some(0), ""),
        ],
        "the notice, or silence under TEMPER_DISABLE=1"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn empty_and_non_object_input_read_as_an_empty_object() -> TestResult {
    let dir = workspace()?;
    let plugin = tempfile::tempdir()?;

    let runs = ["", "5", "[1]", "\"s\""]
        .map(|stdin| hook_in(dir.path(), "user-prompt", stdin, &data(plugin.path())));

    let got: Vec<_> = runs
        .into_iter()
        .map(|run| run.map(|r| (r.code, r.stdout)).map_err(|e| e.to_string()))
        .collect();
    assert_eq!(
        got,
        [(); 4].map(|()| Ok((Some(0), NUDGE.to_owned()))),
        "`input.session_id` and `input.cwd` read as undefined"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn null_input_fails_the_handler_with_v8_s_message() -> TestResult {
    let dir = workspace()?;

    let runs = [
        hook_in(dir.path(), "post-tool", "null", &[])?,
        hook_in(dir.path(), "user-prompt", "null", &[])?,
    ];

    assert_eq!(
        runs.map(|r| (r.code, r.stdout)),
        [
            (
                Some(0),
                "{\"systemMessage\":\"temper hook post-tool failed: Cannot read properties of null (reading 'tool_input')\"}".to_owned()
            ),
            (
                Some(0),
                "{\"systemMessage\":\"temper hook user-prompt failed: Cannot read properties of null (reading 'session_id')\"}".to_owned()
            ),
        ],
        "port plan 2-22"
    );
    Ok(())
}

/// The first row of `log`, with its time cut out, and the time.
fn row_without_time(log: &Path) -> Result<(String, String), Box<dyn std::error::Error>> {
    let text = fs::read_to_string(log)?;
    let row = text.strip_suffix('\n').ok_or("a row ends with a newline")?;
    let rest = row.strip_prefix("{\"t\":\"").ok_or("t comes first")?;
    let (at, rest) = rest.split_once('"').ok_or("t is a string")?;
    Ok((format!("{{\"t\":\"<T>\"{rest}"), at.to_owned()))
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn post_tool_marks_the_skill_and_user_prompt_then_says_nothing() -> TestResult {
    let dir = workspace()?;
    let plugin = tempfile::tempdir()?;
    let prompt = r#"{"session_id":"s1"}"#;

    let before = hook_in(dir.path(), "user-prompt", prompt, &data(plugin.path()))?;
    let loaded = hook_in(dir.path(), "post-tool", SKILL, &data(plugin.path()))?;
    let after = hook_in(dir.path(), "user-prompt", prompt, &data(plugin.path()))?;

    assert_eq!(
        [
            before.stdout.as_str(),
            loaded.stdout.as_str(),
            after.stdout.as_str()
        ],
        [NUDGE, "", ""],
        "the nudge until the skill is loaded"
    );
    let marker = fs::read_to_string(plugin.path().join("sessions").join("s1").join("skill-main"))?;
    let (row, at) = row_without_time(
        &dir.path()
            .join("target")
            .join("temper")
            .join("telemetry.jsonl"),
    )?;
    assert_eq!(
        row, r#"{"t":"<T>","sid":"s1","agent":"main","event":"skill-loaded","via":"Skill"}"#,
        "the telemetry row, in the state dir of a workspace without git"
    );
    assert!(
        marker.len() == 24 && marker.ends_with('Z') && at.len() == 24,
        "ISO times: {marker}, {at}"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_linked_worktree_logs_to_the_shared_git_dir() -> TestResult {
    let root = tempfile::tempdir()?;
    let main = root.path().join("main");
    write_tree(&main, &[("Cargo.toml", WORKSPACE)])?;
    git_init(&main)?;
    git(&main, &["add", "-A"])?;
    git(&main, &["commit", "-qm", "init"])?;
    git(&main, &["worktree", "add", "-q", "../wt"])?;
    let plugin = tempfile::tempdir()?;

    let run = hook_in(
        &root.path().join("wt"),
        "post-tool",
        SKILL,
        &data(plugin.path()),
    )?;

    assert_eq!((run.code, run.stdout.as_str()), (Some(0), ""), "{run:?}");
    let shared = main.join(".git").join("temper").join("telemetry.jsonl");
    assert!(shared.is_file(), "the row is in {}", shared.display());
    assert!(
        !root.path().join("wt").join("target").exists(),
        "and not in the worktree"
    );
    Ok(())
}

/// The decision `temper hook pre-bash` gives `cmd` in `dir`: `deny`, `ask`, or `""` for none.
fn bash_decision(dir: &Path, cmd: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(bash_answer(dir, cmd)?.0)
}

/// The decision and the reason `temper hook pre-bash` gives `cmd` in `dir`, each `""` when it
/// answers nothing.
fn bash_answer(dir: &Path, cmd: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let stdin = format!(
        r#"{{"tool_name":"Bash","session_id":"s1","tool_input":{{"command":{}}}}}"#,
        JsValue::String(cmd.to_owned()).to_json()
    );
    let run = hook_in(dir, "pre-bash", &stdin, &[])?;
    if run.stdout.is_empty() {
        return Ok((String::new(), String::new()));
    }
    let out = JsValue::from_json(&run.stdout)?;
    let field = |key: &str| {
        out.get("hookSpecificOutput")
            .and_then(|o| o.get(key))
            .and_then(JsValue::as_str)
            .map(str::to_owned)
    };
    let decision =
        field("permissionDecision").ok_or_else(|| format!("no decision in {}", run.stdout))?;
    Ok((
        decision,
        field("permissionDecisionReason").unwrap_or_default(),
    ))
}

/// A committed repository at `repo` with `files`, tagged `v1`.
fn tagged_repo(repo: &Path, files: &[(&str, &str)]) -> TestResult {
    write_tree(repo, files)?;
    git_init(repo)?;
    git(repo, &["add", "."])?;
    git(repo, &["commit", "-qm", "init"])?;
    git(repo, &["tag", "v1"])
}

/// 1.x "Bash: a write into a nested git checkout (.claude/worktrees/<name>) is judged by that
/// checkout": the outer project's `.claude/**` does not cover the checkout's files, and the
/// checkout's own protected files still ask.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_write_into_a_nested_git_checkout_is_judged_by_that_checkout() -> TestResult {
    let root = tempfile::tempdir()?;
    let repo = root.path();
    tagged_repo(repo, &[("Cargo.toml", WORKSPACE), ("README.md", "x\n")])?;
    git(
        repo,
        &["worktree", "add", "-q", "--detach", ".claude/worktrees/w1"],
    )?;
    let w1 = repo.join(".claude").join("worktrees").join("w1");
    let w1f = w1.to_string_lossy().replace('\\', "/");

    for cmd in [
        "cd .claude/worktrees/w1 && cargo test 2>&1 | tee target/log.txt".to_owned(),
        "cd .claude/worktrees/w1 && echo x > README.md".to_owned(),
        format!(r#"cargo test --manifest-path "{w1f}/Cargo.toml" > "{w1f}/target/log.txt""#),
    ] {
        assert_eq!(bash_answer(repo, &cmd)?.0, "", "{cmd}");
    }
    let (decision, reason) = bash_answer(repo, "cd .claude/worktrees/w1 && echo x > clippy.toml")?;
    assert_eq!(decision, "ask");
    assert!(
        reason.contains(".claude/worktrees/w1/clippy.toml is verification"),
        "{reason}"
    );
    for cmd in [
        "echo {} > .claude/temper.json",
        "echo {} > .claude/worktrees/w1/.claude/temper.json",
    ] {
        assert_eq!(bash_answer(repo, cmd)?.0, "ask", "{cmd}");
    }
    Ok(())
}

/// 1.x "Bash: git checkout/restore of a wildcard pathspec from another revision asks when it
/// would change protected configuration": git lists the files.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_wildcard_pathspec_restored_from_another_revision_asks_when_git_lists_protected_files()
-> TestResult {
    let root = tempfile::tempdir()?;
    let repo = root.path();
    let files = [
        ("Cargo.toml", WORKSPACE),
        ("clippy.toml", "a = 1\n"),
        ("docs/x.toml", "a = 1\n"),
        ("README.md", "x\n"),
    ];
    tagged_repo(repo, &files)?;
    write_tree(
        repo,
        &[
            ("clippy.toml", "a = 2\n"),
            ("docs/x.toml", "a = 2\n"),
            ("README.md", "y\n"),
        ],
    )?;

    for cmd in [
        "git checkout v1 -- '*.toml'",
        "git restore -s v1 -- '*.toml'",
        r"git checkout v1 -- \*.toml",
        "git restore --source=v1 ':(glob)**/*.toml'",
    ] {
        let (decision, reason) = bash_answer(repo, cmd)?;
        assert_eq!(decision, "ask", "{cmd}");
        assert!(reason.contains("writes clippy.toml"), "{cmd}: {reason}");
    }
    for cmd in [
        "git checkout v1 -- '*.md'",
        "git restore -s v1 -- 'docs/*.toml'",
        "git checkout -- '*.toml'",
        "git restore '*.toml'",
        "git checkout HEAD -- '*.toml'",
        "cd docs && git checkout v1 -- '*.toml'",
    ] {
        assert_eq!(
            bash_answer(repo, cmd)?,
            (String::new(), String::new()),
            "{cmd}"
        );
    }
    Ok(())
}

/// 1.x "Bash: git checkout/restore of a directory from another revision asks when it would
/// change a protected file inside it".
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_directory_restored_from_another_revision_asks_when_it_would_change_a_protected_file_in_it()
-> TestResult {
    let root = tempfile::tempdir()?;
    let repo = root.path();
    let files = [
        ("Cargo.toml", WORKSPACE),
        ("docs/clippy.toml", "a = 1\n"),
        ("notes/a.md", "x\n"),
    ];
    tagged_repo(repo, &files)?;
    write_tree(
        repo,
        &[("docs/clippy.toml", "a = 2\n"), ("notes/a.md", "y\n")],
    )?;

    for cmd in [
        "git checkout v1 -- docs",
        "git restore -s v1 docs",
        "cd docs && git checkout v1 -- .",
    ] {
        let (decision, reason) = bash_answer(repo, cmd)?;
        assert_eq!(decision, "ask", "{cmd}");
        assert!(reason.contains("docs/clippy.toml"), "{cmd}: {reason}");
    }
    for cmd in [
        "git checkout v1 -- notes",
        "git checkout -- docs",
        "git restore docs",
        "git checkout HEAD -- docs",
    ] {
        assert_eq!(
            bash_answer(repo, cmd)?,
            (String::new(), String::new()),
            "{cmd}"
        );
    }
    Ok(())
}

/// 1.x "Bash: shell writes into the absolute state dir of a git worktree are denied": a linked
/// worktree's state dir is in the main repository's git dir, outside the worktree. Its
/// `python3 -c "open(...)"` row arrives with interpreter code in port Phase 2f.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn shell_writes_into_the_absolute_state_dir_of_a_git_worktree_are_denied() -> TestResult {
    let root = tempfile::tempdir()?;
    let main = root.path().join("main");
    write_tree(&main, &[("Cargo.toml", WORKSPACE)])?;
    git_init(&main)?;
    git(&main, &["add", "-A"])?;
    git(&main, &["commit", "-qm", "init"])?;
    git(&main, &["worktree", "add", "-q", "../wt"])?;
    let wt = root.path().join("wt");
    let common = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(&wt)
        .output()?;
    // git prints `/` separators on every OS.
    let slashed = format!("{}/temper", String::from_utf8(common.stdout)?.trim_end());
    let native = slashed.replace('/', std::path::MAIN_SEPARATOR_STR);
    assert!(
        !slashed.contains("/wt/"),
        "the state dir {slashed} is in the main repository"
    );

    for cmd in [
        format!(
            r#"echo {{}} > "{native}{}verdicts{}a.json""#,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        ),
        format!("cp /tmp/v.json {slashed}/gate/a.json"),
        r#"echo {} > "$(git rev-parse --git-common-dir)/temper/verdicts/a.json""#.to_owned(),
    ] {
        assert_eq!(bash_decision(&wt, &cmd)?, "deny", "{cmd}");
    }
    for cmd in [
        format!(
            r#"cat "{native}{}telemetry.jsonl""#,
            std::path::MAIN_SEPARATOR
        ),
        format!("cp {slashed}/logs/a.log /tmp/"),
        format!("rm {slashed}/telemetry.jsonl"),
        "git worktree list".to_owned(),
    ] {
        assert_eq!(bash_decision(&wt, &cmd)?, "", "{cmd}");
    }
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_marker_that_cannot_be_written_is_reported_and_exits_0() -> TestResult {
    let dir = workspace()?;
    let blocker = dir.path().join("Cargo.toml");

    let run = hook_in(dir.path(), "post-tool", SKILL, &data(&blocker))?;

    assert_eq!(run.code, Some(0), "never blocks");
    assert!(
        run.stdout.starts_with(
            "{\"systemMessage\":\"temper hook post-tool failed: cannot create directory "
        ),
        "{}",
        run.stdout
    );
    Ok(())
}

/// Ported from `messages.test.mjs`.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_hook_given_malformed_input_says_nothing_was_checked_and_still_exits_0_with_no_decision()
-> TestResult {
    let dir = workspace()?;
    let off = [("TEMPER_DISABLE", OsStr::new("1"))];

    let malformed = hook_in(dir.path(), "pre-edit", "{\"tool_name\": \"Write\",", &[])?;
    let empty = hook_in(dir.path(), "pre-edit", "", &[])?;
    let disabled = hook_in(dir.path(), "pre-edit", "not json", &off)?;

    assert_eq!(
        (malformed.code, malformed.stdout.as_str()),
        (
            Some(0),
            "{\"systemMessage\":\"temper hook pre-edit: could not parse the hook input as JSON; nothing was checked\"}"
        ),
        "the notice and no decision"
    );
    assert_eq!(
        [empty, disabled].map(|r| (r.code, r.stdout)),
        [(Some(0), String::new()), (Some(0), String::new())],
        "empty input reads as {{}}, and TEMPER_DISABLE=1 stays silent"
    );
    Ok(())
}

/// The Iron rules' IDs in `skills/rust-craft/SKILL.md`'s table, in order.
fn iron_rule_ids() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/rust-craft/SKILL.md");
    Ok(fs::read_to_string(path)?
        .lines()
        .filter_map(|line| line.strip_prefix("| H-"))
        .filter_map(|rest| rest.split_once(' '))
        .map(|(number, _)| format!("H-{number}"))
        .collect())
}

/// Ported from `messages.test.mjs`; the IDs come from the standard itself.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn the_skill_gate_denial_lists_every_iron_rule_h_01_to_h_16() -> TestResult {
    let dir = workspace()?;
    write_tree(
        dir.path(),
        &[("crates/a/Cargo.toml", "[package]\nname = \"a\"\n")],
    )?;
    let plugin = tempfile::tempdir()?;
    let file = dir
        .path()
        .join("crates")
        .join("a")
        .join("src")
        .join("lib.rs");
    let stdin = format!(
        r#"{{"tool_name":"Write","session_id":"messages","tool_input":{{"file_path":{},"content":"//! A.\n"}}}}"#,
        JsValue::String(file.to_string_lossy().into_owned()).to_json()
    );

    let run = hook_in(dir.path(), "pre-edit", &stdin, &data(plugin.path()))?;

    let out = JsValue::from_json(&run.stdout)?;
    let specific = out.get("hookSpecificOutput").ok_or(run.stdout.as_str())?;
    assert_eq!(
        specific.get("permissionDecision").and_then(JsValue::as_str),
        Some("deny")
    );
    let reason = specific
        .get("permissionDecisionReason")
        .and_then(JsValue::as_str)
        .ok_or("a reason")?;
    assert!(
        reason.contains("the rust-craft standard is not loaded"),
        "{reason}"
    );
    let ids = iron_rule_ids()?;
    assert_eq!(ids.len(), 16, "{ids:?}");
    let missing: Vec<&String> = ids
        .iter()
        .filter(|id| !reason.contains(&format!("{id} ")))
        .collect();
    assert!(missing.is_empty(), "{missing:?} in {reason}");
    for rule in temper_domain::IRON_RULES {
        assert!(reason.contains(rule), "{rule}");
    }
    Ok(())
}

/// Ported from `hooks.test.mjs`: the project is found from a member's directory without git.
/// Its root is the workspace (the lockfile row names `Cargo.lock`), and with no git directory
/// its state dir is `target/temper`.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn the_throwaway_workspace_is_discovered_without_git() -> TestResult {
    let dir = workspace()?;
    write_tree(
        dir.path(),
        &[
            ("crates/a/Cargo.toml", "[package]\nname = \"a\"\n"),
            ("Cargo.lock", "version = 4\n"),
        ],
    )?;
    let lock = dir.path().join("Cargo.lock");
    let stdin = format!(
        r#"{{"tool_name":"Write","session_id":"s1","tool_input":{{"file_path":{},"content":"version = 3\n"}}}}"#,
        JsValue::String(lock.to_string_lossy().into_owned()).to_json()
    );

    let run = hook_in(
        &dir.path().join("crates").join("a"),
        "pre-edit",
        &stdin,
        &[],
    )?;

    let out = JsValue::from_json(&run.stdout)?;
    assert_eq!(
        out.get("hookSpecificOutput")
            .and_then(|o| o.get("permissionDecision"))
            .and_then(JsValue::as_str),
        Some("deny"),
        "{}",
        run.stdout
    );
    let (row, _) = row_without_time(
        &dir.path()
            .join("target")
            .join("temper")
            .join("telemetry.jsonl"),
    )?;
    assert_eq!(
        row,
        r#"{"t":"<T>","sid":"s1","agent":"main","event":"deny-lockfile","file":"Cargo.lock"}"#
    );
    assert!(!dir.path().join(".git").exists(), "no git");
    Ok(())
}

fn registry() -> Result<JsValue, Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../hooks/hooks.json");
    Ok(JsValue::from_json(&fs::read_to_string(path)?)?)
}

/// One registered hook: its event, its group's matcher and its last argument.
type Registered = (String, Option<String>, Option<String>);

/// Every registered hook, in the file's order.
fn registered() -> Result<Vec<Registered>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    let registry = registry()?;
    let Some(JsValue::Object(events)) = registry.get("hooks") else {
        return Err("hooks.json has a `hooks` object".into());
    };
    for (event, groups) in events.iter() {
        let JsValue::Array(groups) = groups else {
            return Err(format!("{event}: an array of groups").into());
        };
        for group in groups {
            let matcher = group
                .get("matcher")
                .and_then(JsValue::as_str)
                .map(str::to_owned);
            let Some(JsValue::Array(hooks)) = group.get("hooks") else {
                return Err(format!("{event}: a group without hooks").into());
            };
            for hook in hooks {
                let last = match hook.get("args") {
                    Some(JsValue::Array(args)) => {
                        args.last().and_then(JsValue::as_str).map(str::to_owned)
                    }
                    _ => None,
                };
                out.push((event.to_owned(), matcher.clone(), last));
            }
        }
    }
    Ok(out)
}

/// Ported from `hooks-registry.test.mjs`.
#[test]
fn the_pre_tool_use_subagent_handback_hook_is_registered_cloud_subagents_deliver_their_report_through_it()
-> TestResult {
    let handback: Vec<Option<String>> = registered()?
        .into_iter()
        .filter(|(event, matcher, _)| {
            event == "PreToolUse" && matcher.as_deref() == Some("SubagentHandback")
        })
        .map(|(_, _, last)| last)
        .collect();

    assert_eq!(handback, [Some("pre-handback".to_owned())], "#97");
    Ok(())
}

/// Ported from `hooks-registry.test.mjs`.
#[test]
fn subagent_start_and_subagent_stop_run_for_every_agent_type_so_telemetry_sees_types_the_handlers_skip()
-> TestResult {
    let subagent: Vec<(String, Option<String>)> = registered()?
        .into_iter()
        .filter(|(event, _, _)| event.starts_with("Subagent"))
        .map(|(event, matcher, _)| (event, matcher))
        .collect();

    assert_eq!(
        subagent,
        [
            ("SubagentStart".to_owned(), None),
            ("SubagentStop".to_owned(), None)
        ],
        "no matcher"
    );
    Ok(())
}
