//! `subagent-stop` (1.x `hooks.mjs` `subagentStop`): the `subagent-stop` row for any agent
//! type, then the reviewer's verdict, checked and recorded unless a hand-back already did. The
//! implementer branch (1.x `completionGate`) waits for port Phase 4 (3-19).

use std::borrow::Cow;
use std::path::Path;

use temper_domain::{
    ExtractedVerdict, HookEvent, HookOutcome, JsValue, TelemetryEvent, VerdictVia,
    extract_verdict_from, is_implementer, is_reviewer, verdict_block,
};

use super::transcript::report_from_transcript;
use super::verdicts::{Checked, check_verdict, invalid_detail, store_verdict};
use super::{HookCall, agent_key, logged, logged_then, session_segment};
use crate::engine::HookError;
use crate::paths;
use crate::ports::{Files, Ports};
use crate::project::Workspace;
use crate::session::session_dir;
use crate::state::read_js_text;

/// 1.x `subagentStop(input)`, in its order: the row before the type is filtered (#97), then
/// for the reviewer its verdict, and for the implementer [`HookError::NotPorted`].
///
/// # Errors
///
/// [`HookError::NullInput`] for `null` input; [`HookError::InputType`] where 1.x throws on a
/// value it converts or reads (the agent type, the session or agent id, a report that is not a
/// string, the transcript's hand-back) or while checking the verdict; [`HookError::Files`] when
/// the hand-back's hash file cannot be removed or a valid verdict cannot be stored (F4);
/// [`HookError::NotPorted`] for the implementer.
pub(crate) fn subagent_stop(call: &HookCall<'_, impl Ports>) -> Result<HookOutcome, HookError> {
    let input = call.input;
    input.require_object("agent_type")?;
    let Some(ws) = call.workspace() else {
        return Ok(HookOutcome::Pass);
    };
    let event = TelemetryEvent::SubagentStop {
        agent_type: input
            .given("agent_type")
            .map_or_else(|| r#""""#.to_owned(), JsValue::to_json),
        msg: matches!(
            input.raw("last_assistant_message"),
            Some(JsValue::String(_))
        ),
        transcript: input
            .raw("agent_transcript_path")
            .is_some_and(JsValue::is_truthy),
    };
    logged_then(call, &ws, &event, || after_row(call, &ws))
}

/// The rest of [`subagent_stop`] after its row.
fn after_row(call: &HookCall<'_, impl Ports>, ws: &Workspace) -> Result<HookOutcome, HookError> {
    let agent_type = match call.input.given("agent_type") {
        Some(value) => value.to_js_string_checked()?,
        None => Cow::Borrowed(""),
    };
    if is_reviewer(&agent_type) {
        return reviewer(call, ws);
    }
    if is_implementer(&agent_type) {
        return Err(HookError::NotPorted {
            event: HookEvent::SubagentStop,
        });
    }
    Ok(HookOutcome::Pass)
}

/// 1.x's reviewer branch: the verdict of the final message, else of the hand-back in the
/// transcript; skipped when this agent already recorded one and the report has no verdict or
/// is the hand-back's own (its one-shot hash, #136); otherwise blocked with its problems (let
/// through on the `stop_hook_active` repeat) or stored.
fn reviewer(call: &HookCall<'_, impl Ports>, ws: &Workspace) -> Result<HookOutcome, HookError> {
    let HookCall { ports, env, input } = *call;
    let (files, style) = (ports.files(), env.style);
    let dir = session_dir(style, ws, &session_segment(input)?);
    let marker = paths::join(style, &dir, &format!("verdict-{}", agent_key(input)?));
    let already = read_js_text(files, &marker).is_some_and(|text| !text.is_empty());
    let transcribed;
    let report = if let Some(message) = input.given("last_assistant_message") {
        Some(message)
    } else {
        transcribed = report_from_transcript(files, env, input.raw("agent_transcript_path"))?
            .map(JsValue::String);
        transcribed.as_ref()
    };
    let verdict = extract_verdict_from(report)?;
    let hash_file = format!("{marker}.hash");
    let seen = read_js_text(files, &hash_file);
    if seen.is_some() {
        files.remove_file(Path::new(&hash_file))?;
    }
    // 1.x `already && (!v || (seen !== null && seen === verdictHash(v)))`, hashed only then.
    let repeated = |v: &ExtractedVerdict| seen.as_deref().is_some_and(|seen| seen == v.hash());
    if already && verdict.as_ref().is_none_or(repeated) {
        return Ok(HookOutcome::Pass);
    }
    match check_verdict(call, ws, verdict)? {
        Checked::Invalid(problems) => {
            let invalid = invalid_detail(call, &problems);
            if input
                .raw("stop_hook_active")
                .is_some_and(JsValue::is_truthy)
            {
                let event = TelemetryEvent::VerdictInvalidFinal(invalid);
                return Ok(logged(call, ws, &event, HookOutcome::Pass));
            }
            let event = TelemetryEvent::VerdictInvalid {
                via: VerdictVia::SubagentStop,
                invalid,
            };
            let block = HookOutcome::Block {
                reason: verdict_block(&problems),
            };
            Ok(logged(call, ws, &event, block))
        }
        Checked::Valid(verdict) => {
            store_verdict(call, ws, &verdict, VerdictVia::SubagentStop)?;
            Ok(HookOutcome::Pass)
        }
    }
}
