---
type: regex
target: { source: file, path: crates/tags/src/lib.rs }
# OWN-13: the caller only iterates or counts the result, so with_prefix returns
# an iterator (or a slice), not a freshly collected Vec.
pattern: 'fn with_prefix\s*(?:<(?:[^<>]|<[^<>]*>)*>)?\s*\([^{;]*->\s*(?:Vec\s*<|Box\s*<\s*\[)'
match: not_contains
---
