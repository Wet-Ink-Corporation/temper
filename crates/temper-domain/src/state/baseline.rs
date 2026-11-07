//! What `status` reads from `.claude/temper-baseline.json` (1.x `baseline.mjs` `parseBaseline`
//! and `temper.mjs` `baselineTotal`). The full baseline model comes with the ratchet (Phase 5).

use crate::js::{JsTypeError, JsonParseError, Sum, Value};

/// The baseline's total finding count and toolchain, as 1.x's `status` prints them.
///
/// # Examples
///
/// ```
/// use temper_domain::BaselineSummary;
///
/// let b = BaselineSummary::parse(r#"{"temper_baseline":1,"totals":{"core":{"clippy::x":2}}}"#)?;
/// assert_eq!(b.total().to_js_string(), "2");
/// assert!(b.toolchain().is_none());
/// # Ok::<(), temper_domain::BaselineParseError>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineSummary {
    total: Value,
    toolchain: Option<Value>,
}

impl BaselineSummary {
    /// 1.x `parseBaseline(text)`'s checks, then `baselineTotal`: the sum of every count in
    /// `totals` with JS `+`, so a count that is a string concatenates, as in 1.x.
    ///
    /// # Errors
    ///
    /// [`BaselineParseError`] with 1.x's message where 1.x throws: text that is not JSON, a
    /// document without `"temper_baseline": 1` and a `totals` table, `targets` that are not a
    /// table of sections with `totals` tables, and a `null` in `totals`, whose
    /// `Object.values(null)` throws a `TypeError`, and a count with its own `toString` key,
    /// which `+` cannot convert.
    pub fn parse(text: &str) -> Result<Self, BaselineParseError> {
        let doc = Value::from_json(text).map_err(|source| BaselineParseError::NotJson {
            detail: source.detail(),
            source,
        })?;
        let tables_ok = doc.get("temper_baseline") == Some(&Value::Number(1.0));
        let Some(Value::Object(totals)) = doc.get("totals").filter(|_| tables_ok) else {
            return Err(BaselineParseError::NotABaseline);
        };
        let sections_ok = match doc.get("targets") {
            None | Some(Value::Null) => true,
            Some(Value::Object(sections)) => sections
                .iter()
                .all(|(_, section)| matches!(section.get("totals"), Some(Value::Object(_)))),
            Some(_) => false,
        };
        if !sections_ok {
            return Err(BaselineParseError::BadTargets);
        }
        let mut total = Sum::Number(0.0);
        for (_, counts) in totals.iter() {
            total = total.add_sum(count_sum(counts)?);
        }
        // The document is ours and dropped here, so the toolchain moves out (#207 review F6).
        let toolchain = match doc {
            Value::Object(mut object) => object.remove("toolchain"),
            _ => None,
        };
        Ok(Self {
            total: total.into_value(),
            toolchain,
        })
    }

    /// `baselineTotal(b)`: a number, or a string when a count was one.
    #[must_use]
    pub fn total(&self) -> &Value {
        &self.total
    }

    /// `b.toolchain`, absent when the key is.
    #[must_use]
    pub fn toolchain(&self) -> Option<&Value> {
        self.toolchain.as_ref()
    }
}

/// `Object.values(l).reduce((x, y) => x + y, 0)` for one entry of `totals`.
fn count_sum(counts: &Value) -> Result<Sum, BaselineParseError> {
    let zero = Sum::Number(0.0);
    Ok(match counts {
        Value::Object(object) => object
            .iter()
            .try_fold(zero, |sum, (_, v)| sum.add(v))
            .map_err(BaselineParseError::NoPrimitive)?,
        Value::Array(items) => items
            .iter()
            .try_fold(zero, Sum::add)
            .map_err(BaselineParseError::NoPrimitive)?,
        // A string's values are its characters, added one by one: the same as adding it whole.
        Value::String(text) if !text.is_empty() => {
            zero.add(counts).map_err(BaselineParseError::NoPrimitive)?
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) => zero,
        Value::Null => {
            return Err(BaselineParseError::Throws(JsTypeError::new(
                "Cannot convert undefined or null to object",
            )));
        }
    })
}

/// Why `status` cannot read the baseline; each `Display` is 1.x's message.
#[derive(Debug, thiserror::Error)]
pub enum BaselineParseError {
    /// The file is not JSON; the detail is `serde_json`'s message (D16(a)).
    #[error(".claude/temper-baseline.json is not valid JSON ({detail})")]
    NotJson {
        /// The parser's message.
        detail: String,
        /// The parser's error.
        #[source]
        source: JsonParseError,
    },
    /// No `"temper_baseline": 1`, or no `totals` table.
    #[error(
        ".claude/temper-baseline.json is not a temper baseline (expected \"temper_baseline\": 1 and a \"totals\" table)"
    )]
    NotABaseline,
    /// `targets` is not a table of sections, each with a `totals` table.
    #[error(
        ".claude/temper-baseline.json is not a temper baseline (\"targets\" must map each host to a section with a \"totals\" table)"
    )]
    BadTargets,
    /// Adding up `totals` throws in 1.x; the message is V8's, which 1.x's `status` prints.
    #[error(transparent)]
    Throws(JsTypeError),
    /// A value 1.x converts to a string (a count it adds, the toolchain it prints) is an
    /// object with its own `toString` key, where V8's `ToPrimitive` throws; the message is
    /// V8's, which 1.x's `status` prints.
    #[error("Cannot convert object to primitive value")]
    NoPrimitive(#[source] JsTypeError),
}
