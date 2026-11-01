//! `forbid-override`: an `#[allow(..)]` or `#[expect(..)]`, inner or outer, also inside
//! `cfg_attr`, that names a lint the crate forbids (1.x `forbidOverrides`). Without the forbidden
//! lints ([`Env::forbid`]) it finds nothing.

use std::borrow::Cow;
use std::collections::BTreeSet;

use regex::Regex;

use super::Found;
use crate::js::is_js_space;
use crate::scan::env::Env;
use crate::scan::hit::Severity;
use crate::scan::lexer::{match_pair, slice, split_top};
use crate::scan::scanner::{ScannerSetupError, compile};

/// The compiled patterns of the finder.
#[derive(Debug)]
pub(crate) struct ForbidPatterns {
    attr: Regex,
    level: Regex,
    lint: Regex,
}

impl ForbidPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            attr: compile("forbid:attr", r"#\s*!?\s*\[")?,
            level: compile("forbid:level", r"\b(allow|expect)\s*\(")?,
            lint: compile("forbid:lint", r"^[a-z_][a-z0-9_]*(?:::[a-z_][a-z0-9_]*)?$")?,
        })
    }
}

/// Pushes a hard hit at the attribute for every forbidden lint each `allow(..)` or `expect(..)`
/// in it names.
pub(crate) fn find(env: &Env<'_>, p: &ForbidPatterns, out: &mut Vec<Found>) {
    let Some(forbid) = env.forbid.filter(|set| !set.is_empty()) else {
        return;
    };
    let code = env.code;
    for attr in p.attr.find_iter(code) {
        // The match ends with its only `[`.
        let open = attr.end().saturating_sub(1);
        let body = slice(
            code,
            open.saturating_add(1),
            match_pair(code, open, b'[', b']'),
        );
        for caps in p.level.captures_iter(body) {
            let level = caps.get(1).map_or("", |l| l.as_str());
            // The match ends with the `(`.
            let lp = caps.get_match().end().saturating_sub(1);
            let inner = slice(body, lp.saturating_add(1), match_pair(body, lp, b'(', b')'));
            let found = forbidden_lints(inner, forbid, p).map(|lint| Found {
                severity: Some(Severity::Hard),
                msg: Some(Cow::Owned(format!(
                    "`#[{level}({lint})]` overrides a lint the workspace forbids (E0453 without --cap-lints)"
                ))),
                ..Found::at(attr.start())
            });
            out.extend(found);
        }
    }
}

/// The items of the lint list `inner`, JS spaces removed, that are lints the crate forbids.
fn forbidden_lints<'i>(
    inner: &'i str,
    forbid: &BTreeSet<String>,
    p: &ForbidPatterns,
) -> impl Iterator<Item = Cow<'i, str>> {
    split_top(inner, b',')
        .map(without_js_spaces)
        .filter(move |lint| p.lint.is_match(lint) && forbid.contains(lint.as_ref()))
}

/// 1.x `item.replace(/\s+/g, '')`: `item` with every JS space removed, borrowed when it has none.
fn without_js_spaces(item: &str) -> Cow<'_, str> {
    if item.contains(is_js_space) {
        Cow::Owned(item.chars().filter(|&c| !is_js_space(c)).collect())
    } else {
        Cow::Borrowed(item)
    }
}
