//! Glob matching as 1.x's `globMatch`: `**/` is any run of directories, `**` anything on one
//! line, `*` anything within a segment, `?` one char of a segment, and `\` reads as `/` in both
//! the pattern and the path.

use std::borrow::Cow;

use regex::Regex;

use crate::js::{self, JsFlags, UnsupportedJs};

/// One compiled glob.
///
/// # Examples
///
/// ```
/// use temper_domain::Glob;
///
/// let glob = Glob::new("**/clippy.toml")?;
/// assert!(glob.is_match("clippy.toml"));
/// assert!(glob.is_match(r"crates\a\clippy.toml"));
/// assert!(!glob.is_match("aclippy.toml"));
/// # Ok::<(), temper_domain::GlobError>(())
/// ```
#[derive(Debug, Clone)]
pub struct Glob(Regex);

/// A glob that does not compile.
#[derive(Debug, thiserror::Error)]
#[error("glob {pattern:?} does not compile")]
pub struct GlobError {
    pattern: String,
    #[source]
    source: GlobCause,
}

/// Why a glob failed: its JS regex has no faithful translation, or `regex` rejects it.
#[derive(Debug, thiserror::Error)]
enum GlobCause {
    #[error(transparent)]
    Js(#[from] UnsupportedJs),
    #[error(transparent)]
    Regex(#[from] regex::Error),
}

impl GlobError {
    /// The glob as it was given.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

impl Glob {
    /// Compiles `pattern` as 1.x does: the same JS regex source, translated with JS's meaning
    /// of `.` (it stops at a line terminator), so `**` never crosses a newline.
    ///
    /// # Errors
    ///
    /// [`GlobError`] when the regex does not compile, which only a pattern past `regex`'s size
    /// limit reaches: every char but `*` and `?` is escaped or literal.
    pub fn new(pattern: &str) -> Result<Self, GlobError> {
        let compiled = js::translate(&js_source(pattern), JsFlags::Plain)
            .map_err(GlobCause::from)
            .and_then(|translated| Regex::new(&translated).map_err(GlobCause::from));
        compiled.map(Self).map_err(|source| GlobError {
            pattern: pattern.to_owned(),
            source,
        })
    }

    /// Does the whole of `rel` match, with `\` read as `/`?
    #[must_use]
    pub fn is_match(&self, rel: &str) -> bool {
        self.0.is_match(&slashed(rel))
    }
}

/// 1.x `norm`: `\` becomes `/`.
fn slashed(text: &str) -> Cow<'_, str> {
    if text.contains('\\') {
        Cow::Owned(text.replace('\\', "/"))
    } else {
        Cow::Borrowed(text)
    }
}

/// The JS regex source 1.x's `globMatch` builds for `pattern`.
fn js_source(pattern: &str) -> String {
    let pattern = slashed(pattern);
    let mut re = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.next_if_eq(&'*').is_some() => {
                re.push_str(if chars.next_if_eq(&'/').is_some() {
                    "(?:.*/)?"
                } else {
                    ".*"
                });
            }
            '*' => re.push_str("[^/]*"),
            // One char, not 1.x's one UTF-16 unit (D16(o)).
            '?' => re.push_str("[^/]"),
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            c => re.push(c),
        }
    }
    re.push('$');
    re
}

/// Several globs, matched as 1.x's `globs.some((p) => globMatch(p, rel))`.
///
/// # Examples
///
/// ```
/// use temper_domain::Globs;
///
/// let protected = Globs::new([".cargo/**", "AGENTS.md"])?;
/// assert!(protected.any_match(".cargo/config.toml"));
/// assert!(!protected.any_match("src/lib.rs"));
/// # Ok::<(), temper_domain::GlobError>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct Globs(Vec<Glob>);

impl Globs {
    /// Compiles each pattern.
    ///
    /// # Errors
    ///
    /// The first [`GlobError`], as [`Glob::new`] gives it.
    pub fn new<'a>(patterns: impl IntoIterator<Item = &'a str>) -> Result<Self, GlobError> {
        patterns
            .into_iter()
            .map(Glob::new)
            .collect::<Result<_, _>>()
            .map(Self)
    }

    /// Does any glob match `rel`?
    #[must_use]
    pub fn any_match(&self, rel: &str) -> bool {
        self.0.iter().any(|glob| glob.is_match(rel))
    }
}

#[cfg(test)]
mod tests;
