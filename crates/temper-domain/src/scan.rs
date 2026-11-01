//! The escape-hatch scanner (1.x `scan.mjs` and `lexer.mjs`): pattern rules over masked Rust
//! source, with test regions and the test, ignore and clone counts, plus path classification
//! and the new-hit difference.

mod classify;
mod counts;
mod doc_blocks;
mod env;
mod finders;
mod hit;
mod lexer;
mod predicates;
mod regions;
mod rules;
mod scanner;
#[cfg(test)]
mod strategies;
#[cfg(test)]
mod tests;
mod text;

pub use classify::{LibTarget, classify_path, is_build_script};
pub use hit::{Hit, HitAt, LineNo, RuleId, RuleKey, Severity, new_hits};
pub use scanner::{CodeKind, FileRole, ScanOptions, ScanReport, Scanner, ScannerSetupError};
pub use text::format_hit;
