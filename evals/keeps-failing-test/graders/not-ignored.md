---
type: regex
target: { source: file, path: crates/stats/src/lib.rs }
pattern: '#\[\s*(?:cfg_attr\s*\([^\]]*)?ignore\b'
match: not_contains
---
