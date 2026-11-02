//! `temper hook <event>`: the protocol, `post-tool` and `user-prompt` against the real file
//! system and git, and `hooks/hooks.json`.

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
