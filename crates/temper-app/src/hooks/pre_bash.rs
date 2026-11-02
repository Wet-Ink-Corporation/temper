//! `pre-bash`: `Bash` calls are checked before they run (1.x `hooks.mjs` `preBash`).
//!
//! The checks run in 1.x's order: the shell writes to `.rs` files, `Cargo.lock`,
//! `Cargo.toml` and the baseline that the command's text shows; the state-dir writes the shell
//! reader (`hooks::shell`) finds; the verification bypasses; then protected configuration
//! written, deleted or patched, whose ask is shown with the dependency or snapshot ask. Where
//! 1.x's reader throws, a rule gives `readerRule`'s fallback. Two of 1.x's checks are not here
//! yet, and the comments mark where they go: the reviewer's patch denials and the scan of
//! copies and patches onto `.rs` files (port plan item 45 to 48).

use temper_domain::{BashPolicy, ConfigAsk, DependencyCheck, HookOutcome};

use super::input::js_string_or_empty;
use super::{HookCall, answered, logged_then};
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
    if state_dir::state_dir_write(call, &workspace, &cmd) {
        return Ok(answered(call, &workspace, policy.state_dir_write(&cmd)));
    }
    if let Some(decision) = policy.verification_bypass(&cmd) {
        return Ok(answered(call, &workspace, decision));
    }
    // Protected configuration written, deleted or patched from the shell asks, after every
    // deny above, and its ask is shown with the dependency or snapshot ask below.
    let changes = protected::protected_shell_writes(call, &workspace, &cmd);
    let config_ask = ConfigAsk::new(changes.as_domain());
    let config = &workspace.config;
    let dependencies = if config.ask_on_new_dependency() {
        DependencyCheck::Ask {
            preapproved: config.preapproved_dependencies(),
        }
    } else {
        DependencyCheck::Off
    };
    let decision = policy.dependency_or_snapshot(&cmd, &dependencies);
    Ok(match config_ask {
        None => decision.map_or(HookOutcome::Pass, |decision| {
            answered(call, &workspace, decision)
        }),
        // The config row comes first, as 1.x logs it before the dependency check.
        Some(ask) => {
            let row = ask.telemetry().clone(); // the ask itself is consumed by the answer
            logged_then(call, &workspace, &row, || match decision {
                Some(decision) => answered(call, &workspace, ask.before(decision)),
                None => ask.into_outcome(),
            })
        }
    })
}

mod protected;
mod state_dir;

#[cfg(test)]
mod tests;
