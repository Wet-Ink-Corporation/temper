use super::gate_config;
use crate::js::Value;

/// Each row's `config` was printed by 1.x's `gateConfig` on its `text` (Node 22,
/// `.temper/probe3b/gate-config.mjs`): no file, an empty one, not JSON, every non-object
/// document, wrong-typed sections, a property the gate does not read, unsorted keys at depth,
/// integer-like keys, `1e21` and `0.1`, a repeated key, `__proto__`, U+2028 and keys past U+FFFF.
#[test]
fn every_row_answers_as_1x_gate_config() {
    let rows = Value::from_json(include_str!("rows.json")).expect("the rows file is JSON");
    let Value::Array(rows) = rows else {
        panic!("the rows file is an array");
    };
    assert_eq!(rows.len(), 22, "rows");
    for row in &rows {
        let text = row.get("text").and_then(Value::as_str);
        let expected = row.get("config").and_then(Value::as_str);
        assert_eq!(Some(gate_config(text).as_str()), expected, "{text:?}");
    }
}

/// P13: a large document (2 MiB here; the 32 MiB release timing is in the plan's Progress) is
/// read once, and only the gate's keys are copied.
#[test]
fn a_large_document_keeps_only_the_gates_keys() {
    let steps = "{\"name\":\"x\",\"cmd\":[\"cargo\",\"test\"]},".repeat(1 << 16);
    let text = format!(
        "{{\"gate\":{{\"steps\":[{}0]}},\"notes\":\"{}\"}}",
        steps,
        "n".repeat(1 << 20)
    );
    let config = gate_config(Some(&text));
    assert!(
        config.starts_with(r#"{"gate":{"allFeatures":true,"doc":true,"scan":true,"stepTimeoutSec":900,"steps":[{"cmd":["cargo","test"],"name":"x"},"#),
        "{}",
        config.get(..200).unwrap_or_default()
    );
    assert!(
        config.ends_with(r#"0]},"manifests":{"ownLints":[]},"scanner":{"exempt":[]}}"#),
        "the tail"
    );
}
