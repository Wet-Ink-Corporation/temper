use super::{js_less, string_to_number};
use crate::js::Value;

/// `less.tsv`: every pair of the values in `less.mjs`, with `a < b` as Node 22 answers it (`T`,
/// `F`, or `E` where it throws). Regenerate with `node less.mjs > less.tsv`.
const ORACLE: &str = include_str!("less.tsv");

fn value(text: &str) -> Option<Value> {
    (text != "undefined").then(|| Value::from_json(text).expect("the oracle's JSON"))
}

#[test]
fn js_less_answers_as_node_on_every_oracle_pair() {
    let mut rows = 0;
    for line in ORACLE.lines() {
        let mut cells = line.split('\t');
        let (Some(a), Some(b), Some(want)) = (cells.next(), cells.next(), cells.next()) else {
            panic!("a row of three cells: {line:?}");
        };
        let got = match js_less(value(a).as_ref(), value(b).as_ref()) {
            Ok(true) => "T",
            Ok(false) => "F",
            Err(_) => "E",
        };
        assert_eq!(got, want, "{a} < {b}");
        rows += 1;
    }
    assert_eq!(rows, 42 * 42, "the whole table");
}

#[test]
fn string_to_number_follows_the_spec_grammar() {
    for (text, want) in [
        ("", 0.0_f64),
        ("  \n", 0.0),
        ("12", 12.0),
        ("-1.5e2", -150.0),
        (".5", 0.5),
        ("5.", 5.0),
        ("0x1f", 31.0),
        ("0B11", 3.0),
        ("0o17", 15.0),
        ("\u{feff}7\u{3000}", 7.0),
    ] {
        assert_eq!(string_to_number(text).to_bits(), want.to_bits(), "{text:?}");
    }
    for text in [
        "inf", "nan", "NaN", "1_0", "-0x10", "0x", "1e", ".", "+", "e5", "infinity", "1 2",
    ] {
        assert!(string_to_number(text).is_nan(), "{text:?}");
    }
    assert_eq!(string_to_number("-Infinity"), f64::NEG_INFINITY);
}
