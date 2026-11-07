//! A cached gate result, `gate/<fingerprint>.json` (1.x `cachedGate`).

use crate::js::Value;

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

    /// `g.ok` is truthy.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.0.get("ok").is_some_and(Value::is_truthy)
    }
}
