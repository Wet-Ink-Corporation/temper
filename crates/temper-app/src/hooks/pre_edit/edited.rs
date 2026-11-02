//! The edit a tool call makes, read from the hook input as 1.x's `applyEdit` reads it.

use temper_domain::{EditOp, EditStep, JsValue, Replacements, apply_edit};

use crate::engine::HookError;
use crate::hooks::input::{given, js_string_or_empty};

/// V8's text when `for (const e of edits)` meets a value that is not iterable.
const EDITS_NOT_ITERABLE: &str = "edits is not iterable";

/// What the edit leaves, as 1.x's `applyEdit` returns it.
pub(super) enum After {
    /// The new text.
    Text(String),
    /// A `Write` whose `content` is neither a string nor absent: 1.x carries the value on and
    /// throws where it first reads it as text.
    NotText,
}

/// 1.x `applyEdit(before, ti, tool)` on the hook input: `None` when the edit fails on its own.
///
/// `Write` takes `content ?? ''`; `MultiEdit` takes each of `edits ?? []` in order; any other
/// tool is one `Edit` step.
///
/// # Errors
///
/// [`HookError::InputType`] or [`HookError::NullInput`] where 1.x's `for` loop throws: on
/// `edits` that are not iterable, and on a `null` step reached after the steps before it
/// applied.
pub(super) fn edited_text(
    before: &str,
    tool: Option<&str>,
    tool_input: Option<&JsValue>,
) -> Result<Option<After>, HookError> {
    let op = match tool {
        Some("Write") => match given(tool_input, "content") {
            None => EditOp::Write { content: "" },
            Some(JsValue::String(content)) => EditOp::Write { content },
            Some(_) => return Ok(Some(After::NotText)),
        },
        Some("MultiEdit") => match given(tool_input, "edits") {
            None => EditOp::Replace(Vec::new()),
            Some(JsValue::Array(items)) => {
                let steps: Vec<EditStep<'_>> = items.iter().map_while(step).collect();
                if steps.len() < items.len() {
                    // A `null` step: 1.x throws on it only once the steps before it applied.
                    return match apply_edit(before, &EditOp::Replace(steps)) {
                        Some(_) => Err(HookError::NullInput {
                            field: "old_string",
                        }),
                        None => Ok(None),
                    };
                }
                EditOp::Replace(steps)
            }
            // A string iterates its chars, which have no `old_string`.
            Some(JsValue::String(chars)) if chars.is_empty() => EditOp::Replace(Vec::new()),
            Some(JsValue::String(_)) => return Ok(None),
            Some(_) => return Err(HookError::InputType(EDITS_NOT_ITERABLE)),
        },
        _ => EditOp::Replace(tool_input.and_then(step).into_iter().collect()),
    };
    Ok(apply_edit(before, &op).map(After::Text))
}

/// One step of an `Edit` or `MultiEdit`; `None` for a `null` step, which 1.x throws on.
fn step(e: &JsValue) -> Option<EditStep<'_>> {
    if matches!(e, JsValue::Null) {
        return None;
    }
    let count = if e.get("replace_all").is_some_and(JsValue::is_truthy) {
        Replacements::All
    } else {
        Replacements::First
    };
    Some(EditStep {
        old: e.get("old_string").and_then(JsValue::as_str),
        new: js_string_or_empty(given(Some(e), "new_string")),
        count,
    })
}
