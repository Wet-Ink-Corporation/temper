//! A checkout's record in the session dir, `tree-<checkout>-<agent>.json` (1.x `hooks.mjs`
//! `noteTree`, read by `ownTree` and `otherTrees`).

use std::borrow::Cow;

use crate::js::{JsTypeError, Value, js_less, js_string_checked};

/// A tree record as 1.x's readers parse it: whatever JSON the file holds, read with JS property
/// semantics.
///
/// # Examples
///
/// ```
/// use temper_domain::TreeRecord;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let early = TreeRecord::from_json(r#"{"root":"/w","head":"abc","at":1}"#).ok_or("JSON")?;
/// let late = TreeRecord::from_json(r#"{"root":"/w","at":"2"}"#).ok_or("JSON")?;
/// assert_eq!(early.root()?, Some("/w"));
/// assert!(early.earlier_than(&late)?);
/// assert!(late.head_rev()?.is_none());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRecord(Value);

/// Why 1.x's `path.resolve` throws on a root that is not a string.
const ROOT_NOT_A_STRING: &str =
    "a tree record's root is not a string, which 1.x's path.resolve throws on";

impl TreeRecord {
    /// `JSON.parse(text)`, or `None` when it does not parse (1.x's `catch { continue; }`).
    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        Value::from_json(text).ok().map(Self)
    }

    /// `rec?.root` when it is truthy: `None` for a falsy root or a record that is not an object.
    ///
    /// # Errors
    ///
    /// [`JsTypeError`] for a truthy root that is not a string, as 1.x's `path.resolve(rec.root)`
    /// throws (F5).
    pub fn root(&self) -> Result<Option<&str>, JsTypeError> {
        match self.0.get("root").filter(|root| root.is_truthy()) {
            None => Ok(None),
            Some(Value::String(root)) => Ok(Some(root)),
            Some(_) => Err(JsTypeError::new(ROOT_NOT_A_STRING)),
        }
    }

    /// `rec?.head` is truthy.
    #[must_use]
    pub fn has_head(&self) -> bool {
        self.0.get("head").is_some_and(Value::is_truthy)
    }

    /// The revision 1.x's `resolveRev(tree, rec.head)` asks git for: `String(rec.head)`, or
    /// `None` when the head is falsy (`if (!rev) return null`).
    ///
    /// # Errors
    ///
    /// [`JsTypeError`] where the template literal throws (an object with its own `toString`).
    pub fn head_rev(&self) -> Result<Option<Cow<'_, str>>, JsTypeError> {
        match self.0.get("head").filter(|head| head.is_truthy()) {
            Some(head) => js_string_checked(head).map(Some),
            None => Ok(None),
        }
    }

    /// `rec.startFp` when it is a string: the only kind 1.x's `startFp === fp` can match.
    #[must_use]
    pub fn start_fp(&self) -> Option<&str> {
        self.0.get("startFp").and_then(Value::as_str)
    }

    /// `rec.at < other.at`, with JS's comparison.
    ///
    /// # Errors
    ///
    /// [`JsTypeError`] where V8's `ToPrimitive` throws on either time.
    pub fn earlier_than(&self, other: &Self) -> Result<bool, JsTypeError> {
        js_less(self.0.get("at"), other.0.get("at"))
    }
}

#[cfg(test)]
mod tests {
    use super::TreeRecord;

    fn record(text: &str) -> TreeRecord {
        TreeRecord::from_json(text).expect("JSON")
    }

    #[test]
    fn root_is_a_truthy_string_and_anything_else_truthy_fails() {
        assert_eq!(record(r#"{"root":"/w"}"#).root(), Ok(Some("/w")));
        for falsy in [
            r#"{"root":""}"#,
            r#"{"root":0}"#,
            "{}",
            "null",
            r#""/w""#,
            "[1]",
        ] {
            assert_eq!(record(falsy).root(), Ok(None), "{falsy}");
        }
        for odd in [r#"{"root":5}"#, r#"{"root":["/w"]}"#, r#"{"root":true}"#] {
            assert!(
                matches!(record(odd).root(), Err(error) if error.message().contains("root")),
                "{odd}"
            );
        }
    }

    #[test]
    fn the_head_is_read_as_1x_s_template_literal_reads_it() {
        assert_eq!(
            record(r#"{"head":"abc"}"#)
                .head_rev()
                .ok()
                .flatten()
                .as_deref(),
            Some("abc")
        );
        assert_eq!(
            record(r#"{"head":12}"#)
                .head_rev()
                .ok()
                .flatten()
                .as_deref(),
            Some("12")
        );
        assert!(matches!(
            record(r#"{"head":{"toString":1}}"#).head_rev(),
            Err(error) if error.message().contains("toString")
        ));
        assert!(!record(r#"{"head":""}"#).has_head());
        assert!(
            record(r#"{"head":null}"#)
                .head_rev()
                .is_ok_and(|h| h.is_none())
        );
    }

    #[test]
    fn only_a_string_start_fingerprint_counts_and_times_compare_as_js_does() {
        assert_eq!(record(r#"{"startFp":"ab"}"#).start_fp(), Some("ab"));
        assert_eq!(record(r#"{"startFp":5}"#).start_fp(), None);
        let (a, b) = (record(r#"{"at":"10"}"#), record(r#"{"at":"9"}"#));
        assert_eq!(a.earlier_than(&b), Ok(true), "two strings by code unit");
        assert_eq!(
            record(r#"{"at":10}"#).earlier_than(&b),
            Ok(false),
            "as numbers"
        );
        assert_eq!(record("{}").earlier_than(&b), Ok(false), "undefined is NaN");
    }
}
