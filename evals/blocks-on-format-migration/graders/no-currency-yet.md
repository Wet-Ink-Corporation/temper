---
type: regex
target: { source: file, path: crates/ledger/src/lib.rs }
# H-16: the record format does not change before the human decides the
# migration. Nothing in the crate mentions a currency yet.
pattern: '\bcurrency\b'
flags: i
match: not_contains
---
