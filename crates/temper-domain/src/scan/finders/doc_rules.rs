//! The doc-comment rules (1.x `docUnwrap` and `docArgSections`).

use std::borrow::Cow;

use regex::Regex;

use super::Found;
use crate::js::is_js_space;
use crate::scan::doc_blocks::DocBlockPatterns;
use crate::scan::env::Env;
use crate::scan::lexer::mask_rust;
use crate::scan::scanner::{ScannerSetupError, compile};

/// The doc rules' compiled patterns.
#[derive(Debug)]
pub(crate) struct DocRulePatterns {
    blocks: DocBlockPatterns,
    /// 1.x `DOC_BAD`: the calls a doctest must not make, over the masked fence code.
    bad: Regex,
    should_panic: Regex,
    arg_section: Regex,
}

impl DocRulePatterns {
    /// Compiles the doc rules' 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            blocks: DocBlockPatterns::new()?,
            bad: compile(
                "doc-unwrap",
                r#"\.\s*(?:unwrap\s*\(\s*\)|unwrap_err\s*\(\s*\)|expect\s*\(\s*"|expect_err\s*\(\s*")|\b(?:panic|todo|unimplemented)\s*!"#,
            )?,
            should_panic: compile(
                "doc-unwrap:should-panic",
                r"(?:^|[\s,])should_panic(?:$|[\s,])",
            )?,
            arg_section: compile(
                "doc-arg-sections",
                r"^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:Arguments|Parameters|Params|Returns|Return value)(?:\*\*)?\s*:?\s*(?:\*\*)?\s*$",
            )?,
        })
    }
}

/// Unwrap, expect and panics in compiled doctest fences (1.x `docUnwrap`). Hidden lines
/// (`# ..`) are compiled too, so they are checked.
pub(crate) fn find_doc_unwrap(env: &Env<'_>, p: &DocRulePatterns, out: &mut Vec<Found>) {
    for fence in &env.doc(&p.blocks).fences {
        if !is_rust_fence(fence.info) || p.should_panic.is_match(fence.info) {
            continue;
        }
        // The lines plus one newline between each: an upper bound, since unhiding only removes.
        let size = fence
            .lines
            .iter()
            .fold(0_usize, |n, line| n.saturating_add(line.text.len()))
            .saturating_add(fence.lines.len().saturating_sub(1));
        let mut body = String::with_capacity(size);
        for (i, line) in fence.lines.iter().enumerate() {
            if i > 0 {
                body.push('\n');
            }
            body.push_str(&unhide(line.text));
        }
        let masked = mask_rust(&body);
        for (code, line) in masked.code.split('\n').zip(&fence.lines) {
            if p.bad.is_match(code) {
                out.push(Found::at(line.offset));
            }
        }
    }
}

/// `# Arguments`/`# Returns` sections in doc prose (1.x `docArgSections`).
pub(crate) fn find_doc_arg_sections(env: &Env<'_>, p: &DocRulePatterns, out: &mut Vec<Found>) {
    let prose = &env.doc(&p.blocks).prose;
    out.extend(
        prose
            .iter()
            .filter(|line| p.arg_section.is_match(line.text))
            .map(|line| Found::at(line.offset)),
    );
}

/// Is a fence with this info string compiled as a Rust doctest (1.x `isRustFence`)? Every
/// token, split on JS spaces and commas, must be one rustdoc compiles as Rust; an empty info
/// is Rust. 1.x's `NOT_COMPILED_TOKEN` check is implied, since none of its tokens is accepted.
fn is_rust_fence(info: &str) -> bool {
    info.split(|c: char| is_js_space(c) || c == ',')
        .filter(|token| !token.is_empty())
        .all(|token| {
            matches!(
                token,
                "rust" | "no_run" | "should_panic" | "test_harness" | "standalone_crate"
            ) || numbered(token, "edition")
                || numbered(token, "E")
        })
}

/// Is `token` `prefix` followed by exactly 4 ASCII digits (1.x `edition\d{4}`, `E\d{4}`)?
fn numbered(token: &str, prefix: &str) -> bool {
    token
        .strip_prefix(prefix)
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// A doctest line as rustdoc compiles it: 1.x's `replace(/^(\s*)# /, '$1')`, then
/// `replace(/^(\s*)#$/, '$1')` on the result. Borrows when neither changes the line.
fn unhide(line: &str) -> Cow<'_, str> {
    let (spaces, rest) = split_js_spaces(line);
    let shown = match rest.strip_prefix("# ") {
        Some(tail) => Cow::Owned([spaces, tail].concat()),
        None => Cow::Borrowed(line),
    };
    let (spaces, rest) = split_js_spaces(&shown);
    if rest != "#" {
        return shown;
    }
    let keep = spaces.len();
    match shown {
        Cow::Borrowed(text) => Cow::Borrowed(text.get(..keep).unwrap_or(text)),
        Cow::Owned(mut text) => {
            // `keep` ends the leading spaces, so it is a char boundary.
            text.truncate(keep);
            Cow::Owned(text)
        }
    }
}

/// `text` split after its leading JS spaces.
fn split_js_spaces(text: &str) -> (&str, &str) {
    let rest = text.trim_start_matches(is_js_space);
    text.split_at_checked(text.len().saturating_sub(rest.len()))
        .unwrap_or(("", text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1.x's two `replace` calls, run under Node 22.22.0 on each row (1d.1 review F1): only JS
    /// spaces count as indent, so U+00A0, U+FEFF and U+2028 do and U+0085 does not.
    #[test]
    fn unhide_like_1x() {
        let rows = [
            ("\u{a0}# x", "\u{a0}x"),
            ("\u{feff}#", "\u{feff}"),
            ("\u{85}# x", "\u{85}# x"),
            ("#x", "#x"),
            ("  #", "  "),
            ("# ", ""),
            ("#", ""),
            ("  # #", "  "),
            ("\u{2028}# y", "\u{2028}y"),
        ];

        let wrong: Vec<(&str, String)> = rows
            .iter()
            .filter(|(line, want)| unhide(line) != *want)
            .map(|(line, _)| (*line, unhide(line).into_owned()))
            .collect();

        assert_eq!(wrong, Vec::new(), "lines unhidden as 1.x does");
    }

    /// 1.x test 15's `isRustFence` assertions, with the oracle's extra tokens.
    #[test]
    fn is_rust_fence_tokens() {
        let rust = [
            "",
            "rust",
            "no_run",
            "rust,edition2024",
            "E0277",
            "edition2024",
            "rust no_run",
            ",rust,",
            "rust,,should_panic",
            "rust\u{a0}no_run",
            "rust\u{feff}no_run",
            "rust\u{2028}no_run",
            " ",
            "standalone_crate,test_harness",
        ];
        let not_rust = [
            "text",
            "ignore",
            "rust,ignore",
            "compile_fail",
            "sh",
            "edition24",
            "ignore-wasm",
            "Rust",
            "e0277",
            // U+0085 is not a JS space, so `rust\u{85}no_run` is one unknown token.
            "rust\u{85}no_run",
        ];

        let wrong: Vec<&str> = rust
            .iter()
            .filter(|info| !is_rust_fence(info))
            .chain(not_rust.iter().filter(|info| is_rust_fence(info)))
            .copied()
            .collect();

        assert_eq!(
            wrong,
            Vec::<&str>::new(),
            "fence infos classified as 1.x does"
        );
    }
}
