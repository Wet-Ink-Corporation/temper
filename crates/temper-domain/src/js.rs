//! JavaScript semantics the port must reproduce: the `String.prototype.trim` whitespace set,
//! UTF-16 length limits, and the translation of 1.x regex sources to `regex` syntax.

mod map;
mod translate;
mod value;

pub(crate) use map::JsMap;
pub(crate) use translate::{JsFlags, UnsupportedJs, translate};
pub(crate) use value::{
    Object, Value, entries, integer_from_radix, js_json, js_string, number_from_text, truthy,
};

/// JS `\s` (the spec's `WhiteSpace` and `LineTerminator`) as the body of a `regex` class.
const JS_SPACE_SET: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";
/// JS `\w` without the `u` flag: ASCII only.
const JS_WORD_SET: &str = "0-9A-Za-z_";
/// JS `.` without the `s` flag: anything but a line terminator.
const JS_DOT: &str = r"[^\n\r\x{2028}\x{2029}]";

/// Is `c` in JS's `\s` set, the set `String.prototype.trim` strips?
pub(crate) fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// JS `text.trim()`: strips the JS whitespace set, which has U+FEFF and lacks U+0085.
///
/// # Examples
///
/// ```
/// use temper_domain::js_trim;
///
/// assert_eq!(js_trim("\u{feff} x\n"), "x");
/// assert_eq!(js_trim("\u{85}x"), "\u{85}x");
/// ```
#[must_use]
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// The longest prefix of `text` that fits in `max_units` UTF-16 code units, cut only between
/// chars, so a surrogate pair is never split (port plan D16(e)).
pub(crate) fn truncate_utf16(text: &str, max_units: usize) -> &str {
    let cut = text
        .char_indices()
        .scan(0_usize, |units, (at, c)| {
            *units = units.saturating_add(c.len_utf16());
            Some((at, *units))
        })
        .find(|&(_, units)| units > max_units)
        .map_or(text.len(), |(at, _)| at);
    // `cut` comes from `char_indices` or is the length, so it is a char boundary.
    text.get(..cut).unwrap_or(text)
}

/// The byte offset in `text` where the last `units` UTF-16 code units before `end` begin: the
/// start of JS's `text.slice(max(0, end16 - units), end16)`. A char that straddles the cut is
/// left out, and `end` is floored to a char boundary first.
pub(crate) fn utf16_window_start(text: &str, end: usize, units: usize) -> usize {
    let end = text.floor_char_boundary(end);
    let mut left = units;
    let mut start = end;
    for (at, c) in text.get(..end).unwrap_or("").char_indices().rev() {
        let Some(rest) = left.checked_sub(c.len_utf16()) else {
            break;
        };
        left = rest;
        start = at;
    }
    start
}

/// The byte length of a leading `\r`, U+2028 or U+2029 in `matched` (0 otherwise): the line
/// terminator that the `m`-flag translation of `^` consumed, so the JS match starts after it.
///
/// This assumes no `m` pattern can itself begin by matching one of them, which holds for the
/// three in the table (they begin with `[ \t]*`, `pub` or `type`, and `fn`).
pub(crate) fn terminator_len(matched: &str) -> usize {
    match matched.chars().next() {
        Some(c @ ('\r' | '\u{2028}' | '\u{2029}')) => c.len_utf8(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests;
