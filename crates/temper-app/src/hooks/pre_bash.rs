//! `pre-bash`: `Bash` calls are checked before they run (1.x `hooks.mjs` `preBash`).
//!
//! The checks run in 1.x's order: for the reviewer, any patch and any command temper cannot
//! rule out applies one; the shell writes to `.rs` files that the command's text shows; the
//! copies, moves, links and patches onto `.rs` files the shell reader finds, scanned as an
//! Edit or Write would be, whose ask or advisories wait for the checks after them; the shell
//! writes to `Cargo.lock`, `Cargo.toml` and the baseline; the state-dir writes; the
//! verification bypasses; then protected configuration written, deleted or patched, whose ask
//! is shown with the dependency or snapshot ask. Where 1.x's reader throws, a rule gives
//! `readerRule`'s fallback.

use temper_domain::{
    BashPolicy, ConfigAsk, DependencyCheck, HookOutcome, PolicyDecision, Scanner, is_reviewer,
};

use super::input::js_string_or_empty;
use super::{HookCall, answered, logged_then};
use crate::engine::HookError;
use crate::paths;
use crate::ports::Ports;
use crate::project::Workspace;

/// `pre-bash`: the policy's stages on `tool_input.command`, in 1.x's order.
///
/// # Errors
///
/// [`HookError::NullInput`] on `null` input, as 1.x's first read throws, and
/// [`HookError::Setup`] when a built-in pattern does not compile.
pub(crate) fn pre_bash(
    call: &HookCall<'_, impl Ports>,
    scanner: &Scanner,
) -> Result<HookOutcome, HookError> {
    let input = call.input;
    input.require_object("tool_input")?;
    // 1.x `String(input.tool_input?.command ?? '')`.
    let tool_input = input.given("tool_input");
    let cmd = js_string_or_empty(super::input::given(tool_input, "command"));
    let Some(workspace) = call.workspace() else {
        return Ok(HookOutcome::Pass);
    };
    let policy = BashPolicy::new()?;
    // 1.x `REVIEWER.test(input.agent_type ?? '')`.
    let agent = js_string_or_empty(input.given("agent_type"));
    if is_reviewer(&agent)
        && let Some(decision) = reviewer::reviewer_patch(call, &workspace, &cmd, policy.code())
    {
        return Ok(answered(call, &workspace, decision));
    }
    if let Some(decision) = policy.rust_source_write(&cmd) {
        return Ok(answered(call, &workspace, decision));
    }
    let scan = rust_writes::Scan {
        policy: &policy,
        scanner,
    };
    // Only a deny returns at once; an ask or advisory waits for the checks below, and a deny
    // from them wins.
    if let Some(own) = rust_writes::shell_rust_writes(call, &workspace, &cmd, &scan) {
        let own = answered(call, &workspace, own);
        if matches!(own, HookOutcome::Deny { .. }) {
            return Ok(own);
        }
        let rest = rest(call, &workspace, &cmd, &policy);
        return Ok(merged(own, rest));
    }
    Ok(rest(call, &workspace, &cmd, &policy))
}

/// 1.x's merge of the copy scan's ask or advisories with the answer of the checks after it:
/// their deny wins; their ask carries the copy's context, or follows the copy's own ask.
fn merged(own: HookOutcome, rest: HookOutcome) -> HookOutcome {
    match (own, rest) {
        (_, rest @ HookOutcome::Deny { .. }) => rest,
        (
            HookOutcome::Ask { reason, context },
            HookOutcome::Ask {
                reason: other,
                context: _,
            },
        ) => HookOutcome::Ask {
            reason: format!("{reason}\n{other}"),
            context,
        },
        (own, HookOutcome::Ask { reason, context }) => {
            let own_context = match own {
                HookOutcome::Context { text, .. } => Some(text),
                HookOutcome::Ask { context, .. } | HookOutcome::Deny { context, .. } => context,
                HookOutcome::Pass | HookOutcome::Notice { .. } => None,
            };
            let joined: Vec<String> = [context, own_context]
                .into_iter()
                .flatten()
                .filter(|c| !c.is_empty())
                .collect();
            HookOutcome::Ask {
                reason,
                context: Some(joined.join("\n")),
            }
        }
        (own, _) => own,
    }
}

/// The checks after the copy scan (1.x's recursive `preBash` with `RS_COPY_SCANNED`), in
/// 1.x's order.
fn rest(
    call: &HookCall<'_, impl Ports>,
    workspace: &Workspace,
    cmd: &str,
    policy: &BashPolicy,
) -> HookOutcome {
    if let Some(decision) = policy.cargo_or_baseline_write(cmd) {
        return answered(call, workspace, decision);
    }
    if state_dir::state_dir_write(call, workspace, cmd, policy.code()) {
        return answered(call, workspace, policy.state_dir_write(cmd));
    }
    if let Some(decision) = policy.verification_bypass(cmd) {
        return answered(call, workspace, decision);
    }
    // Protected configuration written, deleted or patched from the shell asks, after every
    // deny above, and its ask is shown with the dependency or snapshot ask below.
    let changes = protected::protected_shell_writes(call, workspace, cmd, policy.code());
    let config_ask = ConfigAsk::new(changes.as_domain());
    let config = &workspace.config;
    let dependencies = if config.ask_on_new_dependency() {
        DependencyCheck::Ask {
            preapproved: config.preapproved_dependencies(),
        }
    } else {
        DependencyCheck::Off
    };
    let decision = policy.dependency_or_snapshot(cmd, &dependencies);
    match config_ask {
        None => decision.map_or(HookOutcome::Pass, |decision: PolicyDecision| {
            answered(call, workspace, decision)
        }),
        // The config row comes first, as 1.x logs it before the dependency check.
        Some(ask) => {
            let row = ask.telemetry().clone(); // the ask itself is consumed by the answer
            logged_then(call, workspace, &row, || match decision {
                Some(decision) => answered(call, workspace, ask.before(decision)),
                None => ask.into_outcome(),
            })
        }
    }
}

/// 1.x `input.cwd ?? proj.root`, resolved as the shell reader's `parseCommands` resolves it;
/// `None` where `path.resolve` throws on a `cwd` that is not a string.
fn start_dir(call: &HookCall<'_, impl Ports>, ws: &Workspace) -> Option<String> {
    let env = call.env;
    let cwd = env.cwd.to_string_lossy();
    match call.input.given("cwd").map(|dir| dir.as_str()) {
        None => Some(paths::resolve(env.style, &cwd, &ws.root)),
        Some(Some(dir)) => Some(paths::resolve(env.style, &cwd, dir)),
        Some(None) => None,
    }
}

mod protected;
mod reviewer;
mod rust_writes;
mod state_dir;

#[cfg(test)]
mod tests;
