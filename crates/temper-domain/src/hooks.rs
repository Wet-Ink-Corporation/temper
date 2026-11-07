//! The hook protocol's values and texts: the events, what a hook answers, the skill-load rule
//! and the telemetry rows (1.x `scripts/temper.mjs` `hook()` and `hooks.mjs`).

mod bash;
mod edit;
mod event;
mod iron_rules;
mod outcome;
mod setup;
mod skill;
mod telemetry;

pub use bash::{
    BashPolicy, ConfigAsk, ConfigChanges, CopiedFile, InterpreterCode, OpaqueKind, RaiseFlag,
    RustWrites, Unread,
};
pub use edit::{
    DependencyCheck, EditFacts, EditOp, EditStep, EditTarget, EditTool, Exemptions, LazyExemptions,
    ManifestEdit, PolicyDecision, Replacements, RustEdit, STATE_DIR_DENIAL, apply_edit,
    edit_policy, lockfile_denial,
};
pub use event::{HookEvent, UnknownHookEvent};
pub use iron_rules::{IRON_RULES, skill_gate_denial};
pub use outcome::{ContextEvent, HookOutcome};
pub use setup::PolicySetupError;
pub use skill::{SKILL_NOT_LOADED, SkillVia, is_reviewer, skill_load_via, skill_preloaded};
pub use telemetry::{
    ProtectedChange, ShellAskVia, ShellDenyVia, ShellWriteFile, TelemetryEvent, TelemetryRow,
};

#[cfg(test)]
mod tests;
