//! A verdict that passed every check, and the rejection that hands an invalid one back.

use std::fmt;

use crate::Fingerprint;
use crate::js::{JsTypeError, Value};

use super::{ExtractedVerdict, VerdictContext, VerdictProblem, verdict_problems};

/// What the reviewer asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictOutcome {
    /// `"verdict": "approved"`.
    Approved,
    /// `"verdict": "changes-requested"`.
    ChangesRequested,
}

/// A verdict with no problem against its context: a JSON object whose `temper_verdict` is 1,
/// whose fingerprint parses and whose outcome is one of the two.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidVerdict {
    value: Value,
    fingerprint: Fingerprint,
    outcome: VerdictOutcome,
}

impl ValidVerdict {
    /// Checks `verdict` against `ctx` as 1.x's `verdictProblems` does.
    ///
    /// # Errors
    ///
    /// [`VerdictRejected::Problems`] with 1.x's problem list when it has any, and
    /// [`VerdictRejected::Throws`] where 1.x throws; both hand `verdict` back.
    pub fn check(
        verdict: ExtractedVerdict,
        ctx: &VerdictContext<'_>,
    ) -> Result<Self, VerdictRejected> {
        let problems = match verdict_problems(Some(&verdict), ctx) {
            Ok(problems) => problems,
            Err(source) => return Err(VerdictRejected::Throws { source, verdict }),
        };
        if !problems.is_empty() {
            return Err(VerdictRejected::Problems { problems, verdict });
        }
        // With no problem, the value is an object with a valid fingerprint and outcome, so
        // `parts` answers; a verdict it cannot read always has problems.
        match (parts(&verdict), verdict) {
            (Some((fingerprint, outcome)), ExtractedVerdict::Parsed(value)) => Ok(Self {
                value,
                fingerprint,
                outcome,
            }),
            (_, verdict) => Err(VerdictRejected::Problems { problems, verdict }),
        }
    }

    /// The verdict as the reviewer wrote it.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// The fingerprint the verdict reviews.
    #[must_use]
    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Approved or changes requested.
    #[must_use]
    pub fn outcome(&self) -> VerdictOutcome {
        self.outcome
    }

    /// 1.x `(v.findings ?? []).length`; a valid verdict's findings are an array.
    #[must_use]
    pub fn finding_count(&self) -> usize {
        match self.value.get("findings") {
            Some(Value::Array(items)) => items.len(),
            _ => 0,
        }
    }
}

/// The fingerprint and outcome a valid verdict carries.
fn parts(verdict: &ExtractedVerdict) -> Option<(Fingerprint, VerdictOutcome)> {
    let fingerprint = verdict.field("fingerprint")?.as_str()?.parse().ok()?;
    let outcome = match verdict.field("verdict")?.as_str()? {
        "approved" => VerdictOutcome::Approved,
        "changes-requested" => VerdictOutcome::ChangesRequested,
        _ => return None,
    };
    Some((fingerprint, outcome))
}

/// A verdict that is not valid, handed back to the caller (ERR-14).
#[derive(thiserror::Error)]
pub enum VerdictRejected {
    /// 1.x's problems, in its order (never empty for a verdict 1.x would check).
    #[error("the review verdict has {} problem(s)", problems.len())]
    Problems {
        /// What is wrong.
        problems: Vec<VerdictProblem>,
        /// The verdict, handed back.
        verdict: ExtractedVerdict,
    },
    /// 1.x throws while checking this verdict, so the handler fails where 1.x's does.
    #[error("checking the review verdict failed")]
    Throws {
        /// Where 1.x throws.
        source: JsTypeError,
        /// The verdict, handed back.
        verdict: ExtractedVerdict,
    },
}

impl VerdictRejected {
    /// The problems; none where 1.x throws.
    #[must_use]
    pub fn problems(&self) -> &[VerdictProblem] {
        match self {
            Self::Problems { problems, .. } => problems,
            Self::Throws { .. } => &[],
        }
    }

    /// The verdict that was checked.
    #[must_use]
    pub fn into_verdict(self) -> ExtractedVerdict {
        match self {
            Self::Problems { verdict, .. } | Self::Throws { verdict, .. } => verdict,
        }
    }
}

/// Leaves the verdict out: it is the reviewer's whole report block (ERR-14).
impl fmt::Debug for VerdictRejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Problems { problems, .. } => f
                .debug_struct("Problems")
                .field("problems", problems)
                .finish_non_exhaustive(),
            Self::Throws { source, .. } => f
                .debug_struct("Throws")
                .field("source", source)
                .finish_non_exhaustive(),
        }
    }
}
