---
type: regex
target: { source: file, path: crates/paging/src/lib.rs }
# TST-01, TST-05: a test asserts the reported case exactly: 25 items at 10 a
# page is 3 pages (the value may be wrapped in Some or carry a u64 suffix).
# Known miss: the 25 held in a variable first.
pattern: 'assert_eq!\s*\([^;]*\b25(?:_?u64)?\b[^;]*,\s*(?:Some\s*\(\s*)?3(?:_?u64)?\b|assert_eq!\s*\(\s*(?:Some\s*\(\s*)?3(?:_?u64)?\b[^;]*\b25(?:_?u64)?\b'
---
