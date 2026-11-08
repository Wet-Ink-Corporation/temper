//! Gate runs whose hook was killed (1.x `hooks.mjs` `openGateStarts` and `lostGateRow`, #137).

use std::collections::HashMap;

use crate::js::{JsMap, JsTypeError, Object, Value, js_json, js_string_checked, json_string};
use crate::{ModTime, Timestamp};

/// The outcome rows that close a `gate-start` (1.x `STOP_OUTCOMES`), and `gate-lost`.
const CLOSERS: [&str; 7] = [
    "stop-green",
    "stop-blocked",
    "stop-allowed-red",
    "stop-allowed-unreviewed",
    "stop-advisory-unreviewed",
    "gate-unavailable",
    "gate-lost",
];

/// The heartbeat's interval (1.x `BEAT_MS`): a hook last seen within three of these of its
/// deadline was killed by the timeout.
const BEAT_MS: f64 = 10_000.0;

/// 1.x `openGateStarts(rows)`: the `gate-start` rows with no outcome after them.
///
/// A start and its outcome pair by `` `${agent ?? 'main'}\0${pid ?? ''}` ``, in the order a JS `Map` keeps them (a row closed
/// and opened again moves to the end).
///
/// # Errors
///
/// [`JsTypeError`] where 1.x's template literal throws on a row's `agent` or `pid` (an object
/// with its own `toString` key).
pub fn open_gate_starts(rows: &[Value]) -> Result<Vec<&Value>, JsTypeError> {
    let mut slots: Vec<Option<&Value>> = Vec::new();
    let mut open: HashMap<String, usize> = HashMap::new();
    for row in rows {
        let part = |key: &str, missing: &'static str| match row.get(key) {
            None | Some(Value::Null) => Ok(missing.into()),
            Some(value) => js_string_checked(value),
        };
        let key = [part("agent", "main")?, part("pid", "")?].join("\0");
        let event = row.get("event").and_then(Value::as_str);
        if event == Some("gate-start") {
            if let Some(slot) = open.get(&key).and_then(|&at| slots.get_mut(at)) {
                *slot = Some(row);
            } else {
                open.insert(key, slots.len());
                slots.push(Some(row));
            }
        } else if event.is_some_and(|event| CLOSERS.contains(&event))
            && let Some(at) = open.remove(&key)
            && let Some(slot) = slots.get_mut(at)
        {
            *slot = None;
        }
    }
    Ok(slots.into_iter().flatten().collect())
}

/// A gate's progress file as `logLostGates` read it, with its modification time (`mtimeMs`).
#[derive(Debug, Clone, Copy)]
pub struct ProgressSeen<'a> {
    /// The file's JSON.
    pub record: &'a Value,
    /// When it was last written.
    pub modified: ModTime,
}

/// What [`lost_gate_row`] judges a lost gate by.
#[derive(Debug, Clone, Copy)]
pub struct LostGateFacts<'a> {
    /// The `gate-start` row.
    pub start: &'a Value,
    /// The gate's progress file, if it left one.
    pub progress: Option<ProgressSeen<'a>>,
    /// The heartbeat file's JSON, if it parsed (`null` included).
    pub heartbeat: Option<&'a Value>,
    /// When the lost gate was found.
    pub now: Timestamp,
}

/// Why a gate run with no outcome ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostReason {
    /// Last seen alive near the hook's deadline.
    Timeout,
    /// Seen to stop short of it, or found before it.
    Interrupted,
    /// Neither can be told.
    Unknown,
}

impl LostReason {
    const fn name(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Interrupted => "interrupted",
            Self::Unknown => "unknown",
        }
    }
}

/// A `gate-lost` row's detail.
///
/// Its keys are in 1.x's order: `pid`, `reason`, `ranMs`, `foundMs`,
/// `timeoutMs`, `heartbeat`, `progress`, `step` and `steps`. Two are equal when
/// `JSON.stringify` writes them the same, which is all a row records.
#[derive(Debug, Clone)]
pub struct LostGate {
    reason: LostReason,
    detail: Object,
}

impl LostGate {
    /// Why the gate ended.
    #[must_use]
    pub fn reason(&self) -> LostReason {
        self.reason
    }

    /// The detail properties, in order.
    pub(crate) fn detail(&self) -> impl Iterator<Item = (&str, &Value)> + '_ {
        self.detail.iter()
    }

    /// The detail as `JSON.stringify` writes its properties, without the braces.
    fn json(&self) -> String {
        let props: Vec<String> = self
            .detail
            .iter()
            .map(|(key, value)| [json_string(key), js_json(value)].join(":"))
            .collect();
        props.join(",")
    }
}

impl PartialEq for LostGate {
    fn eq(&self, other: &Self) -> bool {
        self.json() == other.json()
    }
}

impl Eq for LostGate {}

/// 1.x `lostGateRow(start, prog, beat, now)`: why the gate run of `start` ended and what it
/// had done.
///
/// `ranMs` is the hook's last sign of life after the start (its heartbeat when that
/// is its own pid's, else the progress file's step start or mtime); within 30 s of the deadline
/// (`timeoutMs` less `afterMs`) that was the timeout; short of it with a heartbeat, or found
/// before the deadline, an interruption; else unknown. JS numbers throughout: a start time
/// `Date.parse` cannot read makes `ranMs` and `foundMs` NaN, written `null`.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x throws: a `null` step (`s.name`), the heartbeat's `at` read with
/// no heartbeat when neither it nor the start has a `pid`, and a time or step name that is an
/// object with its own `toString` key.
pub fn lost_gate_row(facts: &LostGateFacts<'_>) -> Result<LostGate, JsTypeError> {
    let start = facts.start;
    let t0 = date_parse(start.get("t"))?;
    let beat = facts.heartbeat.filter(|beat| **beat != Value::Null);
    let alive = if strict_eq(beat.and_then(|b| b.get("pid")), start.get("pid")) {
        let Some(beat) = beat else {
            return Err(JsTypeError::new(
                "the heartbeat is null, and 1.x reads its `at`",
            ));
        };
        finite(beat.get("at")).unwrap_or(0.0)
    } else {
        0.0
    };
    let record = facts.progress.map(|p| p.record);
    let current = record.and_then(|r| r.get("current"));
    let step_start = match current.and_then(|c| c.get("startedAt")) {
        None | Some(Value::Null) => f64::NAN,
        some => date_parse(some)?,
    };
    // `Date.parse(…) || 0`: NaN and 0 both read as 0.
    let step_start = if step_start.is_nan() { 0.0 } else { step_start };
    let modified = facts.progress.map_or(0.0, |p| p.modified.millis());
    let seen = js_max(&[t0, step_start, modified, alive]);
    let ran = seen - t0;
    let found = facts.now.js_millis() - t0;
    let deadline = match finite(start.get("timeoutMs")) {
        Some(timeout) => Some(Value::Number(
            timeout - finite(start.get("afterMs")).unwrap_or(0.0),
        )),
        None => start.get("timeoutMs").cloned(),
    };
    let reason = match finite(deadline.as_ref()) {
        Some(deadline) if ran >= deadline - 3.0 * BEAT_MS => LostReason::Timeout,
        Some(deadline) if alive != 0.0 || found < deadline => LostReason::Interrupted,
        // No finite deadline, or neither can be told.
        _ => LostReason::Unknown,
    };
    let steps = steps(record)?;
    let step = current
        .and_then(|c| c.get("name"))
        .cloned()
        .unwrap_or(Value::Null);
    // The row owns copies of the values it records from the start row and progress file.
    let props = start
        .get("pid")
        .map(|pid| ("pid", pid.clone()))
        .into_iter()
        .chain([
            ("reason", Value::String(reason.name().to_owned())),
            ("ranMs", Value::Number(ran)),
            ("foundMs", Value::Number(found)),
            ("timeoutMs", deadline.unwrap_or(Value::Null)),
            ("heartbeat", Value::Bool(alive != 0.0)),
            ("progress", Value::Bool(facts.progress.is_some())),
            ("step", step),
            ("steps", steps),
        ])
        .map(|(key, value)| (key.to_owned(), value));
    Ok(LostGate {
        reason,
        detail: props.collect(),
    })
}

/// `Object.fromEntries(steps.map((s) => [s.name, s.ms]))` over the progress file's `steps`
/// when it is an array: a repeated name keeps its first place and its last value, a missing
/// `ms` is `undefined` (left out), and integer-like names come first, as JS orders them.
fn steps(record: Option<&Value>) -> Result<Value, JsTypeError> {
    let mut map: JsMap<String, Option<Value>> = JsMap::new();
    if let Some(Value::Array(steps)) = record.and_then(|r| r.get("steps")) {
        for step in steps {
            if *step == Value::Null {
                return Err(JsTypeError::new(
                    "a progress step is null, and 1.x reads its name",
                ));
            }
            let name = match step.get("name") {
                Some(name) => js_string_checked(name)?.into_owned(),
                None => "undefined".to_owned(),
            };
            map.set(name, step.get("ms").cloned());
        }
    }
    let props = map
        .into_vec()
        .into_iter()
        .filter_map(|(name, ms)| ms.map(|ms| (name, ms)));
    Ok(Value::Object(props.collect()))
}

/// `Date.parse(value)` as a JS number: the value converted to a string first (`undefined` for
/// a missing one), NaN for what [`Timestamp::parse_js`] cannot read (OC-3 A).
fn date_parse(value: Option<&Value>) -> Result<f64, JsTypeError> {
    let text = match value {
        Some(value) => js_string_checked(value)?,
        None => "undefined".into(),
    };
    Ok(Timestamp::parse_js(&text).map_or(f64::NAN, Timestamp::js_millis))
}

/// The value when it is a finite number (`Number.isFinite`, which converts nothing).
fn finite(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(n)) if n.is_finite() => Some(*n),
        _ => None,
    }
}

/// JS `===` on two read values, a missing one being `undefined`: objects and arrays are never
/// equal, since each read parses its own.
fn strict_eq(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) | (Some(Value::Null), Some(Value::Null)) => true,
        (Some(Value::Bool(x)), Some(Value::Bool(y))) => x == y,
        (Some(Value::Number(x)), Some(Value::Number(y))) => x == y,
        (Some(Value::String(x)), Some(Value::String(y))) => x == y,
        _ => false,
    }
}

/// `Math.max(..values)`: NaN when any is NaN.
fn js_max(values: &[f64]) -> f64 {
    values.iter().fold(f64::NEG_INFINITY, |max, &v| {
        if max.is_nan() || v.is_nan() {
            f64::NAN
        } else {
            max.max(v)
        }
    })
}

#[cfg(test)]
mod tests;
