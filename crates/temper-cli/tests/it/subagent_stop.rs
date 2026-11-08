//! `temper hook subagent-stop`'s reviewer branch end to end, with real git (port Phase 3f).
//! Ported from 1.x `handlers.test.mjs`'s `SubagentStop (reviewer)` tests, "SubagentStop: other
//! agent types are not checked", the `SubagentStop` half of "SubagentStart and SubagentStop:
//! every agent type is recorded in telemetry before filtering", and the `subagentStop` tail of
//! "SubagentHandback: only the reviewer is checked…". The implementer branch (1.x
//! `completionGate`) waits for port Phase 4.

use std::fs;
use std::path::{Path, PathBuf};

use temper_domain::JsValue;

use crate::pre_handback::{fenced, input, recorded, rows, verdict};
use crate::subagent_start::{LIB, LIB_RS_2, fingerprint, hook, json_str, repo, seed_gate, state};
use crate::support::{TestResult, hook_in};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const REVIEWER: &str = r#""agent_type":"temper:rust-reviewer""#;

/// `subagent-stop` in session `sid` with `more` input properties.
fn stop(dir: &Path, sid: &str, more: &str) -> Result<JsValue> {
    hook(dir, "subagent-stop", &input(dir, sid, more))
}

/// `subagent-stop` of reviewer `agent` ending with `message`.
fn ends(dir: &Path, sid: &str, agent: &str, message: &str) -> Result<JsValue> {
    let more = format!(
        r#",{REVIEWER},"agent_id":"{agent}","last_assistant_message":{}"#,
        json_str(message)
    );
    stop(dir, sid, &more)
}

fn blocked(out: &JsValue) -> Option<&str> {
    match out.get("decision").and_then(JsValue::as_str) {
        Some("block") => out.get("reason").and_then(JsValue::as_str),
        _ => None,
    }
}

fn field<'a>(value: Option<&'a JsValue>, key: &str) -> Option<&'a str> {
    value?.get(key)?.as_str()
}

/// A changed tree and its fingerprint.
fn changed() -> Result<(tempfile::TempDir, String)> {
    let r = repo()?;
    fs::write(r.path().join(LIB), LIB_RS_2)?;
    let fp = fingerprint(r.path(), "s")?;
    Ok((r, fp))
}

const MINOR: &str = r#"{"rule":"OWN-08","severity":"minor","file":"crates/core/src/lib.rs","line":10,"evidence":"x.clone()","fix":"borrow it"}"#;
const MAJOR: &str = r#"{"rule":"OWN-08","severity":"major","file":"crates/core/src/lib.rs","line":10,"evidence":"x.clone()","fix":"borrow it"}"#;
const BLOCKER: &str = r#"{"rule":"H-02","severity":"blocker","file":"crates/core/src/lib.rs","line":10,"evidence":"x.unwrap()","fix":"propagate with ?"}"#;

fn changes(fp: &str, finding: &str) -> String {
    let findings = format!("[{finding}]");
    fenced(&verdict(
        fp,
        &[
            ("verdict", r#""changes-requested""#),
            ("findings", &findings),
        ],
    ))
}

/// 1.x's `transcript(r, message)`: a text turn, then a hand-back of `message`.
fn transcript(dir: &Path, message: &str) -> Result<PathBuf> {
    let file = dir.join(".temper").join("agent.jsonl");
    fs::create_dir_all(dir.join(".temper"))?;
    let rows = [
        r#"{"type":"user","message":{"role":"user","content":"review the change"}}"#.to_owned(),
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Working on it."}]}}"#.to_owned(),
        format!(r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"SubagentHandback","input":{{"message":{}}}}}]}}}}"#, json_str(message)),
        r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"Report delivered to your caller."}]}}"#.to_owned(),
    ];
    fs::write(&file, format!("{}\n", rows.join("\n")))?;
    Ok(file)
}

fn from_transcript(dir: &Path, sid: &str, agent: &str, file: &Path) -> Result<JsValue> {
    let more = format!(
        r#",{REVIEWER},"agent_id":"{agent}","agent_transcript_path":{}"#,
        json_str(&file.to_string_lossy())
    );
    stop(dir, sid, &more)
}

#[test]
fn an_invalid_verdict_blocks_and_is_let_through_on_the_stop_hook_active_repeat() -> TestResult {
    let (r, fp) = changed()?;
    let dir = r.path();
    let none = ends(dir, "s-rv", "rv1", "Looks fine to me.")?;
    let reason = blocked(&none).ok_or("blocked")?;
    assert!(
        reason.starts_with("temper: your review verdict is invalid:\n- no verdict block found"),
        "{reason}"
    );
    assert!(
        reason.ends_with("End your final report with a corrected ```json verdict block."),
        "{reason}"
    );
    let evidence = r#"{"files_read":["crates/core/Cargo.toml"],"commands_run":["gate"]}"#;
    let unread = ends(
        dir,
        "s-rv",
        "rv1",
        &fenced(&verdict(&fp, &[("evidence", evidence)])),
    )?;
    let reason = blocked(&unread).ok_or("blocked")?;
    assert!(
        reason.contains("- changed files not reviewed: crates/core/src/lib.rs"),
        "{reason}"
    );
    let more = format!(
        r#",{REVIEWER},"agent_id":"rv1","stop_hook_active":true,"last_assistant_message":"Looks fine to me.""#
    );
    assert_eq!(stop(dir, "s-rv", &more)?, JsValue::Null);
    assert_eq!(recorded(dir, &fp), None, "nothing recorded");
    assert_eq!(rows(dir, "s-rv", "verdict-invalid")?.len(), 2);
    let finals = rows(dir, "s-rv", "verdict-invalid-final")?;
    assert_eq!(finals.len(), 1);
    assert_eq!(field(finals.first(), "agent_id"), Some("rv1"));
    Ok(())
}

#[test]
fn an_approval_is_refused_while_the_cached_gate_for_its_fingerprint_is_red() -> TestResult {
    let (r, fp) = changed()?;
    seed_gate(r.path(), &fp, false)?;
    let out = ends(r.path(), "s-rv-red", "rv1", &fenced(&verdict(&fp, &[])))?;
    let reason = blocked(&out).ok_or("blocked")?;
    assert!(
        reason.contains("- \"approved\" is impossible: the gate is red for this fingerprint"),
        "{reason}"
    );
    Ok(())
}

#[test]
fn a_valid_verdict_is_recorded_flagged_stale_when_the_tree_moved() -> TestResult {
    let (r, fp) = changed()?;
    let dir = r.path();
    assert_eq!(
        ends(dir, "s-rv-ok", "rv1", &fenced(&verdict(&fp, &[])))?,
        JsValue::Null
    );
    let rec = recorded(dir, &fp).ok_or("a record")?;
    assert_eq!(field(Some(&rec), "verdict"), Some("approved"));
    assert_eq!(rec.get("stale"), Some(&JsValue::Bool(false)));
    assert_eq!(field(Some(&rec), "agent_id"), Some("rv1"));
    let session = state(dir).join("sessions").join("s-rv-ok");
    assert_eq!(fs::read_to_string(session.join("verdict-agent-rv1"))?, fp);
    // once delivered, a final message without a verdict passes
    assert_eq!(ends(dir, "s-rv-ok", "rv1", "Done.")?, JsValue::Null);
    // a verdict for a fingerprint that is no longer the tree's is recorded as stale
    let old = "fedcba9876543210";
    let cr = fenced(&verdict(old, &[("verdict", r#""changes-requested""#)]));
    assert_eq!(ends(dir, "s-rv-ok", "rv2", &cr)?, JsValue::Null);
    let stale = recorded(dir, old).ok_or("a record")?;
    assert_eq!(stale.get("stale"), Some(&JsValue::Bool(true)));
    Ok(())
}

#[test]
fn changes_requested_without_a_blocker_or_major_finding_keep_an_approval_of_the_same_tree()
-> TestResult {
    let (r, fp) = changed()?;
    let dir = r.path();
    let sid = "s-rv-keep";
    assert_eq!(
        ends(dir, sid, "rv1", &fenced(&verdict(&fp, &[])))?,
        JsValue::Null
    );
    assert_eq!(ends(dir, sid, "rv2", &changes(&fp, MINOR))?, JsValue::Null);
    let rec = recorded(dir, &fp);
    assert_eq!(field(rec.as_ref(), "verdict"), Some("approved"));
    assert_eq!(field(rec.as_ref(), "agent_id"), Some("rv1"));
    let history = fs::read_to_string(
        state(dir)
            .join("verdicts")
            .join(format!("{fp}.history.jsonl")),
    )?;
    let rows_of: Vec<(Option<String>, Option<String>)> = history
        .lines()
        .map(|l| {
            let row = JsValue::from_json(l).expect("a JSON row");
            let get = |k| field(Some(&row), k).map(str::to_owned);
            (get("agent_id"), get("verdict"))
        })
        .collect();
    let some = |s: &str| Some(s.to_owned());
    assert_eq!(rows_of, [(some("rv2"), some("changes-requested"))]);
    // a major finding overturns the approval, as before
    assert_eq!(ends(dir, sid, "rv3", &changes(&fp, MAJOR))?, JsValue::Null);
    assert_eq!(field(recorded(dir, &fp).as_ref(), "agent_id"), Some("rv3"));
    let verdicts: Vec<(Option<String>, Option<String>)> = rows(dir, sid, "verdict")?
        .iter()
        .map(|row| {
            (
                field(Some(row), "verdict").map(str::to_owned),
                field(Some(row), "kept").map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(
        verdicts,
        [
            (some("approved"), None),
            (some("changes-requested"), some("approved")),
            (some("changes-requested"), None)
        ]
    );
    Ok(())
}

#[test]
fn a_resumed_reviewer_s_new_verdict_on_the_same_tree_is_checked_and_recorded() -> TestResult {
    let (r, fp) = changed()?;
    let dir = r.path();
    let sid = "s-rv-resume";
    assert_eq!(
        ends(dir, sid, "rv1", &fenced(&verdict(&fp, &[])))?,
        JsValue::Null
    );
    // resumed, the same agent ends again on the same tree with changes requested
    assert_eq!(
        ends(dir, sid, "rv1", &changes(&fp, BLOCKER))?,
        JsValue::Null
    );
    assert_eq!(
        field(recorded(dir, &fp).as_ref(), "verdict"),
        Some("changes-requested")
    );
    // ... and an invalid new verdict is still refused
    let findings = format!("[{BLOCKER}]");
    let bad = ends(
        dir,
        sid,
        "rv1",
        &fenced(&verdict(&fp, &[("findings", &findings)])),
    )?;
    assert!(
        blocked(&bad).is_some_and(|r| r.contains("\"approved\" is inconsistent")),
        "{bad:?}"
    );
    // and the reverse: approved again after changes requested
    assert_eq!(
        ends(dir, sid, "rv1", &fenced(&verdict(&fp, &[])))?,
        JsValue::Null
    );
    assert_eq!(
        field(recorded(dir, &fp).as_ref(), "verdict"),
        Some("approved")
    );
    let verdicts: Vec<_> = rows(dir, sid, "verdict")?
        .iter()
        .map(|row| field(Some(row), "verdict").map(str::to_owned))
        .collect();
    let some = |s: &str| Some(s.to_owned());
    assert_eq!(
        verdicts,
        [
            some("approved"),
            some("changes-requested"),
            some("approved")
        ]
    );
    // subagent-stop never leaves a marker that would skip a later verdict
    assert!(
        !state(dir)
            .join("sessions")
            .join(sid)
            .join("verdict-agent-rv1.hash")
            .exists()
    );
    Ok(())
}

#[test]
fn a_resumed_reviewer_repeating_its_own_verdict_after_another_approved_is_recorded() -> TestResult {
    let (r, fp) = changed()?;
    let dir = r.path();
    let sid = "s-rv-repeat";
    let cr = changes(&fp, BLOCKER);
    assert_eq!(ends(dir, sid, "rv1", &cr)?, JsValue::Null);
    assert_eq!(
        ends(dir, sid, "rv2", &fenced(&verdict(&fp, &[])))?,
        JsValue::Null
    );
    assert_eq!(
        field(recorded(dir, &fp).as_ref(), "verdict"),
        Some("approved")
    );
    assert_eq!(ends(dir, sid, "rv1", &cr)?, JsValue::Null);
    let rec = recorded(dir, &fp);
    assert_eq!(field(rec.as_ref(), "verdict"), Some("changes-requested"));
    assert_eq!(field(rec.as_ref(), "agent_id"), Some("rv1"));
    Ok(())
}

#[test]
fn a_verdict_the_hand_back_recorded_is_not_recorded_again_from_the_transcript() -> TestResult {
    let (r, fp) = changed()?;
    let dir = r.path();
    let sid = "s-rv-once";
    // rv0's approval, recorded by an earlier stop
    assert_eq!(
        ends(dir, sid, "rv0", &fenced(&verdict(&fp, &[])))?,
        JsValue::Null
    );
    let report = changes(&fp, MINOR);
    let more = format!(
        r#",{REVIEWER},"agent_id":"rv1","tool_name":"SubagentHandback","tool_input":{{"message":{}}}"#,
        json_str(&report)
    );
    assert_eq!(
        hook(dir, "pre-handback", &input(dir, sid, &more))?,
        JsValue::Null
    );
    assert_eq!(
        from_transcript(dir, sid, "rv1", &transcript(dir, &report)?)?,
        JsValue::Null
    );
    let history = fs::read_to_string(
        state(dir)
            .join("verdicts")
            .join(format!("{fp}.history.jsonl")),
    )?;
    assert_eq!(history.lines().count(), 1);
    // the hand-back's marker is one-shot: used by that subagent-stop, then removed
    assert!(
        !state(dir)
            .join("sessions")
            .join(sid)
            .join("verdict-agent-rv1.hash")
            .exists()
    );
    let verdicts: Vec<_> = rows(dir, sid, "verdict")?
        .iter()
        .map(|row| {
            (
                field(Some(row), "via").map(str::to_owned),
                field(Some(row), "kept").map(str::to_owned),
            )
        })
        .collect();
    let some = |s: &str| Some(s.to_owned());
    assert_eq!(
        verdicts,
        [
            (some("subagent-stop"), None),
            (some("handback"), some("approved"))
        ]
    );
    // the reviewer's context counts the verdict kept as history
    let start = format!(r#",{REVIEWER},"agent_id":"rv2""#);
    let context = hook(dir, "subagent-start", &input(dir, sid, &start))?;
    let text = context
        .get("hookSpecificOutput")
        .and_then(|s| s.get("additionalContext"))
        .and_then(JsValue::as_str)
        .unwrap_or_default();
    assert!(
        text.contains("1 later changes-requested verdict(s) gave no reason to overturn it"),
        "{text}"
    );
    // a verdict for another fingerprint from the same agent is still checked and recorded
    let other = "fedcba9876543210";
    let cr = fenced(&verdict(other, &[("verdict", r#""changes-requested""#)]));
    assert_eq!(ends(dir, sid, "rv1", &cr)?, JsValue::Null);
    assert_eq!(
        field(recorded(dir, other).as_ref(), "agent_id"),
        Some("rv1")
    );
    Ok(())
}

#[test]
fn without_last_assistant_message_the_verdict_is_read_from_the_hand_back_in_the_transcript()
-> TestResult {
    let (r, fp) = changed()?;
    let dir = r.path();
    let out = from_transcript(
        dir,
        "s-rv-tx",
        "rv1",
        &transcript(dir, "Looks fine to me.")?,
    )?;
    assert!(
        blocked(&out).is_some_and(|r| r.contains("- no verdict block found")),
        "{out:?}"
    );
    let ok = from_transcript(
        dir,
        "s-rv-tx",
        "rv1",
        &transcript(dir, &fenced(&verdict(&fp, &[])))?,
    )?;
    assert_eq!(ok, JsValue::Null);
    assert_eq!(
        field(recorded(dir, &fp).as_ref(), "verdict"),
        Some("approved")
    );
    let verdicts = rows(dir, "s-rv-tx", "verdict")?;
    assert_eq!(field(verdicts.first(), "via"), Some("subagent-stop"));
    Ok(())
}

#[test]
fn after_a_hand_back_the_stop_without_a_message_passes() -> TestResult {
    let (r, fp) = changed()?;
    let dir = r.path();
    let sid = "s-hb";
    let more = format!(
        r#",{REVIEWER},"agent_id":"rv1","tool_name":"SubagentHandback","tool_input":{{"message":{}}}"#,
        json_str(&fenced(&verdict(&fp, &[])))
    );
    assert_eq!(
        hook(dir, "pre-handback", &input(dir, sid, &more))?,
        JsValue::Null
    );
    assert_eq!(
        stop(dir, sid, &format!(r#",{REVIEWER},"agent_id":"rv1""#))?,
        JsValue::Null
    );
    assert_eq!(rows(dir, sid, "verdict")?.len(), 1);
    Ok(())
}

#[test]
fn other_agent_types_are_not_checked() -> TestResult {
    let (r, _) = changed()?;
    for agent_type in [
        r#","agent_type":"general-purpose""#,
        r#","agent_type":"temper:rust-architect""#,
        "",
    ] {
        let more = format!(r#"{agent_type},"agent_id":"x1""#);
        assert_eq!(
            stop(r.path(), "s-other", &more)?,
            JsValue::Null,
            "{agent_type}"
        );
    }
    Ok(())
}

#[test]
fn every_agent_type_is_recorded_in_telemetry_before_filtering() -> TestResult {
    let (r, _) = changed()?;
    let dir = r.path();
    let more = r#","agent_type":"general-purpose","agent_id":"g1","agent_transcript_path":"/nowhere.jsonl""#;
    assert_eq!(stop(dir, "s-tel", more)?, JsValue::Null);
    let stops = rows(dir, "s-tel", "subagent-stop")?;
    assert_eq!(field(stops.first(), "type"), Some("general-purpose"));
    assert_eq!(
        stops.first().and_then(|row| row.get("msg")),
        Some(&JsValue::Bool(false))
    );
    assert_eq!(
        stops.first().and_then(|row| row.get("transcript")),
        Some(&JsValue::Bool(true))
    );
    Ok(())
}

/// A failed hook's notice: `temper hook subagent-stop failed: …`, exit 0.
fn failed(dir: &Path, more: &str) -> Result<String> {
    let run = hook_in(
        dir,
        "subagent-stop",
        &input(dir, "s-fail", more),
        &[("CLAUDE_PLUGIN_DATA", dir.as_os_str())],
    )?;
    if run.code != Some(0) {
        return Err(format!("exit {:?}", run.code).into());
    }
    let out = JsValue::from_json(&run.stdout)?;
    Ok(field(Some(&out), "systemMessage")
        .unwrap_or_default()
        .to_owned())
}

#[test]
fn a_report_that_is_not_text_fails_as_1x_s_match_all_and_the_implementer_waits_for_phase_4()
-> TestResult {
    let (r, _) = changed()?;
    let dir = r.path();
    let numeric = failed(dir, &format!(r#",{REVIEWER},"last_assistant_message":5"#))?;
    assert!(
        numeric.starts_with("temper hook subagent-stop failed: "),
        "{numeric}"
    );
    // a falsy report is no report, as 1.x's `if (!text) return null`
    let zero = format!(r#",{REVIEWER},"last_assistant_message":0"#);
    let out = stop(dir, "s-zero", &zero)?;
    assert!(
        blocked(&out).is_some_and(|r| r.contains("- no verdict block found")),
        "{out:?}"
    );
    let implementer = failed(dir, r#","agent_type":"temper:rust-implementer""#)?;
    assert_eq!(
        implementer,
        "temper hook subagent-stop failed: temper hook subagent-stop is not ported to this engine yet"
    );
    // the row is written before the type is filtered
    let stops = rows(dir, "s-fail", "subagent-stop")?;
    assert_eq!(stops.len(), 2);
    Ok(())
}
