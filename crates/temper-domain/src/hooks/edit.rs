//! The pre-edit policy: an edit applied to the file's text (1.x `hooks.mjs` `applyEdit`), and
//! what the hook decides about the result (`preEdit`).

use std::borrow::Cow;

mod policy;
mod texts;

pub(crate) use policy::{Counts, Findings, hit_findings, is_mod_rs};
pub use policy::{
    DependencyCheck, EditFacts, EditTarget, EditTool, LazyExemptions, ManifestEdit,
    PolicyDecision, RustEdit, edit_policy,
};
pub use texts::{STATE_DIR_DENIAL, lockfile_denial};

/// An `Edit`, `MultiEdit` or `Write` tool call, as the application read it from the hook input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp<'a> {
    /// `Write`: the file's new text (1.x `ti.content ?? ''`).
    Write {
        /// The text.
        content: &'a str,
    },
    /// `Edit` (one step) or `MultiEdit` (each of `edits`), applied in order.
    Replace(Vec<EditStep<'a>>),
}

/// One replacement of an `Edit` or `MultiEdit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditStep<'a> {
    /// `old_string` when it is a string; `None` makes the edit fail, as 1.x's `typeof` check.
    pub old: Option<&'a str>,
    /// `new_string`, as JS's `String(new_string ?? '')` reads it.
    pub new: Cow<'a, str>,
    /// `replace_all`, as JS truthiness reads it.
    pub count: Replacements,
}

/// How many occurrences of `old` a step replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replacements {
    /// The first (`text.replace(old, () => new)`).
    First,
    /// Every one (`text.split(old).join(new)`).
    All,
}

/// The file's text after the edit (1.x `applyEdit`), or `None` when the edit would fail on its
/// own: a step's `old` is not a string or is not in the text it is applied to.
///
/// Each step sees the text the one before it left, and the replacement is literal (no `$&`).
/// An empty `old` with [`Replacements::All`] puts `new` between chars, never inside one (port
/// plan D16(o)); with [`Replacements::First`] it goes before the text.
///
/// # Examples
///
/// ```
/// use std::borrow::Cow;
/// use temper_domain::{EditOp, EditStep, Replacements, apply_edit};
///
/// let step = EditStep { old: Some("1"), new: Cow::Borrowed("2"), count: Replacements::All };
/// assert_eq!(apply_edit("1 + 1", &EditOp::Replace(vec![step])).as_deref(), Some("2 + 2"));
/// assert_eq!(apply_edit("old", &EditOp::Write { content: "new" }).as_deref(), Some("new"));
/// ```
#[must_use]
pub fn apply_edit(before: &str, op: &EditOp<'_>) -> Option<String> {
    let steps = match op {
        EditOp::Write { content } => return Some((*content).to_owned()),
        EditOp::Replace(steps) => steps,
    };
    let mut text = before.to_owned();
    for step in steps {
        let old = step.old.filter(|old| text.contains(old))?;
        text = match step.count {
            Replacements::First => text.replacen(old, &step.new, 1),
            Replacements::All if old.is_empty() => between_chars(&text, &step.new),
            Replacements::All => text.replace(old, &step.new),
        };
    }
    Some(text)
}

/// `text.split('').join(sep)`, per char rather than per UTF-16 unit.
fn between_chars(text: &str, sep: &str) -> String {
    let mut out = String::with_capacity(
        text.len()
            .saturating_add(sep.len().saturating_mul(text.len())),
    );
    for (i, c) in text.chars().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests;
