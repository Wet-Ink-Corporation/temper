//! The subagent's final report, read from its transcript (1.x `hooks.mjs` `reportFromTranscript`).

use temper_domain::{JsTypeError, JsValue, js_trim};

use super::input::given;
use crate::engine::RunEnv;
use crate::paths;
use crate::ports::Files;
use crate::state::read_js_text;

/// 1.x `reportFromTranscript(file)`: the subagent's final report from its transcript, the
/// message of the last `SubagentHandback` call of the last assistant line that has one, else
/// that line's text when it is not blank. Lines are read from the end, each parsed at most
/// once (P3); a line that does not parse, or whose `message.content` is not an array, is
/// skipped. `None` when no line gives a report, or the transcript is missing or unreadable.
///
/// `file` is the input's `agent_transcript_path`: a non-empty string, read from the working
/// directory when relative. Any other value reads as no transcript (port plan item 47; 1.x's
/// `readFileSync` would take an integer as a file descriptor).
///
/// # Errors
///
/// [`JsTypeError`] where 1.x's `String(hb.input?.message ?? '')` or the texts' `join` throws on
/// a value it cannot convert (an object with its own `toString` key).
pub(crate) fn report_from_transcript(
    files: &impl Files,
    env: &RunEnv,
    file: Option<&JsValue>,
) -> Result<Option<String>, JsTypeError> {
    let Some(file) = file.and_then(JsValue::as_str).filter(|f| !f.is_empty()) else {
        return Ok(None);
    };
    let cwd = env.cwd.to_string_lossy();
    let Some(text) = read_js_text(files, &paths::resolve(env.style, &cwd, file)) else {
        // 1.x: `if (!t) return null`, for a missing or unreadable file alike.
        return Ok(None);
    };
    for line in text.rsplit('\n') {
        // A line that does not parse is 1.x's `catch { continue; }` (D16(n), OC-3 A).
        let Ok(rec) = JsValue::from_json(line) else {
            continue;
        };
        if let Some(report) = line_report(&rec)? {
            return Ok(Some(report));
        }
    }
    Ok(None)
}

/// The report one transcript line gives, if any.
fn line_report(rec: &JsValue) -> Result<Option<String>, JsTypeError> {
    let message = rec.get("message");
    let assistant = message
        .and_then(|m| m.get("role"))
        .and_then(JsValue::as_str)
        == Some("assistant");
    let Some(JsValue::Array(content)) =
        message.and_then(|m| m.get("content")).filter(|_| assistant)
    else {
        return Ok(None);
    };
    let is = |c: &JsValue, kind: &str| c.get("type").and_then(JsValue::as_str) == Some(kind);
    let hand_back = content.iter().rev().find(|c| {
        is(c, "tool_use") && c.get("name").and_then(JsValue::as_str) == Some("SubagentHandback")
    });
    if let Some(hb) = hand_back {
        let message = given(given(Some(hb), "input"), "message");
        let report = match message {
            Some(message) => message.to_js_string_checked()?.into_owned(),
            None => String::new(),
        };
        return Ok(Some(report));
    }
    let mut text = String::new();
    for (i, c) in content.iter().filter(|c| is(c, "text")).enumerate() {
        if i > 0 {
            text.push('\n');
        }
        // `join` writes `null` and `undefined` as empty, anything else as `String(x)`.
        if let Some(part) = given(Some(c), "text") {
            text.push_str(&part.to_js_string_checked()?);
        }
    }
    Ok((!js_trim(&text).is_empty()).then_some(text))
}

#[cfg(test)]
mod tests;
