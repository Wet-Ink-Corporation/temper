//! The record 1.x's `recordVerdict` keeps for a valid verdict, and the condition under which a
//! later changes request leaves an approval in place (#136).

use crate::Timestamp;
use crate::js::{JsTypeError, Object, Value, truthy};

use super::summary::overturns;
use super::{ValidVerdict, VerdictOutcome};

/// The verdict's record: 1.x `{ ...v, recorded_at: new Date().toISOString(), ...meta }`.
///
/// `meta` is `{ agent_id: input.agent_id, stale }`: the verdict's own properties come in their
/// order, then `recorded_at`, `agent_id` and `stale`, each keeping its place when the verdict
/// already has it.
///
/// An absent `agent_id` is JS's `undefined`, which `JSON.stringify` leaves out, so a verdict's
/// own `agent_id` is dropped.
#[must_use]
pub fn verdict_record(
    verdict: &ValidVerdict,
    recorded_at: Timestamp,
    agent_id: Option<&Value>,
    stale: bool,
) -> Value {
    let mut rec = match verdict.value() {
        // The record is a new object beside the verdict, which the caller keeps for its hash
        // and telemetry.
        Value::Object(object) => object.clone(),
        // A valid verdict is an object (`temper_verdict` is one of its own properties); the
        // spread of a number, boolean or null is `{}`.
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            Object::default()
        }
    };
    rec.insert("recorded_at", Value::String(recorded_at.to_string()));
    match agent_id {
        // The record owns a copy of the input's id, which the hook input keeps.
        Some(id) => rec.insert("agent_id", id.clone()),
        None => {
            // `undefined`: the key is written nowhere, whatever the verdict held.
            rec.remove("agent_id");
        }
    }
    rec.insert("stale", Value::Bool(stale));
    Value::Object(rec)
}

/// 1.x `{ ...rec, kept: 'approved' }`: the record of a changes request that left an approval in
/// place, as appended to `verdicts/<fp>.history.jsonl`.
#[must_use]
pub fn kept_record(record: Value) -> Value {
    match record {
        Value::Object(mut rec) => {
            rec.insert("kept", Value::String("approved".to_owned()));
            Value::Object(rec)
        }
        // A record from [`verdict_record`] is always an object; `{ ...x }` of a scalar is `{}`.
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            let mut rec = Object::default();
            rec.insert("kept", Value::String("approved".to_owned()));
            Value::Object(rec)
        }
    }
}

impl ValidVerdict {
    /// 1.x `recordVerdict`'s condition: `prev?.verdict === 'approved' && !prev.stale &&
    /// v.verdict === 'changes-requested' && !overturns(v)`, where `previous` is the verdict
    /// recorded for the same fingerprint, any JSON value.
    ///
    /// # Errors
    ///
    /// [`JsTypeError`] where 1.x's `overturns(v)` throws; a verdict that passed its check never
    /// reaches one, since `overturns` reads only what the check read.
    pub fn keeps_approval_over(&self, previous: Option<&Value>) -> Result<bool, JsTypeError> {
        let Some(prev) = previous else {
            return Ok(false);
        };
        let approved = prev.get("verdict").and_then(Value::as_str) == Some("approved");
        let fresh = !prev.get("stale").is_some_and(truthy);
        if !(approved && fresh && self.outcome() == VerdictOutcome::ChangesRequested) {
            return Ok(false);
        }
        Ok(!overturns(self.value())?)
    }
}
