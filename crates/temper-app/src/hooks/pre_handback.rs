//! `pre-handback` (1.x `hooks.mjs` `preHandback`): a reviewer's verdict is checked before its
//! `SubagentHandback` report is delivered, and an invalid one is denied with the problem list
//! (#97).

use std::borrow::Cow;

use temper_domain::{
    HookOutcome, JsValue, TelemetryEvent, VerdictVia, extract_verdict, handback_denial, is_reviewer,
};

use super::input::given;
use super::verdicts::{Checked, check_verdict, invalid_detail, store_verdict};
use super::{HookCall, logged, logged_then};
use crate::engine::HookError;
use crate::ports::Ports;
use crate::project::Workspace;

/// 1.x `preHandback(input)`, in its order: the `handback` row for any agent type, then for the
/// reviewer the verdict in `tool_input.message`, denied with its problems or stored.
///
/// # Errors
///
/// [`HookError::NullInput`] for `null` input; [`HookError::InputType`] where 1.x throws on a
/// value it converts (the agent type, the message, the verdict's fingerprint, the session or
/// agent id) or while checking the verdict; [`HookError::Files`] when storing a valid verdict
/// fails (F3).
pub(crate) fn pre_handback(call: &HookCall<'_, impl Ports>) -> Result<HookOutcome, HookError> {
    let input = call.input;
    input.require_object("cwd")?;
    let Some(ws) = call.workspace() else {
        return Ok(HookOutcome::Pass);
    };
    let given_type = input.given("agent_type");
    // `REVIEWER.test(type)` converts the type while the row's detail is built, before it is
    // written.
    let agent_type = match given_type {
        Some(value) => value.to_js_string_checked()?,
        None => Cow::Borrowed(""),
    };
    let reviewer = is_reviewer(&agent_type);
    let event = TelemetryEvent::Handback {
        agent_type: given_type.map_or_else(|| r#""""#.to_owned(), JsValue::to_json),
        reviewer,
    };
    if !reviewer {
        return Ok(logged(call, &ws, &event, HookOutcome::Pass));
    }
    logged_then(call, &ws, &event, || checked(call, &ws))
}

/// The reviewer's hand-back after its row: the verdict denied with its problems, or stored.
fn checked(call: &HookCall<'_, impl Ports>, ws: &Workspace) -> Result<HookOutcome, HookError> {
    let message = given(call.input.given("tool_input"), "message");
    let report = match message {
        Some(value) => value.to_js_string_checked()?,
        None => Cow::Borrowed(""),
    };
    match check_verdict(call, ws, extract_verdict(&report))? {
        Checked::Invalid(problems) => {
            let event = TelemetryEvent::VerdictInvalid {
                via: VerdictVia::Handback,
                invalid: invalid_detail(call, &problems),
            };
            let deny = HookOutcome::Deny {
                reason: handback_denial(&problems),
                context: None,
            };
            Ok(logged(call, ws, &event, deny))
        }
        Checked::Valid(verdict) => {
            store_verdict(call, ws, &verdict, VerdictVia::Handback)?;
            Ok(HookOutcome::Pass)
        }
    }
}
