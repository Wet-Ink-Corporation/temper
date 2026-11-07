//! `ValidVerdict::check` and the denial texts of `preHandback` and `subagentStop`.

use crate::js::Value;

use super::super::{
    ExtractedVerdict, GateState, ProblemKind, ValidVerdict, VerdictContext, VerdictOutcome,
    VerdictProblem, VerdictRejected, handback_denial, verdict_block, verdict_problems,
};
use super::{finding, verdict};

const NONE: VerdictContext<'static> = VerdictContext {
    changed_rust: &[],
    gate: GateState::NotRed,
};

#[test]
fn a_valid_verdict_gives_its_fingerprint_outcome_and_finding_count() {
    let findings = format!(
        "[{},{}]",
        finding(&[]),
        finding(&[("severity", r#""nit""#)])
    );
    let v = verdict(&[("findings", &findings)]);
    let valid = ValidVerdict::check(ExtractedVerdict::Parsed(v.clone()), &NONE).expect("valid");
    assert_eq!(valid.fingerprint().to_string(), "0123456789abcdef");
    assert_eq!(valid.outcome(), VerdictOutcome::Approved);
    assert_eq!(valid.finding_count(), 2);
    assert_eq!(valid.value(), &v);
    let cr = verdict(&[("verdict", r#""changes-requested""#)]);
    let valid = ValidVerdict::check(ExtractedVerdict::Parsed(cr), &NONE).expect("valid");
    assert_eq!(valid.outcome(), VerdictOutcome::ChangesRequested);
}

#[test]
fn an_invalid_verdict_is_handed_back_with_its_problems() {
    let v = ExtractedVerdict::Parsed(verdict(&[("temper_verdict", "2")]));
    let Err(rejected) = ValidVerdict::check(v.clone(), &NONE) else {
        panic!("temper_verdict 2 is invalid");
    };
    let kinds: Vec<ProblemKind> = rejected
        .problems()
        .iter()
        .map(VerdictProblem::kind)
        .collect();
    assert_eq!(kinds, [ProblemKind::SchemaVersion]);
    assert_eq!(rejected.to_string(), "the review verdict has 1 problem(s)");
    assert_eq!(rejected.into_verdict(), v);
    let bad = ExtractedVerdict::Unparseable {
        message: "x".to_owned(),
    };
    let Err(rejected) = ValidVerdict::check(bad.clone(), &NONE) else {
        panic!("an unparseable verdict is invalid");
    };
    assert_eq!(rejected.into_verdict(), bad);
}

#[test]
fn an_approval_with_a_null_finding_fails_as_1x_throws_and_is_handed_back() {
    let v = ExtractedVerdict::Parsed(verdict(&[("findings", "[null]")]));
    assert!(
        verdict_problems(Some(&v), &NONE).is_err(),
        "1.x throws here"
    );
    let Err(rejected) = ValidVerdict::check(v.clone(), &NONE) else {
        panic!("not valid");
    };
    assert!(
        matches!(rejected, VerdictRejected::Throws { .. }),
        "{rejected}"
    );
    assert!(
        rejected.problems().is_empty(),
        "no problem list where 1.x throws"
    );
    assert_eq!(rejected.into_verdict(), v);
}

#[test]
fn the_debug_form_leaves_the_verdict_out() {
    let v = ExtractedVerdict::Parsed(verdict(&[("verdict", r#""secret-value""#)]));
    let Err(rejected) = ValidVerdict::check(v, &NONE) else {
        panic!("not valid");
    };
    let debug = format!("{rejected:?}");
    assert!(!debug.contains("secret-value"), "{debug}");
}

#[test]
fn the_handback_denial_and_the_stop_block_are_1xs_texts() {
    let v = ExtractedVerdict::Parsed(verdict(&[
        ("temper_verdict", "2"),
        ("verdict", r#""lgtm""#),
    ]));
    let problems = verdict_problems(Some(&v), &NONE).expect("no throw");
    assert_eq!(
        handback_denial(&problems),
        "temper: the review verdict is invalid, so the report was not delivered:\n- \"temper_verdict\" must be 1\n- \"verdict\" must be \"approved\" or \"changes-requested\"\nFix the verdict block and hand back again."
    );
    assert_eq!(
        verdict_block(&problems),
        "temper: your review verdict is invalid:\n- \"temper_verdict\" must be 1\n- \"verdict\" must be \"approved\" or \"changes-requested\"\nEnd your final report with a corrected ```json verdict block."
    );
}

#[test]
fn a_parsed_value_that_is_not_an_object_is_never_valid() {
    for text in ["[1]", r#""temper_verdict""#, "5"] {
        let v = ExtractedVerdict::Parsed(Value::from_json(text).expect("JSON"));
        assert!(ValidVerdict::check(v, &NONE).is_err(), "{text}");
    }
}
