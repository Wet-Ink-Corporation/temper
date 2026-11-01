//! The 1.x masking lexer (`lexer.mjs`): blanks comments and literal contents so the pattern rules
//! see only code, plus the offset helpers the rules share. Offsets are UTF-8 byte offsets.

use std::ops::Range;

use super::hit::LineNo;

/// `src` with comments and the contents of string and char literals replaced by spaces.
///
/// The result has the same length, and every byte is either unchanged or a space; `\n` and `\r`
/// are never replaced, so offsets and line numbers carry over to the source. Lifetimes and labels
/// stay. A masked char becomes one space per UTF-8 byte (1.x writes one per UTF-16 unit).
pub(crate) fn mask_rust(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut spans = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let step = lex_at(src, at);
        spans.extend(step.mask);
        at = step.next;
    }
    render(src, &spans)
}

/// What the lexer does at one offset: the span to blank, if any, and where to continue.
#[derive(Debug)]
struct Step {
    mask: Option<Range<usize>>,
    next: usize,
}

impl Step {
    const fn skip(next: usize) -> Self {
        Self { mask: None, next }
    }
}

/// The 1.x lexer's dispatch, branch for branch and in its order.
fn lex_at(src: &str, at: usize) -> Step {
    let bytes = src.as_bytes();
    let c = bytes.get(at).copied();
    let d = bytes.get(at.saturating_add(1)).copied();
    match (c, d) {
        (Some(b'/'), Some(b'/')) => line_comment(bytes, at),
        (Some(b'/'), Some(b'*')) => block_comment(bytes, at),
        (Some(b'r'), _) | (Some(b'b' | b'c'), Some(b'r')) if !follows_ident(bytes, at) => {
            raw_string(bytes, at).unwrap_or_else(|| Step::skip(at.saturating_add(1)))
        }
        (Some(b'"'), _) => string(bytes, at),
        (Some(b'\''), _) => char_or_lifetime(src, at),
        _ => Step::skip(at.saturating_add(1)),
    }
}

/// Is the byte before `at` an identifier byte (`[A-Za-z0-9_]`)?
fn follows_ident(bytes: &[u8], at: usize) -> bool {
    at.checked_sub(1)
        .and_then(|prev| bytes.get(prev))
        .is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The offset of the first `needle` at or after `from`, if any.
fn find_from(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let rest = bytes.get(from..)?;
    let found = rest.windows(needle.len()).position(|w| w == needle)?;
    Some(from.saturating_add(found))
}

/// `//` to the end of the line.
fn line_comment(bytes: &[u8], at: usize) -> Step {
    let end = find_from(bytes, at, b"\n").unwrap_or(bytes.len());
    Step {
        mask: Some(at..end),
        next: end,
    }
}

/// `/* .. */`, nested as in Rust, or to the end when unterminated.
fn block_comment(bytes: &[u8], at: usize) -> Step {
    let mut depth = 1_usize;
    let mut j = at.saturating_add(2);
    while j < bytes.len() && depth > 0 {
        match (bytes.get(j), bytes.get(j.saturating_add(1))) {
            (Some(b'/'), Some(b'*')) => {
                depth = depth.saturating_add(1);
                j = j.saturating_add(2);
            }
            (Some(b'*'), Some(b'/')) => {
                depth = depth.saturating_sub(1);
                j = j.saturating_add(2);
            }
            _ => j = j.saturating_add(1),
        }
    }
    Step {
        mask: Some(at..j),
        next: j,
    }
}

/// `r".."`, `r#".."#`, `br".."` or `cr".."`; `None` when no `"` follows the `#`s.
fn raw_string(bytes: &[u8], at: usize) -> Option<Step> {
    let prefix = if bytes.get(at) == Some(&b'r') { 1 } else { 2 };
    let hashes_from = at.saturating_add(prefix);
    let hashes = bytes
        .get(hashes_from..)
        .map_or(0, |rest| rest.iter().take_while(|&&b| b == b'#').count());
    let quote = hashes_from.saturating_add(hashes);
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }
    let mut close = vec![b'"'];
    close.resize(hashes.saturating_add(1), b'#');
    let body = quote.saturating_add(1);
    Some(match find_from(bytes, body, &close) {
        Some(end) => Step {
            mask: Some(body..end),
            next: end.saturating_add(close.len()),
        },
        None => Step {
            mask: Some(body..bytes.len()),
            next: bytes.len(),
        },
    })
}

/// `".."` (also the body of `b".."` and `c".."`), where `\` skips the next byte.
fn string(bytes: &[u8], at: usize) -> Step {
    let mut j = at.saturating_add(1);
    while let Some(&b) = bytes.get(j) {
        if b == b'"' {
            break;
        }
        j = j.saturating_add(if b == b'\\' { 2 } else { 1 });
    }
    let body = at.saturating_add(1)..j.min(bytes.len());
    Step {
        mask: Some(body),
        next: j.saturating_add(1),
    }
}

/// A char literal (`'x'`, `'\''`, `'\u{1F600}'`), or a lifetime or label left as code.
fn char_or_lifetime(src: &str, at: usize) -> Step {
    let bytes = src.as_bytes();
    let body = at.saturating_add(1);
    if bytes.get(body) == Some(&b'\\') {
        // Skip the escaped char, which may itself be a quote.
        let mut j = at.saturating_add(3);
        while bytes.get(j).is_some_and(|&b| b != b'\'' && b != b'\n') {
            j = j.saturating_add(1);
        }
        if bytes.get(j) == Some(&b'\'') {
            return Step {
                mask: Some(body..j),
                next: j.saturating_add(1),
            };
        }
    } else if let Some(c) = src.get(body..).and_then(|rest| rest.chars().next()) {
        let close = body.saturating_add(c.len_utf8());
        if c != '\'' && bytes.get(close) == Some(&b'\'') {
            return Step {
                mask: Some(body..close),
                next: close.saturating_add(1),
            };
        }
    }
    Step::skip(body)
}

/// Copies `src`, replacing every char that starts inside a span (sorted, not overlapping) with
/// one space per byte, except `\n` and `\r`.
fn render(src: &str, spans: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(src.len());
    let mut pending = spans.iter().peekable();
    for (at, c) in src.char_indices() {
        while pending.next_if(|span| span.end <= at).is_some() {}
        let masked = pending.peek().is_some_and(|span| span.contains(&at));
        if masked && c != '\n' && c != '\r' {
            out.extend(std::iter::repeat_n(' ', c.len_utf8()));
        } else {
            out.push(c);
        }
    }
    out
}

/// The start offset of every line of a text, for offset-to-line lookups.
#[derive(Debug)]
pub(crate) struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    /// Indexes the lines of `text`.
    pub(crate) fn new(text: &str) -> Self {
        let breaks = text
            .bytes()
            .enumerate()
            .filter(|&(_, b)| b == b'\n')
            .map(|(at, _)| at.saturating_add(1));
        Self {
            starts: std::iter::once(0).chain(breaks).collect(),
        }
    }

    /// The line holding `offset`; an offset past the end is on the last line.
    pub(crate) fn line_of(&self, offset: usize) -> LineNo {
        let starts_before = self.starts.partition_point(|&start| start <= offset);
        LineNo::from_index(starts_before.saturating_sub(1))
    }

    /// The offset where `line` starts, or 0 past the last line (1.x `?? 0`).
    pub(crate) fn line_start(&self, line: LineNo) -> usize {
        self.starts
            .get(line.get().saturating_sub(1))
            .copied()
            .unwrap_or(0)
    }

    /// The text of `line` in `source` (a text with the same newlines), without its `\n`.
    pub(crate) fn line_text<'s>(&self, line: LineNo, source: &'s str) -> &'s str {
        let rest = source.get(self.line_start(line)..).unwrap_or("");
        rest.split_once('\n').map_or(rest, |(text, _)| text)
    }
}

/// The offset of the `}` closing the `{` at `open`, or the last offset when unbalanced.
pub(crate) fn match_brace(code: &str, open: usize) -> usize {
    // Signed, as in 1.x: a `}` before any `{` takes the depth below zero.
    let mut depth = 0_isize;
    let tail = code.as_bytes().iter().enumerate().skip(open);
    for (at, &b) in tail {
        match b {
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return at;
                }
            }
            _ => {}
        }
    }
    code.len().saturating_sub(1)
}

/// JS `text.slice(from, to)` on byte offsets: both ends clamp to the length and floor to a char
/// boundary, and a reversed range is empty.
pub(crate) fn slice(text: &str, from: usize, to: usize) -> &str {
    let (from, to) = (text.floor_char_boundary(from), text.floor_char_boundary(to));
    text.get(from..to).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::strategies::token_soup;
    use proptest::prelude::*;

    #[test]
    fn masks_strings_raw_strings_chars_and_comments_but_keeps_lifetimes() {
        let src = "fn f<'a>(x: &'a str) -> char { let s = \"x.unwrap()\"; let r = r#\"panic!(\"no\")\"#; // .unwrap()\n let c = '\"'; /* unsafe { } */ 'q' }";

        let code = mask_rust(src);

        assert!(
            !code.contains("unwrap"),
            "string and comment text is masked"
        );
        assert!(!code.contains("panic"), "raw string text is masked");
        assert!(!code.contains("unsafe"), "block comment text is masked");
        assert!(code.contains("<'a>"), "lifetimes survive");
        assert_eq!(code.len(), src.len(), "masking keeps the length");
        assert_eq!(
            code,
            "fn f<'a>(x: &'a str) -> char { let s = \"          \"; let r = r#\"            \"#;             \n let c = ' ';                  ' ' }",
            "the exact 1.x output"
        );
    }

    #[test]
    fn masks_edge_literals_like_1x() {
        let cases = [
            ("a /* x /* y */ z */ b", "a                   b"),
            ("let s = \"abc", "let s = \"   "),
            ("x /* never closed", "x                "),
            ("let r = br#\"a\"b\"#; c", "let r = br#\"   \"#; c"),
            (
                "let r = cr\"x\"; let k = r#ident;",
                "let r = cr\" \"; let k = r#ident;",
            ),
            ("let b = b\"\\x00\\\"q\"; z", "let b = b\"       \"; z"),
            (
                "let q = '\\''; let n = '\\n'; let u = '\\u{1F600}';",
                "let q = '  '; let n = '  '; let u = '         ';",
            ),
            (
                "'outer: loop { break 'outer; } fn g<'b>() {}",
                "'outer: loop { break 'outer; } fn g<'b>() {}",
            ),
            ("let x = ''; y", "let x = ''; y"),
            ("for r in v { bar\"x\" }", "for r in v { bar\" \" }"),
            ("let s = \"ab\\", "let s = \"   "),
            ("a // c\r\nb \"s\r\nt\" c", "a     \r\nb \" \r\n \" c"),
            (
                "/// doc\n//// not doc\n//! inner\n/** block */\n/**/ x",
                "       \n            \n         \n            \n     x",
            ),
        ];

        for (src, expected) in cases {
            assert_eq!(mask_rust(src), expected, "masking {src:?}");
        }
    }

    #[test]
    fn masks_an_astral_char_literal_as_four_spaces() {
        assert_eq!(
            mask_rust("let e = '😀'; x"),
            "let e = '    '; x",
            "a masked char becomes one space per UTF-8 byte"
        );
    }

    #[test]
    fn lexer_escaped_quote_char_literal() {
        assert_eq!(
            mask_rust("let q = ['\\'', ',', 'a']; let s = \".unwrap()\";"),
            "let q = ['  ', ' ', ' ']; let s = \"         \";",
            "the 1.x escaped-quote case"
        );
    }

    #[test]
    fn line_index_maps_offsets_to_lines() {
        let src = "a\nbc\n\nd";
        let idx = LineIndex::new(src);

        let lines: Vec<usize> = [0, 1, 2, 4, 5, 6, 99]
            .into_iter()
            .map(|o| idx.line_of(o).get())
            .collect();

        assert_eq!(idx.starts, [0, 2, 5, 6], "a line starts after each newline");
        assert_eq!(
            lines,
            [1, 1, 2, 2, 3, 4, 4],
            "offsets past the end are on the last line"
        );
        assert_eq!(
            idx.line_text(LineNo::from_index(1), src),
            "bc",
            "line 2 is bc"
        );
        assert_eq!(
            idx.line_text(LineNo::from_index(8), src),
            "a",
            "a line past the end reads from offset 0, as 1.x does"
        );
    }

    #[test]
    fn match_brace_returns_last_offset_when_unbalanced() {
        assert_eq!(
            match_brace("{ { }", 0),
            4,
            "unbalanced runs to the last offset"
        );
        assert_eq!(match_brace("a{b}c", 1), 3, "balanced finds the closer");
    }

    #[test]
    fn slice_floors_to_char_boundaries() {
        assert_eq!(
            slice("aé", 0, 2),
            "a",
            "the end inside é floors to its start"
        );
        assert_eq!(slice("ab", 2, 1), "", "a reversed range is empty");
        assert_eq!(slice("ab", 0, 99), "ab", "the end clamps to the length");
    }

    fn assert_mask_shape(src: &str) -> Result<(), TestCaseError> {
        let masked = mask_rust(src);
        prop_assert_eq!(masked.len(), src.len());
        for (k, (&m, &s)) in masked.as_bytes().iter().zip(src.as_bytes()).enumerate() {
            prop_assert!(m == s || m == b' ', "byte {} changed to {:?}", k, m);
            prop_assert_eq!(m == b'\n', s == b'\n', "newline moved at {}", k);
            prop_assert_eq!(m == b'\r', s == b'\r', "carriage return moved at {}", k);
        }
        Ok(())
    }

    proptest! {
        #[test]
        fn mask_preserves_length_and_newlines(src in any::<String>()) {
            assert_mask_shape(&src)?;
        }

        #[test]
        fn mask_preserves_length_and_newlines_on_token_soup(src in token_soup()) {
            assert_mask_shape(&src)?;
        }
    }
}
