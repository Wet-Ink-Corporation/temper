//! `JSON.stringify(value, null, 2)`: the form 1.x writes `verdicts/<fp>.json` in.

use super::Value;
use super::value::{js_number, json_string};

impl Value {
    /// JS `JSON.stringify(value, null, 2)`: each array item and object property on its own
    /// line, indented two spaces per level, `"key": value`; an empty array or object stays
    /// `[]` or `{}`, and non-finite numbers are `null`.
    ///
    /// It recurses once per nesting level, which `serde_json` stops at 128 (P9).
    ///
    /// # Examples
    ///
    /// ```
    /// use temper_domain::JsValue;
    ///
    /// let v = JsValue::from_json(r#"{"a":[1],"b":{}}"#)?;
    /// assert_eq!(v.to_json_pretty(), "{\n  \"a\": [\n    1\n  ],\n  \"b\": {}\n}");
    /// # Ok::<(), temper_domain::JsonParseError>(())
    /// ```
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        let mut out = String::new();
        push_pretty(&mut out, self, 0);
        out
    }
}

fn push_pretty(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) if n.is_finite() => out.push_str(&js_number(*n)),
        Value::Null | Value::Number(_) => out.push_str("null"),
        Value::String(s) => out.push_str(&json_string(s)),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Object(object) if object.is_empty() => out.push_str("{}"),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                open_line(out, i, depth + 1);
                push_pretty(out, item, depth + 1);
            }
            close_line(out, depth);
            out.push(']');
        }
        Value::Object(object) => {
            out.push('{');
            for (i, (key, item)) in object.iter().enumerate() {
                open_line(out, i, depth + 1);
                out.push_str(&json_string(key));
                out.push_str(": ");
                push_pretty(out, item, depth + 1);
            }
            close_line(out, depth);
            out.push('}');
        }
    }
}

/// The separator before item `i`, then its indent.
fn open_line(out: &mut String, i: usize, depth: usize) {
    if i > 0 {
        out.push(',');
    }
    close_line(out, depth);
}

/// A line break and `depth` levels of indent.
fn close_line(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str("  ");
    }
}
