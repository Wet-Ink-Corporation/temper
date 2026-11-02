use proptest::prelude::*;

use super::*;

fn object(entries: &[(&str, Value)]) -> Value {
    Value::Object(
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    )
}

fn text(s: &str) -> Value {
    Value::String(s.to_owned())
}

#[test]
fn numbers_print_as_node_22_number_to_string() {
    // Each pair was printed by Node 22.22.0: `String(x)`, then `JSON.stringify(x)`.
    let vectors: &[(f64, &str, &str)] = &[
        (0.0, "0", "0"),
        (-0.0, "0", "0"),
        (1.0, "1", "1"),
        (-1.0, "-1", "-1"),
        (0.1, "0.1", "0.1"),
        (1e21, "1e+21", "1e+21"),
        (-1e21, "-1e+21", "-1e+21"),
        (1e20, "100000000000000000000", "100000000000000000000"),
        (1e-7, "1e-7", "1e-7"),
        (1e-6, "0.000001", "0.000001"),
        (1.2e-6, "0.0000012", "0.0000012"),
        (
            123_456_789_012_345_680_000.0,
            "123456789012345680000",
            "123456789012345680000",
        ),
        (1.5e300, "1.5e+300", "1.5e+300"),
        (5e-324, "5e-324", "5e-324"),
        (
            9_007_199_254_740_992.0,
            "9007199254740992",
            "9007199254740992",
        ),
        (123.456, "123.456", "123.456"),
        (100.0, "100", "100"),
        // Exact ties between two shortest candidates (-2893561012494.28125 and
        // 1198927741179043.25): JS takes the even last digit.
        (
            f64::from_bits(0xC285_0DAD_6908_7240),
            "-2893561012494.2812",
            "-2893561012494.2812",
        ),
        (
            f64::from_bits(0x4311_09AC_787F_528D),
            "1198927741179043.2",
            "1198927741179043.2",
        ),
        (f64::INFINITY, "Infinity", "null"),
        (f64::NEG_INFINITY, "-Infinity", "null"),
        (f64::NAN, "NaN", "null"),
    ];
    for &(x, string, json) in vectors {
        assert_eq!(js_number(x), string, "String({x:e})");
        assert_eq!(js_json(&Value::Number(x)), json, "JSON.stringify({x:e})");
    }
}

#[test]
fn integer_text_converts_as_js_number_does() {
    // Node 22: Number("9007199254740993"), Number("0x1FFFFFFFFFFFFF1"), Number("0x" + 40 Fs),
    // Number("12345678901234567890123"), Number("0o17"), Number("0b101").
    assert_eq!(
        js_number(integer_from_radix("9007199254740993", 10)),
        "9007199254740992"
    );
    assert_eq!(
        js_number(integer_from_radix("1FFFFFFFFFFFFF1", 16)),
        "144115188075855860"
    );
    assert_eq!(
        js_number(integer_from_radix(&"F".repeat(40), 16)),
        "1.461501637330903e+48"
    );
    assert_eq!(
        js_number(integer_from_radix("12345678901234567890123", 10)),
        "1.2345678901234568e+22"
    );
    assert_eq!(js_number(integer_from_radix("17", 8)), "15");
    assert_eq!(js_number(integer_from_radix("101", 2)), "5");
    assert_eq!(js_number(integer_from_radix("-0", 10)), "0");
    assert!(
        integer_from_radix("1Z", 16).is_nan(),
        "a digit outside the radix is NaN, as Number() gives"
    );
}

#[test]
fn objects_put_integer_like_keys_first_in_ascending_order() {
    // Node 22: JSON.stringify({b:1,"2":2,a:3,"1":4,"01":5,"4294967294":6,"4294967295":7,"-1":8})
    let v = object(&[
        ("b", Value::Number(1.0)),
        ("2", Value::Number(2.0)),
        ("a", Value::Number(3.0)),
        ("1", Value::Number(4.0)),
        ("01", Value::Number(5.0)),
        ("4294967294", Value::Number(6.0)),
        ("4294967295", Value::Number(7.0)),
        ("-1", Value::Number(8.0)),
    ]);
    assert_eq!(
        js_json(&v),
        r#"{"1":4,"2":2,"4294967294":6,"b":1,"a":3,"01":5,"4294967295":7,"-1":8}"#
    );
    let keys: Vec<_> = entries(&v).map(|(k, _)| k.into_owned()).collect();
    assert_eq!(
        keys,
        ["1", "2", "4294967294", "b", "a", "01", "4294967295", "-1"],
        "Object.keys order"
    );
}

#[test]
fn arrays_and_nesting_render_compactly() {
    let v = Value::Array(vec![
        Value::Bool(true),
        object(&[("x", Value::Array(vec![]))]),
        text("q\"\\"),
    ]);
    assert_eq!(js_json(&v), r#"[true,{"x":[]},"q\"\\"]"#);
}

#[test]
fn string_coerces_as_js_string_does() {
    // Node 22: String([1,[2,"x"],{}]), String(true), String({}).
    let v = Value::Array(vec![
        Value::Number(1.0),
        Value::Array(vec![Value::Number(2.0), text("x")]),
        object(&[]),
    ]);
    assert_eq!(js_string(&v), "1,2,x,[object Object]");
    assert_eq!(js_string(&Value::Bool(true)), "true");
    assert_eq!(
        js_string(&object(&[("a", Value::Bool(false))])),
        "[object Object]"
    );
    assert_eq!(js_string(&text("plain")), "plain");
}

#[test]
fn truthiness_follows_js() {
    for falsy in [
        Value::Bool(false),
        Value::Number(0.0),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        text(""),
    ] {
        assert!(!truthy(&falsy), "{falsy:?} is falsy");
    }
    for truthy_value in [
        Value::Bool(true),
        Value::Number(-1.0),
        text("0"),
        Value::Array(vec![]),
        object(&[]),
    ] {
        assert!(truthy(&truthy_value), "{truthy_value:?} is truthy");
    }
}

#[test]
fn entries_of_an_array_are_its_indices_and_of_a_scalar_none() {
    let v = Value::Array(vec![text("a"), text("b")]);
    let got: Vec<_> = entries(&v)
        .map(|(k, v)| (k.into_owned(), js_string(v).into_owned()))
        .collect();
    assert_eq!(
        got,
        [
            ("0".to_owned(), "a".to_owned()),
            ("1".to_owned(), "b".to_owned())
        ]
    );
    assert_eq!(
        entries(&text("ab")).count(),
        0,
        "a string's chars have no properties temper reads"
    );
    assert_eq!(entries(&Value::Number(1.0)).count(), 0);
}

/// `JSON.stringify(String.fromCharCode(c))` for every `c` below U+0080, as Node 22 prints it.
fn node_escape(c: char) -> String {
    match c {
        '"' => r#""\"""#.to_owned(),
        '\\' => r#""\\""#.to_owned(),
        '\u{8}' => r#""\b""#.to_owned(),
        '\t' => r#""\t""#.to_owned(),
        '\n' => r#""\n""#.to_owned(),
        '\u{C}' => r#""\f""#.to_owned(),
        '\r' => r#""\r""#.to_owned(),
        c if u32::from(c) < 0x20 => format!("\"\\u{:04x}\"", u32::from(c)),
        c => format!("\"{c}\""),
    }
}

#[test]
fn string_escapes_match_node_for_ascii_and_the_separators() {
    // Node 22: JSON.stringify leaves U+2028, U+2029, U+FEFF and U+007F unescaped.
    for c in (0_u8..0x80)
        .map(char::from)
        .chain(['\u{2028}', '\u{2029}', '\u{FEFF}'])
    {
        assert_eq!(
            js_json(&Value::String(c.to_string())),
            node_escape(c),
            "U+{:04X}",
            u32::from(c)
        );
    }
}

proptest! {
    #[test]
    fn a_string_round_trips_through_json(s in any::<String>()) {
        let rendered = js_json(&Value::String(s.clone()));
        let back: String = serde_json::from_str(&rendered).expect("js_json writes valid JSON");
        prop_assert_eq!(back, s);
    }
}
