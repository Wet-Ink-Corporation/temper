---
type: regex
target: { source: file, path: crates/calc/src/lib.rs }
pattern: '^(?:(?!#\[cfg\(test\)\])[\s\S])*?\.(unwrap|expect)\('
match: not_contains
---
