//! What is wrong with a verdict (1.x `verdictProblems`), in 1.x's order and words.

use crate::Fingerprint;
use crate::js::{JsTypeError, Value, js_string, truthy};

use super::coverage::unreviewed;
use super::{ALWAYS_SCORED, DIMENSIONS, ExtractedVerdict};

const SEVERITIES: [&str; 4] = ["blocker", "major", "minor", "nit"];

/// Whether the cached gate for the verdict's fingerprint is red: 1.x's
/// `ctx.gate && ctx.gate.ok === false`. No gate record, or one that is not red, is `NotRed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateState {
    /// The gate failed for this fingerprint.
    Red,
    /// No record, or a record that does not say the gate failed.
    NotRed,
}

/// What a verdict is checked against: the changed Rust files of the reviewed tree and its gate.
#[derive(Debug, Clone, Copy)]
pub struct VerdictContext<'a> {
    /// The changed files, as 1.x's `rustChanges(..).filter((f) => f.endsWith('.rs'))` lists them.
    pub changed_rust: &'a [String],
    /// The cached gate of the verdict's fingerprint.
    pub gate: GateState,
}

/// A short, stable name per problem for telemetry ([`ProblemKind::name`]); the message is prose
/// and may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProblemKind {
    /// `no-block`: the report has no verdict block.
    NoBlock,
    /// `json-parse`: the block is not JSON.
    JsonParse,
    /// `schema-version`: `temper_verdict` is not 1.
    SchemaVersion,
    /// `verdict-value`: `verdict` is neither `approved` nor `changes-requested`.
    VerdictValue,
    /// `fingerprint`: not 16 lowercase hex digits.
    Fingerprint,
    /// `findings-array`: `findings` is not an array.
    FindingsArray,
    /// `finding-object`: a finding is not an object.
    FindingObject,
    /// `finding-rule`: a finding's rule is not a rule ID.
    FindingRule,
    /// `finding-severity`: a finding's severity is not one of the four.
    FindingSeverity,
    /// `finding-file`: a finding has no file.
    FindingFile,
    /// `finding-evidence`: a finding has no evidence.
    FindingEvidence,
    /// `finding-fix`: a finding has no fix.
    FindingFix,
    /// `scores-missing`: `scores` is not an object.
    ScoresMissing,
    /// `score-missing`: a dimension has no score.
    ScoreMissing,
    /// `score-range`: a score is neither 0 to 4 nor `n/a`.
    ScoreRange,
    /// `score-na`: an always-scored dimension is `n/a`.
    ScoreNa,
    /// `files-read`: `evidence.files_read` is empty or missing.
    FilesRead,
    /// `commands-run`: `evidence.commands_run` is empty or missing.
    CommandsRun,
    /// `files-unreviewed`: a changed `.rs` file is not in `files_read`.
    FilesUnreviewed,
    /// `approved-with-findings`: an approval with a blocker or major finding.
    ApprovedWithFindings,
    /// `approved-low-score`: an approval with a score below 3.
    ApprovedLowScore,
    /// `approved-red-gate`: an approval while the gate is red.
    ApprovedRedGate,
    /// `approved-gate-entry`: an approval with a gate entry that is not `pass`.
    ApprovedGateEntry,
}

impl ProblemKind {
    /// 1.x's `kind` string, as telemetry writes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::NoBlock => "no-block",
            Self::JsonParse => "json-parse",
            Self::SchemaVersion => "schema-version",
            Self::VerdictValue => "verdict-value",
            Self::Fingerprint => "fingerprint",
            Self::FindingsArray => "findings-array",
            Self::FindingObject => "finding-object",
            Self::FindingRule => "finding-rule",
            Self::FindingSeverity => "finding-severity",
            Self::FindingFile => "finding-file",
            Self::FindingEvidence => "finding-evidence",
            Self::FindingFix => "finding-fix",
            Self::ScoresMissing => "scores-missing",
            Self::ScoreMissing => "score-missing",
            Self::ScoreRange => "score-range",
            Self::ScoreNa => "score-na",
            Self::FilesRead => "files-read",
            Self::CommandsRun => "commands-run",
            Self::FilesUnreviewed => "files-unreviewed",
            Self::ApprovedWithFindings => "approved-with-findings",
            Self::ApprovedLowScore => "approved-low-score",
            Self::ApprovedRedGate => "approved-red-gate",
            Self::ApprovedGateEntry => "approved-gate-entry",
        }
    }
}

/// One problem: its kind and the message the reviewer reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictProblem {
    kind: ProblemKind,
    message: String,
}

impl VerdictProblem {
    /// The problem's kind.
    #[must_use]
    pub fn kind(&self) -> ProblemKind {
        self.kind
    }

    /// The message, 1.x's text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// The distinct kinds of `problems` in first-seen order (1.x `problemKinds`).
#[must_use]
pub fn problem_kinds(problems: &[VerdictProblem]) -> Vec<ProblemKind> {
    let mut kinds = Vec::new();
    for problem in problems {
        // At most 23 kinds, so the scan stays constant per problem.
        if !kinds.contains(&problem.kind) {
            kinds.push(problem.kind);
        }
    }
    kinds
}

/// 1.x `verdictProblems(v, ctx)`: every problem of `verdict`, in 1.x's order; none means valid.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x throws: an approval whose `findings` holds a `null`, which 1.x's
/// `findings.filter((f) => f.severity …)` reads a property of.
pub fn verdict_problems(
    verdict: Option<&ExtractedVerdict>,
    ctx: &VerdictContext<'_>,
) -> Result<Vec<VerdictProblem>, JsTypeError> {
    match verdict {
        Some(ExtractedVerdict::Parsed(value)) => problems_of(value, VerdictField::AsIs, ctx),
        Some(ExtractedVerdict::Unparseable { message }) => Ok(vec![parse_problem(message)]),
        None => Ok(vec![no_block()]),
    }
}

/// How the checks read `verdict`: as the value has it, or as `approved`, which is 1.x's
/// `overturns(v)`: `verdictProblems({ ...v, verdict: 'approved' })`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VerdictField {
    AsIs,
    Approved,
}

fn no_block() -> VerdictProblem {
    VerdictProblem {
        kind: ProblemKind::NoBlock,
        message: "no verdict block found: end the final report with a ```json block containing \"temper_verdict\": 1".to_owned(),
    }
}

fn parse_problem(detail: &str) -> VerdictProblem {
    VerdictProblem {
        kind: ProblemKind::JsonParse,
        message: format!("verdict JSON does not parse: {detail}"),
    }
}

/// The problems of a parsed value, read as `field` says.
pub(super) fn problems_of(
    v: &Value,
    field: VerdictField,
    ctx: &VerdictContext<'_>,
) -> Result<Vec<VerdictProblem>, JsTypeError> {
    if !truthy(v) {
        return Ok(vec![no_block()]);
    }
    if let Some(error) = v.get("__parseError").filter(|e| truthy(e)) {
        return Ok(vec![parse_problem(&js_string(error))]);
    }
    let verdict = match field {
        VerdictField::AsIs => v.get("verdict").and_then(Value::as_str),
        VerdictField::Approved => Some("approved"),
    };
    let mut p = Problems(Vec::new());
    header(&mut p, v, verdict);
    findings_problems(&mut p, findings_list(v));
    scores_problems(&mut p, scores_object(v));
    evidence_problems(&mut p, v, ctx);
    if verdict == Some("approved") {
        approval_problems(&mut p, v, ctx)?;
    }
    Ok(p.0)
}

/// The list 1.x's `add` pushes to.
struct Problems(Vec<VerdictProblem>);

impl Problems {
    fn add(&mut self, kind: ProblemKind, message: impl Into<String>) {
        self.0.push(VerdictProblem {
            kind,
            message: message.into(),
        });
    }
}

/// `typeof x === 'object'` for a truthy value: objects and arrays.
fn is_object(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Array(_))
}

/// `Number.isInteger(x)`: a finite number with no fraction (`-0` and `4.0` are integers).
fn integer(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(n)) if n.is_finite() && n.trunc().total_cmp(n).is_eq() => Some(*n),
        _ => None,
    }
}

/// The verdict's `findings` when it is an array (1.x reads none otherwise).
fn findings_list(v: &Value) -> &[Value] {
    match v.get("findings") {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

/// `v.scores && typeof v.scores === 'object' ? v.scores : null`.
fn scores_object(v: &Value) -> Option<&Value> {
    v.get("scores").filter(|s| is_object(s))
}

fn header(p: &mut Problems, v: &Value, verdict: Option<&str>) {
    // JS `=== 1`: exact, and `1.0` is the same number; `total_cmp` is exact away from zero.
    let schema =
        matches!(v.get("temper_verdict"), Some(Value::Number(n)) if n.total_cmp(&1.0).is_eq());
    if !schema {
        p.add(ProblemKind::SchemaVersion, r#""temper_verdict" must be 1"#);
    }
    if !matches!(verdict, Some("approved" | "changes-requested")) {
        p.add(
            ProblemKind::VerdictValue,
            r#""verdict" must be "approved" or "changes-requested""#,
        );
    }
    let fingerprint = v.get("fingerprint").and_then(Value::as_str);
    if fingerprint.is_none_or(|text| text.parse::<Fingerprint>().is_err()) {
        p.add(
            ProblemKind::Fingerprint,
            r#""fingerprint" must be the 16-hex fingerprint given in your review ticket"#,
        );
    }
    if !matches!(v.get("findings"), Some(Value::Array(_))) {
        p.add(
            ProblemKind::FindingsArray,
            r#""findings" must be an array (empty if none)"#,
        );
    }
}

fn findings_problems(p: &mut Problems, findings: &[Value]) {
    for (i, f) in findings.iter().enumerate() {
        // `!f || typeof f !== 'object'`: objects and arrays are always truthy.
        if !is_object(f) {
            p.add(
                ProblemKind::FindingObject,
                format!("findings[{i}] must be an object"),
            );
            continue;
        }
        let rule = match f.get("rule") {
            None | Some(Value::Null) => "".into(),
            Some(rule) => js_string(rule),
        };
        let severity = f.get("severity").and_then(Value::as_str);
        let none_nit = rule == "none" && severity == Some("nit");
        if !(is_rule_id(&rule) || none_nit) {
            p.add(
                ProblemKind::FindingRule,
                format!("findings[{i}].rule must be a rule ID like OWN-08 (\"none\" is allowed only for a nit no rule covers)"),
            );
        }
        if !severity.is_some_and(|s| SEVERITIES.contains(&s)) {
            p.add(
                ProblemKind::FindingSeverity,
                format!(
                    "findings[{i}].severity must be one of {}",
                    SEVERITIES.join("/")
                ),
            );
        }
        let present = |key: &str| f.get(key).is_some_and(truthy);
        if !present("file") {
            p.add(
                ProblemKind::FindingFile,
                format!("findings[{i}].file is required"),
            );
        }
        if !present("evidence") {
            p.add(
                ProblemKind::FindingEvidence,
                format!("findings[{i}].evidence is required (quote the code)"),
            );
        }
        if !present("fix") {
            p.add(
                ProblemKind::FindingFix,
                format!("findings[{i}].fix is required"),
            );
        }
    }
}

/// 1.x `/^[A-Z]{1,4}-\d{2}$/`.
fn is_rule_id(rule: &str) -> bool {
    let Some((letters, digits)) = rule.split_once('-') else {
        return false;
    };
    (1..=4).contains(&letters.len())
        && letters.bytes().all(|b| b.is_ascii_uppercase())
        && digits.len() == 2
        && digits.bytes().all(|b| b.is_ascii_digit())
}

fn scores_problems(p: &mut Problems, scores: Option<&Value>) {
    let Some(scores) = scores else {
        p.add(ProblemKind::ScoresMissing, r#""scores" object is required"#);
        return;
    };
    for d in DIMENSIONS {
        match scores.get(d) {
            None => p.add(
                ProblemKind::ScoreMissing,
                format!(r#"scores.{d} is missing (0-4 or "n/a")"#),
            ),
            Some(Value::String(s)) if s == "n/a" => {}
            Some(s) if integer(Some(s)).is_some_and(|n| (0.0..=4.0).contains(&n)) => {}
            Some(_) => p.add(
                ProblemKind::ScoreRange,
                format!(r#"scores.{d} must be 0-4 or "n/a""#),
            ),
        }
    }
    let na: Vec<&str> = ALWAYS_SCORED
        .into_iter()
        .filter(|d| scores.get(d).and_then(Value::as_str) == Some("n/a"))
        .collect();
    if !na.is_empty() {
        p.add(
            ProblemKind::ScoreNa,
            format!(
                "scores must rate correctness, escape_hatches and craft 0-4: they apply to any change (\"n/a\" only where another dimension does not apply); \"n/a\": {}",
                na.join(", ")
            ),
        );
    }
}

/// `v.evidence ?? {}`: the evidence object, or nothing to read from.
fn evidence_list<'v>(v: &'v Value, key: &str) -> Option<&'v [Value]> {
    match v.get("evidence")?.get(key)? {
        Value::Array(items) => Some(items),
        _ => None,
    }
}

fn evidence_problems(p: &mut Problems, v: &Value, ctx: &VerdictContext<'_>) {
    let files_read = evidence_list(v, "files_read");
    if files_read.is_none_or(<[Value]>::is_empty) {
        p.add(
            ProblemKind::FilesRead,
            "evidence.files_read must list the files you actually read",
        );
    }
    if evidence_list(v, "commands_run").is_none_or(<[Value]>::is_empty) {
        p.add(
            ProblemKind::CommandsRun,
            "evidence.commands_run must list the commands you ran (at least the gate)",
        );
    }
    if let Some(read) = files_read.filter(|_| !ctx.changed_rust.is_empty()) {
        let missed = unreviewed(ctx.changed_rust, read);
        if !missed.is_empty() {
            let shown: Vec<&str> = missed.into_iter().take(10).collect();
            p.add(
                ProblemKind::FilesUnreviewed,
                format!("changed files not reviewed: {}", shown.join(", ")),
            );
        }
    }
}

/// `f.severity === 'blocker' || f.severity === 'major'`.
pub(super) fn is_serious(finding: &Value) -> bool {
    matches!(
        finding.get("severity").and_then(Value::as_str),
        Some("blocker" | "major")
    )
}

/// The dimensions `scores` rates below 3 (1.x `lowScores`), in `DIMENSIONS` order.
pub(super) fn low_scores(scores: Option<&Value>) -> Vec<&'static str> {
    DIMENSIONS
        .into_iter()
        .filter(|d| integer(scores.and_then(|s| s.get(d))).is_some_and(|n| n < 3.0))
        .collect()
}

fn approval_problems(
    p: &mut Problems,
    v: &Value,
    ctx: &VerdictContext<'_>,
) -> Result<(), JsTypeError> {
    let findings = findings_list(v);
    let scores = scores_object(v);
    if findings.iter().any(|f| matches!(f, Value::Null)) {
        return Err(JsTypeError::new(
            "an approval's findings hold a null, which 1.x reads the severity of",
        ));
    }
    let bad = findings.iter().filter(|f| is_serious(f)).count();
    if bad > 0 {
        p.add(
            ProblemKind::ApprovedWithFindings,
            format!(r#""approved" is inconsistent with {bad} blocker/major finding(s): use "changes-requested""#),
        );
    }
    if scores.is_some() {
        let low = low_scores(scores);
        if !low.is_empty() {
            p.add(
                ProblemKind::ApprovedLowScore,
                format!(
                    r#""approved" requires every applicable dimension >= 3; low: {}"#,
                    low.join(", ")
                ),
            );
        }
    }
    if ctx.gate == GateState::Red {
        p.add(
            ProblemKind::ApprovedRedGate,
            r#""approved" is impossible: the gate is red for this fingerprint"#,
        );
    }
    if gate_entries_fail(v) {
        p.add(
            ProblemKind::ApprovedGateEntry,
            r#""approved" requires every gate entry to be "pass""#,
        );
    }
    Ok(())
}

/// `Object.values(v.gate ?? {}).some((x) => x !== 'pass' && x !== 'n/a')`. A string's values
/// are its UTF-16 code units, none of which is `pass` or `n/a`.
fn gate_entries_fail(v: &Value) -> bool {
    let fails = |x: &Value| !matches!(x.as_str(), Some("pass" | "n/a"));
    match v.get("gate") {
        Some(Value::Object(gate)) => gate.iter().any(|(_, x)| fails(x)),
        Some(Value::Array(items)) => items.iter().any(fails),
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    }
}
