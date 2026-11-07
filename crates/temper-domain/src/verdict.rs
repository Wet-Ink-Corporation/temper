//! Review verdicts: the block a reviewer ends its report with, its validation against the tree
//! under review, and the summaries the hooks print (1.x `verdict.mjs`).

mod extract;

pub use extract::{ExtractedVerdict, extract_verdict};

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

#[cfg(test)]
mod tests;
