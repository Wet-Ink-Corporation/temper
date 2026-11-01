use super::*;
use regex::Regex;

fn compiled_with(js_source: &str, flags: JsFlags) -> Regex {
    let translated = translate(js_source, flags).expect("the test pattern translates");
    Regex::new(&translated).expect("the translated test pattern compiles")
}

fn compiled(js_source: &str) -> Regex {
    compiled_with(js_source, JsFlags::Plain)
}

fn plain(js_source: &str) -> Result<String, UnsupportedJs> {
    translate(js_source, JsFlags::Plain)
}

const fn unsupported(construct: Construct, at: usize) -> Result<String, UnsupportedJs> {
    Err(UnsupportedJs { construct, at })
}

#[test]
fn empty_class_is_plain() {
    assert_eq!(
        plain("a[]b"),
        unsupported(Construct::EmptyClass, 1),
        "JS `[]` never matches; regex would read `]` as a literal"
    );
}

#[test]
fn negated_empty_class_is_plain() {
    assert_eq!(
        plain("[^]"),
        unsupported(Construct::AnyCharClass, 0),
        "JS `[^]` matches any char, newlines included"
    );
}

#[test]
fn regex_only_anchors_are_plain() {
    assert_eq!(
        (plain(r"\A"), plain(r"x\z")),
        (
            unsupported(Construct::Escape('A'), 0),
            unsupported(Construct::Escape('z'), 1)
        ),
        "JS reads \\A and \\z as the literals A and z"
    );
}

#[test]
fn unicode_property_escapes_are_plain() {
    assert_eq!(
        (plain(r"\p{L}"), plain(r"\P{L}")),
        (
            unsupported(Construct::Escape('p'), 0),
            unsupported(Construct::Escape('P'), 0)
        ),
        "JS without the u flag reads p{{L}} literally"
    );
}

#[test]
fn braced_hex_escapes_are_plain() {
    assert_eq!(
        (plain(r"\u{41}"), plain(r"\x{41}")),
        (
            unsupported(Construct::BareHexEscape('u'), 0),
            unsupported(Construct::BareHexEscape('x'), 0)
        ),
        "JS reads \\u{{2}} as `u` twice, not as a code point"
    );
    assert!(
        compiled(r"A").is_match("A") && compiled(r"\x41").is_match("A"),
        "four and two hex digits translate unchanged"
    );
}

#[test]
fn double_dash_in_class_is_plain() {
    let constructs = ["[+--]", "[a-c--]"].map(|s| plain(s).map_err(|e| e.construct));

    assert_eq!(
        constructs,
        [
            Err(Construct::ClassDoubleDash),
            Err(Construct::ClassDoubleDash)
        ],
        "regex reads `--` in a class as a class difference"
    );
}

#[test]
fn dash_next_to_class_escape_is_literal() {
    let after = compiled(r"^[\w-.]$");
    let before = compiled(r"^[+-\d]$");

    assert_eq!(
        ["-", ".", "a", "/", "+"].map(|s| after.is_match(s)),
        [true, true, true, false, false],
        "JS reads [\\w-.] as \\w, `-` and `.`"
    );
    assert_eq!(
        ["+", "-", "5", ","].map(|s| before.is_match(s)),
        [true, true, true, false],
        "JS reads [+-\\d] as `+`, `-` and digits"
    );
}

#[test]
fn non_ascii_identity_escape_is_the_char() {
    assert!(
        compiled("\\é").is_match("é"),
        "JS reads \\é as é; regex rejects the escape"
    );
}

#[test]
fn ignore_case_folds_ascii_only() {
    let seqcst = compiled_with(r"\bSeqCst\b", JsFlags::IgnoreCase);
    let k = compiled_with("k", JsFlags::IgnoreCase);
    let class = compiled_with("^[ab_]$", JsFlags::IgnoreCase);

    assert_eq!(
        ["seqcst", "SEQCST", "\u{17F}eqcst"].map(|s| seqcst.is_match(s)),
        [true, true, false],
        "JS non-u /i folds ASCII letters, never ſ to s"
    );
    assert!(
        k.is_match("K") && !k.is_match("\u{212A}"),
        "nor the Kelvin sign to k"
    );
    assert_eq!(
        ["A", "b", "_", "c"].map(|s| class.is_match(s)),
        [true, true, true, false],
        "letters in a class fold too"
    );
}

#[test]
fn ignore_case_rejects_what_it_cannot_fold() {
    assert_eq!(
        [
            translate("é", JsFlags::IgnoreCase),
            translate("x[a-c]", JsFlags::IgnoreCase),
            translate(r"y\x41", JsFlags::IgnoreCase),
        ],
        [
            unsupported(Construct::Folded, 0),
            unsupported(Construct::Folded, 3),
            unsupported(Construct::Folded, 1),
        ],
        "JS /i folds é to É, [a-c] to B and \\x41 to a; no 1.x pattern needs any of them"
    );
}

#[test]
fn utf16_window_start_counts_units_not_bytes() {
    assert_eq!(
        utf16_window_start("aé😀b", 8, 3),
        3,
        "😀 is two units and b one, so the window starts at 😀"
    );
    assert_eq!(
        utf16_window_start("😀b", 5, 2),
        4,
        "a char that straddles the cut is left out"
    );
    assert_eq!(
        utf16_window_start("aé", 99, 1),
        1,
        "an end past the length is floored to it"
    );
}

#[test]
fn multiline_caret_matches_after_every_js_line_terminator() {
    let caret = compiled_with("^a", JsFlags::Multiline);

    let found: Vec<bool> = ["x\na", "x\ra", "x\u{2028}a", "x\u{2029}a", "xa"]
        .into_iter()
        .map(|s| caret.is_match(s))
        .collect();

    assert_eq!(
        found,
        [true, true, true, true, false],
        "JS `^` under m follows \\n, \\r, U+2028 and U+2029"
    );
    assert_eq!(
        ["a", "\ra", "\u{2028}a", "\u{2029}a"].map(terminator_len),
        [0, 1, 3, 3],
        "the byte length of the terminator the translated `^` consumed"
    );
    assert_eq!(
        translate("a$", JsFlags::Multiline),
        unsupported(Construct::MultilineDollar, 1),
        "no 1.x pattern needs `$` under m"
    );
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
fn is_js_space_equals_translated_class_for_every_char() {
    let space = compiled(r"^\s$");
    let mut buf = [0_u8; 4];

    let disagree: Vec<char> = (char::MIN..=char::MAX)
        .filter(|&c| is_js_space(c) != space.is_match(c.encode_utf8(&mut buf)))
        .collect();
    let set: Vec<u32> = (char::MIN..=char::MAX)
        .filter(|&c| is_js_space(c))
        .map(u32::from)
        .collect();

    assert_eq!(
        disagree,
        [],
        "is_js_space and the compiled \\s agree on every char"
    );
    assert_eq!(
        set,
        [
            0x9, 0xA, 0xB, 0xC, 0xD, 0x20, 0xA0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004,
            0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200A, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000,
            0xFEFF
        ],
        "the 25 chars Node's /\\s/ matches over every code point"
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
        plain(r"[\s,]"),
        Ok(format!("[{JS_SPACE_SET},]")),
        "\\s inside a class expands to the JS space set"
    );
    assert_eq!(
        plain(r"[^\w-]").as_deref(),
        Ok("[^0-9A-Za-z_-]"),
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
