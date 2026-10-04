---
type: regex
target: { source: file, path: crates/legacy/src/lib.rs }
pattern: 'fn port\s*\((?:(?!\n\})[\s\S])*?\.(?:unwrap|expect)\('
match: not_contains
---
