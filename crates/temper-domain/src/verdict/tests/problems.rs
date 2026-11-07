//! `validateVerdict` and `verdictProblems`: one test per 1.x test, with the exact messages.

use crate::js::Value;

use super::super::{
    ExtractedVerdict, GateState, ProblemKind, VerdictContext, problem_kinds, verdict_problems,
};
use super::{finding, scores, verdict, verdict_json};

/// The problem messages for `v` against `changed` and `gate`.
fn check(v: &Value, changed: &[&str], gate: GateState) -> Vec<String> {
    let changed: Vec<String> = changed.iter().map(|f| (*f).to_owned()).collect();
    let ctx = VerdictContext {
        changed_rust: &changed,
        gate,
    };
    verdict_problems(Some(&ExtractedVerdict::Parsed(v.clone())), &ctx)
        .expect("1.x does not throw on this verdict")
        .iter()
        .map(|p| p.message().to_owned())
        .collect()
}

/// The problem messages for `v` with no changes and no red gate.
fn messages(v: &Value) -> Vec<String> {
    check(v, &[], GateState::NotRed)
}

/// Asserts that `problems` has `expected` among them.
fn has(problems: &[String], expected: &str) {
    assert!(
        problems.iter().any(|p| p == expected),
        "expected {expected:?}, got:\n{}",
        problems.join("\n")
    );
}

/// Asserts that some problem starts with `prefix`.
fn has_prefix(problems: &[String], prefix: &str) {
    assert!(
        problems.iter().any(|p| p.starts_with(prefix)),
        "expected a problem starting {prefix:?}, got:\n{}",
        problems.join("\n")
    );
}

#[test]
fn a_well_formed_approval_has_no_problems() {
    assert_eq!(messages(&verdict(&[])), Vec::<String>::new());
    assert_eq!(
        check(&verdict(&[]), &["crates/core/src/lib.rs"], GateState::NotRed),
        Vec::<String>::new()
    );
    // minor and nit findings, a "none" nit, and an "n/a" dimension or gate entry are consistent
    let findings = format!(
        "[{},{}]",
        finding(&[]),
        finding(&[("rule", r#""none""#), ("severity", r#""nit""#)])
    );
    let v = verdict(&[
        ("scores", &scores(&[("concurrency", r#""n/a""#)])),
        ("gate", r#"{"fmt":"pass","doc":"n/a"}"#),
        ("findings", &findings),
    ]);
    assert_eq!(messages(&v), Vec::<String>::new());
    // a changes-requested verdict may carry blockers and low scores
    let cr = verdict(&[
        ("verdict", r#""changes-requested""#),
        ("findings", &format!("[{}]", finding(&[("severity", r#""blocker""#)]))),
        ("scores", &scores(&[("errors", "1")])),
        ("gate", r#"{"clippy":"fail"}"#),
    ]);
    assert_eq!(check(&cr, &[], GateState::Red), Vec::<String>::new());
}

#[test]
fn no_verdict_block() {
    let ctx = VerdictContext {
        changed_rust: &[],
        gate: GateState::NotRed,
    };
    let problems = verdict_problems(None, &ctx).expect("1.x does not throw");
    let messages: Vec<&str> = problems.iter().map(|p| p.message()).collect();
    assert_eq!(
        messages,
        [
            "no verdict block found: end the final report with a ```json block containing \"temper_verdict\": 1"
        ]
    );
}

#[test]
fn json_that_does_not_parse_is_one_problem() {
    let ctx = VerdictContext {
        changed_rust: &[],
        gate: GateState::NotRed,
    };
    let bad = ExtractedVerdict::Unparseable {
        message: "expected value at line 1 column 31".to_owned(),
    };
    let problems = verdict_problems(Some(&bad), &ctx).expect("1.x does not throw");
    let messages: Vec<&str> = problems.iter().map(|p| p.message()).collect();
    assert_eq!(
        messages,
        ["verdict JSON does not parse: expected value at line 1 column 31"]
    );
}

#[test]
fn header_fields() {
    has(
        &messages(&verdict(&[("temper_verdict", "2")])),
        r#""temper_verdict" must be 1"#,
    );
    has(
        &messages(&verdict(&[("verdict", r#""lgtm""#)])),
        r#""verdict" must be "approved" or "changes-requested""#,
    );
    for fingerprint in [
        "undefined",
        r#""abc""#,
        r#""0123456789ABCDEF""#,
        r#""0123456789abcdef0""#,
        "1234",
    ] {
        has_prefix(
            &messages(&verdict(&[("fingerprint", fingerprint)])),
            r#""fingerprint" must be the 16-hex fingerprint"#,
        );
    }
    has(
        &messages(&verdict(&[("findings", r#""none""#)])),
        r#""findings" must be an array (empty if none)"#,
    );
}

#[test]
fn each_finding_needs_a_rule_id_a_severity_a_file_evidence_and_a_fix() {
    let one = |over: &[(&str, &str)]| format!("[{}]", finding(over));
    has(
        &messages(&verdict(&[
            ("verdict", r#""changes-requested""#),
            ("findings", "[null]"),
        ])),
        "findings[0] must be an object",
    );
    has_prefix(
        &messages(&verdict(&[("findings", &one(&[("rule", r#""own-8""#)]))])),
        "findings[0].rule must be a rule ID like OWN-08",
    );
    has_prefix(
        &messages(&verdict(&[("findings", &one(&[("rule", r#""none""#)]))])),
        "findings[0].rule must be a rule ID",
    );
    has(
        &messages(&verdict(&[(
            "findings",
            &one(&[("severity", r#""critical""#)]),
        )])),
        "findings[0].severity must be one of blocker/major/minor/nit",
    );
    has(
        &messages(&verdict(&[("findings", &one(&[("file", r#""""#)]))])),
        "findings[0].file is required",
    );
    let second = format!("[{},{}]", finding(&[]), finding(&[("evidence", "undefined")]));
    has(
        &messages(&verdict(&[("findings", &second)])),
        "findings[1].evidence is required (quote the code)",
    );
    has(
        &messages(&verdict(&[("findings", &one(&[("fix", r#""""#)]))])),
        "findings[0].fix is required",
    );
}

#[test]
fn every_dimension_needs_a_score_of_0_to_4_or_na() {
    has(
        &messages(&verdict(&[("scores", "undefined")])),
        r#""scores" object is required"#,
    );
    has(
        &messages(&verdict(&[("scores", &scores(&[("docs", "undefined")]))])),
        r#"scores.docs is missing (0-4 or "n/a")"#,
    );
    for bad in ["5", "-1", "2.5", r#""4""#, r#""na""#] {
        has(
            &messages(&verdict(&[("scores", &scores(&[("api", bad)]))])),
            r#"scores.api must be 0-4 or "n/a""#,
        );
    }
}

#[test]
fn evidence_lists_the_files_read_and_the_commands_run() {
    has(
        &messages(&verdict(&[(
            "evidence",
            r#"{"files_read":[],"commands_run":["gate"]}"#,
        )])),
        "evidence.files_read must list the files you actually read",
    );
    has(
        &messages(&verdict(&[(
            "evidence",
            r#"{"files_read":["a.rs"],"commands_run":[]}"#,
        )])),
        "evidence.commands_run must list the commands you ran (at least the gate)",
    );
    let problems = messages(&verdict(&[("evidence", "undefined")]));
    has_prefix(&problems, "evidence.files_read");
    has_prefix(&problems, "evidence.commands_run");
}

#[test]
fn a_changed_rs_file_missing_from_files_read_is_reported() {
    let changed = [
        "crates/core/src/lib.rs",
        "crates/core/src/new.rs",
        "crates/core/Cargo.toml",
    ];
    assert_eq!(
        check(&verdict(&[]), &changed, GateState::NotRed),
        ["changed files not reviewed: crates/core/src/new.rs"]
    );
    // Windows separators in files_read compare equal
    let win = verdict(&[(
        "evidence",
        r#"{"files_read":["crates\\core\\src\\lib.rs","crates/core/src/new.rs"],"commands_run":["gate"]}"#,
    )]);
    assert_eq!(check(&win, &changed, GateState::NotRed), Vec::<String>::new());
}

/// The problems of a verdict that read `files_read` against `changed`.
fn read(changed: &[&str], files_read: &[&str]) -> Vec<String> {
    let list: Vec<String> = files_read.iter().map(|f| format!("{f:?}")).collect();
    let evidence = format!(
        r#"{{"files_read":[{}],"commands_run":["gate"]}}"#,
        list.join(",")
    );
    check(&verdict(&[("evidence", &evidence)]), changed, GateState::NotRed)
}

#[test]
fn files_read_covers_a_changed_file_by_whole_path_components_not_by_a_bare_suffix() {
    let changed = ["crates/core/src/lib.rs"];
    let missed = ["changed files not reviewed: crates/core/src/lib.rs"];
    for entry in [
        "s",
        "lib.rs",
        "src/lib.rs",
        "core/src/lib.rs",
        "ore/src/lib.rs",
        "xcrates/core/src/lib.rs",
        "crates/core/src/lib",
        "crates/core/src/lib.rs/",
        "",
        "proj/crates/core/src/lib.rs",
        "../crates/core/src/lib.rs",
    ] {
        assert_eq!(read(&changed, &[entry]), missed, "files_read {entry:?}");
    }
    for entry in [
        "crates/core/src/lib.rs",
        "./crates/core/src/lib.rs",
        ".\\crates\\core\\src\\lib.rs",
        " crates/core/src/lib.rs ",
        "/home/dev/proj/crates/core/src/lib.rs",
        "C:\\Users\\dev\\proj\\crates\\core\\src\\lib.rs",
        "C:/Users/dev/proj/crates/core/src/lib.rs",
        "\\\\?\\C:\\proj\\crates\\core\\src\\lib.rs",
        "\\\\server\\share\\proj\\crates\\core\\src\\lib.rs",
    ] {
        assert_eq!(read(&changed, &[entry]), Vec::<String>::new(), "files_read {entry:?}");
    }
    // a crate at the repository root: the short path is the whole changed path
    let root = ["src/main.rs", "build.rs"];
    assert_eq!(read(&root, &["src/main.rs", "/repo/build.rs"]), Vec::<String>::new());
    assert_eq!(
        read(&root, &["main.rs", "build.rs"]),
        ["changed files not reviewed: src/main.rs"]
    );
    // one entry covers only its own file
    let two = ["crates/a/src/lib.rs", "crates/b/src/lib.rs"];
    assert_eq!(
        read(&two, &["crates/a/src/lib.rs", "lib.rs"]),
        ["changed files not reviewed: crates/b/src/lib.rs"]
    );
    // a root package next to a member crate: a member's path does not cover the root file
    let nested = ["src/lib.rs", "crates/core/src/lib.rs", "build.rs"];
    let only_src = ["changed files not reviewed: src/lib.rs"];
    assert_eq!(read(&nested, &["crates/core/src/lib.rs", "build.rs"]), only_src);
    assert_eq!(
        read(
            &nested,
            &["/repo/crates/core/src/lib.rs", "C:\\repo\\crates\\core\\build.rs", "build.rs"]
        ),
        only_src
    );
    assert_eq!(
        read(&nested, &["src/lib.rs", "crates/core/src/lib.rs", "crates/core/build.rs"]),
        ["changed files not reviewed: build.rs"]
    );
    assert_eq!(
        read(&nested, &["src/lib.rs", "crates/core/src/lib.rs", "build.rs"]),
        Vec::<String>::new()
    );
    assert_eq!(
        read(&nested, &["/repo/src/lib.rs", "/repo/crates/core/src/lib.rs", "C:\\repo\\build.rs"]),
        Vec::<String>::new()
    );
}

#[test]
fn every_verdict_rates_correctness_escape_hatches_and_craft_with_an_integer_score() {
    use super::super::{ALWAYS_SCORED, DIMENSIONS};
    assert_eq!(ALWAYS_SCORED, ["correctness", "escape_hatches", "craft"]);
    let rated = |names: &[&str]| -> String {
        let over: Vec<(&str, &str)> = DIMENSIONS
            .iter()
            .map(|d| (*d, if names.contains(d) { "3" } else { r#""n/a""# }))
            .collect();
        scores(&over)
    };
    let msg = |na: &[&str]| {
        format!(
            "scores must rate correctness, escape_hatches and craft 0-4: they apply to any change (\"n/a\" only where another dimension does not apply); \"n/a\": {}",
            na.join(", ")
        )
    };
    assert_eq!(messages(&verdict(&[("scores", &rated(&[]))])), [msg(&ALWAYS_SCORED)]);
    assert_eq!(
        messages(&verdict(&[("scores", &rated(&["correctness", "escape_hatches", "ownership"]))])),
        [msg(&["craft"])]
    );
    assert_eq!(
        messages(&verdict(&[(
            "scores",
            &rated(&["ownership", "errors", "api", "escape_hatches", "craft"])
        )])),
        [msg(&["correctness"])]
    );
    let cr = r#""changes-requested""#;
    assert_eq!(
        messages(&verdict(&[("scores", &rated(&[])), ("verdict", cr)])),
        [msg(&ALWAYS_SCORED)]
    );
    assert_eq!(messages(&verdict(&[("scores", &rated(&ALWAYS_SCORED))])), Vec::<String>::new());
    assert_eq!(
        messages(&verdict(&[("scores", &rated(&ALWAYS_SCORED)), ("verdict", cr)])),
        Vec::<String>::new()
    );
    let zero = rated(&ALWAYS_SCORED).replace(r#""correctness":3"#, r#""correctness":0"#);
    assert_eq!(
        messages(&verdict(&[("verdict", cr), ("scores", &zero)])),
        Vec::<String>::new()
    );
    // a missing or malformed score reports that problem, not this one as well
    assert!(
        !messages(&verdict(&[("scores", "undefined")]))
            .iter()
            .any(|p| p.starts_with("scores must rate")),
        "no score-na without scores"
    );
    let no_craft = rated(&ALWAYS_SCORED).replace(r#","craft":3"#, "");
    assert_eq!(
        messages(&verdict(&[("scores", &no_craft)])),
        [r#"scores.craft is missing (0-4 or "n/a")"#]
    );
    let craft5 = rated(&ALWAYS_SCORED).replace(r#""craft":3"#, r#""craft":5"#);
    assert_eq!(
        messages(&verdict(&[("scores", &craft5)])),
        [r#"scores.craft must be 0-4 or "n/a""#]
    );
}

#[test]
fn approved_must_agree_with_the_findings_the_scores_and_the_gate() {
    let two = format!(
        "[{},{}]",
        finding(&[("severity", r#""major""#)]),
        finding(&[("severity", r#""blocker""#)])
    );
    has(
        &messages(&verdict(&[("findings", &two)])),
        r#""approved" is inconsistent with 2 blocker/major finding(s): use "changes-requested""#,
    );
    has(
        &messages(&verdict(&[("scores", &scores(&[("tests", "2"), ("docs", "0")]))])),
        r#""approved" requires every applicable dimension >= 3; low: tests, docs"#,
    );
    has(
        &check(&verdict(&[]), &[], GateState::Red),
        r#""approved" is impossible: the gate is red for this fingerprint"#,
    );
    has(
        &messages(&verdict(&[("gate", r#"{"fmt":"pass","clippy":"fail"}"#)])),
        r#""approved" requires every gate entry to be "pass""#,
    );
}

#[test]
fn a_stable_kind_per_problem_with_the_same_messages() {
    // telemetry.test.mjs: "verdictProblems: a stable kind per problem, ..."
    let v = verdict(&[
        ("fingerprint", r#""nothex""#),
        ("findings", r#"[{"rule":"X","severity":"major"}]"#),
        ("scores", &scores(&[("craft", r#""n/a""#), ("docs", "7")])),
        ("evidence", r#"{"files_read":[],"commands_run":["gate"]}"#),
        ("gate", r#"{"clippy":"fail"}"#),
    ]);
    let changed = vec!["src/lib.rs".to_owned()];
    let ctx = VerdictContext {
        changed_rust: &changed,
        gate: GateState::NotRed,
    };
    let problems =
        verdict_problems(Some(&ExtractedVerdict::Parsed(v.clone())), &ctx).expect("no throw");
    let names: Vec<&str> = problem_kinds(&problems).iter().map(|k| k.name()).collect();
    assert_eq!(
        names,
        [
            "fingerprint",
            "finding-rule",
            "finding-file",
            "finding-evidence",
            "finding-fix",
            "score-range",
            "score-na",
            "files-read",
            "files-unreviewed",
            "approved-with-findings",
            "approved-gate-entry",
        ]
    );
    let none = VerdictContext {
        changed_rust: &[],
        gate: GateState::NotRed,
    };
    let kinds = |verdict: Option<&ExtractedVerdict>, ctx: &VerdictContext<'_>| {
        problem_kinds(&verdict_problems(verdict, ctx).expect("no throw"))
    };
    assert_eq!(kinds(None, &none), [ProblemKind::NoBlock]);
    let parse_error = Value::from_json(r#"{"__parseError":"x"}"#).expect("JSON");
    assert_eq!(
        kinds(Some(&ExtractedVerdict::Parsed(parse_error)), &none),
        [ProblemKind::JsonParse]
    );
    let text = verdict_json(&[
        ("fingerprint", r#""nothex""#),
        ("findings", r#""none""#),
        ("scores", "null"),
        ("evidence", r#"{"files_read":[],"commands_run":["gate"]}"#),
        ("gate", "{}"),
    ]);
    let red = VerdictContext {
        changed_rust: &[],
        gate: GateState::Red,
    };
    let v = ExtractedVerdict::Parsed(Value::from_json(&text).expect("JSON"));
    let names: Vec<&str> = kinds(Some(&v), &red).iter().map(|k| k.name()).collect();
    assert_eq!(
        names,
        ["fingerprint", "findings-array", "scores-missing", "files-read", "approved-red-gate"]
    );
}
