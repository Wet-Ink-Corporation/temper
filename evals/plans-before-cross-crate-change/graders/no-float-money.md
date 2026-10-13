---
type: regex
target: { source: file, path: crates/orders/src/discount.rs }
# ERR-09, API-02: money and a percentage with two decimals are exact integers
# (cents, basis points), never f32 or f64, in production code.
pattern: '^(?:(?!#\[cfg\(test\)\])[\s\S])*?\bf(?:32|64)\b'
match: not_contains
---
