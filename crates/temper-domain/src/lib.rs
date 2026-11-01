//! temper's pure policy layer: the scanner, the manifest, config and verdict rules, the findings,
//! baseline and report models, and every message text the hooks print.
//!
//! Nothing here touches the file system, processes, the environment or the clock; the
//! application layer (`temper-app`) supplies those through ports. The port of the 1.x Node
//! engine is under way (`docs/plans/temper-2-rust-port.md`); so far the crate has the scanner's
//! lexer, test regions and 46 of the 60 rules (Phases 1a, 1b), which nothing calls yet.
//!
//! # Quick start
//!
//! ```
//! use temper_domain::{CodeKind, FileRole, ScanOptions, Scanner, Severity};
//!
//! let scanner = Scanner::new()?;
//! let src = "pub fn first(v: &[u8]) -> u8 {\n    *v.first().unwrap()\n}\n";
//! let report = scanner.scan(src, &ScanOptions { kind: CodeKind::Prod, file: FileRole::Source });
//!
//! let hits: Vec<_> = report.hits().iter().map(|h| (h.id().as_str(), h.severity(), h.line().get())).collect();
//! assert_eq!(hits, [("H-02", Severity::Hard, 2)]);
//! # Ok::<(), temper_domain::ScannerSetupError>(())
//! ```

mod js;
mod scan;

pub use scan::{
    CodeKind, FileRole, Hit, LineNo, RuleId, RuleKey, ScanOptions, ScanReport, Scanner,
    ScannerSetupError, Severity, is_build_script,
};
