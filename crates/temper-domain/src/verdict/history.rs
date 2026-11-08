//! The verdicts kept as history beside an approval (1.x `verdict.mjs` `historyFor`'s filter).

use crate::js::{JsTypeError, Value, js_less, js_string_checked};

/// 1.x `String(h.recorded_at ?? '') > since`: was history row `row` recorded after `since`?
///
/// `since` is an earlier verdict's `recorded_at`, any JSON value; `None` is `historyFor`'s
/// default `''`. The comparison is JS's: two strings by UTF-16 code units, otherwise as
/// numbers.
///
/// # Examples
///
/// ```
/// use temper_domain::{JsValue, recorded_after};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let row = JsValue::from_json(r#"{"recorded_at":"2026-11-08T10:00:00.000Z"}"#)?;
/// let since = JsValue::String("2026-11-07T10:00:00.000Z".to_owned());
/// assert!(recorded_after(&row, Some(&since))?);
/// assert!(recorded_after(&row, None)?);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// [`JsTypeError`] where 1.x throws: a `null` row (`h.recorded_at`), or a time or `since` that
/// V8 cannot convert to a primitive.
pub fn recorded_after(row: &Value, since: Option<&Value>) -> Result<bool, JsTypeError> {
    if matches!(row, Value::Null) {
        return Err(JsTypeError::new(
            "a history row is null, which 1.x's h.recorded_at throws on",
        ));
    }
    let at = match row.get("recorded_at") {
        None | Some(Value::Null) => String::new(),
        Some(at) => js_string_checked(at)?.into_owned(),
    };
    let empty = Value::String(String::new());
    js_less(Some(since.unwrap_or(&empty)), Some(&Value::String(at)))
}

#[cfg(test)]
mod tests {
    use super::recorded_after;
    use crate::js::{JsTypeError, Value};

    fn json(text: &str) -> Value {
        Value::from_json(text).expect("JSON")
    }

    #[test]
    fn rows_compare_with_since_as_1x_s_filter_does() {
        let since = json(r#""2026-11-07T10:00:00.000Z""#);
        for (row, since, want) in [
            (
                r#"{"recorded_at":"2026-11-08T10:00:00.000Z"}"#,
                Some(&since),
                true,
            ),
            (
                r#"{"recorded_at":"2026-11-06T10:00:00.000Z"}"#,
                Some(&since),
                false,
            ),
            (
                r#"{"recorded_at":"2026-11-07T10:00:00.000Z"}"#,
                Some(&since),
                false,
            ),
            ("{}", None, false),
            (r#"{"recorded_at":null}"#, None, false),
            ("5", None, false),
            (r#"{"recorded_at":"x"}"#, None, true),
            (r#"{"recorded_at":10}"#, Some(&json("9")), true),
            (r#"{"recorded_at":"10"}"#, Some(&json(r#""9""#)), false),
            (r#"{"recorded_at":[]}"#, Some(&json("null")), false),
        ] {
            assert_eq!(
                recorded_after(&json(row), since),
                Ok(want),
                "{row} > {since:?}"
            );
        }
    }

    #[test]
    fn a_null_row_or_an_unconvertible_time_throws_as_in_1x() {
        let failed = |result: Result<bool, JsTypeError>, why: &str| {
            assert!(
                matches!(&result, Err(error) if error.message().contains(why)),
                "{result:?}"
            );
        };
        failed(recorded_after(&Value::Null, None), "null");
        let time = json(r#"{"recorded_at":{"toString":1}}"#);
        failed(recorded_after(&time, None), "toString");
        let since = json(r#"{"toString":1}"#);
        failed(recorded_after(&json("{}"), Some(&since)), "toString");
    }
}
