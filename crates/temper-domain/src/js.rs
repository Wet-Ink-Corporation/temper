//! JavaScript semantics the port must reproduce: the `String.prototype.trim` whitespace set,
//! UTF-16 length limits, and the translation of 1.x regex sources to `regex` syntax.

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
pub(crate) fn js_trim(text: &str) -> &str {
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

/// Translates a 1.x JS regex source (no flags) to `regex` syntax with the same meaning.
///
/// `\s`, `\w`, `\d` and their negations become explicit classes, `\b` becomes the ASCII word
/// boundary, `.` excludes JS line terminators, and inside a class `\b` is a backspace and
/// `[`, `&` and `~` are literals. Anything else passes through unchanged; `regex` rejects what
/// it cannot read, so a bad translation fails `Scanner::new`, never a scan.
pub(crate) fn translate(js_source: &str) -> String {
    let mut out = String::with_capacity(js_source.len().saturating_mul(2));
    let mut in_class = false;
    let mut chars = js_source.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(escaped) => push_escape(&mut out, escaped, in_class),
                None => out.push('\\'),
            },
            '[' | '&' | '~' if in_class => {
                out.push('\\');
                out.push(c);
            }
            '[' => {
                in_class = true;
                out.push(c);
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            '.' if !in_class => out.push_str(JS_DOT),
            _ => out.push(c),
        }
    }
    out
}

/// Appends the translation of the JS escape `\escaped`.
fn push_escape(out: &mut String, escaped: char, in_class: bool) {
    let class = |out: &mut String, negated: bool, set: &str| {
        out.push_str(if negated { "[^" } else { "[" });
        out.push_str(set);
        out.push(']');
    };
    match (escaped, in_class) {
        ('s', true) => out.push_str(JS_SPACE_SET),
        ('w', true) => out.push_str(JS_WORD_SET),
        ('d', true) => out.push_str("0-9"),
        ('s' | 'S', _) => class(out, escaped == 'S', JS_SPACE_SET),
        ('w' | 'W', _) => class(out, escaped == 'W', JS_WORD_SET),
        ('d' | 'D', _) => class(out, escaped == 'D', "0-9"),
        ('b', true) => out.push_str(r"\x08"),
        ('b', false) => out.push_str(r"(?-u:\b)"),
        ('B', false) => out.push_str(r"(?-u:\B)"),
        // `regex` reads `\<` and `\>` as word-start and word-end assertions; JS as literals.
        ('<' | '>', _) => out.push(escaped),
        _ => {
            out.push('\\');
            out.push(escaped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    fn compiled(js_source: &str) -> Regex {
        Regex::new(&translate(js_source)).expect("the translated test pattern compiles")
    }

    #[test]
    fn js_space_matches_nbsp_and_bom_but_not_nel() {
        let space = compiled(r"\s");

        let verdicts: Vec<(bool, bool)> = ['\u{A0}', '\u{FEFF}', '\u{85}', '\u{180E}']
            .into_iter()
            .map(|c| (is_js_space(c), space.is_match(&c.to_string())))
            .collect();

        assert_eq!(
            verdicts,
            [(true, true), (true, true), (false, false), (false, false)],
            "is_js_space and the translated \\s agree with JS on NBSP, BOM, NEL and U+180E"
        );
    }

    #[test]
    fn js_trim_strips_bom_and_keeps_nel() {
        assert_eq!(
            js_trim("\u{85}x\u{FEFF}"),
            "\u{85}x",
            "JS trim() removes U+FEFF but keeps U+0085"
        );
    }

    #[test]
    fn word_boundary_next_to_non_ascii_letter_is_a_boundary() {
        let found = compiled(r"\bfoo").find("éfoo").map(|m| m.start());

        assert_eq!(
            found,
            Some(2),
            "é is not a JS word char, so \\b holds before foo"
        );
    }

    #[test]
    fn dot_excludes_js_line_terminators() {
        let dot = compiled("a.b");

        assert_eq!(
            (dot.is_match("a\u{2028}b"), dot.is_match("a\u{85}b")),
            (false, true),
            "JS `.` excludes U+2028 and matches U+0085"
        );
    }

    #[test]
    fn class_escapes_expand_inside_brackets() {
        assert_eq!(
            translate(r"[\s,]"),
            format!("[{JS_SPACE_SET},]"),
            "\\s inside a class expands to the JS space set"
        );
        assert_eq!(
            translate(r"[^\w-]"),
            "[^0-9A-Za-z_-]",
            "\\w inside a class expands to the ASCII word set"
        );
        assert!(
            compiled(r"[\b]").is_match("\u{8}"),
            "[\\b] is a backspace in JS"
        );
    }

    #[test]
    fn literal_bracket_inside_a_class_is_escaped() {
        assert!(
            compiled(r"[.[]").is_match("["),
            "JS reads `[` inside a class as a literal"
        );
    }

    #[test]
    fn truncate_utf16_never_splits_a_surrogate_pair() {
        let cut = format!("{}😀", "x".repeat(159));
        let whole = format!("{}😀", "x".repeat(158));

        assert_eq!(
            truncate_utf16(&cut, 160),
            "x".repeat(159),
            "the emoji needs two units and only one is left"
        );
        assert_eq!(
            truncate_utf16(&whole, 160),
            whole,
            "158 units plus a pair is exactly 160"
        );
    }
}
