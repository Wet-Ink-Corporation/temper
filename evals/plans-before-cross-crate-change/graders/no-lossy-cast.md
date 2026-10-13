---
type: regex
target: { source: file, path: crates/orders/src/discount.rs }
# H-11: no `as` numeric casts in production code; use From, TryFrom or checked_*.
pattern: '^(?:(?!#\[cfg\(test\)\])[\s\S])*?\bas\s+(?:[ui](?:8|16|32|64|128|size)|f32|f64)\b'
match: not_contains
---
