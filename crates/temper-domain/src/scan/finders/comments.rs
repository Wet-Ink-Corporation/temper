//! Comments that are not documentation: untracked `TODO`s (1.x `todoComments`) and
//! commented-out code (1.x `commentedCode`). Doc comments are skipped by both.

use regex::Regex;

use super::Found;
use crate::js::js_trim;
use crate::scan::env::Env;
use crate::scan::lexer::{CommentKind, slice};
use crate::scan::scanner::{ScannerSetupError, compile};

/// The compiled patterns of both finders.
#[derive(Debug)]
pub(crate) struct CommentPatterns {
    todo: Regex,
    tracked: Regex,
    code_start: Regex,
    code_char: Regex,
    code_end: Regex,
    code_hint: Regex,
    prose_end: Regex,
}

impl CommentPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            todo: compile("comments:todo", r"\b(TODO|FIXME|XXX|HACK)\b")?,
            tracked: compile(
                "comments:tracked",
                r"(#\d+|https?:\/\/|\b[A-Z][A-Z0-9]+-\d+\b)",
            )?,
            code_start: compile(
                "comments:code-start",
                r"^(let|fn|use|pub|if|match|return|for|while|impl|struct|enum|mod|const|static)\b",
            )?,
            code_char: compile("comments:code-char", r"[;{}()=<>]")?,
            code_end: compile("comments:code-end", r"[;{}]\s*$")?,
            code_hint: compile("comments:code-hint", r"[()=:<>]|\b(let|fn|return)\b")?,
            prose_end: compile("comments:prose-end", r"[.!?…]\s*$")?,
        })
    }
}

/// The plain comments' bodies, with their offsets.
fn plain_bodies<'e>(env: &Env<'e>) -> impl Iterator<Item = (usize, &'e str)> {
    env.comments
        .iter()
        .filter(|c| c.kind == CommentKind::Plain)
        .map(|c| (c.start, comment_body(c.text)))
}

/// Pushes every plain comment with a `TODO`, `FIXME`, `XXX` or `HACK` and no issue reference,
/// URL or ticket key.
pub(crate) fn find_todos(env: &Env<'_>, p: &CommentPatterns, out: &mut Vec<Found>) {
    let found = plain_bodies(env)
        .filter(|&(_, body)| p.todo.is_match(body) && !p.tracked.is_match(body))
        .map(|(start, _)| Found::at(start));
    out.extend(found);
}

/// Pushes every plain comment that starts or ends like code and does not end like prose.
pub(crate) fn find_commented_code(env: &Env<'_>, p: &CommentPatterns, out: &mut Vec<Found>) {
    let found = plain_bodies(env)
        .filter(|&(_, body)| {
            let starts_like_code = p.code_start.is_match(body) && p.code_char.is_match(body);
            let ends_like_code = p.code_end.is_match(body) && p.code_hint.is_match(body);
            (starts_like_code || ends_like_code) && !p.prose_end.is_match(body)
        })
        .map(|(start, _)| Found::at(start));
    out.extend(found);
}

/// 1.x `text.replace(/^\/\/+|^\/\*+|\*+\/$/g, '').trim()`: the comment without its leading `//`
/// or `/*` run and its trailing `*/` run, JS-trimmed. The trailing run starts no earlier than
/// the leading one ends, so `/**/` gives `/`.
fn comment_body(text: &str) -> &str {
    let run_after_slash = |mark: u8| {
        let run = text.bytes().skip(1).take_while(|&b| b == mark).count();
        run.saturating_add(1)
    };
    let lead_end = if text.starts_with("//") {
        run_after_slash(b'/')
    } else if text.starts_with("/*") {
        run_after_slash(b'*')
    } else {
        0
    };
    let slash = text.len().saturating_sub(1);
    let trail_start = match text.strip_suffix('/') {
        Some(head) if head.ends_with('*') => {
            let run = head.bytes().rev().take_while(|&b| b == b'*').count();
            let from = slash.saturating_sub(run).max(lead_end);
            if from < slash { from } else { text.len() }
        }
        _ => text.len(),
    };
    js_trim(slice(text, lead_end, trail_start))
}

#[cfg(test)]
mod tests {
    use super::comment_body;

    #[test]
    fn comment_body_strips_like_1x() {
        let cases = [
            ("// a ", "a"),
            ("//// a", "a"),
            ("/* a */", "a"),
            ("/*** a ***/", "a"),
            ("/**/", "/"),
            ("/***/", "/"),
            ("/*/", "/"),
            ("/* a", "a"),
            ("// a */", "a"),
            ("//", ""),
        ];

        let bodies = cases.map(|(text, _)| comment_body(text));

        assert_eq!(
            bodies,
            cases.map(|(_, body)| body),
            "the 1.x replace and trim"
        );
    }
}
