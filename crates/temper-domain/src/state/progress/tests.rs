use super::{ProgressQuery, gate_progress};
use crate::js::Value;
use crate::{Fingerprint, Timestamp};

const FP: &str = "0123456789abcdef";
/// The live pid the rows name as `$PID`.
const PID: f64 = 4242.0;

fn fp(text: &str) -> Fingerprint {
    text.parse().expect("a fingerprint")
}

/// 2026-10-11T12:00:00.000Z, the `now` of the rows.
fn now() -> Timestamp {
    Timestamp::parse_js("2026-10-11T12:00:00.000Z").expect("an ISO time")
}

/// The query for `fingerprint` at [`now`] with the step timeout `step`.
fn query(fingerprint: &str, step: f64) -> ProgressQuery {
    ProgressQuery {
        fp: fp(fingerprint),
        now: now(),
        step_timeout_sec: step,
    }
}

fn alive(pid: &Value) -> bool {
    *pid == Value::Number(PID)
}

/// 1.x's answer to one row, as `rows.json` records it.
fn answer(text: &str, step: f64) -> String {
    match gate_progress(text, &query(FP, step), alive) {
        Ok(Some(progress)) => progress.to_json().replace("4242", "$PID"),
        Ok(None) => "null".to_owned(),
        Err(_) => "throws".to_owned(),
    }
}

/// Each row's answer printed by 1.x's `gateProgress` (`.temper/probe3c/progress-rows.mjs`,
/// Node 22), with a live pid as `$PID`: JS property reads, `??`, `Date.parse` of any value,
/// the key left out when `startedAt` is, and the step timeout plus 300 s.
#[test]
fn gate_progress_answers_as_1x_on_every_row() {
    let rows = Value::from_json(include_str!("rows.json")).expect("the rows file is JSON");
    let Value::Array(rows) = rows else {
        panic!("the rows file is an array");
    };
    let text_of = |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);

    let mismatches: Vec<(String, String, String)> = rows
        .iter()
        .filter_map(|row| {
            let name = text_of(row, "name").expect("a name");
            let text = text_of(row, "text")
                .expect("a text")
                .replace("$PID", "4242");
            let Some(Value::Number(step)) = row.get("step") else {
                panic!("{name}: a step timeout");
            };
            let want = match row.get("want") {
                Some(Value::String(json)) => json.clone(),
                Some(Value::Null) => "null".to_owned(),
                _ => "throws".to_owned(),
            };
            let got = answer(&text, *step);
            (got != want).then_some((name, want, got))
        })
        .collect();

    assert_eq!(rows.len(), 42, "every row ran");
    assert_eq!(mismatches, vec![], "(row, 1.x, Rust)");
}

/// The ported 1.x test "gate progress: a file whose process is gone, or whose step is older
/// than the step timeout, is stale".
#[test]
fn a_progress_file_whose_process_is_gone_or_whose_step_is_too_old_is_stale() {
    let at = |ms_ago: i64| Timestamp::from_unix_millis(now().unix_millis() - ms_ago).to_string();
    let file = |pid: f64, started: i64, current: &str, current_at: i64| {
        format!(
            r#"{{"fp":"{FP}","pid":{pid},"startedAt":"{}","steps":[],"current":{{"name":"{current}","startedAt":"{}"}}}}"#,
            at(started),
            at(current_at)
        )
    };
    let read = |text: &str, fingerprint: &str| {
        gate_progress(text, &query(fingerprint, 900.0), alive).expect("no throw")
    };

    let running = read(&file(PID, 60_000, "clippy", 5000), FP);
    let other_tree = read(&file(PID, 60_000, "clippy", 5000), "fedcba9876543210");
    let outlived = read(&file(PID, 2_000_000, "test", 1_300_000), FP);
    let in_margin = read(&file(PID, 2_000_000, "clippy", 1_000_000), FP);
    let gone = read(&file(2_147_483_646.0, 1000, "test", 1000), FP);
    let not_json = read("{ not json", FP);

    let want = format!(
        r#"{{"startedAt":"{}","steps":[],"current":{{"name":"clippy","startedAt":"{}"}}}}"#,
        at(60_000),
        at(5000)
    );
    assert_eq!(
        running.map(|p| p.to_json()),
        Some(want),
        "a gate running now"
    );
    assert_eq!(other_tree, None, "another fingerprint has no gate running");
    assert_eq!(
        outlived, None,
        "the current step outlived the step timeout and its margin"
    );
    assert!(
        in_margin.is_some(),
        "a ratchet step within the margin is still running"
    );
    assert_eq!(gone, None, "the gate process is gone");
    assert_eq!(not_json, None, "not JSON");
}

/// OC-3 A: a start in local time (no offset) reads as NaN, so the file is stale, where V8
/// reads the time in the host's zone.
#[test]
fn a_start_in_local_time_reads_as_stale() {
    let text = format!(r#"{{"fp":"{FP}","pid":4242,"startedAt":"2026-10-11T11:59:55.000"}}"#);

    let progress = gate_progress(&text, &query(FP, 900.0), alive);

    assert_eq!(progress.ok(), Some(None));
}

/// The pid is asked about only when the file is for this fingerprint, as 1.x's `||` orders
/// the checks, so a probe runs only when its answer matters.
#[test]
fn the_pid_is_asked_about_only_for_this_fingerprint() {
    let asked = std::cell::Cell::new(0);
    let counting = |pid: &Value| {
        asked.set(asked.get() + 1);
        alive(pid)
    };
    let other = r#"{"fp":"fedcba9876543210","pid":4242,"startedAt":"2026-10-11T11:59:55.000Z"}"#;

    let progress = gate_progress(other, &query(FP, 900.0), counting);

    assert_eq!((progress.ok(), asked.get()), (Some(None), 0));
}
