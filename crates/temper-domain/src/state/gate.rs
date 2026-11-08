//! A cached gate result, `gate/<fingerprint>.json` (1.x `cachedGate`).

use std::borrow::Cow;

use crate::js::{JsTypeError, Value, js_string_checked};
use crate::verdict::GateState;

/// A cached gate result as 1.x's `cachedGate` returns it: whatever JSON the file holds.
///
/// # Examples
///
/// ```
/// use temper_domain::GateRecord;
///
/// let green = GateRecord::from_json(r#"{"ok":true,"steps":[]}"#);
/// assert!(green.is_some_and(|g| g.is_present() && g.ok()));
/// assert!(GateRecord::from_json("not json").is_none());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GateRecord(Value);

impl GateRecord {
    /// `JSON.parse(text)`, or `None` when it does not parse (1.x's `catch { return null; }`;
    /// D16(n) extended by OC-3 A to what `serde_json` rejects and V8 reads).
    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        match Value::from_json(text) {
            Ok(value) => Some(Self(value)),
            Err(_) => None,
        }
    }

    /// The record is truthy, as 1.x's `g ? … : …` tests it.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.0.is_truthy()
    }

    /// The record as parsed, any JSON value.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.0
    }

    /// 1.x's `ctx.gate && ctx.gate.ok === false`, as a verdict's check reads the record.
    ///
    /// # Examples
    ///
    /// ```
    /// use temper_domain::{GateRecord, GateState};
    ///
    /// let state = |text: &str| GateRecord::from_json(text).map(|g| g.state());
    /// assert_eq!(state(r#"{"ok":false}"#), Some(GateState::Red));
    /// assert_eq!(state(r#"{"ok":0}"#), Some(GateState::NotRed));
    /// assert_eq!(state("{}"), Some(GateState::NotRed));
    /// assert_eq!(state("false"), Some(GateState::NotRed));
    /// ```
    #[must_use]
    pub fn state(&self) -> GateState {
        match self.0.get("ok") {
            Some(Value::Bool(false)) if self.is_present() => GateState::Red,
            _ => GateState::NotRed,
        }
    }

    /// `g.ok` is truthy.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.0.get("ok").is_some_and(Value::is_truthy)
    }

    /// 1.x `g.steps.filter((s) => !s.ok).map((s) => s.name).join(', ')`: the names of the
    /// steps that did not pass, a missing or `null` name as nothing.
    ///
    /// # Errors
    ///
    /// [`JsTypeError`] where 1.x throws: `steps` not an array, a `null` step, or a name that
    /// cannot be converted to a string.
    pub fn failed_steps(&self) -> Result<String, JsTypeError> {
        let Some(Value::Array(steps)) = self.0.get("steps") else {
            return Err(JsTypeError::new(
                "the gate record's steps is not an array, which 1.x's filter throws on",
            ));
        };
        let mut names = Vec::new();
        for step in steps {
            if matches!(step, Value::Null) {
                return Err(JsTypeError::new(
                    "a gate record's step is null, which 1.x's s.ok throws on",
                ));
            }
            if step.get("ok").is_some_and(Value::is_truthy) {
                continue;
            }
            names.push(match step.get("name") {
                None | Some(Value::Null) => Cow::Borrowed(""),
                Some(name) => js_string_checked(name)?,
            });
        }
        Ok(names.join(", "))
    }
}
