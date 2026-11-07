//! JS semantics in `verdictProblems`: strict equality on numbers (`1.0 === 1`), `Number.isInteger`,
//! `String(x)`, truthiness, `typeof`, `Object.values` of a string, `??`, and `JSON.parse`'s own
//! `__proto__` and repeated keys.
//!
//! `js_rows.json` holds 1.x's answers: each row's verdict text, changed files and the
//! `[kind, message]` pairs that `verdictProblems` returned under Node 22, or `null` where it
//! threw. The rows were printed by the local probe recorded in the Phase 3 plan's Progress.

use crate::js::Value;

use super::super::{ExtractedVerdict, GateState, VerdictContext, verdict_problems};

const ROWS: &str = include_str!("js_rows.json");

#[test]
fn each_row_gets_the_problems_1x_gives_or_fails_where_1x_throws() {
    let rows = Value::from_json(ROWS).expect("the fixture is JSON");
    let Value::Array(rows) = rows else {
        panic!("the fixture is an array");
    };
    assert_eq!(rows.len(), 39, "every row is read");
    for row in &rows {
        let name = row.get("name").and_then(Value::as_str).expect("a name");
        let text = row.get("text").and_then(Value::as_str).expect("a text");
        let changed: Vec<String> = match row.get("changed") {
            Some(Value::Array(files)) => files
                .iter()
                .map(|f| f.as_str().expect("a path").to_owned())
                .collect(),
            _ => panic!("{name}: changed is an array"),
        };
        let expected: Option<Vec<(String, String)>> = match row.get("problems") {
            Some(Value::Array(pairs)) => Some(
                pairs
                    .iter()
                    .map(|pair| match pair {
                        Value::Array(kv) => match kv.as_slice() {
                            [Value::String(k), Value::String(m)] => (k.clone(), m.clone()),
                            _ => panic!("{name}: a [kind, message] pair"),
                        },
                        _ => panic!("{name}: a pair"),
                    })
                    .collect(),
            ),
            Some(Value::Null) => None,
            _ => panic!("{name}: problems is an array or null"),
        };
        let verdict = ExtractedVerdict::Parsed(Value::from_json(text).expect(name));
        let ctx = VerdictContext {
            changed_rust: &changed,
            gate: GateState::NotRed,
        };
        let actual = verdict_problems(Some(&verdict), &ctx).ok().map(|problems| {
            problems
                .iter()
                .map(|p| (p.kind().name().to_owned(), p.message().to_owned()))
                .collect::<Vec<_>>()
        });
        assert_eq!(actual, expected, "{name}");
    }
}
