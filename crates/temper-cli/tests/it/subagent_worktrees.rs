//! `temper hook subagent-start` with the session's worktrees (#135), with real git: ported from
//! 1.x `handlers.test.mjs`'s worktree `SubagentStart` tests. Each subagent's tree is recorded
//! by `hook pre-edit`, as 1.x's `subagentEdits` does.

use std::fs;
use std::path::{Path, PathBuf};

use temper_domain::JsValue;

use crate::subagent_start::{
    LIB, LIB_RS_2, context, fingerprint, head, hook, json_str, repo, start,
};
use crate::support::{TestResult, git, write_tree};

/// `LIB_RS_2` with a third function.
fn lib_rs_3() -> String {
    format!("{LIB_RS_2}\n/// Three.\n#[must_use]\npub fn three() -> u8 {{\n    3\n}}\n")
}

/// `hook session-start` from `startup` in `dir`.
fn session_start(dir: &Path, sid: &str) -> TestResult {
    let input = format!(
        r#"{{"cwd":{},"session_id":"{sid}","source":"startup"}}"#,
        json_str(&dir.to_string_lossy())
    );
    hook(dir, "session-start", &input)?;
    Ok(())
}

/// 1.x `worktree(r, name)`: a detached worktree at `.claude/worktrees/<name>`, by its real path.
fn worktree(dir: &Path, name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let at = dir.join(".claude").join("worktrees").join(name);
    git(
        dir,
        &["worktree", "add", "-q", "--detach", &at.to_string_lossy()],
    )?;
    Ok(fs::canonicalize(at)?)
}

/// 1.x `subagentEdits(w, sid, agent_id, rel, text)`: a general-purpose subagent's `Write`
/// through `hook pre-edit` (which records the tree and is refused by the skill gate), then the
/// write itself.
fn subagent_edits(w: &Path, sid: &str, agent_id: &str, (rel, text): (&str, &str)) -> TestResult {
    let input = format!(
        r#"{{"cwd":{},"session_id":"{sid}","agent_id":"{agent_id}","agent_type":"general-purpose","tool_name":"Write","tool_input":{{"file_path":{},"content":{}}}}}"#,
        json_str(&w.to_string_lossy()),
        json_str(&w.join(rel).to_string_lossy()),
        json_str(text)
    );
    hook(w, "pre-edit", &input)?;
    write_tree(w, &[(rel, text)])
}

fn lines_starting<'t>(text: &'t str, prefix: &str) -> Vec<&'t str> {
    text.lines()
        .filter(|line| line.starts_with(prefix))
        .collect()
}

#[test]
fn a_reviewer_with_changes_in_the_main_checkout_and_a_worktree_targets_the_main_tree() -> TestResult
{
    let repo = repo()?;
    let dir = repo.path();
    let sid = "s-wt-both";
    session_start(dir, sid)?;
    write_tree(dir, &[(LIB, LIB_RS_2)])?;
    let w = worktree(dir, "agent-c1")?;
    subagent_edits(&w, sid, "c1", (LIB, &lib_rs_3()))?;

    let told = context(&start(dir, sid, Some("temper:rust-reviewer"), "rv1")?);

    assert_eq!(
        lines_starting(&told, "- Target:"),
        Vec::<&str>::new(),
        "{told}"
    );
    let w_fp = fingerprint(&w, sid)?;
    assert!(told.contains(&format!("Also changed in this session: the worktree .claude/worktrees/agent-c1 (fingerprint {w_fp}, 1 Rust file(s))")), "{told}");
    let fp = fingerprint(dir, sid)?;
    assert!(told.contains(&format!("must be {fp}, or, if you were asked to review one of the worktrees listed above, that worktree's fingerprint.")), "{told}");
    Ok(())
}

#[test]
fn parallel_agents_in_their_own_worktrees_each_keep_their_own_tree() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    let sid = "s-wt-par";
    session_start(dir, sid)?;
    let w1 = worktree(dir, "agent-p1")?;
    let w2 = worktree(dir, "agent-p2")?;
    start(&w1, sid, Some("temper:rust-implementer"), "p1")?;
    subagent_edits(&w1, sid, "p1", (LIB, LIB_RS_2))?;

    let second = context(&start(&w2, sid, Some("temper:rust-implementer"), "p2")?);
    assert!(lines_starting(&second, "- Target").is_empty(), "{second}");
    assert!(
        lines_starting(&second, "- Also changed").is_empty(),
        "{second}"
    );
    assert!(
        second.contains(&format!("- Project root: {}", w2.display())),
        "{second}"
    );
    assert!(
        second.contains("\n- Changed Rust files (0): none\n"),
        "{second}"
    );

    let rv = context(&start(&w1, sid, Some("temper:rust-reviewer"), "rv1")?);
    assert!(lines_starting(&rv, "- Target").is_empty(), "{rv}");
    assert!(lines_starting(&rv, "- Also changed").is_empty(), "{rv}");
    assert!(
        rv.contains(&format!("- Project root: {}", w1.display())),
        "{rv}"
    );
    let w1_fp = fingerprint(&w1, sid)?;
    assert!(
        rv.contains(&format!(
            "\n- Your verdict's \"fingerprint\" field must be {w1_fp}. "
        )),
        "{rv}"
    );
    let gate = lines_starting(&rv, "- Gate for this fingerprint:");
    let command = gate
        .first()
        .and_then(|line| line.split_once("Gate command: "))
        .map(|(_, c)| c);
    let base = head(dir)?;
    assert_eq!(
        command,
        Some(format!("TEMPER_BASE={base} temper gate --check").as_str()),
        "{rv}"
    );
    Ok(())
}

#[test]
fn a_worktree_on_a_later_commit_than_the_session_base_names_its_own_base() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    let sid = "s-wt-ahead";
    session_start(dir, sid)?;
    write_tree(dir, &[("README.md", "later\n")])?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", "later"])?;
    let later = head(dir)?;
    let w = worktree(dir, "agent-e1")?;
    subagent_edits(&w, sid, "e1", (LIB, LIB_RS_2))?;

    let told = context(&start(dir, sid, Some("temper:rust-reviewer"), "rv1")?);

    assert_eq!(
        lines_starting(&told, "- Target: the worktree .claude/worktrees/agent-e1. ").len(),
        1,
        "{told}"
    );
    assert!(
        told.contains(&format!("&& TEMPER_BASE={later} temper gate --check")),
        "{told}"
    );
    assert!(
        told.contains(&format!("\n- Base revision: {later}; ")),
        "{told}"
    );
    assert_eq!(
        lines_starting(&told, "- Changed Rust files (1): ").len(),
        1,
        "{told}"
    );
    let rows = fs::read_to_string(dir.join(".git/temper/telemetry.jsonl"))?;
    let starts = rows
        .lines()
        .filter_map(|line| JsValue::from_json(line).ok())
        .filter(|row| row.get("event").and_then(JsValue::as_str) == Some("subagent-start"))
        .count();
    assert_eq!(starts, 1, "one row per start");
    Ok(())
}
