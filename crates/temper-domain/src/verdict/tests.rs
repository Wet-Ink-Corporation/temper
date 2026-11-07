//! Ports of `tests/verdict.test.mjs` (the domain half) and the verdict rows of
//! `tests/telemetry.test.mjs`, one module per 1.x test group, with JS-semantics rows whose
//! expected answers were printed by 1.x on the same JSON.

mod extract;
mod js_semantics;
mod problems;

use crate::js::Value;

use super::DIMENSIONS;

/// A JSON object written from `base` with `over` applied as JS's `{ ...base, ...over }`: a key
/// already in `base` keeps its place, a new key goes last, and the text `undefined` drops the
/// key, as `JSON.stringify` drops an `undefined` property.
fn object(base: &[(&str, String)], over: &[(&str, &str)]) -> String {
    let mut props: Vec<(String, String)> =
        base.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect();
    for (key, text) in over {
        match props.iter_mut().find(|(k, _)| k == key) {
            Some(prop) => prop.1 = (*text).to_owned(),
            None => props.push(((*key).to_owned(), (*text).to_owned())),
        }
    }
    let body: Vec<String> = props
        .iter()
        .filter(|(_, v)| v != "undefined")
        .map(|(k, v)| format!("{k:?}:{v}"))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// Every dimension scored 4, with `over` applied.
fn scores(over: &[(&str, &str)]) -> String {
    let base: Vec<(&str, String)> = DIMENSIONS.iter().map(|d| (*d, "4".to_owned())).collect();
    object(&base, over)
}

/// 1.x's `verdict(over)`: a well-formed approval as JSON text.
fn verdict_json(over: &[(&str, &str)]) -> String {
    let base = [
        ("temper_verdict", "1".to_owned()),
        ("verdict", r#""approved""#.to_owned()),
        ("fingerprint", r#""0123456789abcdef""#.to_owned()),
        ("findings", "[]".to_owned()),
        ("scores", scores(&[])),
        (
            "gate",
            r#"{"fmt":"pass","clippy":"pass","test":"pass","doc":"pass"}"#.to_owned(),
        ),
        (
            "evidence",
            r#"{"files_read":["crates/core/src/lib.rs"],"commands_run":["node temper.mjs gate"]}"#
                .to_owned(),
        ),
    ];
    object(&base, over)
}

/// 1.x's `verdict(over)` as a value.
fn verdict(over: &[(&str, &str)]) -> Value {
    Value::from_json(&verdict_json(over)).expect("the builder writes JSON")
}

/// 1.x's `finding(over)` as JSON text.
fn finding(over: &[(&str, &str)]) -> String {
    let base = [
        ("rule", r#""OWN-08""#.to_owned()),
        ("severity", r#""minor""#.to_owned()),
        ("file", r#""crates/core/src/lib.rs""#.to_owned()),
        ("line", "3".to_owned()),
        ("evidence", r#""x.clone()""#.to_owned()),
        ("fix", r#""borrow it""#.to_owned()),
    ];
    object(&base, over)
}
