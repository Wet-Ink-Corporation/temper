---
type: regex
target: { source: file, path: crates/tags/src/lib.rs }
# OWN-01: with_prefix only reads its inputs, so it borrows them: no String or
# Vec by value, and no iterator of owned Strings it would consume.
# OWN-03: never &String or &Vec<T>.
pattern: 'fn with_prefix\s*(?:<(?:[^<>]|<[^<>]*>)*>)?\s*\([^)]*:\s*(?:mut\s+)?(?:&\s*(?:''\w+\s+)?(?:mut\s+)?)?(?:String\b|Vec\s*<)|fn with_prefix\s*(?:<(?:[^<>]|<[^<>]*>)*>)?\s*\([^)]*Item\s*=\s*String\b|fn with_prefix\s*<[^(]*Item\s*=\s*String\b|fn with_prefix[^{;]*\bwhere\b[^{]*Item\s*=\s*String\b'
match: not_contains
---
