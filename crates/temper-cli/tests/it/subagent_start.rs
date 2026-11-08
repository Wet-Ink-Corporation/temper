//! `temper hook subagent-start` end to end, with real git and `cargo fmt` (port Phase 3e).
//! Ported from 1.x `handlers.test.mjs`'s `SubagentStart` tests; the worktree ones drive
//! `hook pre-edit` to record each subagent's tree, as 1.x's `subagentEdits` does.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use temper_domain::JsValue;

use crate::support::{TestResult, git, hook_in, write_tree};

const ROOT: &str = "[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n\n[workspace.package]\nedition = \"2024\"\nversion = \"0.1.0\"\npublish = false\n";
const MEMBER: &str = "[package]\nname = \"core\"\nedition.workspace = true\nversion.workspace = true\npublish.workspace = true\n";
pub(crate) const LIB_RS: &str =
    "//! Core.\n\n/// One.\n#[must_use]\npub fn one() -> u8 {\n    1\n}\n";
pub(crate) const LIB_RS_2: &str = "//! Core.\n\n/// One.\n#[must_use]\npub fn one() -> u8 {\n    1\n}\n\n/// Two.\n#[must_use]\npub fn two() -> u8 {\n    2\n}\n";
pub(crate) const LIB: &str = "crates/core/src/lib.rs";

/// 1.x `handlers.test.mjs`'s `repo()`: a committed one-crate workspace.
pub(crate) fn repo() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    write_tree(
        dir.path(),
        &[
            ("Cargo.toml", ROOT),
            ("crates/core/Cargo.toml", MEMBER),
            (LIB, LIB_RS),
        ],
    )?;
    git(dir.path(), &["init", "-q"])?;
    git(dir.path(), &["config", "core.autocrlf", "false"])?;
    git(dir.path(), &["add", "-A"])?;
    git(dir.path(), &["commit", "-qm", "base"])?;
    Ok(dir)
}

pub(crate) fn head(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()?;
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}

pub(crate) fn state(dir: &Path) -> PathBuf {
    dir.join(".git").join("temper")
}

pub(crate) fn json_str(text: &str) -> String {
    JsValue::String(text.to_owned()).to_json()
}

/// `hook <event>` in `dir` with `input`: stdout as JSON, or `Null` when empty.
pub(crate) fn hook(
    dir: &Path,
    event: &str,
    input: &str,
) -> Result<JsValue, Box<dyn std::error::Error>> {
    let run = hook_in(
        dir,
        event,
        input,
        &[("CLAUDE_PLUGIN_DATA", dir.as_os_str())],
    )?;
    if run.code != Some(0) {
        return Err(format!("exit {:?}: {}", run.code, run.stderr).into());
    }
    if run.stdout.is_empty() {
        return Ok(JsValue::Null);
    }
    Ok(JsValue::from_json(&run.stdout)?)
}

/// `hook subagent-start` in `cwd` for session `sid`, an agent of `agent_type` (if any) with
/// id `agent_id`.
pub(crate) fn start(
    cwd: &Path,
    sid: &str,
    agent_type: Option<&str>,
    agent_id: &str,
) -> Result<JsValue, Box<dyn std::error::Error>> {
    let agent_type = agent_type.map_or_else(String::new, |t| format!(r#","agent_type":"{t}""#));
    let input = format!(
        r#"{{"cwd":{},"session_id":"{sid}","agent_id":"{agent_id}"{agent_type}}}"#,
        json_str(&cwd.to_string_lossy())
    );
    hook(cwd, "subagent-start", &input)
}

pub(crate) fn context(json: &JsValue) -> String {
    json.get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(JsValue::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The fingerprint `temper status --session <sid>` prints for the checkout at `dir`.
pub(crate) fn fingerprint(dir: &Path, sid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new(env!("CARGO_BIN_EXE_temper"))
        .args(["status", "--session", sid])
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", std::env::temp_dir())
        .output()?;
    let stdout = String::from_utf8(out.stdout)?;
    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix("fingerprint: "))
        .ok_or("a fingerprint line")?;
    Ok(line.to_owned())
}

/// 1.x `r.gate(ok)`: the cached gate record for fingerprint `fp`.
pub(crate) fn seed_gate(dir: &Path, fp: &str, ok: bool) -> TestResult {
    let gate = state(dir).join("gate");
    fs::create_dir_all(&gate)?;
    let summary = if ok { "clean" } else { "error: seeded failure" };
    fs::write(
        gate.join(format!("{fp}.json")),
        format!(
            r#"{{"ok":{ok},"fp":"{fp}","at":"2026-11-08T10:00:00.000Z","steps":[{{"name":"fmt","ok":true,"ms":0,"summary":"applied"}},{{"name":"clippy","ok":{ok},"ms":0,"summary":"{summary}"}}]}}"#
        ),
    )?;
    Ok(())
}

#[test]
fn the_reviewer_implementer_and_architect_get_the_fingerprint_and_base_others_nothing() -> TestResult
{
    let repo = repo()?;
    let dir = repo.path();
    let head = head(dir)?;
    write_tree(
        dir,
        &[(LIB, LIB_RS_2), (".temper/plans/core.md", "# plan\n")],
    )?;
    let fp = fingerprint(dir, "s-sub")?;
    seed_gate(dir, &fp, false)?;

    let rev = start(dir, "s-sub", Some("temper:rust-reviewer"), "rv1")?;
    let text = context(&rev);

    assert_eq!(
        rev.get("hookSpecificOutput")
            .and_then(|h| h.get("hookEventName")),
        Some(&JsValue::String("SubagentStart".to_owned()))
    );
    assert!(
        text.starts_with("temper context for temper:rust-reviewer:\n"),
        "{text}"
    );
    for line in [
        format!("- Base revision: {head}; review scope: `git diff {head}` plus untracked files."),
        format!("- Current tree fingerprint: {fp}"),
        format!("- Changed Rust files (1): {LIB}"),
        "- Gate for this fingerprint: red (clippy).".to_owned(),
        "- Plans: .temper/plans/core.md".to_owned(),
        format!("- Your verdict's \"fingerprint\" field must be {fp}."),
    ] {
        assert!(text.contains(&line), "{line}\n{text}");
    }
    assert!(
        text.contains("- Diff stat:\n") && text.contains("crates/core/src/lib.rs"),
        "{text}"
    );
    assert!(!text.contains("earlier review"), "{text}");

    let implementer = context(&start(dir, "s-sub", Some("rust-implementer"), "im1")?);
    assert!(
        implementer.starts_with("temper context for rust-implementer:\n"),
        "{implementer}"
    );
    assert!(implementer.contains(&format!("- Current tree fingerprint: {fp}")));
    assert!(!implementer.contains("Your verdict's"), "{implementer}");
    let architect = context(&start(dir, "s-sub", Some("temper:rust-architect"), "ar1")?);
    assert!(
        architect.starts_with("temper context for temper:rust-architect:\n"),
        "{architect}"
    );

    assert_eq!(
        start(dir, "s-sub", Some("general-purpose"), "g1")?,
        JsValue::Null
    );
    assert_eq!(start(dir, "s-sub", None, "x1")?, JsValue::Null);
    Ok(())
}

/// A recorded verdict for `fp` in 1.x `recordVerdict`'s shape: its verdict, findings and
/// staleness.
fn seed_verdict(
    dir: &Path,
    fp: &str,
    (verdict, findings, stale): (&str, &str, bool),
) -> TestResult {
    let verdicts = state(dir).join("verdicts");
    fs::create_dir_all(&verdicts)?;
    fs::write(
        verdicts.join(format!("{fp}.json")),
        format!(
            r#"{{"temper_verdict":1,"verdict":"{verdict}","fingerprint":"{fp}","findings":{findings},"recorded_at":"2026-11-08T10:00:00.000Z","agent_id":"rv1","stale":{stale}}}"#
        ),
    )?;
    Ok(())
}

#[test]
fn a_reviewer_of_a_tree_with_a_verdict_is_told_the_earlier_verdict_and_its_findings() -> TestResult
{
    let repo = repo()?;
    let dir = repo.path();
    write_tree(dir, &[(LIB, LIB_RS_2)])?;
    let fp = fingerprint(dir, "s-sub-prior")?;
    let review = || -> Result<String, Box<dyn std::error::Error>> {
        Ok(context(&start(
            dir,
            "s-sub-prior",
            Some("temper:rust-reviewer"),
            "rv2",
        )?))
    };

    seed_verdict(dir, &fp, ("approved", "[]", false))?;
    let text = review()?;
    assert!(text.contains("\n- An earlier review of this same tree is recorded: approved at 2026-11-08T10:00:00.000Z, with no blocker or major finding.\n"), "{text}");
    assert!(text.contains("Re-check each earlier finding"), "{text}");
    assert!(text.contains("Overturn that approval only with a blocker or major finding, a score below 3 or a gate entry that is not \"pass\", quoting the evidence"), "{text}");

    let finding = format!(
        r#"[{{"rule":"API-01","severity":"major","file":"{LIB}","line":10,"evidence":"pub fn two","fix":"name it for what it returns"}}]"#
    );
    seed_verdict(dir, &fp, ("changes-requested", &finding, false))?;
    let text = review()?;
    assert!(text.contains(&format!("\n- An earlier review of this same tree is recorded: changes-requested at 2026-11-08T10:00:00.000Z.\n- [major] API-01 {LIB}:10: name it for what it returns\n")), "{text}");

    seed_verdict(dir, &fp, ("approved", "[]", true))?;
    assert!(
        !review()?.contains("earlier review"),
        "a stale record says nothing"
    );

    seed_verdict(dir, &fp, ("approved", "[]", false))?;
    let implementer = context(&start(dir, "s-sub-prior", Some("rust-implementer"), "im1")?);
    assert!(!implementer.contains("earlier review"), "{implementer}");
    Ok(())
}

#[test]
fn the_kept_history_after_an_approval_is_counted() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    write_tree(dir, &[(LIB, LIB_RS_2)])?;
    let fp = fingerprint(dir, "s-hist")?;
    seed_verdict(dir, &fp, ("approved", "[]", false))?;
    let rows = [
        r#"{"verdict":"changes-requested","recorded_at":"2026-11-08T11:00:00.000Z"}"#,
        r#"{"verdict":"changes-requested","recorded_at":"2026-11-08T09:00:00.000Z"}"#,
        "not json",
        r#"{"verdict":"changes-requested","recorded_at":"2026-11-09T00:00:00.000Z"}"#,
    ];
    fs::write(
        state(dir)
            .join("verdicts")
            .join(format!("{fp}.history.jsonl")),
        rows.join("\n") + "\n",
    )?;

    let text = context(&start(dir, "s-hist", Some("rust-reviewer"), "rv1")?);

    assert!(text.contains("\n  2 later changes-requested verdict(s) gave no reason to overturn it and are kept as history beside it.\n"), "{text}");
    Ok(())
}

#[test]
fn every_agent_type_is_recorded_in_telemetry_before_filtering() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    write_tree(dir, &[(LIB, LIB_RS_2)])?;

    assert_eq!(
        start(dir, "s-tel", Some("general-purpose"), "g1")?,
        JsValue::Null
    );

    let log = fs::read_to_string(state(dir).join("telemetry.jsonl"))?;
    let row = JsValue::from_json(log.lines().last().ok_or("a row")?)?;
    assert_eq!(
        row.get("event").and_then(JsValue::as_str),
        Some("subagent-start")
    );
    assert_eq!(
        row.get("type").and_then(JsValue::as_str),
        Some("general-purpose")
    );
    assert_eq!(
        row.get("agent").and_then(JsValue::as_str),
        Some("general-purpose")
    );
    Ok(())
}

#[test]
fn a_red_gate_record_whose_steps_1x_cannot_filter_fails_the_hook_open() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    write_tree(dir, &[(LIB, LIB_RS_2)])?;
    let fp = fingerprint(dir, "s-f5")?;
    let gate = state(dir).join("gate");
    fs::create_dir_all(&gate)?;
    fs::write(
        gate.join(format!("{fp}.json")),
        r#"{"ok":false,"steps":"x"}"#,
    )?;

    let json = start(dir, "s-f5", Some("rust-implementer"), "im1")?;

    let message = json
        .get("systemMessage")
        .and_then(JsValue::as_str)
        .unwrap_or_default();
    assert!(
        message.starts_with("temper hook subagent-start failed: "),
        "{message}"
    );
    Ok(())
}
