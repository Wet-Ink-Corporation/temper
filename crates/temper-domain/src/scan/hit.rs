//! What a scan reports: one [`Hit`] per rule match, with its key, rule id, severity and line.

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroUsize;

/// How a hit is enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Blocks the edit or fails the gate.
    Hard,
    /// An exception request (`#[expect(..)]`) that a human must approve.
    Ask,
    /// Advice; nothing is blocked.
    Soft,
}

/// A scanner rule's key, such as `unwrap`. Only the rule table creates them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleKey(&'static str);

impl RuleKey {
    /// Wraps a key from the rule table.
    pub(crate) const fn new(key: &'static str) -> Self {
        Self(key)
    }

    /// The key text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for RuleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// The rust-craft rule a hit enforces, such as `H-02`. Only the rule table creates them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(&'static str);

impl RuleId {
    /// Wraps an id from the rule table.
    pub(crate) const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// The id text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A 1-based line number; never 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LineNo(NonZeroUsize);

impl LineNo {
    /// The line number.
    #[must_use]
    pub fn get(self) -> usize {
        self.0.get()
    }

    /// The line at 0-based `index` (saturating at `usize::MAX`).
    pub(crate) fn from_index(index: usize) -> Self {
        Self(NonZeroUsize::MIN.saturating_add(index))
    }

    /// The line `n` lines earlier, never before line 1.
    pub(crate) fn saturating_sub(self, n: usize) -> Self {
        NonZeroUsize::new(self.get().saturating_sub(n)).map_or(Self(NonZeroUsize::MIN), Self)
    }
}

impl fmt::Display for LineNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One rule match in a scanned source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub(crate) key: RuleKey,
    pub(crate) id: RuleId,
    pub(crate) severity: Severity,
    pub(crate) line: LineNo,
    /// The trimmed source line, owned because a hit outlives the source it came from.
    pub(crate) text: String,
    pub(crate) msg: Cow<'static, str>,
    pub(crate) fix: Cow<'static, str>,
    /// The lint an `#[expect(..)]` names, taken from the source.
    pub(crate) exception: Option<String>,
}

impl Hit {
    /// The rule's key.
    #[must_use]
    pub fn key(&self) -> RuleKey {
        self.key
    }

    /// The rust-craft rule id.
    #[must_use]
    pub fn id(&self) -> RuleId {
        self.id
    }

    /// How the hit is enforced in this file's context.
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The line of the match.
    #[must_use]
    pub fn line(&self) -> LineNo {
        self.line
    }

    /// The source line, trimmed and cut to 160 UTF-16 code units, as 1.x reports it.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// What is wrong.
    #[must_use]
    pub fn msg(&self) -> &str {
        &self.msg
    }

    /// How to fix it.
    #[must_use]
    pub fn fix(&self) -> &str {
        &self.fix
    }

    /// For an exception request, the lint the `#[expect(..)]` names.
    #[must_use]
    pub fn exception(&self) -> Option<&str> {
        self.exception.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_no_counts_from_one_and_floors_at_one() {
        let line = LineNo::from_index(4);

        assert_eq!(
            (
                line.get(),
                line.saturating_sub(3).get(),
                line.saturating_sub(12).get()
            ),
            (5, 2, 1),
            "index 4 is line 5, and subtraction stops at line 1"
        );
    }
}
