//! The hook protocol's values and texts: the events, what a hook answers, the skill-load rule
//! and the telemetry rows (1.x `scripts/temper.mjs` `hook()` and `hooks.mjs`).

mod bash;
mod edit;
mod event;
mod iron_rules;
mod lost_gate;
mod outcome;
mod session;
mod setup;
mod skill;
mod subagent;
mod telemetry;

pub use bash::{
    BashPolicy, ConfigAsk, ConfigChanges, CopiedFile, InterpreterCode, OpaqueKind, RaiseFlag,
    RustWrites, Unread,
};
pub use edit::{
    DependencyCheck, EditFacts, EditOp, EditStep, EditTarget, EditTool, LazyExemptions,
    ManifestEdit, PolicyDecision, Replacements, RustEdit, STATE_DIR_DENIAL, apply_edit,
    edit_policy, lockfile_denial,
};
pub use event::{HookEvent, UnknownHookEvent};
pub use iron_rules::{IRON_RULES, skill_gate_denial};
pub use lost_gate::{
    LostGate, LostGateFacts, LostReason, ProgressSeen, lost_gate_claim, lost_gate_row,
    open_gate_starts,
};
pub use outcome::{ContextEvent, HookOutcome};
pub use session::{
    EnvExports, SessionStartFacts, SessionState, TreeState, env_file_lines, session_start_context,
    with_runtime_warnings,
};
pub use setup::PolicySetupError;
pub use skill::{
    SKILL_NOT_LOADED, SkillVia, is_implementer, is_reviewer, skill_load_via, skill_preloaded,
};
pub use subagent::{
    CachedGate, OtherTree, ReviewerExtras, SubagentStartFacts, Target, subagent_start_context,
    tree_gate_command,
};
pub use telemetry::{
    InvalidVerdict, ProtectedChange, ShellAskVia, ShellDenyVia, ShellWriteFile, TelemetryEvent,
    TelemetryRow, VerdictRow, VerdictVia,
};

#[cfg(test)]
mod tests;
