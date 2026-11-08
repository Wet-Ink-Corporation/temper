//! Checking a reviewer's verdict against the tree it reviews and storing a valid one (1.x
//! `hooks.mjs` `checkVerdict` and `storeVerdict`), shared by `pre-handback` and `subagent-stop`.

use std::path::Path;

use temper_domain::{
    ExtractedVerdict, GateState, InvalidVerdict, JsValue, TelemetryEvent, ValidVerdict,
    VerdictContext, VerdictProblem, VerdictRejected, VerdictRow, VerdictVia, problem_kinds,
    verdict_hash, verdict_problems,
};

use super::{HookCall, agent_key, session_segment};
use crate::changes::{fingerprint, rust_changes};
use crate::engine::HookError;
use crate::paths;
use crate::ports::{Files, Ports};
use crate::project::Workspace;
use crate::session::session_dir;
use crate::state::{RecordMeta, cached_gate_named, record_verdict};
use crate::telemetry;
use crate::trees::reviewed_tree;

/// A verdict after [`check_verdict`].
#[derive(Debug)]
pub(super) enum Checked {
    /// No problem: ready to store.
    Valid(ValidVerdict),
    /// 1.x's problem list, never empty.
    Invalid(Vec<VerdictProblem>),
}

/// 1.x `checkVerdict(proj, input, v)`: `verdict`'s problems against the Rust files changed in
/// the tree it reviews ([`reviewed_tree`]) and the cached gate its `fingerprint` names.
///
/// # Errors
///
/// [`HookError::InputType`] where 1.x throws: in [`reviewed_tree`], on a `fingerprint` that
/// `String` cannot convert (`gate/${fp}.json`), and in `verdictProblems`.
pub(super) fn check_verdict(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    verdict: Option<ExtractedVerdict>,
) -> Result<Checked, HookError> {
    let HookCall { ports, env, .. } = *call;
    let fp = verdict.as_ref().and_then(|v| v.field("fingerprint"));
    let tree = reviewed_tree(call, ws, fp)?;
    let gate = match fp.filter(|fp| fp.is_truthy()) {
        Some(fp) => cached_gate_named(ports.files(), env.style, ws, &fp.to_js_string_checked()?),
        None => None,
    };
    let mut changed = rust_changes(ports, env, &tree.ws, tree.scope.base.as_deref());
    changed.retain(|f| f.ends_with(".rs"));
    let ctx = VerdictContext {
        changed_rust: &changed,
        gate: gate.map_or(GateState::NotRed, |g| g.state()),
    };
    let Some(verdict) = verdict else {
        return Ok(Checked::Invalid(verdict_problems(None, &ctx)?));
    };
    match ValidVerdict::check(verdict, &ctx) {
        Ok(valid) => Ok(Checked::Valid(valid)),
        Err(VerdictRejected::Problems { problems, .. }) => Ok(Checked::Invalid(problems)),
        Err(VerdictRejected::Throws { source, .. }) => Err(source.into()),
    }
}

/// The detail of a `verdict-invalid` row for `problems`, with the input's `agent_id`.
pub(super) fn invalid_detail(
    call: &HookCall<'_, impl Ports>,
    problems: &[VerdictProblem],
) -> InvalidVerdict {
    InvalidVerdict {
        problems: problems.len(),
        kinds: problem_kinds(problems),
        agent_id: call.input.raw("agent_id").map(JsValue::to_json),
    }
}

/// 1.x `storeVerdict(proj, input, v, via)`: records `verdict`, flagged stale when the reviewed
/// tree has moved on, writes the session's `verdict-<agent>` with its fingerprint and, from a
/// hand-back, `verdict-<agent>.hash` for this agent's `subagent-stop` to skip it, then the
/// `verdict` row.
///
/// # Errors
///
/// [`HookError::Files`] when a write fails, and [`HookError::InputType`] when the session or
/// agent id is a value 1.x's `String` throws on; both after the record is written, as in 1.x.
pub(super) fn store_verdict(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    verdict: &ValidVerdict,
    via: VerdictVia,
) -> Result<(), HookError> {
    let HookCall { ports, env, input } = *call;
    let style = env.style;
    let fp = verdict.fingerprint();
    let named = JsValue::String(fp.to_string());
    let tree = reviewed_tree(call, ws, Some(&named))?;
    let stale = fp != fingerprint(ports, env, &tree.ws, tree.scope.base.as_deref());
    let agent_id = input.raw("agent_id");
    let rec = record_verdict(ports, style, ws, verdict, RecordMeta { agent_id, stale })?;
    let dir = session_dir(style, ws, &session_segment(input)?);
    let marker = paths::join(style, &dir, &format!("verdict-{}", agent_key(input)?));
    let files = ports.files();
    files.write_atomic(Path::new(&marker), &fp.to_string())?;
    if via == VerdictVia::Handback {
        // One-shot: this agent's `subagent-stop` skips the same report read back from its
        // transcript.
        let hash = format!("{marker}.hash");
        files.write_atomic(Path::new(&hash), &verdict_hash(verdict.value()))?;
    }
    let row = VerdictRow {
        via,
        agent_id: agent_id.map(JsValue::to_json),
        outcome: verdict.outcome(),
        fingerprint: fp,
        stale,
        findings: verdict.finding_count(),
        kept: rec
            .get("kept")
            .filter(|k| k.is_truthy())
            .map(JsValue::to_json),
    };
    match telemetry::log(call, ws, &TelemetryEvent::Verdict(row)) {
        // Telemetry is best effort (1.x): a row that cannot be written changes nothing.
        Ok(()) | Err(_) => Ok(()),
    }
}
