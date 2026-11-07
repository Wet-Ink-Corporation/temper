//! temper's pure policy layer: the scanner, the manifest, config and verdict rules, the findings,
//! baseline and report models, and every message text the hooks print.
//!
//! Nothing here touches the file system, processes, the environment or the clock; the
//! application layer (`temper-app`) supplies those through ports. The port of the 1.x Node
//! engine is under way (`docs/plans/temper-2-rust-port.md`); so far the crate has the scanner
//! with its lexer, test regions and all 60 rules, path classification and the new-hit difference
//! (Phases 1a to 1d), the manifest policy, the config and glob matching (Phase 2a), the hook
//! policies (Phases 2b to 2f), and the review verdict rules with the tree fingerprint (Phase 3a).
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

mod config;
mod fingerprint;
mod glob;
mod hooks;
mod js;
mod manifest;
mod paths;
mod scan;
mod time;
mod verdict;

pub use config::{
    CargoFeatures, Config, ConfigLoad, ConfigProblem, ExemptRule, GateConfig, ReviewMode, Switch,
};
pub use fingerprint::{
    ContentSink, DiffSink, Fingerprint, FingerprintHasher, FingerprintParseError,
    TEMPER_CONFIG_FILE,
};
pub use glob::{Glob, GlobError, Globs, LazyGlobs};
pub use hooks::{
    BashPolicy, ConfigAsk, ConfigChanges, ContextEvent, CopiedFile, DependencyCheck, EditFacts,
    EditOp, EditStep, EditTarget, EditTool, Exemptions, HookEvent, HookOutcome, IRON_RULES,
    InterpreterCode, LazyExemptions, ManifestEdit, OpaqueKind, PolicyDecision, PolicySetupError,
    ProtectedChange, RaiseFlag, Replacements, RustEdit, RustWrites, SKILL_NOT_LOADED,
    STATE_DIR_DENIAL, ShellAskVia, ShellDenyVia, ShellWriteFile, SkillVia, TelemetryEvent,
    TelemetryRow, UnknownHookEvent, Unread, apply_edit, edit_policy, is_reviewer, lockfile_denial,
    skill_gate_denial, skill_load_via, skill_preloaded,
};
pub use js::{JsTypeError, JsonParseError, Object as JsObject, Value as JsValue, js_trim};
pub use manifest::{
    Before, DepKind, DepName, GoverningRoot, Manifest, ManifestEditFacts, ManifestFindings,
    ManifestParseError, ManifestProblem, OwnLints, RootRelation, SplitProblems, WatchedKey,
    benign_when_new, dep_names, dep_requirements, dependency_names, lint_config_changed,
    lint_sections, major_of, manifest_checks, member_problems, non_caret_requirement,
    split_problems, watched_keys,
};
pub use paths::{PathStyle, unverbatim};
pub use scan::{
    CodeKind, FileRole, Hit, HitAt, LibTarget, LineNo, NOT_A_RUST_PROJECT, RuleId, RuleKey,
    ScanOptions, ScanReport, Scanner, ScannerSetupError, Severity, classify_path, format_hit,
    is_build_script, new_hits, render_scan_text,
};
pub use time::Timestamp;
pub use verdict::{
    ALWAYS_SCORED, DIMENSIONS, ExtractedVerdict, GateState, ProblemKind, ValidVerdict,
    VerdictContext, VerdictOutcome, VerdictProblem, VerdictRejected, earlier_review,
    extract_verdict, handback_denial, problem_kinds, summarize_findings, verdict_block,
    verdict_hash, verdict_problems,
};
