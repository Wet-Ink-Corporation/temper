//! Review verdicts: the block a reviewer ends its report with, its validation against the tree
//! under review, and the summaries the hooks print (1.x `verdict.mjs`).

mod coverage;
mod extract;
mod problems;
mod summary;

pub use extract::{ExtractedVerdict, extract_verdict};
pub use problems::{
    GateState, ProblemKind, VerdictContext, VerdictProblem, problem_kinds, verdict_problems,
};
pub use summary::{earlier_review, summarize_findings};

use sha2::{Digest, Sha256};

use crate::Fingerprint;
use crate::js::{Value, canonical};

/// 1.x `verdictHash(v)`: the first 16 hex digits of the SHA-256 of
/// `JSON.stringify(canonical(v))`, so the same report read twice hashes the same whatever its
/// key order.
///
/// # Examples
///
/// ```
/// use temper_domain::{JsValue, verdict_hash};
///
/// let a = JsValue::from_json(r#"{"verdict":"approved","temper_verdict":1}"#)?;
/// let b = JsValue::from_json(r#"{"temper_verdict":1,"verdict":"approved"}"#)?;
/// assert_eq!(verdict_hash(&a), verdict_hash(&b));
/// # Ok::<(), temper_domain::JsonParseError>(())
/// ```
#[must_use]
pub fn verdict_hash(verdict: &Value) -> String {
    let digest: [u8; 32] = Sha256::digest(canonical(verdict).to_json()).into();
    // The same 16-hex form as a tree fingerprint.
    Fingerprint::from_sha256(digest).to_string()
}

/// The review dimensions, in 1.x's order.
pub const DIMENSIONS: [&str; 12] = [
    "correctness",
    "escape_hatches",
    "ownership",
    "errors",
    "api",
    "architecture",
    "tests",
    "performance",
    "concurrency",
    "observability",
    "craft",
    "docs",
];

/// The dimensions every verdict must rate 0 to 4 rather than `n/a`: they apply to any Rust
/// change, however small.
pub const ALWAYS_SCORED: [&str; 3] = ["correctness", "escape_hatches", "craft"];

#[cfg(test)]
mod tests;
