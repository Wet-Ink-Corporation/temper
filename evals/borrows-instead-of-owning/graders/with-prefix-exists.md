---
type: regex
target: { source: file, path: crates/tags/src/lib.rs }
pattern: 'pub fn with_prefix\s*(?:<(?:[^<>]|<[^<>]*>)*>)?\s*\('
---
