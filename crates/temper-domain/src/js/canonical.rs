//! 1.x's `canonical` (object keys sorted at every depth) and the UTF-16 string order JS sorts by.

use std::cmp::Ordering;

use super::Value;

/// JS's default string comparison, as `Array.prototype.sort()` uses it: by UTF-16 code units.
/// It differs from byte order only where a char past U+FFFF meets one in U+E000 to U+FFFF.
/// No allocation; linear in the shorter common prefix.
pub(crate) fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// JS's `strings.sort()`: by UTF-16 code units ([`utf16_cmp`]), stable, without allocating per
/// comparison (P5).
///
/// # Examples
///
/// ```
/// use temper_domain::js_sort;
///
/// let mut names = ["\u{E000}", "\u{1F600}", "a"];
/// js_sort(&mut names);
/// assert_eq!(names, ["a", "\u{1F600}", "\u{E000}"]);
/// ```
pub fn js_sort(strings: &mut [impl AsRef<str>]) {
    strings.sort_by(|a, b| utf16_cmp(a.as_ref(), b.as_ref()));
}

/// 1.x `canonical(x)`: arrays mapped, objects rebuilt by `Object.fromEntries` from their keys in
/// `Object.keys(x).sort()` order, which puts integer-like keys first in ascending order again.
///
/// It recurses once per nesting level, which `serde_json` stops at 128 (P9).
pub(crate) fn canonical(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        Value::Object(object) => {
            let mut props: Vec<(&str, &Value)> = object.iter().collect();
            props.sort_by(|(a, _), (b, _)| utf16_cmp(a, b));
            // `Object` orders integer-like keys first, as `Object.fromEntries` does.
            Value::Object(
                props
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), canonical(v)))
                    .collect(),
            )
        }
        // A new tree is built; its scalars are copies of the input's.
        scalar => scalar.clone(),
    }
}
