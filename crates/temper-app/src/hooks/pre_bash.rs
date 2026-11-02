//! `pre-bash`: `Bash` calls are checked before they run (1.x `hooks.mjs` `preBash`).
//!
//! Port Phase 2d brings the checks that read only the command's text, in 1.x's order. The
//! ones that read its words with 1.x's shell reader (`scripts/lib/shell.mjs`) and the file
//! system arrive with that reader: the reviewer's patch denials and copies onto `.rs` files in
//! Phase 2f, state-dir writes and protected configuration in Phase 2e. Each is marked where it
//! goes. `pre-bash` graduates in 2f.

use temper_domain::{BashPolicy, DependencyCheck, HookOutcome};

use super::input::js_string_or_empty;
use super::{HookCall, answered};
use crate::engine::HookError;
use crate::ports::Ports;

/// `pre-bash`: the policy's stages on `tool_input.command`, in 1.x's order.
///
/// # Errors
///
/// [`HookError::NullInput`] on `null` input, as 1.x's first read throws, and
/// [`HookError::Setup`] when a built-in pattern does not compile.
pub(crate) fn pre_bash(call: &HookCall<'_, impl Ports>) -> Result<HookOutcome, HookError> {
    let input = call.input;
    input.require_object("tool_input")?;
    // 1.x `String(input.tool_input?.command ?? '')`.
    let tool_input = input.given("tool_input");
    let cmd = js_string_or_empty(super::input::given(tool_input, "command"));
    let Some(workspace) = call.workspace() else {
        return Ok(HookOutcome::Pass);
    };
    let policy = BashPolicy::new()?;
    // Phase 2f: the reviewer's patch and opaque-command denials come first.
    if let Some(decision) = policy.rust_source_write(&cmd) {
        return Ok(answered(call, &workspace, decision));
    }
    // Phase 2f: copies onto `.rs` files and `git apply`, scanned like an Edit or Write.
    if let Some(decision) = policy.cargo_or_baseline_write(&cmd) {
        return Ok(answered(call, &workspace, decision));
    }
    // Phase 2e: shell writes into the state dir.
    if let Some(decision) = policy.verification_bypass(&cmd) {
        return Ok(answered(call, &workspace, decision));
    }
    // Phase 2e: protected configuration written, deleted or patched from the shell asks, and
    // its ask is shown with the ones below.
    let config = &workspace.config;
    let dependencies = if config.ask_on_new_dependency() {
        DependencyCheck::Ask {
            preapproved: config.preapproved_dependencies(),
        }
    } else {
        DependencyCheck::Off
    };
    Ok(match policy.dependency_or_snapshot(&cmd, &dependencies) {
        Some(decision) => answered(call, &workspace, decision),
        None => HookOutcome::Pass,
    })
}

#[cfg(test)]
mod tests;
