//! `#[expect(..)]` attributes: the `expect-attr` finder, and the lookup that turns a hard hit
//! below a reasoned `#[expect]` into an exception request (1.x `expectAttrs`, `exceptionFor`).

use std::borrow::Cow;

use regex::Regex;

use super::Found;
use crate::js::{is_js_space, js_trim};
use crate::scan::env::Env;
use crate::scan::hit::Severity;
use crate::scan::lexer::slice;
use crate::scan::scanner::{ScannerSetupError, compile};

/// How many lines above a hit an `#[expect(..)]` may start and still cover it.
const WINDOW_LINES: usize = 12;

const NO_REASON_MSG: &str = "`#[expect]` without a reason";
const NO_REASON_FIX: &str =
    "Add `reason = \"...\"` explaining why the lint does not apply. A human will review it.";

/// The compiled patterns behind the finder and the lookup.
#[derive(Debug)]
pub(crate) struct ExpectPatterns {
    attr: Regex,
    reason: Regex,
}

impl ExpectPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            attr: compile("expect:attr", r"#\s*!?\s*\[\s*expect\s*\(([^\]]*)\)\s*\]")?,
            reason: compile("expect:reason", r"\breason\s*=")?,
        })
    }
}

/// Pushes every `#[expect(..)]` onto `out`: hard without a reason, otherwise an exception request
/// for its first lint.
pub(crate) fn find(env: &Env<'_>, patterns: &ExpectPatterns, out: &mut Vec<Found>) {
    let found = patterns.attr.captures_iter(env.code).map(|caps| {
        let offset = caps.get_match().start();
        let args = caps.get(1).map_or("", |args| args.as_str());
        if patterns.reason.is_match(args) {
            let lint = js_trim(args.split_once(',').map_or(args, |(first, _)| first));
            Found {
                severity: Some(Severity::Ask),
                msg: Some(Cow::Owned(format!("lint exception requested for `{lint}`"))),
                exception: Some(lint.to_owned()),
                ..Found::at(offset)
            }
        } else {
            Found {
                severity: Some(Severity::Hard),
                msg: Some(Cow::Borrowed(NO_REASON_MSG)),
                fix: Some(NO_REASON_FIX),
                ..Found::at(offset)
            }
        }
    });
    out.extend(found);
}

/// The lint of `lints` that a reasoned `#[expect(..)]` starting at most 12 lines above `offset`
/// names. The last such attribute wins; within it, the first lint in table order.
pub(crate) fn exception_for(
    env: &Env<'_>,
    offset: usize,
    lints: &'static [&'static str],
    patterns: &ExpectPatterns,
) -> Option<&'static str> {
    if lints.is_empty() {
        return None;
    }
    let first_line = env.idx.line_of(offset).saturating_sub(WINDOW_LINES);
    let window = slice(env.code, env.idx.line_start(first_line), offset);
    patterns
        .attr
        .captures_iter(window)
        .filter_map(|caps| caps.get(1))
        .map(|args| args.as_str())
        .filter(|args| patterns.reason.is_match(args))
        .filter_map(|args| {
            let squeezed: String = args.chars().filter(|&c| !is_js_space(c)).collect();
            lints.iter().copied().find(|lint| squeezed.contains(lint))
        })
        .last()
}
