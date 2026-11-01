//! Shared proptest strategies for the lexer and scanner properties.

use proptest::collection::vec;
use proptest::prelude::*;

/// Sources built from the tokens the lexer branches on and the rule predicates read, mixed with
/// arbitrary chars, so the scan properties reach every predicate and resume path.
pub(super) fn token_soup() -> impl Strategy<Value = String> {
    let predicate_token = prop_oneof![
        Just("\u{2028}"),
        Just("SeqCst"),
        Just("compiler_fence("),
        Just("extern crate "),
        Just(".unwrap_or_default()"),
        Just("parse()"),
        Just("impl "),
        Just(" for "),
        Just("const "),
        Just("{"),
        Just("}"),
        Just("("),
        Just(")"),
        Just(";"),
        Just("[&"),
        Just("spawn("),
        Just("get_"),
        Just("é"),
        Just("😀"),
    ]
    .prop_map(str::to_owned);
    let token = prop_oneof![
        predicate_token,
        Just("\"".to_owned()),
        Just("'".to_owned()),
        Just("r#\"".to_owned()),
        Just("\"#".to_owned()),
        Just("br\"".to_owned()),
        Just("/*".to_owned()),
        Just("*/".to_owned()),
        Just("//".to_owned()),
        Just("\n".to_owned()),
        Just("\r".to_owned()),
        Just("\\".to_owned()),
        Just("#".to_owned()),
        Just("a".to_owned()),
        Just(" ".to_owned()),
        any::<char>().prop_map(String::from),
    ];
    vec(token, 0..64).prop_map(|tokens| tokens.concat())
}
