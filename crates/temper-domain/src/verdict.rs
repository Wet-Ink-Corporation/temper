//! Review verdicts: the block a reviewer ends its report with, its validation against the tree
//! under review, and the summaries the hooks print (1.x `verdict.mjs`).

mod coverage;
mod extract;
mod problems;

pub use extract::{ExtractedVerdict, extract_verdict};
pub use problems::{
    GateState, ProblemKind, VerdictContext, VerdictProblem, problem_kinds, verdict_problems,
};

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
