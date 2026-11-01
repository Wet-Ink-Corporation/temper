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
fn match_pair_and_statement_end_follow_1x() {
    assert_eq!(
        [
            match_pair("f(a(b)c)d", 1, b'(', b')'),
            match_pair("f(a(b", 1, b'(', b')'),
            match_pair("x)", 0, b'(', b')'),
        ],
        [7, 4, 1],
        "the closer, or the last offset when unbalanced"
    );
    assert_eq!(
        [
            statement_end("let a = f(x; y); b", 0),
            statement_end("{ g(h) } k", 2),
            statement_end("m(n) }", 0),
            statement_end("tail", 0),
            statement_end("a { b; } c;", 0),
        ],
        [15, 7, 5, 4, 10],
        "the `;` at depth 0, the closer of the enclosing block, or the length"
    );
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
