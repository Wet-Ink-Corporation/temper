//! The verdict record 1.x's `recordVerdict` writes, `{ ...v, recorded_at, ...meta }`, the kept
//! record `{ ...rec, kept: 'approved' }` and the condition that keeps an approval (#136). The
//! expected texts were printed by Node 22 on the same objects.

use crate::Timestamp;
use crate::js::Value;

use super::super::{ExtractedVerdict, GateState, ValidVerdict, VerdictContext};
use super::super::{kept_record, verdict_record};
use super::{finding, verdict, verdict_json};

const NONE: VerdictContext<'static> = VerdictContext {
    changed_rust: &[],
    gate: GateState::NotRed,
};

/// 2026-11-08T10:00:00.000Z.
const AT: Timestamp = Timestamp::from_unix_millis(1_794_132_000_000);

fn valid(over: &[(&str, &str)]) -> ValidVerdict {
    ValidVerdict::check(ExtractedVerdict::Parsed(verdict(over)), &NONE).expect("valid")
}

fn json(text: &str) -> Value {
    Value::from_json(text).expect("JSON")
}

const BODY: &str = r#""temper_verdict":1,"verdict":"approved","fingerprint":"0123456789abcdef","findings":[],"scores":{"correctness":4,"escape_hatches":4,"ownership":4,"errors":4,"api":4,"architecture":4,"tests":4,"performance":4,"concurrency":4,"observability":4,"craft":4,"docs":4},"gate":{"fmt":"pass","clippy":"pass","test":"pass","doc":"pass"},"evidence":{"files_read":["crates/core/src/lib.rs"],"commands_run":["node temper.mjs gate"]}"#;

#[test]
fn the_record_spreads_the_verdict_then_the_time_and_the_meta_as_1x_does() {
    let v = valid(&[
        ("agent_id", r#""mine""#),
        ("recorded_at", r#""old""#),
        ("7", "true"),
        ("kept", r#""x""#),
    ]);
    // An absent agent id is `agent_id: undefined`: the verdict's own key is dropped.
    let rec = verdict_record(&v, AT, None, false);
    assert_eq!(
        rec.to_json(),
        format!(
            r#"{{"7":true,{BODY},"recorded_at":"2026-11-08T10:00:00.000Z","kept":"x","stale":false}}"#
        )
    );
    assert_eq!(
        kept_record(rec).to_json(),
        format!(
            r#"{{"7":true,{BODY},"recorded_at":"2026-11-08T10:00:00.000Z","kept":"approved","stale":false}}"#
        )
    );
    let id = json(r#"{"a":[1]}"#);
    assert_eq!(
        verdict_record(&valid(&[]), AT, Some(&id), true).to_json(),
        format!(
            r#"{{{BODY},"recorded_at":"2026-11-08T10:00:00.000Z","agent_id":{{"a":[1]}},"stale":true}}"#
        )
    );
}

#[test]
fn the_pretty_form_is_json_stringify_with_two_spaces() {
    let value = json(
        r#"{"2":[],"10":1,"temper_verdict":1,"e":{},"n":[1,{"a":-0}],"s":" \"","big":1e21,"z":null}"#,
    );
    assert_eq!(
        value.to_json_pretty(),
        "{\n  \"2\": [],\n  \"10\": 1,\n  \"temper_verdict\": 1,\n  \"e\": {},\n  \"n\": [\n    1,\n    {\n      \"a\": 0\n    }\n  ],\n  \"s\": \"\u{2028}\\\"\",\n  \"big\": 1e+21,\n  \"z\": null\n}"
    );
    for (text, want) in [("[]", "[]"), ("{}", "{}"), ("5", "5"), (r#""x""#, r#""x""#)] {
        assert_eq!(json(text).to_json_pretty(), want, "{text}");
    }
}

#[test]
fn a_changes_request_without_a_reason_to_overturn_keeps_a_fresh_approval() {
    let minor = format!(
        "[{},{}]",
        finding(&[]),
        finding(&[("severity", r#""nit""#)])
    );
    let cr = |over: &[(&str, &str)]| {
        let mut all = vec![("verdict", r#""changes-requested""#)];
        all.extend_from_slice(over);
        valid(&all)
    };
    let approval = json(&verdict_json(&[("stale", "false")]));
    let keeps =
        |v: &ValidVerdict, prev: Option<&Value>| v.keeps_approval_over(prev).expect("no throw");
    assert!(keeps(&cr(&[("findings", &minor)]), Some(&approval)));
    assert!(keeps(&cr(&[]), Some(&approval)));
    // "n/a" scores and gate entries are no reason to overturn
    let scores = super::scores(&[("docs", r#""n/a""#)]);
    let na = cr(&[
        ("scores", &scores),
        (
            "gate",
            r#"{"fmt":"pass","clippy":"pass","test":"pass","doc":"n/a"}"#,
        ),
    ]);
    assert!(keeps(&na, Some(&approval)));
    // a major finding, a score below 3 or a failing gate entry overturns it
    let major = format!("[{}]", finding(&[("severity", r#""major""#)]));
    let low = super::scores(&[("tests", "2")]);
    for over in [
        [("findings", major.as_str())],
        [("scores", low.as_str())],
        [(
            "gate",
            r#"{"fmt":"pass","clippy":"fail","test":"pass","doc":"pass"}"#,
        )],
        [("gate", r#""ab""#)],
    ] {
        assert!(!keeps(&cr(&over), Some(&approval)), "{over:?}");
    }
    // only a fresh approval is kept, and only by a changes request
    for prev in [
        None,
        Some(json("null")),
        Some(json(r#""approved""#)),
        Some(json(&verdict_json(&[("stale", "true")]))),
        Some(json(&verdict_json(&[("stale", r#""x""#)]))),
        Some(json(&verdict_json(&[(
            "verdict",
            r#""changes-requested""#,
        )]))),
    ] {
        assert!(!keeps(&cr(&[]), prev.as_ref()), "{prev:?}");
    }
    assert!(keeps(
        &cr(&[]),
        Some(&json(&verdict_json(&[("stale", "0")])))
    ));
    assert!(!keeps(&valid(&[]), Some(&approval)));
}
