//! The hook protocol's values and texts: the events, what a hook answers, the skill-load rule
//! and the telemetry rows (1.x `scripts/temper.mjs` `hook()` and `hooks.mjs`).

mod event;
mod outcome;
mod skill;
mod telemetry;

pub use event::{HookEvent, UnknownHookEvent};
pub use outcome::{ContextEvent, HookOutcome};
pub use skill::{SKILL_NOT_LOADED, SkillVia, skill_load_via};
pub use telemetry::{TelemetryEvent, TelemetryRow};

#[cfg(test)]
mod tests;
