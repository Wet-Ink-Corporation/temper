//! temper's pure policy layer: the scanner, the manifest, config and verdict rules, the findings,
//! baseline and report models, and every message text the hooks print.
//!
//! Nothing here touches the file system, processes, the environment or the clock; the
//! application layer (`temper-app`) supplies those through ports. The port of the 1.x Node
//! engine is under way (`docs/plans/temper-2-rust-port.md`); so far the crate has the scanner
//! with its lexer, test regions and all 60 rules, path classification and the new-hit difference
//! (Phases 1a to 1d), which nothing calls yet.
//!
//! # Quick start
//!
//! ```
//! use temper_domain::{CodeKind, FileRole, ScanOptions, Scanner, Severity};
//!
//! let scanner = Scanner::new()?;
//! let src = "pub fn first(v: &[u8]) -> u8 {\n    *v.first().unwrap()\n}\n";
//! let options = ScanOptions { kind: CodeKind::Prod, file: FileRole::Source, forbid: None };
//! let report = scanner.scan(src, &options);
//!
//! let hits: Vec<_> = report.hits().iter().map(|h| (h.id().as_str(), h.severity(), h.line().get())).collect();
//! assert_eq!(hits, [("H-02", Severity::Hard, 2)]);
//! # Ok::<(), temper_domain::ScannerSetupError>(())
//! ```

mod js;
mod paths;
mod scan;

pub use js::js_trim;
pub use paths::{PathStyle, unverbatim};
pub use scan::{
    CodeKind, FileRole, Hit, HitAt, LibTarget, LineNo, NOT_A_RUST_PROJECT, RuleId, RuleKey,
    ScanOptions, ScanReport, Scanner, ScannerSetupError, Severity, classify_path, format_hit,
    is_build_script, new_hits, render_scan_text,
};
