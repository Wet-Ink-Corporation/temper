//! A running gate's progress file, `gate-progress-<fingerprint>.json` (1.x `gate.mjs`
//! `gateProgress`).

use crate::js::{JsTypeError, Value, js_string_checked, number_from_text};
use crate::{Fingerprint, Timestamp};

/// 1.x `STALE_MARGIN_SEC`: a ratchet step runs `cargo metadata` before clippy, so a step may
/// outlive the step timeout by this much.
const STALE_MARGIN_SEC: f64 = 300.0;

/// What [`gate_progress`] asks of a progress file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressQuery {
    /// The tree whose gate is asked about.
    pub fp: Fingerprint,
    /// The time now (1.x `Date.now()`).
    pub now: Timestamp,
    /// The config's `gate.stepTimeoutSec`.
    pub step_timeout_sec: f64,
}

/// 1.x `gateProgress(proj, fp, now)` over the progress file's text: the progress of a gate
/// running now for the query's tree, as `{ startedAt, steps, current }`, or `None`.
///
/// It is `None` when the text is empty or not JSON (D16(n), OC-3 A), is for another
/// fingerprint, names a process `alive` says is gone (asked only once the fingerprint
/// matches, at most once), or when its current step (or its start, between steps) began
/// longer than the step timeout plus 300 s before `now`. A time `Date.parse` cannot read is
/// stale too, and `startedAt` is left out of the answer when the file has none, as
/// `JSON.stringify` drops `undefined`.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x's `Date.parse` throws converting the time to a string (an
/// object with its own `toString` key, also inside an array), which fails 1.x's `status`.
///
/// # Examples
///
/// ```
/// use temper_domain::{JsValue, ProgressQuery, Timestamp, gate_progress};
///
/// let text = r#"{"fp":"0123456789abcdef","pid":7,"startedAt":"2026-10-11T11:59:00.000Z"}"#;
/// let query = ProgressQuery {
///     fp: "0123456789abcdef".parse()?,
///     now: Timestamp::parse_js("2026-10-11T12:00:00.000Z").ok_or("not a time")?,
///     step_timeout_sec: 900.0,
/// };
/// let alive = |pid: &JsValue| *pid == JsValue::Number(7.0);
/// let running = gate_progress(text, &query, alive)?;
/// assert_eq!(
///     running.map(|p| p.to_json()).as_deref(),
///     Some(r#"{"startedAt":"2026-10-11T11:59:00.000Z","steps":[],"current":null}"#)
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn gate_progress(
    text: &str,
    query: &ProgressQuery,
    alive: impl FnOnce(&Value) -> bool,
) -> Result<Option<Value>, JsTypeError> {
    let ProgressQuery {
        fp,
        now,
        step_timeout_sec,
    } = *query;
    // `if (!t) return null`, and not JSON is `catch { return null; }`. Any value but an
    // object has no own `fp`, so `p.fp !== fp` holds for it.
    let Ok(Value::Object(mut p)) = Value::from_json(text) else {
        return Ok(None);
    };
    let ours = matches!(p.get("fp"), Some(Value::String(f)) if *f == fp.to_string());
    if !ours || !p.get("pid").is_some_and(alive) {
        return Ok(None);
    }
    // `p.current?.startedAt ?? p.startedAt`; `Date.parse(undefined)` is NaN.
    let current_at = p
        .get("current")
        .and_then(|c| c.get("startedAt"))
        .filter(|at| **at != Value::Null);
    let since = match current_at.or_else(|| p.get("startedAt")) {
        Some(at) => Timestamp::parse_js(&js_string_checked(at)?),
        None => None,
    };
    let Some(since) = since else {
        return Ok(None);
    };
    // `now - since` in doubles: the exact difference, rounded once, as IEEE subtraction is.
    let age = now.unix_millis().saturating_sub(since.unix_millis());
    if number_from_text(&age.to_string()) > (step_timeout_sec + STALE_MARGIN_SEC) * 1000.0 {
        return Ok(None);
    }
    let steps = match p.remove("steps") {
        Some(steps @ Value::Array(_)) => steps,
        _ => Value::Array(Vec::new()),
    };
    let current = p.remove("current").unwrap_or(Value::Null);
    let started = p.remove("startedAt").map(|at| ("startedAt".to_owned(), at));
    let props = started
        .into_iter()
        .chain([("steps".to_owned(), steps), ("current".to_owned(), current)]);
    Ok(Some(Value::Object(props.collect())))
}

#[cfg(test)]
mod tests;
