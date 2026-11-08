//! Ported from 1.x `tests/telemetry.test.mjs` ("lostGateRow: …" and "openGateStarts: …"),
//! with JS number rows printed by 1.x's `lostGateRow` on Node 22.

use super::{
    LostGateFacts, LostReason, ProgressSeen, lost_gate_claim, lost_gate_row, open_gate_starts,
};
use crate::hooks::{TelemetryEvent, TelemetryRow};
use crate::js::Value;
use crate::{ModTime, Timestamp};

/// 2026-10-16T23:00:00.000Z.
const T0: i64 = 1_792_191_600_000;

fn json(text: &str) -> Value {
    Value::from_json(text).expect("JSON")
}

/// A start row at [`T0`] with `fields` after its `t`.
fn started(fields: &str) -> Value {
    json(&format!(r#"{{"t":"2026-10-16T23:00:00.000Z",{fields}}}"#))
}

fn at(seconds: i64) -> String {
    Timestamp::from_unix_millis(T0 + seconds * 1000).to_string()
}

fn mtime(ms: i64) -> ModTime {
    ModTime::from_unix_nanos(i128::from(ms) * 1_000_000)
}

/// The row's detail as `JSON.stringify` writes it.
fn row(
    start: &Value,
    progress: Option<(&Value, ModTime)>,
    beat: Option<&Value>,
    now: i64,
) -> String {
    let facts = LostGateFacts {
        start,
        progress: progress.map(|(record, modified)| ProgressSeen { record, modified }),
        heartbeat: beat,
        now: Timestamp::from_unix_millis(now),
    };
    let lost = lost_gate_row(&facts).expect("no throw");
    format!("{{{}}}", lost.json())
}

fn reason(
    start: &Value,
    progress: Option<(&Value, ModTime)>,
    beat: Option<&Value>,
    now: i64,
) -> LostReason {
    let facts = LostGateFacts {
        start,
        progress: progress.map(|(record, modified)| ProgressSeen { record, modified }),
        heartbeat: beat,
        now: Timestamp::from_unix_millis(now),
    };
    lost_gate_row(&facts).expect("no throw").reason()
}

fn beat(seconds: i64, pid: u32) -> Value {
    json(&format!(r#"{{"pid":{pid},"at":{}}}"#, T0 + seconds * 1000))
}

#[test]
fn timeout_near_the_deadline_interrupted_short_of_it() {
    let start = json(&format!(
        r#"{{"t":"{}","pid":7,"timeoutMs":1200000}}"#,
        at(0)
    ));
    let prog = json(&format!(
        r#"{{"steps":[{{"name":"fmt","ok":true,"ms":900}},{{"name":"scan","ok":true,"ms":40}}],"current":{{"name":"clippy","startedAt":"{}"}}}}"#,
        at(30)
    ));
    let seen = Some((&prog, mtime(T0 + 30_000)));

    assert_eq!(
        row(&start, seen, Some(&beat(1185, 7)), T0 + 5_000_000),
        r#"{"pid":7,"reason":"timeout","ranMs":1185000,"foundMs":5000000,"timeoutMs":1200000,"heartbeat":true,"progress":true,"step":"clippy","steps":{"fmt":900,"scan":40}}"#,
        "the heartbeat says how long the hook lived"
    );
    assert_eq!(
        reason(&start, seen, Some(&beat(400, 7)), T0 + 5_000_000),
        LostReason::Interrupted,
        "seen to stop short"
    );
    assert!(
        row(&start, seen, Some(&beat(1199, 8)), T0 + 600_000)
            .contains(r#""reason":"interrupted","ranMs":30000,"foundMs":600000,"timeoutMs":1200000,"heartbeat":false"#),
        "another process's heartbeat says nothing"
    );
    assert_eq!(
        reason(&start, seen, None, T0 + 1_300_000),
        LostReason::Unknown,
        "found late"
    );
    let late = Some((&prog, mtime(T0 + 1_190_000)));
    assert_eq!(
        reason(&start, late, None, T0 + 1_300_000),
        LostReason::Timeout,
        "mtime near it"
    );
    assert_eq!(
        row(&start, None, None, T0 + 1000),
        r#"{"pid":7,"reason":"interrupted","ranMs":0,"foundMs":1000,"timeoutMs":1200000,"heartbeat":false,"progress":false,"step":null,"steps":{}}"#,
        "no progress file: it died formatting"
    );
    let no_deadline = json(&format!(r#"{{"t":"{}","pid":7}}"#, at(0)));
    assert_eq!(
        reason(&no_deadline, None, Some(&beat(5, 7)), T0 + 1000),
        LostReason::Unknown,
        "no deadline recorded"
    );
    let later = json(&format!(
        r#"{{"t":"{}","pid":7,"timeoutMs":1200000,"afterMs":600000}}"#,
        at(0)
    ));
    assert!(
        row(&later, seen, Some(&beat(590, 7)), T0 + 5_000_000)
            .contains(r#""reason":"timeout","ranMs":590000,"foundMs":5000000,"timeoutMs":600000"#),
        "a worktree gated after others had what was left of the deadline"
    );
    assert_eq!(
        reason(&later, seen, Some(&beat(400, 7)), T0 + 5_000_000),
        LostReason::Interrupted,
        "short of the rest"
    );
}

/// Each expected row printed by 1.x's `lostGateRow` with the same arguments.
#[test]
fn odd_start_rows_answer_as_1x() {
    let other_beat = json(&format!(r#"{{"pid":8,"at":{}}}"#, T0 + 5));
    let rows = [
        row(&started(r#""pid":7"#), None, None, T0 + 1000),
        row(
            &started(r#""pid":7,"timeoutMs":"x""#),
            None,
            None,
            T0 + 1000,
        ),
        row(
            &started(r#""pid":7,"timeoutMs":1000,"afterMs":"y""#),
            None,
            None,
            T0 + 500,
        ),
        row(
            &started(r#""pid":7,"timeoutMs":100000,"afterMs":40000"#),
            None,
            Some(&other_beat),
            T0 + 70_000,
        ),
        row(
            &json(r#"{"t":"nope","pid":7,"timeoutMs":1200000}"#),
            None,
            None,
            T0,
        ),
    ];

    assert_eq!(
        rows,
        [
            r#"{"pid":7,"reason":"unknown","ranMs":0,"foundMs":1000,"timeoutMs":null,"heartbeat":false,"progress":false,"step":null,"steps":{}}"#,
            r#"{"pid":7,"reason":"unknown","ranMs":0,"foundMs":1000,"timeoutMs":"x","heartbeat":false,"progress":false,"step":null,"steps":{}}"#,
            r#"{"pid":7,"reason":"timeout","ranMs":0,"foundMs":500,"timeoutMs":1000,"heartbeat":false,"progress":false,"step":null,"steps":{}}"#,
            r#"{"pid":7,"reason":"unknown","ranMs":0,"foundMs":70000,"timeoutMs":60000,"heartbeat":false,"progress":false,"step":null,"steps":{}}"#,
            r#"{"pid":7,"reason":"unknown","ranMs":null,"foundMs":null,"timeoutMs":1200000,"heartbeat":false,"progress":false,"step":null,"steps":{}}"#,
        ],
        "1.x lostGateRow"
    );
}

/// Each expected row printed by 1.x's `lostGateRow` with the same arguments.
#[test]
fn odd_progress_files_and_heartbeats_answer_as_1x() {
    let odd_steps = json(
        r#"{"steps":[{"name":"b","ms":1},{"name":"a"},{"name":2,"ms":3},{"name":"b","ms":4},5,{"ms":6}],"current":{"name":null,"startedAt":2026}}"#,
    );
    let odd_mtime = ModTime::from_unix_nanos(1_792_191_601_234_567_800);
    let string_steps = json(r#"{"steps":"x","current":"c"}"#);
    let odd = Some((&odd_steps, odd_mtime));
    let rows = [
        row(
            &started(r#""pid":7,"timeoutMs":1200000"#),
            odd,
            None,
            T0 + 2000,
        ),
        row(
            &started(r#""timeoutMs":{}"#),
            Some((&string_steps, mtime(0))),
            Some(&json(r#"{"at":5}"#)),
            T0 + 2000,
        ),
        row(
            &started(r#""pid":7,"timeoutMs":null"#),
            None,
            Some(&json(r#"{"pid":7,"at":"x"}"#)),
            T0,
        ),
    ];

    assert_eq!(
        rows,
        [
            r#"{"pid":7,"reason":"interrupted","ranMs":1234.56787109375,"foundMs":2000,"timeoutMs":1200000,"heartbeat":false,"progress":true,"step":null,"steps":{"2":3,"b":4,"undefined":6}}"#,
            r#"{"reason":"unknown","ranMs":0,"foundMs":2000,"timeoutMs":{},"heartbeat":true,"progress":true,"step":null,"steps":{}}"#,
            r#"{"pid":7,"reason":"unknown","ranMs":0,"foundMs":0,"timeoutMs":null,"heartbeat":false,"progress":false,"step":null,"steps":{}}"#,
        ],
        "1.x lostGateRow"
    );
}

#[test]
fn the_claim_names_the_pid_and_the_start_time_as_1x() {
    let claims = [
        r#"{"t":"2026-10-16T23:00:00.000Z","pid":7}"#,
        r#"{"t":"nope","pid":1e21}"#,
        r#"{"pid":-0}"#,
        r#"{"t":null,"pid":12}"#,
    ]
    .map(|row| lost_gate_claim(&json(row)).expect("no throw"));

    assert_eq!(
        claims,
        [
            "gate-lost-7-1792191600000",
            "gate-lost-1e+21-0",
            "gate-lost-0-0",
            "gate-lost-12-0"
        ],
        "1.x's gate-lost-${{r.pid}}-${{Date.parse(r.t) || 0}}"
    );
    assert!(
        lost_gate_claim(&json(r#"{"t":{"toString":1},"pid":7}"#)).is_err(),
        "the template literal throws"
    );
}

#[test]
fn the_telemetry_row_writes_the_detail_after_the_event() {
    let start = started(r#""pid":7,"timeoutMs":1000"#);
    let facts = LostGateFacts {
        start: &start,
        progress: None,
        heartbeat: None,
        now: Timestamp::from_unix_millis(T0 + 500),
    };
    let event = TelemetryEvent::GateLost(lost_gate_row(&facts).expect("no throw"));
    let sid = Value::String("s1".to_owned());

    let line = TelemetryRow {
        at: Timestamp::from_unix_millis(T0),
        sid: Some(&sid),
        ccr: None,
        agent: None,
        event: &event,
    }
    .to_json();

    assert_eq!(
        line,
        r#"{"t":"2026-10-16T23:00:00.000Z","sid":"s1","agent":"main","event":"gate-lost","pid":7,"reason":"timeout","ranMs":0,"foundMs":500,"timeoutMs":1000,"heartbeat":false,"progress":false,"step":null,"steps":{}}"#,
        "1.x telemetry(proj, who, 'gate-lost', lostGateRow(…))"
    );
}

#[test]
fn a_null_step_and_a_null_heartbeat_read_throw_as_in_1x() {
    let null_step = json(r#"{"steps":[null]}"#);
    let facts = LostGateFacts {
        start: &json(r#"{"t":"x","pid":1}"#),
        progress: Some(ProgressSeen {
            record: &null_step,
            modified: mtime(0),
        }),
        heartbeat: None,
        now: Timestamp::from_unix_millis(0),
    };
    assert!(
        lost_gate_row(&facts).is_err(),
        "Cannot read properties of null (reading 'name')"
    );
    let no_pid = LostGateFacts {
        start: &json(r#"{"t":"x"}"#),
        progress: None,
        heartbeat: Some(&Value::Null),
        now: Timestamp::from_unix_millis(0),
    };
    assert!(
        lost_gate_row(&no_pid).is_err(),
        "Cannot read properties of null (reading 'at')"
    );
}

#[test]
fn a_gate_start_stays_open_until_an_outcome_or_gate_lost_of_the_same_agent_and_pid() {
    let rows: Vec<Value> = [
        r#"{"event":"gate-start","agent":"main","pid":1}"#,
        r#"{"event":"stop-green","agent":"main","pid":1}"#,
        r#"{"event":"gate-start","agent":"main","pid":2}"#,
        r#"{"event":"gate-start","agent":"temper:rust-implementer","pid":3}"#,
        r#"{"event":"stop-blocked","agent":"temper:rust-implementer","pid":4}"#,
        r#"{"event":"gate-start","agent":"main","pid":5}"#,
        r#"{"event":"gate-lost","agent":"main","pid":5}"#,
        r#"{"event":"gate-start","agent":"main"}"#,
        r#"{"event":"stop-advisory-unreviewed","agent":"main"}"#,
    ]
    .map(json)
    .into();

    let open = open_gate_starts(&rows).expect("no throw");

    assert_eq!(
        open.iter().map(|r| r.get("pid")).collect::<Vec<_>>(),
        [Some(&Value::Number(2.0)), Some(&Value::Number(3.0))],
        "1.x openGateStarts"
    );
}

#[test]
fn a_reopened_gate_moves_to_the_end_and_null_agents_are_main() {
    let rows: Vec<Value> = [
        r#"{"event":"gate-start","agent":"main","pid":1,"n":1}"#,
        r#"{"event":"gate-start","agent":null,"pid":2}"#,
        r#"{"event":"gate-start","pid":1,"n":2}"#,
        r#"{"event":"gate-lost","agent":"main","pid":2}"#,
        r#"{"event":"gate-start","agent":"main","pid":2,"n":3}"#,
        "5",
        r#"{"event":"gate-start","agent":{"toString":1}}"#,
    ]
    .map(json)
    .into();

    assert!(
        open_gate_starts(&rows).is_err(),
        "a toString key throws in the template literal"
    );
    let open = open_gate_starts(rows.get(..6).expect("six rows")).expect("no throw");
    assert_eq!(
        open.iter().map(|r| r.get("n")).collect::<Vec<_>>(),
        [Some(&Value::Number(2.0)), Some(&Value::Number(3.0))],
        "a set keeps its key's place; a reopened key goes last"
    );
}
