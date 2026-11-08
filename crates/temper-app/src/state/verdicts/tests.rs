//! Ported from 1.x `tests/verdict.test.mjs`: "recordVerdict and verdictFor round-trip by
//! fingerprint", "recordVerdict: a changes-requested verdict without a blocker or major finding
//! keeps an approval of the same tree (#136)" and "recordVerdict: changes requested for a score
//! below 3 or a failing gate entry replace an approval, without any finding (#136)".

use temper_domain::PathStyle::Posix;
use temper_domain::{
    Config, DIMENSIONS, ExtractedVerdict, Fingerprint, GateState, JsValue, ValidVerdict,
    VerdictContext,
};

use super::{RecordMeta, record_verdict};
use crate::engine::{HookError, RunEnv};
use crate::fakes::{FakeFiles, FakePorts, NOW};
use crate::hooks::{HookCall, HookInput};
use crate::project::Workspace;
use crate::state::verdict_for;

const FP: &str = "0123456789abcdef";
const RECORD: &str = "/w/.git/temper/verdicts/0123456789abcdef.json";
const HISTORY: &str = "/w/.git/temper/verdicts/0123456789abcdef.history.jsonl";

fn workspace() -> Workspace {
    Workspace {
        root: "/w".to_owned(),
        state_dir: "/w/.git/temper".to_owned(),
        config: Config::defaults(),
        config_problem: None,
        git: true,
    }
}

fn fp(text: &str) -> Fingerprint {
    text.parse().expect("16 hex")
}

const MINOR: &str = r#"{"rule":"OWN-08","severity":"minor","file":"crates/core/src/lib.rs","line":3,"evidence":"x.clone()","fix":"borrow it"}"#;
const NIT: &str = r#"{"rule":"OWN-08","severity":"nit","file":"crates/core/src/lib.rs","line":3,"evidence":"x.clone()","fix":"borrow it"}"#;
const MAJOR: &str = r#"{"rule":"OWN-08","severity":"major","file":"crates/core/src/lib.rs","line":3,"evidence":"x.clone()","fix":"borrow it"}"#;

/// 1.x's `verdict({ verdict, findings, scores, gate })` with each part given as JSON.
fn verdict_with(
    outcome: &str,
    findings: &str,
    low: Option<(&str, &str)>,
    gate: &str,
) -> ValidVerdict {
    let scores: Vec<String> = DIMENSIONS
        .iter()
        .map(|d| match low {
            Some((name, score)) if name == *d => format!("{d:?}:{score}"),
            _ => format!("{d:?}:4"),
        })
        .collect();
    let text = format!(
        r#"{{"temper_verdict":1,"verdict":"{outcome}","fingerprint":"{FP}","findings":{findings},"scores":{{{}}},"gate":{gate},"evidence":{{"files_read":["crates/core/src/lib.rs"],"commands_run":["node temper.mjs gate"]}}}}"#,
        scores.join(",")
    );
    let ctx = VerdictContext {
        changed_rust: &[],
        gate: GateState::NotRed,
    };
    let value = JsValue::from_json(&text).expect("JSON");
    ValidVerdict::check(ExtractedVerdict::Parsed(value), &ctx).expect("valid")
}

const PASS: &str = r#"{"fmt":"pass","clippy":"pass","test":"pass","doc":"pass"}"#;

fn approval() -> ValidVerdict {
    verdict_with("approved", "[]", None, PASS)
}

fn changes(findings: &str) -> ValidVerdict {
    verdict_with("changes-requested", findings, None, PASS)
}

fn id(text: &str) -> JsValue {
    JsValue::String(text.to_owned())
}

fn empty_input() -> HookInput {
    HookInput::parse("{}").expect("JSON")
}

/// [`record_verdict`] run from `/w`; `meta` carries the agent id, so the input is empty.
fn record_with(
    ports: &FakePorts,
    v: &ValidVerdict,
    meta: RecordMeta<'_>,
) -> Result<JsValue, HookError> {
    let input = empty_input();
    let call = HookCall {
        ports,
        env: &RunEnv::at("/w", Posix),
        input: &input,
    };
    record_verdict(&call, &workspace(), v, meta)
}

fn record(ports: &FakePorts, v: &ValidVerdict, agent: &str, stale: bool) -> JsValue {
    let agent = id(agent);
    let meta = RecordMeta {
        agent_id: Some(&agent),
        stale,
    };
    record_with(ports, v, meta).expect("recorded")
}

fn recorded(ports: &FakePorts) -> Option<JsValue> {
    verdict_for(&ports.files, Posix, &workspace(), fp(FP))
}

fn agent_of(value: Option<&JsValue>) -> Option<String> {
    value?.get("agent_id")?.as_str().map(str::to_owned)
}

fn history(ports: &FakePorts) -> Vec<JsValue> {
    ports
        .files
        .text(HISTORY)
        .unwrap_or_default()
        .lines()
        .map(|line| JsValue::from_json(line).expect("a JSON row"))
        .collect()
}

#[test]
fn record_verdict_and_verdict_for_round_trip_by_fingerprint() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    assert_eq!(recorded(&ports), None);
    let rec = record(&ports, &approval(), "a1", false);
    assert_eq!(agent_of(Some(&rec)).as_deref(), Some("a1"));
    assert_eq!(rec.get("stale"), Some(&JsValue::Bool(false)));
    assert_eq!(
        rec.get("recorded_at").and_then(JsValue::as_str),
        Some(NOW.to_string().as_str())
    );
    // `JSON.stringify(rec, null, 2)`, no newline after it.
    assert_eq!(ports.files.text(RECORD), Some(rec.to_json_pretty()));
    assert_eq!(recorded(&ports), Some(rec));
    let other = verdict_for(&ports.files, Posix, &workspace(), fp("fedcba9876543210"));
    assert_eq!(other, None);
    // an unreadable record reads as no verdict
    let broken = FakePorts::new(FakeFiles::new(Posix).with_file(RECORD, "{"));
    assert_eq!(recorded(&broken), None);
}

#[test]
fn a_changes_request_without_a_blocker_or_major_finding_keeps_an_approval_of_the_same_tree() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    let approval_rec = record(&ports, &approval(), "a1", false);
    assert_eq!(approval_rec.get("kept"), None);
    assert_eq!(ports.files.text(HISTORY), None);
    // minor and nit findings only: the approval stays, and the later verdict goes to history
    let minor = record(&ports, &changes(&format!("[{MINOR},{NIT}]")), "a2", false);
    assert_eq!(
        minor.get("kept").and_then(JsValue::as_str),
        Some("approved")
    );
    assert_eq!(recorded(&ports), Some(approval_rec));
    let rows = history(&ports);
    assert_eq!(rows, std::slice::from_ref(&minor));
    assert_eq!(
        ports.files.text(HISTORY),
        Some(format!("{}\n", minor.to_json()))
    );
    // a major finding replaces the approval, as before
    let major = record(&ports, &changes(&format!("[{MAJOR}]")), "a3", false);
    assert_eq!(major.get("kept"), None);
    assert_eq!(agent_of(recorded(&ports).as_ref()).as_deref(), Some("a3"));
    // with changes requested on record, a minor-only verdict replaces it; an approval always does
    record(&ports, &changes("[]"), "a4", false);
    assert_eq!(agent_of(recorded(&ports).as_ref()).as_deref(), Some("a4"));
    record(&ports, &approval(), "a5", false);
    assert_eq!(agent_of(recorded(&ports).as_ref()).as_deref(), Some("a5"));
    assert_eq!(history(&ports).len(), 1);
    // a stale approval does not hold
    record(&ports, &approval(), "a6", true);
    record(&ports, &changes(&format!("[{MINOR}]")), "a7", false);
    assert_eq!(agent_of(recorded(&ports).as_ref()).as_deref(), Some("a7"));
}

#[test]
fn changes_requested_for_a_low_score_or_a_failing_gate_entry_replace_an_approval() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    let findings = format!("[{MINOR}]");
    let cases = [
        (
            "low score",
            verdict_with("changes-requested", &findings, Some(("tests", "2")), PASS),
        ),
        (
            "failing gate entry",
            verdict_with(
                "changes-requested",
                &findings,
                None,
                r#"{"fmt":"pass","clippy":"fail","test":"pass","doc":"pass"}"#,
            ),
        ),
    ];
    for (name, v) in &cases {
        record(&ports, &approval(), "ok", false);
        let rec = record(&ports, v, name, false);
        assert_eq!(rec.get("kept"), None, "{name}");
        assert_eq!(agent_of(recorded(&ports).as_ref()).as_deref(), Some(*name));
    }
    // "n/a" scores and gate entries are no reason to overturn
    record(&ports, &approval(), "ok", false);
    let na = verdict_with(
        "changes-requested",
        "[]",
        Some(("docs", r#""n/a""#)),
        r#"{"fmt":"pass","clippy":"pass","test":"pass","doc":"n/a"}"#,
    );
    let rec = record(&ports, &na, "na", false);
    assert_eq!(rec.get("kept").and_then(JsValue::as_str), Some("approved"));
    assert_eq!(agent_of(recorded(&ports).as_ref()).as_deref(), Some("ok"));
}

#[test]
fn an_absent_agent_id_is_left_out_of_the_record() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    let meta = RecordMeta {
        agent_id: None,
        stale: true,
    };
    let rec = record_with(&ports, &approval(), meta).expect("recorded");
    assert_eq!(rec.get("agent_id"), None);
    assert!(
        !ports
            .files
            .text(RECORD)
            .unwrap_or_default()
            .contains("agent_id")
    );
}

#[test]
fn a_write_that_fails_fails_the_record_as_1x_throws() {
    let ports = FakePorts::new(FakeFiles::new(Posix).with_read_only("/w/.git/temper/verdicts"));
    let meta = RecordMeta {
        agent_id: None,
        stale: false,
    };
    let result = record_with(&ports, &approval(), meta);
    assert!(matches!(result, Err(HookError::Files(_))), "{result:?}");
    // the history append of a kept verdict fails the same way
    let approved = format!(r#"{{"verdict":"approved","stale":false,"fingerprint":"{FP}"}}"#);
    let files = FakeFiles::new(Posix)
        .with_file(RECORD, &approved)
        .with_read_only("/w/.git/temper/verdicts");
    let ports = FakePorts::new(files);
    let result = record_with(&ports, &changes("[]"), meta);
    assert!(matches!(result, Err(HookError::Files(_))), "{result:?}");
    assert_eq!(ports.files.text(HISTORY), None);
}
