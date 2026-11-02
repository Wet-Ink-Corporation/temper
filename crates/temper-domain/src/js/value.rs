//! The JS data model that 1.x's TOML reader (`toml.mjs`) produces, and the JS operations 1.x
//! applies to it: `JSON.stringify`, `String(x)`, truthiness and `Object.entries`.
//!
//! `toml.mjs` returns plain objects and arrays, every number as a JS number, and a date or time
//! as its source text, so a TOML datetime is a [`Value::String`] here too.

use std::borrow::Cow;
use std::fmt::Write as _;

/// A JS value as `toml.mjs` builds it. JS's `undefined` is the absence of a value (`None`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    /// `true` or `false`.
    Bool(bool),
    /// A JS number: TOML integers and floats alike.
    Number(f64),
    /// A string, or a date or time kept as its TOML source text.
    String(String),
    /// An array (an inline array or an array of tables).
    Array(Vec<Self>),
    /// A plain object (a table).
    Object(Object),
}

/// A plain JS object: its own properties in JS order, which puts integer-like keys first in
/// ascending order, then the other keys in insertion order.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Object(Vec<(String, Value)>);

impl Object {
    /// The own property `key`, if the object has one.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The own properties in JS order (`Object.entries`).
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &Value)> + '_ {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// `Object.keys(obj).length === 0`.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Builds an object from properties in insertion order, as JS assigns them. The keys are
/// unique, as a TOML table's are.
impl FromIterator<(String, Value)> for Object {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        let mut props: Vec<(String, Value)> = iter.into_iter().collect();
        // A stable sort: integer-like keys first in ascending order, the rest as inserted.
        props.sort_by_key(|(k, _)| array_index(k).map_or((1, 0), |index| (0, index)));
        Self(props)
    }
}

/// `key` as an array index, the keys JS orders first: canonical decimal below 2^32 - 1.
fn array_index(key: &str) -> Option<u32> {
    key.parse::<u32>()
        .ok()
        .filter(|&n| n != u32::MAX && n.to_string() == key)
}

impl Value {
    /// JS `value?.[key]` for the keys temper reads: an object's own property, else `undefined`.
    /// No key temper reads is an array index, `length` or an inherited name.
    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(object) => object.get(key),
            _ => None,
        }
    }

    /// The string, if this is one (`typeof v === 'string'`).
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
}

/// `Object.entries(value)` for what temper iterates: an object's own properties, or an array's
/// items under their index keys. A scalar has none; a string's chars carry no property temper
/// reads, so they are left out too.
pub(crate) fn entries(value: &Value) -> impl Iterator<Item = (Cow<'_, str>, &Value)> + '_ {
    let object = match value {
        Value::Object(object) => Some(object.iter()),
        _ => None,
    };
    let array = match value {
        Value::Array(items) => Some(items.iter()),
        _ => None,
    };
    let props = object
        .into_iter()
        .flatten()
        .map(|(k, v)| (Cow::Borrowed(k), v));
    let items = array
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, v)| (Cow::Owned(i.to_string()), v));
    props.chain(items)
}

/// JS truthiness: `false`, `0`, `-0`, `NaN` and `""` are falsy; objects and arrays are truthy.
pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Number(n) => !n.is_nan() && *n != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// JS `String(value)`: numbers as `Number#toString`, arrays joined with `,`, objects as
/// `[object Object]`.
pub(crate) fn js_string(value: &Value) -> Cow<'_, str> {
    match value {
        Value::Bool(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
        Value::Number(n) => Cow::Owned(js_number(*n)),
        Value::String(s) => Cow::Borrowed(s),
        Value::Array(items) => {
            Cow::Owned(items.iter().map(js_string).collect::<Vec<_>>().join(","))
        }
        Value::Object(_) => Cow::Borrowed("[object Object]"),
    }
}

/// JS `JSON.stringify(value)`: compact, keys in JS order, non-finite numbers as `null`.
///
/// It recurses once per nesting level; the `toml` parser caps a document's nesting.
pub(crate) fn js_json(value: &Value) -> String {
    let mut out = String::new();
    push_json(&mut out, value);
    out
}

fn push_json(out: &mut String, value: &Value) {
    match value {
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) if n.is_finite() => out.push_str(&js_number(*n)),
        Value::Number(_) => out.push_str("null"),
        Value::String(s) => push_json_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json(out, item);
            }
            out.push(']');
        }
        Value::Object(object) => {
            out.push('{');
            for (i, (key, item)) in object.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(out, key);
                out.push(':');
                push_json(out, item);
            }
            out.push('}');
        }
    }
}

/// `JSON.stringify` of a string: `"` and `\` escaped, the C0 controls as `\b`, `\t`, `\n`, `\f`,
/// `\r` or `\u00xx`, and everything else (U+2028, U+2029 and U+007F included) as is.
fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c < ' ' => {
                // Writing to a `String` cannot fail.
                if write!(out, "\\u{:04x}", u32::from(c)).is_err() {
                    out.push(c);
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// JS `Number#toString` (radix 10): the shortest round-trip digits, in plain form for decimal
/// exponents from -7 to 20 and in exponent form (`1e+21`, `1e-7`) outside them.
pub(crate) fn js_number(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if x == 0.0 {
        return "0".to_owned();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // `{:e}` gives the fewest digits that round-trip, as JS needs. Among those, JS takes the
    // decimal nearest `x`, and the even one on a tie; `{:e}` breaks a tie upwards, so the digits
    // come from formatting at that precision, which rounds exactly with ties to even. At a power
    // of two the gap below `x` is half the gap above, so that nearest decimal may fall outside
    // the round-trip interval; then the shortest digits are the only candidate and are kept.
    let shortest = format!("{:e}", x.abs());
    let precision = shortest
        .find('e')
        .unwrap_or(shortest.len())
        .saturating_sub(2);
    let nearest = format!("{:.precision$e}", x.abs());
    let sci = if nearest.parse::<f64>() == Ok(x.abs()) {
        nearest
    } else {
        shortest
    };
    let Some((mantissa, exp)) = sci.split_once('e') else {
        return format!("{sign}{sci}");
    };
    let Ok(exp) = exp.parse::<i64>() else {
        return format!("{sign}{sci}");
    };
    let digits = mantissa.replace('.', "");
    format!("{sign}{}", place_point(&digits, exp.saturating_add(1)))
}

/// ECMA-262 `Number::toString` steps 6 to 12 for the digits `digits` and point position `n`
/// (the value is `0.digits × 10^n`).
fn place_point(digits: &str, n: i64) -> String {
    let k = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    let zeros = |count: i64| "0".repeat(usize::try_from(count).unwrap_or(0));
    if k <= n && n <= 21 {
        return format!("{digits}{}", zeros(n.saturating_sub(k)));
    }
    if 0 < n && n <= 21 {
        let at = usize::try_from(n).unwrap_or(0);
        if let Some((int, frac)) = digits.split_at_checked(at) {
            return format!("{int}.{frac}");
        }
    }
    if -6 < n && n <= 0 {
        return format!("0.{}{digits}", zeros(n.saturating_neg()));
    }
    let (first, rest) = digits.split_at_checked(1).unwrap_or((digits, ""));
    let point = if rest.is_empty() { "" } else { "." };
    let e = n.saturating_sub(1);
    let e_sign = if e < 0 { '-' } else { '+' };
    format!("{first}{point}{rest}e{e_sign}{}", e.unsigned_abs())
}

/// JS `Number(text)` for a TOML integer's digits in `radix` (2, 8, 10 or 16; decimal may carry a
/// sign): the nearest double, so precision is lost past 2^53 exactly as in 1.x. A digit outside
/// the radix gives `NaN`, as `Number()` does.
pub(crate) fn integer_from_radix(text: &str, radix: u32) -> f64 {
    const BASE: u64 = 1_000_000_000;
    if radix == 10 {
        return number_from_text(text);
    }
    if text.is_empty() {
        return f64::NAN;
    }
    // Little-endian base-10^9 limbs, so any length converts exactly before the one rounding.
    let mut limbs: Vec<u64> = Vec::new();
    for c in text.chars() {
        let Some(digit) = c.to_digit(radix) else {
            return f64::NAN;
        };
        let mut carry = u64::from(digit);
        for limb in &mut limbs {
            // A limb is below 10^9 and the radix at most 16, so this stays far below u64::MAX.
            let x = *limb * u64::from(radix) + carry;
            *limb = x % BASE;
            carry = x / BASE;
        }
        while carry > 0 {
            limbs.push(carry % BASE);
            carry /= BASE;
        }
    }
    let mut decimal = String::new();
    for (i, limb) in limbs.iter().rev().enumerate() {
        // Writing to a `String` cannot fail.
        let written = if i == 0 {
            write!(decimal, "{limb}")
        } else {
            write!(decimal, "{limb:09}")
        };
        if written.is_err() {
            return f64::NAN;
        }
    }
    if decimal.is_empty() {
        decimal.push('0');
    }
    number_from_text(&decimal)
}

/// JS `Number(text)` for the decimal and float forms `toml` hands over (digits with an optional
/// sign, fraction and exponent, or `inf` and `nan`): Rust's float parsing rounds to nearest, as
/// JS's does.
pub(crate) fn number_from_text(text: &str) -> f64 {
    // Text that is not a number is `NaN` to `Number()`: that is the value, not a lost failure.
    text.parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests;
