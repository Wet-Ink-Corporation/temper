use proptest::prelude::*;

use super::super::{Value, js_json, js_string, truthy};

proptest! {
    /// `JSON.parse` reads a number as the double nearest its text, as Rust's `f64` parser does.
    #[test]
    fn numbers_read_as_the_nearest_double(
        digits in "[1-9][0-9]{0,24}",
        point in 1_usize..26,
        exp in -330_i32..310,
    ) {
        let (int, frac) = digits.split_at(point.min(digits.len()));
        let frac = if frac.is_empty() { String::new() } else { format!(".{frac}") };
        let text = format!("{int}{frac}e{exp}");
        let want: f64 = text.parse().expect("a decimal literal");
        prop_assume!(want.is_finite());

        let got = Value::from_json(&text).map_err(|e| e.to_string());

        prop_assert_eq!(got, Ok(Value::Number(want)), "{}", text);
    }
}

/// Node 22.22.0: `[text, JSON.stringify(JSON.parse(text)), String(JSON.parse(text))]`.
const NODE: [(&str, &str, &str); 12] = [
    (
        r#"{"b":1,"a":2,"2":3,"1":4,"b":5}"#,
        r#"{"1":4,"2":3,"b":5,"a":2}"#,
        "[object Object]",
    ),
    (
        r#"[1,"x",null,true,{"k":[]}]"#,
        r#"[1,"x",null,true,{"k":[]}]"#,
        "1,x,,true,[object Object]",
    ),
    (
        "12345678901234567890",
        "12345678901234567000",
        "12345678901234567000",
    ),
    ("1e21", "1e+21", "1e+21"),
    ("-0", "0", "0"),
    ("0.1", "0.1", "0.1"),
    (
        r#"{"__proto__":{"cwd":"/x"}}"#,
        r#"{"__proto__":{"cwd":"/x"}}"#,
        "[object Object]",
    ),
    (r#"" \u0001""#, "\"\u{2028}\\u0001\"", "\u{2028}\u{1}"),
    ("9007199254740993", "9007199254740992", "9007199254740992"),
    (
        "1.7976931348623157e308",
        "1.7976931348623157e+308",
        "1.7976931348623157e+308",
    ),
    ("5e-324", "5e-324", "5e-324"),
    (
        "123456789012345678901234567890",
        "1.2345678901234568e+29",
        "1.2345678901234568e+29",
    ),
];

#[test]
fn json_reads_as_json_parse_reads_it() {
    let got: Vec<(String, String)> = NODE
        .iter()
        .map(|(text, _, _)| match Value::from_json(text) {
            Ok(value) => (js_json(&value), js_string(&value).into_owned()),
            Err(error) => (format!("error: {error}"), String::new()),
        })
        .collect();
    let want: Vec<(String, String)> = NODE
        .iter()
        .map(|(_, json, string)| ((*json).to_owned(), (*string).to_owned()))
        .collect();

    assert_eq!(got, want, "JSON.stringify and String() of JSON.parse");
}

#[test]
fn null_is_falsy_and_prints_as_null() {
    let null = Value::from_json("null").expect("null is JSON");

    assert_eq!(
        (truthy(&null), js_json(&null), js_string(&null).into_owned()),
        (false, "null".to_owned(), "null".to_owned()),
        "JS null"
    );
}

#[test]
fn a_duplicate_key_keeps_its_first_place_and_its_last_value() {
    let value = Value::from_json(r#"{"a":1,"b":2,"a":3}"#).expect("valid JSON");

    assert_eq!(
        js_json(&value),
        r#"{"a":3,"b":2}"#,
        "as JSON.parse keeps it"
    );
}

/// Text JSON.parse rejects, and text it accepts that `serde_json` does not (D16(n): the hook
/// answers both as unparseable).
#[test]
fn what_json_parse_rejects_and_d16n_inputs_are_errors() {
    let deep = format!("{}{}", "[".repeat(128), "]".repeat(128));
    let rejected = [
        "",
        " ",
        "{",
        r#"{"a":1,}"#,
        "\u{feff}{}",
        "{} x",
        r#""\ud800""#,
        "1e400",
        deep.as_str(),
    ];

    let accepted: Vec<&str> = rejected
        .iter()
        .filter(|text| Value::from_json(text).is_ok())
        .copied()
        .collect();

    assert_eq!(accepted, Vec::<&str>::new(), "each is an error");
}

#[test]
fn nesting_up_to_serde_json_s_limit_is_read() {
    let deep = format!("{}{}", "[".repeat(127), "]".repeat(127));

    let value = Value::from_json(&deep).map(|v| js_json(&v).len());

    assert_eq!(
        value.ok(),
        Some(254),
        "127 levels; serde_json refuses the 128th"
    );
}
