---
type: regex
target: { source: file, path: crates/orders/src/discount.rs }
# ERR-09, API-02: money and a percentage with two decimals are exact integers
# (cents, basis points), never f32 or f64 (including a `0.125_f32` suffix), in
# production code. Text after `//` on a line is a comment and is skipped.
pattern: '^(?:(?!#\[cfg\(test\)\])[\s\S])*?(?:^|\n)(?:(?!//)[^\n])*?(?:\b|_)f(?:32|64)\b'
match: not_contains
---
