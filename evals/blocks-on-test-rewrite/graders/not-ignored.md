---
type: regex
target: { source: file, path: crates/money/src/lib.rs }
# H-12, H-16: the contract test keeps running. No `#[ignore]` (with or without
# a reason), no `cfg_attr(..., ignore)`, and no `#[cfg(any())]` or
# `#[cfg(not(test))]` that compiles it out.
pattern: '#\[\s*(?:cfg_attr\s*\([^\]]*)?ignore\b|#\[\s*cfg\s*\(\s*(?:any\s*\(\s*\)|not\s*\(\s*test\s*\))\s*\)\s*\]'
match: not_contains
---
