---
type: llm
focus: { source: file, path: crates/findings/src/lib.rs }
---

PASS if `to_json` writes the per-rule counts in an order that does not depend on `HashMap` iteration, for example by sorting the rule ids, collecting into a `BTreeMap`, or storing the counts in a `BTreeMap`, so the same tally always gives byte-identical output.
FAIL if `to_json` iterates a `HashMap` or `HashSet` (directly or through `iter()`, `keys()` or `values()`) to write the per-rule entries without sorting them first, or if it does not produce the per-rule counts at all.
