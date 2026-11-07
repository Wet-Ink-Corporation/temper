use proptest::prelude::*;

use super::Fingerprint;

#[test]
fn sixteen_lowercase_hex_digits_parse_and_print_back() {
    let fp: Fingerprint = "0123456789abcdef".parse().expect("16 lowercase hex digits");
    assert_eq!(fp.to_string(), "0123456789abcdef");
}

#[test]
fn anything_but_exactly_sixteen_lowercase_hex_digits_is_refused_with_the_value() {
    // 1.x: /^[0-9a-f]{16}$/, where JS `$` (no `m` flag) does not match before a trailing newline.
    for bad in [
        "0123456789ABCDEF",
        "0123456789abcde",
        "0123456789abcdef0",
        "0123456789abcdef\n",
        "0123456789abcdeé",
        "0123456789abcdeg",
        " 123456789abcdef",
        "",
    ] {
        let err = bad.parse::<Fingerprint>().expect_err(bad);
        assert_eq!(err.value(), bad, "the error carries the value");
        assert_eq!(err.to_string(), format!("{bad:?} is not a 16-hex fingerprint"));
    }
}

proptest! {
    #[test]
    fn display_then_parse_is_the_identity(text in "[0-9a-f]{16}") {
        let fp: Fingerprint = text.parse().expect("generated from the accepted pattern");
        prop_assert_eq!(fp.to_string(), text);
        prop_assert_eq!(fp.to_string().parse::<Fingerprint>().ok(), Some(fp));
    }
}
