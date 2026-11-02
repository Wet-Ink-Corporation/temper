use proptest::prelude::*;

use super::*;

fn matches(pattern: &str, rel: &str) -> bool {
    Glob::new(pattern)
        .expect("the test glob compiles")
        .is_match(rel)
}

#[test]
fn globs_match_as_1x_glob_match() {
    // Each answer was printed by 1.x's `globMatch` (Node 22).
    let cases = [
        ("**/clippy.toml", "clippy.toml", true),
        ("**/clippy.toml", "a/b/clippy.toml", true),
        ("**/clippy.toml", "aclippy.toml", false),
        (".cargo/**", ".cargo/config.toml", true),
        (".cargo/**", ".cargo", false),
        (".cargo/**", ".cargox/a", false),
        ("crates/*", "crates/a", true),
        ("crates/*", "crates/a/b", false),
        ("a?c", "abc", true),
        ("a?c", "a/c", false),
        ("a.c", "abc", false),
        ("a+(b)|[c]{d}$^", "a+(b)|[c]{d}$^", true),
        ("**x", "a/bx", true),
        ("*.rs", "src/x.rs", false),
    ];
    for (pattern, rel, want) in cases {
        assert_eq!(matches(pattern, rel), want, "{pattern:?} against {rel:?}");
    }
}

#[test]
fn backslashes_read_as_slashes_on_either_side() {
    assert!(matches(r"a\b", "a/b"), "in the pattern");
    assert!(matches("a/b", r"a\b"), "in the path");
}

#[test]
fn a_star_crosses_newlines_but_a_double_star_does_not() {
    // `*` is `[^/]*`, which takes `\n`; `**` is JS `.*`, which stops at a line terminator.
    assert!(matches("a*", "a\nb"), "`*`");
    assert!(!matches("**", "a\nb"), "`**` at \\n");
    assert!(!matches("**", "a\u{2028}b"), "`**` at U+2028");
}

#[test]
fn a_question_mark_takes_one_char() {
    // D16(o): 1.x's `?` takes one UTF-16 unit, so an astral char needed `??`.
    assert!(matches("a?", "a\u{1F600}"), "one astral char");
    assert!(!matches("a??", "a\u{1F600}"), "not two");
}

#[test]
fn globs_match_when_any_pattern_does() {
    let globs = Globs::new(["crates/*", "**/deny.toml"]).expect("the test globs compile");
    assert!(globs.any_match("x/deny.toml"), "the second");
    assert!(globs.any_match("crates/a"), "the first");
    assert!(!globs.any_match("src/lib.rs"), "neither");
    assert!(
        !Globs::new([]).expect("no globs").any_match(""),
        "an empty list matches nothing"
    );
}

#[test]
fn the_error_names_the_pattern() {
    let err = GlobError {
        pattern: "x".to_owned(),
        source: GlobCause::Regex(regex::Error::Syntax("bad".to_owned())),
    };
    assert_eq!(err.to_string(), r#"glob "x" does not compile"#);
    assert_eq!(err.pattern(), "x");
}

proptest! {
    #[test]
    fn a_pattern_without_wildcards_matches_only_itself(text in "[^*?]{0,24}", extra in "[^*?]{1,4}") {
        let glob = Glob::new(&text).expect("a literal glob compiles");
        prop_assert!(glob.is_match(&text), "{text:?} matches itself");
        let longer = format!("{text}{extra}");
        prop_assert!(!glob.is_match(&longer), "{text:?} does not match {longer:?}");
    }
}
