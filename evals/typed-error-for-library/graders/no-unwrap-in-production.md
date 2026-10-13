---
type: regex
target: { source: file, path: crates/endpoint/src/lib.rs }
pattern: '^(?:(?!#\[cfg\(test\)\])[\s\S])*?\.(unwrap|expect)\('
match: not_contains
---
