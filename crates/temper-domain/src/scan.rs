//! The escape-hatch scanner (1.x `scan.mjs` and `lexer.mjs`): pattern rules over masked Rust
//! source, with test regions and the test, ignore and clone counts.

mod counts;
mod finders;
mod hit;
mod lexer;
mod regions;
mod rules;
mod scanner;
#[cfg(test)]
mod strategies;
#[cfg(test)]
mod tests;

pub use hit::{Hit, LineNo, RuleId, RuleKey, Severity};
pub use scanner::{
    CodeKind, FileRole, ScanOptions, ScanReport, Scanner, ScannerSetupError, is_build_script,
};
