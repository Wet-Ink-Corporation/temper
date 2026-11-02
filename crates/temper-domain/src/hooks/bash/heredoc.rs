//! 1.x `HEREDOC`,
//! `(?<!<)<<-?[ \t]*(['"]?)([\w.-]+)\1([^\n]*)\n[\s\S]*?\n\t*\2(?=\n|$)`, matched by hand: its
//! look-behind and back-references have no `regex` form (2-16).
//!
//! JS tries the `-` taken, then not, and for an unquoted tag every length from the longest
//! down, each with its first closing line. Every length shares the opening line, so the
//! closing lines are indexed once per pass by their text (a `\n`, tabs, then a run of
//! `[\w.-]`), and a candidate looks up the prefixes of its tag run instead of rescanning the
//! rest of the command per length (review of #200, F3).

use std::collections::HashMap;

use super::next::Next;

/// A here-document `HEREDOC` matches.
#[derive(Debug)]
pub(super) struct Heredoc<'s> {
    /// Where the match ends: after the closing tag.
    pub(super) end: usize,
    /// Group 1: the quote around the tag, or empty.
    pub(super) quote: &'s str,
    /// Group 2: the tag.
    pub(super) tag: &'s str,
    /// Group 3: the rest of the opening line.
    pub(super) rest: &'s str,
}

/// The here-document search over one text, kept between its candidates, which come in
/// increasing order.
#[derive(Debug)]
pub(super) struct HeredocScan<'s> {
    s: &'s str,
    /// Built at the first `<<` that could open a here-document.
    closings: Option<ClosingLines<'s>>,
    /// The first newline at or after a position.
    newline: Next,
}

impl<'s> HeredocScan<'s> {
    pub(super) const fn new(s: &'s str) -> Self {
        Self {
            s,
            closings: None,
            newline: Next::Unknown,
        }
    }

    /// `HEREDOC` at byte `at`, which holds `<<`.
    pub(super) fn at(&mut self, at: usize) -> Option<Heredoc<'s>> {
        let s = self.s;
        let bytes = s.as_bytes();
        if !s.get(at..)?.starts_with("<<")
            || at.checked_sub(1).and_then(|b| bytes.get(b)) == Some(&b'<')
        {
            return None;
        }
        let after = at.saturating_add(2);
        let dashes: &[usize] = if bytes.get(after) == Some(&b'-') {
            &[1, 0]
        } else {
            &[0]
        };
        dashes.iter().find_map(|&dash| {
            let mut p = after.saturating_add(dash);
            while matches!(bytes.get(p), Some(b' ' | b'\t')) {
                p = p.saturating_add(1);
            }
            let quote = match bytes.get(p) {
                Some(b'\'' | b'"') => s.get(p..p.saturating_add(1))?,
                _ => "",
            };
            let tag_start = p.saturating_add(quote.len());
            let run = s.get(tag_start..)?;
            let run = run.get(..run.find(|c: char| !is_tag_char(c)).unwrap_or(run.len()))?;
            if run.is_empty() {
                return None;
            }
            // A quoted tag is the whole run, since its quote must follow it.
            let shortest = if quote.is_empty() {
                1
            } else {
                s.get(tag_start.saturating_add(run.len())..)?
                    .strip_prefix(quote)?;
                run.len()
            };
            let search = tag_start.saturating_add(shortest);
            let line_end = self.newline.at_or_after(search, |from| {
                s.get(from..)?.find('\n').map(|i| from.saturating_add(i))
            })?;
            // The body may be empty, but a `\n` comes before the closing line.
            let first_closing = line_end.saturating_add(2);
            let (tag, end) = self
                .closings
                .get_or_insert_with(|| ClosingLines::new(s))
                .closing(run, shortest, first_closing)?;
            let rest_start = tag_start
                .saturating_add(tag.len())
                .saturating_add(quote.len());
            Some(Heredoc {
                end,
                quote,
                tag,
                rest: s.get(rest_start..line_end)?,
            })
        })
    }
}

/// The lines a closing tag can be: after a `\n`, tabs, then a run of `[\w.-]` up to a
/// newline or the end.
#[derive(Debug)]
struct ClosingLines<'s> {
    /// Each run, with the start and end of its lines in order.
    lines: HashMap<&'s str, Vec<(usize, usize)>>,
    /// The runs' distinct lengths, longest first.
    lens: Vec<usize>,
}

impl<'s> ClosingLines<'s> {
    fn new(s: &'s str) -> Self {
        let mut lines: HashMap<&'s str, Vec<(usize, usize)>> = HashMap::new();
        for (nl, _) in s.match_indices('\n') {
            let start = nl.saturating_add(1);
            let rest = s.get(start..).unwrap_or("");
            let line = rest
                .get(..rest.find('\n').unwrap_or(rest.len()))
                .unwrap_or("");
            let run = line.trim_start_matches('\t');
            if !run.is_empty() && run.chars().all(is_tag_char) {
                lines
                    .entry(run)
                    .or_default()
                    .push((start, start.saturating_add(line.len())));
            }
        }
        let mut lens: Vec<usize> = lines.keys().map(|run| run.len()).collect();
        lens.sort_unstable_by(|a, b| b.cmp(a));
        lens.dedup();
        Self { lines, lens }
    }

    /// The longest prefix of `run`, at least `shortest` bytes, with a closing line starting
    /// at or after `from`, and the end of its first such line.
    fn closing<'r>(&self, run: &'r str, shortest: usize, from: usize) -> Option<(&'r str, usize)> {
        self.lens
            .iter()
            .filter(|&&len| shortest <= len && len <= run.len())
            .find_map(|&len| {
                let tag = run.get(..len)?;
                let lines = self.lines.get(tag)?;
                let first = lines.partition_point(|&(start, _)| start < from);
                lines.get(first).map(|&(_, end)| (tag, end))
            })
    }
}

/// JS `[\w.-]`.
pub(super) fn is_tag_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}
