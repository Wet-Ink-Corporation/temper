//! JS `text.replace(re, f)` with the `g` flag, for patterns whose matches are found by hand: a
//! rewritten look-ahead's consumed trailer or a back-reference checked after the regex.

/// One match a [`replace_all`] search found: the bytes `start..end` become `with`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Replacement {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) with: String,
}

/// `text` with each match replaced, as JS's global `replace` walks it: `next(from)` finds the
/// leftmost match at or after byte `from`, and the next search starts where it ended, one char
/// later after an empty match.
pub(crate) fn replace_all(
    text: &str,
    mut next: impl FnMut(usize) -> Option<Replacement>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut from = 0;
    while from <= text.len() {
        let Some(found) = next(from) else { break };
        // A match never starts before the search or ends before it starts; clamp in case.
        let start = found.start.clamp(copied, text.len());
        let end = found.end.clamp(start, text.len());
        out.push_str(text.get(copied..start).unwrap_or(""));
        out.push_str(&found.with);
        copied = end;
        from = if end > start {
            end
        } else {
            match text.get(end..).and_then(|rest| rest.chars().next()) {
                Some(c) => end.saturating_add(c.len_utf8()),
                None => break,
            }
        };
    }
    out.push_str(text.get(copied..).unwrap_or(""));
    out
}

/// The byte offset of the char after the one at `at` in `text`, or `text.len()` at the end.
pub(crate) fn next_boundary(text: &str, at: usize) -> usize {
    text.get(at..)
        .and_then(|rest| rest.chars().next())
        .map_or(text.len(), |c| at.saturating_add(c.len_utf8()))
}
