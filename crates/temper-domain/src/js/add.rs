//! JS's `+` on the values 1.x adds up: a number or string so far, plus a JSON value.

use super::value::js_number;
use super::{Value, js_string};

/// What a JS `+` chain holds so far: a number, until a string joins in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Sum {
    /// Every operand so far was a number, a boolean or `null`.
    Number(f64),
    /// A string joined in, so everything after it concatenates.
    Text(String),
}

impl Sum {
    /// JS `sum + value`: `value` goes to a primitive first (an array joins, an object is
    /// `[object Object]`); then a string on either side concatenates, and otherwise both are
    /// numbers (`null` is 0, a boolean 0 or 1) and add.
    pub(crate) fn add(self, value: &Value) -> Self {
        match (self, value) {
            (Self::Number(sum), Value::Null) => Self::Number(sum),
            (Self::Number(sum), Value::Bool(b)) => Self::Number(sum + f64::from(u8::from(*b))),
            (Self::Number(sum), Value::Number(n)) => Self::Number(sum + n),
            (Self::Number(sum), text) => Self::Text(js_number(sum) + &js_string(text)),
            (Self::Text(mut sum), value) => {
                sum.push_str(&js_string(value));
                Self::Text(sum)
            }
        }
    }

    /// JS `sum + other` for two sums.
    pub(crate) fn add_sum(self, other: Self) -> Self {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => Self::Number(a + b),
            (Self::Number(a), Self::Text(b)) => Self::Text(js_number(a) + &b),
            (Self::Text(mut a), Self::Number(b)) => {
                a.push_str(&js_number(b));
                Self::Text(a)
            }
            (Self::Text(mut a), Self::Text(b)) => {
                a.push_str(&b);
                Self::Text(a)
            }
        }
    }

    /// The sum as the JS value it is.
    pub(crate) fn into_value(self) -> Value {
        match self {
            Self::Number(n) => Value::Number(n),
            Self::Text(text) => Value::String(text),
        }
    }
}
