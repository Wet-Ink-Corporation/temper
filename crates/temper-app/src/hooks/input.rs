//! The hook input, read as 1.x reads it: any JSON value, each field coerced where a handler
//! reads it (port plan 2-10 A).

use std::borrow::Cow;

use temper_domain::{JsObject, JsValue, JsonParseError};

use crate::engine::HookError;

/// The JSON a hook receives on stdin.
///
/// It is not a typed DTO (OUT-05): 1.x accepts any JSON and coerces each field where it reads
/// it (`String(x)`, `??`, truthiness), so a field of an unexpected type must read as 1.x reads
/// it rather than fail. A value that is not an object reads as one without properties.
#[derive(Debug, Clone, PartialEq)]
pub struct HookInput(JsValue);

/// The hook input is not JSON.
#[derive(Debug, thiserror::Error)]
#[error("the hook input is not JSON")]
pub struct HookInputError(#[source] JsonParseError);

impl HookInput {
    /// 1.x's `JSON.parse(raw || '{}')`: empty text is an empty object.
    ///
    /// # Errors
    ///
    /// [`HookInputError`] when `text` is not JSON, as `JSON.parse` would throw, or is JSON that
    /// `serde_json` refuses (port plan D16(n)).
    pub fn parse(text: &str) -> Result<Self, HookInputError> {
        if text.is_empty() {
            return Ok(Self(JsValue::Object(JsObject::default())));
        }
        JsValue::from_json(text).map(Self).map_err(HookInputError)
    }

    /// Fails as V8 does when the input is `null` and the handler first reads `field`.
    pub(crate) fn require_object(&self, field: &'static str) -> Result<(), HookError> {
        match self.0 {
            JsValue::Null => Err(HookError::NullInput { field }),
            _ => Ok(()),
        }
    }

    /// `input[key]`, `null` included.
    pub(crate) fn raw(&self, key: &str) -> Option<&JsValue> {
        self.0.get(key)
    }

    /// `input[key]` where `??` would replace it: `null` reads as missing.
    pub(crate) fn given(&self, key: &str) -> Option<&JsValue> {
        given(Some(&self.0), key)
    }
}

/// `value?.[key]` where `??` would replace it: `null` reads as missing.
pub(crate) fn given<'a>(value: Option<&'a JsValue>, key: &str) -> Option<&'a JsValue> {
    value
        .and_then(|value| value.get(key))
        .filter(|field| !matches!(field, JsValue::Null))
}

/// JS `String(value ?? '')`.
pub(crate) fn js_string_or_empty(value: Option<&JsValue>) -> Cow<'_, str> {
    value.map_or(Cow::Borrowed(""), JsValue::to_js_string)
}

/// 1.x `agentKey(input)`: `agent-<agent_id>` with the id made safe for a file name, or `main`
/// when the id is missing or falsy.
pub(crate) fn agent_key(input: &HookInput) -> String {
    match input.raw("agent_id").filter(|id| id.is_truthy()) {
        Some(id) => format!("agent-{}", file_safe(&id.to_js_string())),
        None => "main".to_owned(),
    }
}

/// The session's directory name, as 1.x's `sessionDir` and `markerDir` make it:
/// `String(session_id || 'unknown')` made safe for a file name.
pub(crate) fn session_segment(input: &HookInput) -> String {
    match input.raw("session_id").filter(|sid| sid.is_truthy()) {
        Some(sid) => file_safe(&sid.to_js_string()),
        None => "unknown".to_owned(),
    }
}

/// `.replace(/[^\w.-]/g, '_')`: each UTF-16 unit outside `[A-Za-z0-9_.-]` becomes `_`, so an
/// astral char becomes two.
fn file_safe(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') {
            out.push(c);
        } else {
            out.extend(std::iter::repeat_n('_', c.len_utf16()));
        }
    }
    out
}
