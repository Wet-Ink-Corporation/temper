//! Silently discarded errors: the `empty-if-let-err` finder (1.x `emptyIfLetErr`), and the test
//! behind `empty-err-arm`'s keep that another arm of the match binds the error (1.x
//! `otherArmBindsErr`).

use regex::Regex;

use super::Found;
use crate::js::{js_trim, utf16_window_start};
use crate::scan::env::Env;
use crate::scan::lexer::{match_brace, next_char, slice};
use crate::scan::scanner::{ScannerSetupError, compile};

/// The UTF-16 code units before a match's `{` that 1.x searches for `match` (`open - 300`).
const MATCH_HEAD_WINDOW_UNITS: usize = 300;

/// The compiled patterns of the `empty-if-let-err` finder.
#[derive(Debug)]
pub(crate) struct IfLetErrPatterns {
    /// 1.x `\bif\s+let\s+Err\s*\(\s*(?:_\w*|\.\.)\s*\)\s*=(?!=)`, without the look-ahead,
    /// which [`find`] tests on the byte after the match.
    head: Regex,
    else_after: Regex,
}

impl IfLetErrPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            head: compile(
                "if-let-err:head",
                r"\bif\s+let\s+Err\s*\(\s*(?:_\w*|\.\.)\s*\)\s*=",
            )?,
            else_after: compile("if-let-err:else", r"^\s*else\b")?,
        })
    }
}

/// The compiled patterns of [`other_arm_binds_err`].
#[derive(Debug)]
pub(crate) struct ErrArmPatterns {
    match_head: Regex,
    unbound_err: Regex,
    bound_err: Regex,
}

impl ErrArmPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            match_head: compile("err-arms:match-head", r"\bmatch\b[^;{}]*$")?,
            unbound_err: compile("err-arms:unbound", r"\bErr\s*\(\s*_\w*\s*\)")?,
            bound_err: compile(
                "err-arms:bound",
                r"\bErr\s*\(\s*(?:ref\s+)?(?:mut\s+)?[a-z]\w*\s*[)@]",
            )?,
        })
    }
}

/// Pushes every `if let Err(_) = <scrutinee> {}` whose body is empty and has no `else`.
pub(crate) fn find(env: &Env<'_>, p: &IfLetErrPatterns, out: &mut Vec<Found>) {
    let code = env.code;
    let mut at = 0;
    // `find_at` panics past the end; every step moves `at` forward, so the loop ends.
    while at <= code.len() {
        let Some(m) = p.head.find_at(code, at) else {
            break;
        };
        if code.as_bytes().get(m.end()) == Some(&b'=') {
            // The look-ahead fails: JS tries the next start.
            at = next_char(code, m.start());
            continue;
        }
        at = m.end();
        let Some(open) = if_let_body(code, m.end()) else {
            continue;
        };
        let close = match_brace(code, open);
        let empty = js_trim(slice(code, open.saturating_add(1), close)).is_empty();
        let rest = slice(code, close.saturating_add(1), code.len());
        if empty && !p.else_after.is_match(rest) {
            out.push(Found::at(m.start()));
        }
    }
}

/// The `{` opening the body of the `if let` whose scrutinee starts at `from`: the first `{` at
/// `(`/`[` depth 0, skipping blocks inside brackets. `None` when a `;` or `}` at depth <= 0 or
/// the end comes first.
fn if_let_body(code: &str, from: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    // Signed, as in 1.x: a closer before any opener takes the depth below zero.
    let mut depth = 0_isize;
    let mut k = from;
    while let Some(&b) = bytes.get(k) {
        match b {
            b'(' | b'[' => depth = depth.saturating_add(1),
            b')' | b']' => depth = depth.saturating_sub(1),
            b'{' if depth == 0 => return Some(k),
            b'{' => k = match_brace(code, k),
            b';' | b'}' if depth <= 0 => return None,
            _ => {}
        }
        k = k.saturating_add(1);
    }
    None
}

/// Does another arm of the `match` enclosing `offset` bind an `Err` payload to a variable?
/// The enclosing block must follow a `match` head in the 300 UTF-16 units before it.
pub(crate) fn other_arm_binds_err(env: &Env<'_>, p: &ErrArmPatterns, offset: usize) -> bool {
    let code = env.code;
    let Some(open) = enclosing_open(code, offset) else {
        return false;
    };
    let window_start = utf16_window_start(env.src, open, MATCH_HEAD_WINDOW_UNITS);
    if !p.match_head.is_match(slice(code, window_start, open)) {
        return false;
    }
    let arms = arms_at_own_depth(code, open, match_brace(code, open));
    p.bound_err.is_match(&p.unbound_err.replace_all(&arms, ""))
}

/// The `{` of the innermost block that holds `offset`, walking back over braces.
fn enclosing_open(code: &str, offset: usize) -> Option<usize> {
    let mut depth = 0_usize;
    let before = code.as_bytes().get(..offset)?;
    for (k, &b) in before.iter().enumerate().rev() {
        match b {
            b'}' => depth = depth.saturating_add(1),
            b'{' => match depth.checked_sub(1) {
                Some(outer) => depth = outer,
                None => return Some(k),
            },
            _ => {}
        }
    }
    None
}

/// The code between `open` and `close` with every nested `{ .. }` replaced by one space per
/// byte, so only the arms at the match's own depth remain.
fn arms_at_own_depth(code: &str, open: usize, close: usize) -> String {
    let bytes = code.as_bytes();
    let mut arms = String::with_capacity(close.saturating_sub(open));
    let mut piece = open.saturating_add(1);
    let mut k = piece;
    while k < close {
        if bytes.get(k) == Some(&b'{') {
            arms.push_str(slice(code, piece, k));
            let end = match_brace(code, k);
            arms.extend(std::iter::repeat_n(
                ' ',
                end.saturating_sub(k).saturating_add(1),
            ));
            k = end.saturating_add(1);
            piece = k;
        } else {
            k = k.saturating_add(1);
        }
    }
    arms.push_str(slice(code, piece, close));
    arms
}
