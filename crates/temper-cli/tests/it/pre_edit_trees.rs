//! `temper hook pre-edit` records the checkout a Rust or manifest edit lands in (1.x `noteTree`,
//! port Phase 3e), before the skill gate, with real git.

use std::fs;
use std::path::Path;

use temper_domain::{JsValue, tree_id};

use crate::subagent_start::{LIB, LIB_RS_2, head, hook, json_str, repo, state};
use crate::support::TestResult;

/// `hook pre-edit` for a `Write` of `content` to `rel` under `dir`, by the main agent of
/// session `sid`.
fn write(
    dir: &Path,
    sid: &str,
    rel: &str,
    content: &str,
) -> Result<JsValue, Box<dyn std::error::Error>> {
    let file = dir.join(rel);
    let input = format!(
        r#"{{"cwd":{},"session_id":"{sid}","tool_name":"Write","tool_input":{{"file_path":{},"content":{}}}}}"#,
        json_str(&dir.to_string_lossy()),
        json_str(&file.to_string_lossy()),
        json_str(content)
    );
    hook(dir, "pre-edit", &input)
}

fn record_path(dir: &Path, sid: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let root = fs::canonicalize(dir)?;
    let id = tree_id(&root.to_string_lossy());
    Ok(state(dir)
        .join("sessions")
        .join(sid)
        .join(format!("tree-{id}-main.json")))
}

#[test]
fn a_denied_rust_edit_still_records_the_tree() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();

    let json = write(dir, "s-note", LIB, LIB_RS_2)?;

    let reason = json
        .get("hookSpecificOutput")
        .and_then(|h| h.get("permissionDecision"))
        .and_then(JsValue::as_str);
    assert_eq!(reason, Some("deny"), "the skill gate refuses it");
    let rec = JsValue::from_json(&fs::read_to_string(record_path(dir, "s-note")?)?)?;
    assert_eq!(
        rec.get("head").and_then(JsValue::as_str),
        Some(head(dir)?.as_str())
    );
    assert!(
        rec.get("startFp")
            .and_then(JsValue::as_str)
            .is_some_and(|fp| fp.len() == 16)
    );
    assert!(
        rec.get("branch").is_some(),
        "the branch, or null when detached"
    );
    Ok(())
}

#[test]
fn a_manifest_edit_records_the_tree_and_another_edit_does_not() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();

    write(dir, "s-other", "README.md", "hello\n")?;
    assert!(!record_path(dir, "s-other")?.exists(), "not a Rust edit");

    write(
        dir,
        "s-manifest",
        "crates/core/Cargo.toml",
        "[package]\nname = \"core\"\n",
    )?;
    assert!(record_path(dir, "s-manifest")?.exists(), "a manifest edit");
    Ok(())
}

#[test]
fn a_tree_record_that_cannot_be_written_fails_the_hook_open() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    let sessions = state(dir).join("sessions");
    fs::create_dir_all(&sessions)?;
    // The session's directory is a file, so the record's directory cannot be made.
    fs::write(sessions.join("s-blocked"), "")?;

    let json = write(dir, "s-blocked", LIB, LIB_RS_2)?;

    let message = json
        .get("systemMessage")
        .and_then(JsValue::as_str)
        .unwrap_or_default();
    assert!(
        message.starts_with("temper hook pre-edit failed: "),
        "{json:?}"
    );
    Ok(())
}
