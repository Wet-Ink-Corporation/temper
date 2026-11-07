//! `verdictHash` and the `canonical` it hashes, against Node 22's answers.

use proptest::prelude::*;

use crate::js::{Value, canonical, json_string};

use super::super::verdict_hash;
use super::{verdict, verdict_json};

/// (JSON text, `JSON.stringify(canonical(JSON.parse(text)))`, `verdictHash`), printed by 1.x.
const VECTORS: [(&str, &str, &str); 6] = [
    (
        r#"{"b":1,"10":2,"a":3,"2":4,"01":5,"4294967295":6,"4294967294":7}"#,
        r#"{"2":4,"10":2,"4294967294":7,"01":5,"4294967295":6,"a":3,"b":1}"#,
        "674c6ad2adcde306",
    ),
    (
        "{\"\u{1F600}\":1,\"\u{E000}\":2,\"z\":3,\"\u{e9}\":4}",
        "{\"z\":3,\"\u{e9}\":4,\"\u{1F600}\":1,\"\u{E000}\":2}",
        "7c185e536042fd72",
    ),
    (
        "[[{\"b\":1,\"a\":[{\"d\":null,\"c\":-0}]}],1e21,0.1,\"\u{2028}\"]",
        "[[{\"a\":[{\"c\":0,\"d\":null}],\"b\":1}],1e+21,0.1,\"\u{2028}\"]",
        "07dedc8b4a1f039e",
    ),
    (
        r#"{"__proto__":{"y":1,"x":2},"a":true}"#,
        r#"{"__proto__":{"x":2,"y":1},"a":true}"#,
        "d8543d16d4a35410",
    ),
    (
        r#""temper_verdict""#,
        r#""temper_verdict""#,
        "7f69d6827853f83d",
    ),
    ("null", "null", "74234e98afe7498f"),
];

#[test]
fn canonical_and_the_hash_agree_with_node_on_each_vector() {
    for (text, canonical_json, hash) in VECTORS {
        let value = Value::from_json(text).expect("JSON");
        assert_eq!(canonical(&value).to_json(), canonical_json, "{text}");
        assert_eq!(verdict_hash(&value), hash, "{text}");
    }
}

#[test]
fn the_same_verdict_hashes_the_same_whatever_its_key_order_a_different_one_does_not() {
    let v = verdict(&[]);
    let Value::Object(object) = &v else {
        panic!("the builder writes an object");
    };
    let mut reversed: Vec<String> = object
        .iter()
        .map(|(k, item)| format!("{k:?}:{}", item.to_json()))
        .collect();
    reversed.reverse();
    let reordered = Value::from_json(&format!("{{{}}}", reversed.join(","))).expect("JSON");
    assert_ne!(reordered.to_json(), v.to_json(), "the order differs");
    assert_eq!(verdict_hash(&v), verdict_hash(&reordered));
    let hash = verdict_hash(&v);
    assert!(
        hash.len() == 16 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        "{hash}"
    );
    let other =
        Value::from_json(&verdict_json(&[("verdict", r#""changes-requested""#)])).expect("JSON");
    assert_ne!(verdict_hash(&v), verdict_hash(&other));
}

proptest! {
    #[test]
    fn canonical_is_idempotent_and_keeps_every_key(
        keys in prop::collection::vec("[0-9a-z\u{e000}\u{1F600}]{0,3}", 0..12),
    ) {
        let props: Vec<String> = keys.iter().enumerate().map(|(i, k)| format!("{}:{i}", json_string(k))).collect();
        let value = Value::from_json(&format!("{{{}}}", props.join(","))).expect("JSON");
        let once = canonical(&value);
        prop_assert_eq!(canonical(&once).to_json(), once.to_json());
        let (Value::Object(before), Value::Object(after)) = (&value, &once) else {
            panic!("objects stay objects");
        };
        let mut a: Vec<&str> = before.iter().map(|(k, _)| k).collect();
        let mut b: Vec<&str> = after.iter().map(|(k, _)| k).collect();
        a.sort_unstable();
        b.sort_unstable();
        prop_assert_eq!(a, b);
    }
}

#[test]
fn canonical_of_a_wide_object_is_n_log_n() {
    // P9: 2^16 keys in one object.
    let props: Vec<String> = (0..1 << 16)
        .rev()
        .map(|i| format!("\"k{i}\":{i}"))
        .collect();
    let value = Value::from_json(&format!("{{{}}}", props.join(","))).expect("JSON");
    let Value::Object(object) = canonical(&value) else {
        panic!("an object");
    };
    let first: Vec<&str> = object.iter().take(3).map(|(k, _)| k).collect();
    assert_eq!(first, ["k0", "k1", "k10"]);
}
