//! The 1.x masking lexer (`lexer.mjs`): blanks comments and literal contents so the pattern rules
//! see only code, plus the offset helpers the rules share. Offsets are UTF-8 byte offsets.

use std::ops::Range;

use super::hit::LineNo;

/// A source with its comments and literal contents masked, and its comments (1.x `maskRust`).
#[derive(Debug)]
pub(crate) struct Masked<'s> {
    /// The source with comments and the contents of string and char literals replaced by
    /// spaces: the same length, every byte unchanged or a space, `\n` and `\r` never replaced.
    pub(crate) code: String,
    /// Every `//` and `/* */` comment, in source order.
    pub(crate) comments: Vec<Comment<'s>>,
}

/// Whether a comment is a doc comment (1.x `doc`): `///` but not `////`, `//!`, `/**` but not
/// `/**/`, and `/*!` (decision 1c-5, wi-91ace5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentKind {
    Plain,
    Doc,
}

/// A comment of the source (1.x `{ start, end, text, doc }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Comment<'s> {
    /// The offset of its first `/`.
    pub(crate) start: usize,
    /// The offset after it (exclusive).
    pub(crate) end: usize,
    /// `src[start..end]`.
    pub(crate) text: &'s str,
    pub(crate) kind: CommentKind,
}

impl CommentKind {
    /// The kind of the comment whose whole text is `text`.
    fn of(text: &str) -> Self {
        let line_doc =
            (text.starts_with("///") || text.starts_with("//!")) && !text.starts_with("////");
        let block_doc =
            (text.starts_with("/**") || text.starts_with("/*!")) && !text.starts_with("/**/");
        if line_doc || block_doc {
            Self::Doc
        } else {
            Self::Plain
        }
    }
}

/// Masks `src` and records its comments. Offsets and line numbers carry over from the source to
/// the masked code. Lifetimes and labels stay. A masked char becomes one space per UTF-8 byte
/// (1.x writes one per UTF-16 unit).
pub(crate) fn mask_rust(src: &str) -> Masked<'_> {
    let bytes = src.as_bytes();
    let mut spans = Vec::new();
    let mut comments = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let step = lex_at(src, at);
        match step.span {
            Some(Span::Comment(range)) => {
                let text = slice(src, range.start, range.end);
                comments.push(Comment {
                    start: range.start,
                    end: range.end,
                    text,
                    kind: CommentKind::of(text),
                });
                spans.push(range);
            }
            Some(Span::Literal(range)) => spans.push(range),
            None => {}
        }
        at = step.next;
    }
    Masked {
        code: render(src, &spans),
        comments,
    }
}

/// A span the lexer blanks.
#[derive(Debug)]
enum Span {
    /// The contents of a string or char literal.
    Literal(Range<usize>),
    /// A whole comment.
    Comment(Range<usize>),
}

/// What the lexer does at one offset: the span to blank, if any, and where to continue.
#[derive(Debug)]
struct Step {
    span: Option<Span>,
    next: usize,
}

impl Step {
    const fn skip(next: usize) -> Self {
        Self { span: None, next }
    }

    const fn literal(range: Range<usize>, next: usize) -> Self {
        Self {
            span: Some(Span::Literal(range)),
            next,
        }
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
        span: Some(Span::Comment(at..end)),
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
        span: Some(Span::Comment(at..j)),
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
        Some(end) => Step::literal(body..end, end.saturating_add(close.len())),
        None => Step::literal(body..bytes.len(), bytes.len()),
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
    Step::literal(body, j.saturating_add(1))
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
            return Step::literal(body..j, j.saturating_add(1));
        }
    } else if let Some(c) = src.get(body..).and_then(|rest| rest.chars().next()) {
        let close = body.saturating_add(c.len_utf8());
        if c != '\'' && bytes.get(close) == Some(&b'\'') {
            return Step::literal(body..close, close.saturating_add(1));
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
    match_pair(code, open, b'{', b'}')
}

/// The offset of the `close` byte that closes the `open_byte` at `open`, or the last offset when
/// unbalanced (1.x `matchPair`).
pub(crate) fn match_pair(code: &str, open: usize, open_byte: u8, close: u8) -> usize {
    // Signed, as in 1.x: a closer before any opener takes the depth below zero.
    let mut depth = 0_isize;
    let tail = code.as_bytes().iter().enumerate().skip(open);
    for (at, &b) in tail {
        if b == open_byte {
            depth = depth.saturating_add(1);
        } else if b == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return at;
            }
        }
    }
    code.len().saturating_sub(1)
}

/// The offset of the first `{` (a body) at `(`/`[` depth <= 0 from `from`, or `None` when a `;`
/// at that depth (no body) or the end comes first (1.x `bodyBrace`).
pub(crate) fn body_brace(code: &str, from: usize) -> Option<usize> {
    // Signed, as in 1.x: a closer before any opener takes the depth below zero.
    let mut depth = 0_isize;
    for (at, &b) in code.as_bytes().iter().enumerate().skip(from) {
        match b {
            b'(' | b'[' => depth = depth.saturating_add(1),
            b')' | b']' => depth = depth.saturating_sub(1),
            b'{' if depth <= 0 => return Some(at),
            b';' if depth <= 0 => return None,
            _ => {}
        }
    }
    None
}

/// Is the byte before `at` a `-`, so a `>` at `at` is the arrow of `->`?
fn after_dash(bytes: &[u8], at: usize) -> bool {
    at.checked_sub(1).and_then(|prev| bytes.get(prev)) == Some(&b'-')
}

/// The offset of the `>` closing the generic list whose `<` is at `open` (a `>` after `-` closes
/// none), or `None` when a `{` or `;` comes first or the code ends (1.x `matchAngle`'s -1).
pub(crate) fn match_angle(code: &str, open: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    // Signed, as in 1.x: a closer before any opener takes the depth below zero.
    let mut depth = 0_isize;
    for (at, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'<' => depth = depth.saturating_add(1),
            b'>' if !after_dash(bytes, at) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(at);
                }
            }
            b'{' | b';' => return None,
            _ => {}
        }
    }
    None
}

/// `s` split on the ASCII byte `sep` at bracket depth 0, where `<([{` open and `)]}` and a `>`
/// not after `-` close (1.x `splitTop`). Always yields at least one piece: `""` gives `[""]`.
pub(crate) fn split_top(s: &str, sep: u8) -> SplitTop<'_> {
    SplitTop {
        rest: s,
        sep,
        done: false,
    }
}

/// The pieces of [`split_top`], in order.
#[derive(Debug, Clone)]
pub(crate) struct SplitTop<'s> {
    rest: &'s str,
    sep: u8,
    done: bool,
}

impl<'s> Iterator for SplitTop<'s> {
    type Item = &'s str;

    fn next(&mut self) -> Option<&'s str> {
        if self.done {
            return None;
        }
        let bytes = self.rest.as_bytes();
        // Every piece starts at depth 0, since 1.x splits only there.
        let mut depth = 0_isize;
        for (at, &b) in bytes.iter().enumerate() {
            match b {
                b'<' | b'(' | b'[' | b'{' => depth = depth.saturating_add(1),
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b'>' if !after_dash(bytes, at) => depth = depth.saturating_sub(1),
                _ if b == self.sep && depth == 0 => {
                    // `sep` is ASCII, so `at` and `at + 1` are char boundaries.
                    if let Some((piece, tail)) = self.rest.split_at_checked(at) {
                        self.rest = tail.get(1..).unwrap_or("");
                        return Some(piece);
                    }
                }
                _ => {}
            }
        }
        self.done = true;
        Some(self.rest)
    }
}

/// The offset of the `;` that ends the statement or item starting at `from`, or of the closer
/// of the enclosing block when the statement is a tail expression; the length when neither
/// follows (1.x `statementEnd`).
pub(crate) fn statement_end(code: &str, from: usize) -> usize {
    let mut depth = 0_isize;
    for (at, &b) in code.as_bytes().iter().enumerate().skip(from) {
        match b {
            b'(' | b'[' | b'{' => depth = depth.saturating_add(1),
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth < 0 {
                    return at;
                }
            }
            b';' if depth == 0 => return at,
            _ => {}
        }
    }
    code.len()
}

/// The offset after the char at `at`, or `at + 1` at the end of `text`.
pub(crate) fn next_char(text: &str, at: usize) -> usize {
    let width = text
        .get(at..)
        .and_then(|rest| rest.chars().next())
        .map_or(1, char::len_utf8);
    at.saturating_add(width)
}

/// JS `text.slice(from, to)` on byte offsets: both ends clamp to the length and floor to a char
/// boundary, and a reversed range is empty.
pub(crate) fn slice(text: &str, from: usize, to: usize) -> &str {
    let (from, to) = (text.floor_char_boundary(from), text.floor_char_boundary(to));
    text.get(from..to).unwrap_or("")
}

#[cfg(test)]
mod tests;
