use super::{StatusDoc, render_no_project_json, render_status_json};
use crate::js::Value;
use crate::state::{BaselineParseError, BaselineSummary, GateRecord};
use crate::status::{BaselineLine, StatusFacts};
use crate::{Config, ConfigLoad};

/// The D16(a) detail of a baseline that is not JSON, which differs from V8's by design.
fn mask_detail(json: &str) -> String {
    match (
        json.find(" is not valid JSON ("),
        json.rfind(")\"},\"config\""),
    ) {
        (Some(start), Some(end)) if start < end => {
            let head = json.get(..start).unwrap_or_default();
            let tail = json.get(end..).unwrap_or_default();
            format!("{head} is not valid JSON (<detail>{tail}")
        }
        _ => json.to_owned(),
    }
}

/// One row of `rows.json` rendered by Rust.
fn render_row(row: &Value) -> String {
    let text = |key: &str| row.get(key).and_then(Value::as_str);
    let gate =
        text("gate").map(|json| GateRecord::from_json(json).expect("the row's gate is JSON"));
    let review = text("verdict").map(|json| Value::from_json(json).expect("JSON"));
    let baseline: Option<Result<BaselineSummary, BaselineParseError>> =
        text("baseline").map(BaselineSummary::parse);
    let mut config_text = String::from("{");
    if let Some(review) = text("review") {
        config_text.push_str(r#""review":""#);
        config_text.push_str(review);
        config_text.push_str(r#"","#);
    }
    if row.get("skillGate") == Some(&Value::Bool(false)) {
        config_text.push_str(r#""skillGate":false,"#);
    }
    config_text.push_str(r#""x":0}"#);
    let ConfigLoad { config, problem } = Config::load(Some(&config_text));
    assert_eq!(problem, None, "the row's config reads");
    let changed: Vec<String> = match row.get("changed") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|c| c.as_str().expect("a path").to_owned())
            .collect(),
        _ => Vec::new(),
    };
    let facts = StatusFacts {
        root: text("root").unwrap_or("/w"),
        state_dir: text("stateDir").unwrap_or("/w/.git/temper"),
        base: text("base"),
        fingerprint: "0123456789abcdef".parse().expect("a fingerprint"),
        changed: &changed,
        gate: gate.as_ref(),
        review: review.as_ref(),
        baseline: match &baseline {
            None => BaselineLine::Absent,
            Some(Ok(summary)) => BaselineLine::Read(summary),
            Some(Err(error)) => BaselineLine::Unreadable(error),
        },
        config: &config,
        config_problem: None,
    };
    let doc = StatusDoc {
        version: "1.6.0",
        running: row.get("running").filter(|r| **r != Value::Null),
        disabled: row.get("disabled") == Some(&Value::Bool(true)),
    };
    render_status_json(&facts, &doc)
}

/// Each row's document as 1.x's `statusJson` object literal writes it
/// (`.temper/probe3c/statusjson-rows.mjs`, Node 22): key order, `undefined` keys left out,
/// `??` against `||`, `Boolean()`, entries that are not records dropped, and JS's
/// `JSON.stringify` of the values copied from the files.
#[test]
fn render_status_json_writes_1xs_document_for_every_row() {
    let rows = Value::from_json(include_str!("rows.json")).expect("the rows file is JSON");
    let Value::Array(rows) = rows else {
        panic!("the rows file is an array");
    };

    let mismatches: Vec<(String, String, String)> = rows
        .iter()
        .filter_map(|row| {
            let name = row.get("name").and_then(Value::as_str).expect("a name");
            let want = row.get("want").and_then(Value::as_str).expect("a document");
            let got = render_row(row);
            (got != want).then(|| (name.to_owned(), want.to_owned(), got))
        })
        .collect();

    assert_eq!(rows.len(), 23, "every row ran");
    assert_eq!(mismatches, vec![], "(row, 1.x, Rust)");
}

#[test]
fn a_baseline_that_is_not_json_keeps_1xs_message_around_the_detail() {
    let error = BaselineSummary::parse("{ not json").expect_err("not JSON");
    let config = Config::defaults();
    let facts = StatusFacts {
        root: "/w",
        state_dir: "/w/.git/temper",
        base: None,
        fingerprint: "0123456789abcdef".parse().expect("a fingerprint"),
        changed: &[],
        gate: None,
        review: None,
        baseline: BaselineLine::Unreadable(&error),
        config: &config,
        config_problem: None,
    };
    let doc = StatusDoc {
        version: "1.6.0",
        running: None,
        disabled: false,
    };

    let json = mask_detail(&render_status_json(&facts, &doc));

    let tail = r#""baseline":{"error":".claude/temper-baseline.json is not valid JSON (<detail>)"},"config":{"review":"required","skillGate":true}}"#;
    assert!(json.ends_with(&format!("{tail}\n")), "{json}");
}

/// OC-5 A (wi-018023): `config.problem` follows `skillGate` only when the config file has a
/// problem; the rest of the document, `temper_status: 1` included, is unchanged.
#[test]
fn a_config_problem_adds_config_problem_after_skill_gate() {
    let ConfigLoad { config, problem } = Config::load(Some(r#"{"skillGate":"no"}"#));
    let problem = problem.expect("a wrong-typed key is a problem");
    let facts = |config_problem| StatusFacts {
        root: "/w",
        state_dir: "/w/.git/temper",
        base: None,
        fingerprint: "0123456789abcdef".parse().expect("a fingerprint"),
        changed: &[],
        gate: None,
        review: None,
        baseline: BaselineLine::Absent,
        config: &config,
        config_problem,
    };
    let doc = StatusDoc {
        version: "1.6.0",
        running: None,
        disabled: true,
    };

    let with = render_status_json(&facts(Some(&problem)), &doc);
    let without = render_status_json(&facts(None), &doc);

    let config = r#""config":{"review":"required","skillGate":true}"#;
    let with_problem = format!(
        r#""config":{{"review":"required","skillGate":true,"problem":{}}}"#,
        crate::js::json_string(&problem.to_string())
    );
    assert_eq!(with, without.replace(config, &with_problem));
    assert!(without.contains(config), "{without}");
    assert!(with.starts_with(r#"{"temper_status":1,"#), "{with}");
}

#[test]
fn outside_a_project_the_document_says_project_null() {
    assert_eq!(
        render_no_project_json("1.6.0"),
        "{\"temper_status\":1,\"version\":\"1.6.0\",\"project\":null}\n"
    );
}
