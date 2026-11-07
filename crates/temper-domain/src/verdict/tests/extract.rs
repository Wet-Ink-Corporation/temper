//! `extractVerdict`: the last fenced block containing `temper_verdict`, found as 1.x's
//! ``/```(?:json)?\s*\n([\s\S]*?)```/g`` finds blocks.

use crate::js::Value;

use super::super::{ExtractedVerdict, extract_verdict};
use super::{verdict, verdict_json};

const V: &str = r#"{"temper_verdict":1,"verdict":"approved"}"#;

fn parsed(text: &str) -> Option<Value> {
    match extract_verdict(text) {
        Some(ExtractedVerdict::Parsed(value)) => Some(value),
        Some(ExtractedVerdict::Unparseable { message }) => panic!("unparseable: {message}"),
        None => None,
    }
}

#[test]
fn none_without_text_or_without_a_temper_verdict_block() {
    assert_eq!(extract_verdict(""), None);
    assert_eq!(
        extract_verdict("All good.\n```json\n{\"verdict\":\"approved\"}\n```\n"),
        None
    );
}

#[test]
fn parses_a_fenced_json_block_with_or_without_the_language_tag() {
    let v = verdict(&[]);
    let pretty = format!("Review done.\n\n```json\n{}\n```\n", verdict_json(&[]));
    assert_eq!(parsed(&pretty), Some(v.clone()));
    assert_eq!(parsed(&format!("```\n{}\n```", verdict_json(&[]))), Some(v));
}

#[test]
fn the_last_block_containing_temper_verdict_wins() {
    let first = verdict_json(&[("verdict", r#""changes-requested""#)]);
    let last = verdict_json(&[]);
    let text = format!(
        "Draft:\n```json\n{first}\n```\nFinal:\n```json\n{last}\n```\nOther:\n```json\n{{\"x\":1}}\n```\n"
    );
    let v = parsed(&text).expect("a block");
    assert_eq!(v.get("verdict").and_then(Value::as_str), Some("approved"));
}

#[test]
fn json_that_does_not_parse_is_reported_not_thrown() {
    let Some(ExtractedVerdict::Unparseable { message }) =
        extract_verdict("```json\n{\"temper_verdict\": 1, \"verdict\": }\n```\n")
    else {
        panic!("expected an unparseable verdict");
    };
    // serde_json's wording (D16(a)), not V8's.
    assert!(message.contains("line 1"), "{message}");
}

#[test]
fn blocks_are_found_where_1x_finds_them() {
    // Each row's answer was printed by 1.x's extractVerdict (Node 22).
    let rows: [(&str, String, Option<&str>); 13] = [
        ("no newline after json", format!("```json {V}```"), None),
        ("no newline after the fence", format!("```{V}```"), None),
        (
            "a fence inside a string ends the block",
            "```json\n{\"note\":\"use ``` here\",\"temper_verdict\":1}\n```".to_owned(),
            None,
        ),
        (
            "a rust block's closing fence opens the next match",
            format!("```rust\nfn x() {{}}\n```\nprose\n```json\n{V}\n```\n"),
            None,
        ),
        (
            "four backticks match from the second",
            format!("````json\n{V}\n````"),
            Some(V),
        ),
        (
            "spaces and tabs before the newline",
            format!("```json  \t\n{V}\n```"),
            Some(V),
        ),
        ("another language tag", format!("```json5\n{V}\n```"), None),
        ("no closing fence", format!("```json\n{V}\n"), None),
        ("CRLF", format!("```json\r\n{V}\r\n```"), Some(V)),
        ("an empty block", "```\n```".to_owned(), None),
        (
            "NBSP is JS whitespace",
            format!("```json\u{a0}\n{V}\n```"),
            Some(V),
        ),
        (
            "NEL is not JS whitespace",
            format!("```json\u{85}\n{V}\n```"),
            None,
        ),
        (
            "a later non-verdict block does not hide the verdict",
            format!("```\n{V}\n```\n```\n{{}}\n```"),
            Some(V),
        ),
    ];
    for (name, text, expected) in rows {
        let expected = expected.map(|json| Value::from_json(json).expect("JSON"));
        assert_eq!(parsed(&text), expected, "{name}");
    }
}

#[test]
fn any_json_value_is_parsed_with_json_parse_key_semantics() {
    assert_eq!(
        parsed("```\n[\"temper_verdict\"]\n```"),
        Some(Value::Array(vec![Value::String(
            "temper_verdict".to_owned()
        )]))
    );
    let dup = parsed(
        "```\n{\"temper_verdict\":1,\"verdict\":\"a\",\"verdict\":\"approved\",\"__proto__\":{\"x\":1}}\n```",
    )
    .expect("a block");
    assert_eq!(
        dup.to_json(),
        r#"{"temper_verdict":1,"verdict":"approved","__proto__":{"x":1}}"#
    );
}

#[test]
fn field_reads_a_parsed_objects_own_property_and_nothing_of_an_unparseable_one() {
    let v = extract_verdict(&format!("```\n{V}\n```")).expect("a block");
    assert_eq!(v.field("verdict").and_then(Value::as_str), Some("approved"));
    assert_eq!(v.field("fingerprint"), None);
    let bad = ExtractedVerdict::Unparseable {
        message: "x".to_owned(),
    };
    assert_eq!(bad.field("verdict"), None);
}

#[test]
fn a_report_of_many_fences_is_read_in_one_pass() {
    // P2: 2^16 small blocks and an opening fence with no close; linear in the text.
    let text = format!(
        "{}```\n{V}\n```\n```json\n{}",
        "```\n{}\n```\n".repeat(1 << 16),
        "x".repeat(1 << 20)
    );
    assert_eq!(parsed(&text), Some(Value::from_json(V).expect("JSON")));
}
