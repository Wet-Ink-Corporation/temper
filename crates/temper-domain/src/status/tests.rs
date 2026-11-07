use std::str::FromStr;

use super::{
    BaselineLine, STATUS_USAGE, StatusFacts, baseline_line, render_status_text, temper_base_note,
};
use crate::js::Value;
use crate::state::{BaselineSummary, GateRecord};
use crate::{Config, ConfigProblem, Fingerprint};

fn rows(file: &str) -> Value {
    Value::from_json(file).expect("the rows file is JSON")
}

fn fp() -> Fingerprint {
    Fingerprint::from_str("0123456789abcdef").expect("16 hex")
}

/// The facts of 1.x's `rows.clean` in `.temper/probe3b/status-text.mjs`, changed by `edit`.
fn render<'a>(edit: impl FnOnce(&mut StatusFacts<'a>)) -> String {
    render_with(&DEFAULTS, edit)
}

static DEFAULTS: std::sync::LazyLock<Config> = std::sync::LazyLock::new(Config::defaults);

fn render_with<'a>(config: &'a Config, edit: impl FnOnce(&mut StatusFacts<'a>)) -> String {
    let mut facts = StatusFacts {
        root: "/w",
        state_dir: "/w/.git/temper",
        base: None,
        fingerprint: fp(),
        changed: &[],
        gate: None,
        review: None,
        baseline: BaselineLine::Absent,
        config,
        config_problem: None,
    };
    edit(&mut facts);
    render_status_text(&facts).expect("1.x prints these facts")
}

fn expected(name: &str) -> String {
    let all = rows(include_str!("text_rows.json"));
    all.get(name)
        .and_then(Value::as_str)
        .map_or_else(|| panic!("row {name}"), str::to_owned)
}

fn gate(json: &str) -> GateRecord {
    GateRecord::from_json(json).expect("JSON")
}

fn record(json: &str) -> Value {
    Value::from_json(json).expect("JSON")
}

#[test]
fn a_clean_tree_and_a_changed_one_print_1xs_lines() {
    let changed = ["src/a.rs".to_owned(), ".claude/temper.json".to_owned()];
    assert_eq!(render(|_| {}), expected("clean"));
    assert_eq!(
        render(|f| {
            f.base = Some("abc");
            f.changed = &changed;
        }),
        expected("changed")
    );
    assert_eq!(render(|f| f.base = Some("")), expected("empty_base"));
}

/// `g ? (g.ok ? 'green' : 'red') : …` on any JSON the gate file holds.
#[test]
fn the_gate_line_reads_the_record_as_1x_does() {
    for (json, row) in [
        (r#"{"ok":true}"#, "gate_green"),
        (r#"{"ok":false}"#, "gate_red"),
        (r#""x""#, "gate_x"),
        ("0", "gate_zero"),
        (r#"{"ok":"no"}"#, "gate_ok_string"),
    ] {
        let record = gate(json);
        assert_eq!(render(|f| f.gate = Some(&record)), expected(row), "{json}");
    }
}

/// `${v.verdict}${v.stale ? ' (stale)' : ''}`: a record of `[1]` prints `undefined`.
#[test]
fn the_review_line_prints_the_verdict_as_a_template_literal_does() {
    for (json, row) in [
        (r#"{"verdict":"approved"}"#, "review_approved"),
        (
            r#"{"verdict":"changes-requested","stale":1}"#,
            "review_stale",
        ),
        ("[1]", "review_array"),
        (r#"{"verdict":null,"stale":0}"#, "review_null_verdict"),
        (r#"{"verdict":["a",null,"b"],"stale":""}"#, "review_obj"),
    ] {
        let value = record(json);
        assert_eq!(render(|f| f.review = Some(&value)), expected(row), "{json}");
    }
}

/// A verdict 1.x's template literal cannot convert (an object with its own `toString` key,
/// also inside an array) throws outside any `try` in 1.x, so status fails; a `valueOf` key
/// alone falls back to `[object Object]`.
#[test]
fn a_verdict_1x_cannot_convert_fails_the_status() {
    for json in [
        r#"{"verdict":{"toString":1}}"#,
        r#"{"verdict":[null,[{"toString":"x"}]]}"#,
    ] {
        let value = record(json);
        let mut facts = StatusFacts {
            root: "/w",
            state_dir: "/w/.git/temper",
            base: None,
            fingerprint: fp(),
            changed: &[],
            gate: None,
            review: Some(&value),
            baseline: BaselineLine::Absent,
            config: &DEFAULTS,
            config_problem: None,
        };
        assert!(render_status_text(&facts).is_err(), "{json}");
        let fine = record(r#"{"verdict":{"valueOf":1}}"#);
        facts.review = Some(&fine);
        let text = render_status_text(&facts).expect("[object Object]");
        assert!(text.contains("\nreview:      [object Object]\n"), "{text}");
    }
}

#[test]
fn the_baseline_and_config_lines() {
    let read = BaselineSummary::parse(
        r#"{"temper_baseline":1,"totals":{"a":{"x":"2"}},"toolchain":"1.96.0"}"#,
    )
    .expect("a baseline");
    let bad = BaselineSummary::parse("[]").expect_err("not a baseline");
    let config = Config::load(Some(r#"{"review":"advisory","skillGate":false}"#)).config;
    assert_eq!(
        render(|f| f.baseline = BaselineLine::Read(&read)),
        expected("baseline_read")
    );
    assert_eq!(
        render(|f| f.baseline = BaselineLine::Unreadable(&bad)),
        expected("baseline_bad")
    );
    assert_eq!(render_with(&config, |_| {}), expected("advisory"));
}

/// OC-5 A (wi-018023): the D16(d) notice goes on the `config:` line after `; `, so the nine
/// lines stay.
#[test]
fn a_config_problem_is_appended_to_the_config_line() {
    let problem = ConfigProblem::NotAnObject;
    let text = render(|f| f.config_problem = Some(&problem));
    assert_eq!(
        text.lines().last(),
        Some(
            "config:      review=required skillGate=true; config unreadable (not a JSON object): defaults in use"
        )
    );
    assert_eq!(text.lines().count(), 9);
}

/// Each row's `line` was printed by 1.x (`.temper/probe3b/baseline.mjs`): `parseBaseline`'s
/// three refusals, `baselineTotal`'s JS `+` (numbers add, a string concatenates, arrays join,
/// an object is `[object Object]`, `null` throws V8's `TypeError`), and `toolchain ?? '?'`.
/// A count or toolchain with its own `toString` key, also inside an array, throws V8's
/// `Cannot convert object to primitive value` inside 1.x's `try`.
#[test]
fn every_baseline_row_prints_1xs_line() {
    let Value::Array(all) = rows(include_str!("baseline_rows.json")) else {
        panic!("an array");
    };
    assert_eq!(all.len(), 28, "rows");
    for row in &all {
        let text = row.get("text").and_then(Value::as_str).expect("text");
        let line = row.get("line").and_then(Value::as_str).expect("line");
        let parsed = BaselineSummary::parse(text);
        let got = match &parsed {
            Ok(summary) => baseline_line(&BaselineLine::Read(summary)),
            Err(error) => baseline_line(&BaselineLine::Unreadable(error)),
        };
        assert_eq!(got, line, "{text}");
    }
}

#[test]
fn not_json_names_the_file_and_the_parsers_detail() {
    let error = BaselineSummary::parse("{").expect_err("not JSON");
    let line = baseline_line(&BaselineLine::Unreadable(&error));
    assert!(
        line.starts_with("unreadable: .claude/temper-baseline.json is not valid JSON ("),
        "{line}"
    );
    assert_eq!(
        baseline_line(&BaselineLine::Absent),
        "none: the gate requires zero warnings"
    );
}

/// 1.x `env.slice(0, 12)`, per code point (D16(o)).
#[test]
fn the_temper_base_note_shows_twelve_units_of_the_base() {
    assert_eq!(
        temper_base_note("0123456789abcdef"),
        "temper: TEMPER_BASE 0123456789ab is not a commit in this repository (another session's base?); using HEAD.\n"
    );
    assert_eq!(
        temper_base_note("01234567890\u{1F600}x"),
        "temper: TEMPER_BASE 01234567890 is not a commit in this repository (another session's base?); using HEAD.\n"
    );
    assert_eq!(
        STATUS_USAGE,
        "usage: temper status [--json] [--session ID]\n"
    );
}
