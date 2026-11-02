//! The hook handlers behind `temper hook <event>` (1.x `hooks.mjs`), one module per family.

mod input;
mod pre_bash;
mod pre_edit;
mod shell;
mod skill;

use temper_domain::{HookEvent, HookOutcome, PolicyDecision, Scanner, TelemetryEvent};

pub use input::{HookInput, HookInputError};

use crate::engine::{HookError, RunEnv};
use crate::paths;
use crate::ports::Ports;
use crate::project::{Workspace, discover_at, exists};
use crate::telemetry;

/// One hook call: the ports, the run environment and the input, as every handler reads them.
#[derive(Debug)]
pub(crate) struct HookCall<'a, P> {
    pub(crate) ports: &'a P,
    pub(crate) env: &'a RunEnv,
    pub(crate) input: &'a HookInput,
}

/// The body of [`Engine::hook`](crate::Engine::hook): `event`'s handler on `call`, with the
/// compiled `scanner`, which `pre-edit` reads.
pub(crate) fn run<P: Ports>(
    call: &HookCall<'_, P>,
    scanner: &Scanner,
    event: HookEvent,
) -> Result<HookOutcome, HookError> {
    match event {
        HookEvent::UserPrompt => skill::user_prompt(call),
        HookEvent::PostTool => skill::post_tool(call),
        HookEvent::PreEdit => pre_edit::pre_edit(call, scanner),
        HookEvent::PreBash => pre_bash::pre_bash(call),
        HookEvent::SessionStart
        | HookEvent::PreHandback
        | HookEvent::Stop
        | HookEvent::SubagentStop
        | HookEvent::SubagentStart => Err(HookError::NotPorted { event }),
    }
}

/// `outcome`, after a telemetry row for `event`.
fn logged(
    call: &HookCall<'_, impl Ports>,
    workspace: &Workspace,
    event: &TelemetryEvent,
    outcome: HookOutcome,
) -> HookOutcome {
    match telemetry::log(call, workspace, event) {
        // Telemetry is best effort (1.x): a row that cannot be written does not change the
        // answer.
        Ok(()) | Err(_) => outcome,
    }
}

/// The outcome `then` gives, after a telemetry row for `event`: `then` may log rows of its own,
/// which follow this one.
fn logged_then(
    call: &HookCall<'_, impl Ports>,
    workspace: &Workspace,
    event: &TelemetryEvent,
    then: impl FnOnce() -> HookOutcome,
) -> HookOutcome {
    match telemetry::log(call, workspace, event) {
        // Best effort, as in `logged`.
        Ok(()) | Err(_) => then(),
    }
}

/// A policy's answer, after its telemetry row if it has one (2-14 A: written where 1.x writes
/// it).
fn answered(
    call: &HookCall<'_, impl Ports>,
    workspace: &Workspace,
    decision: PolicyDecision,
) -> HookOutcome {
    match &decision.telemetry {
        Some(event) => logged(call, workspace, event, decision.outcome),
        None => decision.outcome,
    }
}

impl<P: Ports> HookCall<'_, P> {
    /// 1.x `discover(input.cwd)`: the project of the input's `cwd` when that is a non-empty
    /// string naming something that exists, else of the working directory.
    fn workspace(&self) -> Option<Workspace> {
        let cwd = self.env.cwd.to_string_lossy();
        let start = self
            .input
            .raw("cwd")
            .and_then(|dir| dir.as_str())
            .filter(|dir| !dir.is_empty())
            .map(|dir| paths::resolve(self.env.style, &cwd, dir))
            .filter(|dir| exists(self.ports.files(), dir));
        match start {
            Some(dir) => discover_at(self.ports, self.env, &dir),
            None => discover_at(self.ports, self.env, &cwd),
        }
    }

    /// 1.x `discoverFor(input, file)`: the project of the nearest existing directory above
    /// `file`, else of the input's `cwd` as [`Self::workspace`] finds it.
    fn workspace_for_file(&self, file: &str) -> Option<Workspace> {
        let (style, files) = (self.env.style, self.ports.files());
        let cwd = self.env.cwd.to_string_lossy();
        let abs = paths::resolve(style, &cwd, file);
        let mut dir = paths::dirname(style, &abs);
        while !exists(files, dir) {
            let parent = paths::dirname(style, dir);
            if parent == dir {
                break;
            }
            dir = parent;
        }
        // 1.x's `discover(dir)` starts from the working directory when `dir` does not exist.
        let start = if exists(files, dir) { dir } else { &cwd };
        discover_at(self.ports, self.env, start).or_else(|| self.workspace())
    }
}

#[cfg(test)]
mod tests;
