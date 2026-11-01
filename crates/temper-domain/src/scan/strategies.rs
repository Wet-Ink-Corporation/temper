//! Shared proptest strategies for the lexer and scanner properties.

use proptest::collection::vec;
use proptest::prelude::*;

/// Sources built from the tokens the lexer branches on, mixed with arbitrary chars.
pub(super) fn token_soup() -> impl Strategy<Value = String> {
    let token = prop_oneof![
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
