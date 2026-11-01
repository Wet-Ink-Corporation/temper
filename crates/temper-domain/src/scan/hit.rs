//! What a scan reports: one [`Hit`] per rule match, with its key, rule id, severity and line.

use std::borrow::Cow;
use std::collections::HashMap;
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
    /// Always a table text: in 1.x only `msg` is ever formatted, never `fix`.
    pub(crate) fix: &'static str,
    /// For a hard hit turned into an exception request, the rule-table lint that the covering
    /// reasoned `#[expect]` lists; for an `expect-attr` hit, the attribute's first argument as
    /// written. Owned because the second comes from the source.
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
        self.fix
    }

    /// For an exception request, the lint it asks for: for a hard hit below a reasoned
    /// `#[expect(..)]`, the rule's lint that the attribute lists; for the `expect-attr` hit
    /// itself, the attribute's first argument as written, trimmed.
    #[must_use]
    pub fn exception(&self) -> Option<&str> {
        self.exception.as_deref()
    }
}

/// The hits in `after` that `before` does not have (1.x `newHits`).
///
/// A multiset difference on the rule key and the hit's text, so a hit that only moved to
/// another line is not new. Keeps `after`'s order, and hands back `after`'s own hits rather
/// than copies.
///
/// # Examples
///
/// ```
/// use temper_domain::{CodeKind, FileRole, ScanOptions, Scanner, new_hits};
///
/// let scanner = Scanner::new()?;
/// let options = ScanOptions { kind: CodeKind::Prod, file: FileRole::Source, forbid: None };
/// let before = scanner.scan("fn a() { dbg!(1); }", &options).into_hits();
/// let after = scanner.scan("\nfn a() { dbg!(1); }\nfn b() { dbg!(2); }", &options).into_hits();
///
/// let fresh: Vec<usize> = new_hits(&before, after).iter().map(|h| h.line().get()).collect();
/// assert_eq!(fresh, [3]);
/// # Ok::<(), temper_domain::ScannerSetupError>(())
/// ```
#[must_use]
pub fn new_hits(before: &[Hit], mut after: Vec<Hit>) -> Vec<Hit> {
    // The count of each text per key. Only looked up, never iterated, so the maps' order is
    // never observed. One map per key, keyed by `&str`, so a hit of `after` can look up its
    // text by `str` without borrowing for as long as `before`.
    let mut pool: HashMap<RuleKey, HashMap<&str, usize>> = HashMap::new();
    for hit in before {
        pool.entry(hit.key)
            .or_default()
            .entry(hit.text.as_str())
            .and_modify(|n| *n = n.saturating_add(1))
            .or_insert(1);
    }
    after.retain(|hit| {
        let Some(n) = pool
            .get_mut(&hit.key)
            .and_then(|texts| texts.get_mut(hit.text.as_str()))
        else {
            return true;
        };
        match n.checked_sub(1) {
            Some(left) => {
                *n = left;
                false
            }
            None => true,
        }
    });
    after
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{CodeKind, FileRole, ScanOptions, Scanner};

    #[test]
    fn new_hits_is_a_multiset_on_key_and_text() {
        let scanner = Scanner::new().expect("every built-in pattern compiles");
        let hits = |src: &str| {
            let options = ScanOptions {
                kind: CodeKind::Prod,
                file: FileRole::Source,
                forbid: None,
            };
            scanner.scan(src, &options).into_hits()
        };
        let one = "fn a() { f().unwrap(); }";
        let two = format!("{one}\n{one}");
        let cases: [(&str, &str, &[&str]); 7] = [
            (
                "fn a() { f().unwrap(); dbg!(1); }",
                "fn a() { f().unwrap(); dbg!(1); }\nfn a() { f().unwrap(); dbg!(1); }",
                &["unwrap@2", "dbg@2"],
            ),
            (&two, one, &[]),
            (one, "\n\n  fn a() { f().unwrap(); }  ", &[]),
            (one, "fn a() {\n f().unwrap(); }", &["unwrap@2"]),
            (
                "fn a() { f().unwrap().unwrap(); }",
                "fn a() { f().unwrap().unwrap(); }\nfn a() { f().unwrap().unwrap(); }",
                &["unwrap@2", "unwrap@2"],
            ),
            (
                "",
                "fn a() { dbg!(1); f().unwrap(); }",
                &["unwrap@1", "dbg@1"],
            ),
            (one, "", &[]),
        ];

        for (before, after, want) in cases {
            let fresh: Vec<String> = new_hits(&hits(before), hits(after))
                .iter()
                .map(|h| format!("{}@{}", h.key(), h.line()))
                .collect();
            assert_eq!(fresh, want, "{before:?} -> {after:?}");
        }
    }

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
