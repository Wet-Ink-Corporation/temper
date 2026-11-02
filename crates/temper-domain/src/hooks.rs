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

pub use bash::{BashPolicy, RaiseFlag};
pub use edit::{
    DependencyCheck, EditFacts, EditOp, EditStep, EditTarget, EditTool, Exemptions, ManifestEdit,
    PolicyDecision, Replacements, RustEdit, STATE_DIR_DENIAL, apply_edit, edit_policy,
    lockfile_denial,
};
pub use event::{HookEvent, UnknownHookEvent};
pub use iron_rules::{IRON_RULES, skill_gate_denial};
pub use outcome::{ContextEvent, HookOutcome};
pub use setup::PolicySetupError;
pub use skill::{SKILL_NOT_LOADED, SkillVia, skill_load_via, skill_preloaded};
pub use telemetry::{ShellAskVia, ShellWriteFile, TelemetryEvent, TelemetryRow};

#[cfg(test)]
mod tests;
