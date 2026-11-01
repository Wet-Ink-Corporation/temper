//! The doc comments of a source as runs of lines, split into fenced code blocks and prose lines
//! (1.x `docBlocks`).

use regex::Regex;

use super::lexer::{Comment, CommentKind, slice};
use super::scanner::{ScannerSetupError, compile};
use crate::js::js_trim;

/// One line of a doc comment, borrowed from the source: its text after the comment marker and
/// the byte offset of that text in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocLine<'s> {
    pub(crate) text: &'s str,
    pub(crate) offset: usize,
}

/// A fenced code block: its info string (1.x's capture, without the spaces around it) and its
/// lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fence<'s> {
    pub(crate) info: &'s str,
    pub(crate) lines: Vec<DocLine<'s>>,
}

/// The doc comments of a source, split into fences and prose lines (1.x `docBlocks`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DocBlocks<'s> {
    pub(crate) fences: Vec<Fence<'s>>,
    pub(crate) prose: Vec<DocLine<'s>>,
}

/// The fence-line pattern, compiled once.
#[derive(Debug)]
pub(crate) struct DocBlockPatterns {
    /// A fence line: the marker (group 1) and the info string (group 2).
    fence: Regex,
}

impl DocBlockPatterns {
    /// Compiles 1.x's fence pattern.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            fence: compile("doc:fence", r"^ {0,3}(`{3,}|~{3,})\s*([^`]*?)\s*$")?,
        })
    }
}

/// The doc comments among `comments`, grouped into runs and split into fences and prose. Takes
/// the masked `code` and the comments rather than the scan's `Env`, so `Env::doc` can call it.
pub(crate) fn doc_blocks<'s>(
    code: &str,
    comments: &[Comment<'s>],
    patterns: &DocBlockPatterns,
) -> DocBlocks<'s> {
    let mut blocks = DocBlocks::default();
    for run in runs(code, comments) {
        split_fences(&run, &patterns.fence, &mut blocks);
    }
    blocks
}

/// The lines of each run of doc comments. A comment joins the run before it when both are inner
/// (`//!`, `/*!`) or both outer, and only blank masked code lies between them.
fn runs<'s>(code: &str, comments: &[Comment<'s>]) -> Vec<Vec<DocLine<'s>>> {
    let mut runs: Vec<Vec<DocLine<'s>>> = Vec::new();
    // The end and the inner flag of the previous doc comment.
    let mut prev: Option<(usize, bool)> = None;
    for comment in comments.iter().filter(|c| c.kind == CommentKind::Doc) {
        let inner = comment.text.starts_with("//!") || comment.text.starts_with("/*!");
        let joins = prev.is_some_and(|(end, was_inner)| {
            was_inner == inner && js_trim(slice(code, end, comment.start)).is_empty()
        });
        prev = Some((comment.end, inner));
        match runs.last_mut() {
            Some(run) if joins => push_lines(comment, run),
            _ => {
                let mut run = Vec::new();
                push_lines(comment, &mut run);
                runs.push(run);
            }
        }
    }
    runs
}

/// Pushes the lines of one doc comment, without their markers and trailing `\r`.
fn push_lines<'s>(comment: &Comment<'s>, run: &mut Vec<DocLine<'s>>) {
    let text = comment.text;
    if text.starts_with("//") {
        // A doc line comment starts `///` or `//!`; one space after it is part of the marker.
        let marker = if text.as_bytes().get(3) == Some(&b' ') {
            4
        } else {
            3
        };
        run.push(DocLine {
            text: without_cr(text.get(marker..).unwrap_or("")),
            offset: comment.start.saturating_add(marker),
        });
        return;
    }
    let body_end = if text.ends_with("*/") {
        text.len().saturating_sub(2)
    } else {
        text.len()
    };
    let mut pos = 3_usize;
    for line in slice(text, 3, body_end).split('\n') {
        let lead = block_lead(line);
        run.push(DocLine {
            text: without_cr(line.get(lead..).unwrap_or("")),
            offset: comment.start.saturating_add(pos).saturating_add(lead),
        });
        pos = pos.saturating_add(line.len()).saturating_add(1);
    }
}

/// The length of a block doc line's lead: spaces and tabs, then an optional `*` and, after it,
/// an optional space (1.x `/^[ \t]*(?:\* ?)?/`).
fn block_lead(line: &str) -> usize {
    let bytes = line.as_bytes();
    let indent = bytes
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    if bytes.get(indent) != Some(&b'*') {
        return indent;
    }
    let star = indent.saturating_add(1);
    if bytes.get(star) == Some(&b' ') {
        star.saturating_add(1)
    } else {
        star
    }
}

/// `text` without one trailing `\r`.
fn without_cr(text: &str) -> &str {
    text.strip_suffix('\r').unwrap_or(text)
}

/// A fence still open in a run: its marker (three or more backticks or tildes) and the fence.
#[derive(Debug)]
struct OpenFence<'s> {
    marker: &'s str,
    fence: Fence<'s>,
}

impl OpenFence<'_> {
    /// Does a fence line with `marker` and `info` close this fence: the same marker char, at
    /// least as long, and no info?
    fn closed_by(&self, marker: &str, info: &str) -> bool {
        marker.as_bytes().first() == self.marker.as_bytes().first()
            && marker.len() >= self.marker.len()
            && info.is_empty()
    }
}

/// Splits one run into fences and prose. A fence still open at the end of the run is kept, as
/// rustdoc closes it at the end of the docs.
fn split_fences<'s>(run: &[DocLine<'s>], fence_re: &Regex, out: &mut DocBlocks<'s>) {
    let mut open: Option<OpenFence<'s>> = None;
    for &line in run {
        let fence_line = fence_re.captures(line.text).and_then(|caps| {
            let marker = caps.get(1)?.as_str();
            Some((marker, caps.get(2).map_or("", |info| info.as_str())))
        });
        match (open.take(), fence_line) {
            (None, Some((marker, info))) => {
                open = Some(OpenFence {
                    marker,
                    fence: Fence {
                        info,
                        lines: Vec::new(),
                    },
                });
            }
            (None, None) => out.prose.push(line),
            (Some(current), Some((marker, info))) if current.closed_by(marker, info) => {
                out.fences.push(current.fence);
            }
            (Some(mut current), _) => {
                current.fence.lines.push(line);
                open = Some(current);
            }
        }
    }
    if let Some(current) = open {
        out.fences.push(current.fence);
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::scan::lexer::mask_rust;
    use crate::scan::strategies::token_soup;

    proptest! {
        #[test]
        fn doc_lines_are_source_slices(src in token_soup()) {
            let blocks = blocks_of(&src);
            let all = blocks
                .fences
                .iter()
                .flat_map(|f| f.lines.iter())
                .chain(blocks.prose.iter());
            for line in all {
                prop_assert!(
                    src.get(line.offset..).is_some_and(|rest| rest.starts_with(line.text)),
                    "{line:?} is not at its offset in {src:?}"
                );
            }
        }
    }

    /// `(offset, text)` for each line.
    fn lines<'s>(lines: &[DocLine<'s>]) -> Vec<(usize, &'s str)> {
        lines.iter().map(|l| (l.offset, l.text)).collect()
    }

    fn blocks_of(src: &str) -> DocBlocks<'_> {
        let masked = mask_rust(src);
        let patterns = DocBlockPatterns::new().expect("the fence pattern compiles");
        // The lines borrow `src` through the comments, so they outlive the masked code.
        doc_blocks(&masked.code, &masked.comments, &patterns)
    }

    #[test]
    fn doc_blocks_like_1x() {
        let src = "//! inner\n//! ```\n//! a\n/// outer one\n///  two\r\n/// ~~~~ rust , no_run \n/// x\n/// ~~~\n/// still\n/// ~~~~\n\n/// gap\nfn f() {}\n/**\n * ```\n * b\n   c\n */\n/** ```text\nnot closed */\n";

        let blocks = blocks_of(src);
        let fences: Vec<(&str, Vec<(usize, &str)>)> = blocks
            .fences
            .iter()
            .map(|f| (f.info, lines(&f.lines)))
            .collect();

        assert_eq!(
            fences,
            [
                ("", vec![(22, "a")]),
                ("rust , no_run", vec![(76, "x"), (82, "~~~"), (90, "still")]),
                (
                    "",
                    vec![
                        (138, "b"),
                        (143, "c"),
                        (146, ""),
                        (153, "```text"),
                        (161, "not closed ")
                    ]
                ),
            ],
            "fences: info and lines"
        );
        assert_eq!(
            lines(&blocks.prose),
            [
                (4, "inner"),
                (28, "outer one"),
                (42, " two"),
                (110, "gap"),
                (127, "")
            ],
            "prose lines"
        );
    }
}
