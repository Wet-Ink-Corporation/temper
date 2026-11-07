//! JSON text read into the JS value model, as `JSON.parse` reads it: every number a JS number,
//! object keys in JS order, and a repeated key keeping its first place and its last value.

use std::fmt;

use serde::de::{Deserialize, Deserializer, Error, MapAccess, SeqAccess, Visitor};

use super::{JsMap, Value, number_from_text};

/// The text is not JSON that temper reads.
///
/// That is what `JSON.parse` rejects, plus what `serde_json` rejects and V8 accepts: a lone
/// surrogate escape, a number past the `f64` range and nesting 128 deep or more (port plan
/// D16(n)).
#[derive(Debug, thiserror::Error)]
#[error("not valid JSON")]
pub struct JsonParseError(#[source] serde_json::Error);

impl JsonParseError {
    /// `serde_json`'s description of the error, the detail D16(a) puts where 1.x prints V8's.
    pub(crate) fn detail(&self) -> String {
        self.0.to_string()
    }
}

impl Value {
    /// `JSON.parse(text)`.
    ///
    /// # Errors
    ///
    /// [`JsonParseError`] when `text` is not JSON, including empty text.
    ///
    /// # Examples
    ///
    /// ```
    /// use temper_domain::JsValue;
    ///
    /// let value = JsValue::from_json(r#"{"b":1,"2":2,"b":3}"#)?;
    /// assert_eq!(value.to_json(), r#"{"2":2,"b":3}"#);
    /// # Ok::<(), temper_domain::JsonParseError>(())
    /// ```
    pub fn from_json(text: &str) -> Result<Self, JsonParseError> {
        serde_json::from_str(text).map_err(JsonParseError)
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsVisitor)
    }
}

/// Builds a [`Value`] from whatever `serde_json` reads; it recurses once per nesting level,
/// which `serde_json` stops at 128.
struct JsVisitor;

impl<'de> Visitor<'de> for JsVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E: Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    // An integer becomes the double nearest its digits, as `JSON.parse` reads them.
    fn visit_i64<E: Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(number_from_text(&v.to_string())))
    }

    fn visit_u64<E: Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(number_from_text(&v.to_string())))
    }

    fn visit_f64<E: Error>(self, v: f64) -> Result<Value, E> {
        Ok(Value::Number(v))
    }

    fn visit_str<E: Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E: Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E: Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut props = JsMap::new();
        while let Some((key, value)) = map.next_entry::<String, Value>()? {
            props.set(key, value);
        }
        Ok(Value::Object(props.into_vec().into_iter().collect()))
    }
}

#[cfg(test)]
mod tests;
