//! JS's `<` on the values 1.x compares: tree records' `at` and history rows' `recorded_at`.

use std::borrow::Cow;
use std::cmp::Ordering;

use super::canonical::utf16_cmp;
use super::value::{integer_from_radix, js_string_checked};
use super::{JsTypeError, Value, is_js_space};

/// A value after `ToPrimitive` with the number hint, as a JSON value can give it.
enum Primitive<'a> {
    Text(Cow<'a, str>),
    Number(f64),
}

/// JS `a < b` (the abstract relational comparison) for JSON values, `None` being `undefined`.
///
/// Both sides go to primitives (an array joins, an object is `[object Object]`); two strings
/// compare by UTF-16 code units, anything else as numbers, where `NaN` compares false.
///
/// # Errors
///
/// [`JsTypeError`] where V8's `ToPrimitive` throws: an object with its own `toString` key,
/// also inside an array.
pub(crate) fn js_less(a: Option<&Value>, b: Option<&Value>) -> Result<bool, JsTypeError> {
    let (a, b) = (primitive(a)?, primitive(b)?);
    Ok(match (a, b) {
        (Primitive::Text(a), Primitive::Text(b)) => utf16_cmp(&a, &b) == Ordering::Less,
        (a, b) => number(&a) < number(&b),
    })
}

fn primitive(value: Option<&Value>) -> Result<Primitive<'_>, JsTypeError> {
    Ok(match value {
        None => Primitive::Number(f64::NAN),
        Some(Value::Null) => Primitive::Number(0.0),
        Some(Value::Bool(b)) => Primitive::Number(f64::from(u8::from(*b))),
        Some(Value::Number(n)) => Primitive::Number(*n),
        Some(Value::String(s)) => Primitive::Text(Cow::Borrowed(s)),
        Some(other) => Primitive::Text(js_string_checked(other)?),
    })
}

fn number(primitive: &Primitive<'_>) -> f64 {
    match primitive {
        Primitive::Number(n) => *n,
        Primitive::Text(text) => string_to_number(text),
    }
}

/// JS `Number(text)` (the spec's `StringToNumber`): the JS whitespace trimmed, empty is 0,
/// `Infinity` with an optional sign, `0x`, `0o` and `0b` integers without a sign, and decimal
/// literals; anything else is `NaN`.
pub(crate) fn string_to_number(text: &str) -> f64 {
    let text = text.trim_matches(is_js_space);
    if text.is_empty() {
        return 0.0;
    }
    match text {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefixes, radix) in [(["0x", "0X"], 16), (["0o", "0O"], 8), (["0b", "0B"], 2)] {
        if let Some(digits) = prefixes.iter().find_map(|p| text.strip_prefix(p)) {
            return integer_from_radix(digits, radix);
        }
    }
    if is_decimal_literal(text) {
        // The grammar is checked, so Rust's parser reads the same number, rounded to nearest.
        text.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

/// `StrDecimalLiteral` without `Infinity`: a sign, digits with an optional fraction (or a
/// fraction alone), and an optional exponent.
fn is_decimal_literal(text: &str) -> bool {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = match mantissa.split_once('.') {
        Some((whole, fraction)) => {
            digits(whole) && digits(fraction) && !(whole.is_empty() && fraction.is_empty())
        }
        None => !mantissa.is_empty() && digits(mantissa),
    };
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    mantissa_ok && exponent_ok
}

#[cfg(test)]
mod tests;
