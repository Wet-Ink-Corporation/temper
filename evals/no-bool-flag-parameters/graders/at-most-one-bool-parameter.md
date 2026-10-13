---
type: regex
target: { source: file, path: crates/term/src/lib.rs }
# API-10: no function, constructor or method in production code takes two or
# more bool parameters, and none takes an Option<bool>. The options belong in a
# type (API-02). Test code after #[cfg(test)] is not checked.
pattern: '^(?:(?!#\[cfg\(test\)\])[\s\S])*?(?:\bfn\s+\w+\s*(?:<[^>]*>)?\s*\([^)]*:\s*bool\b[^)]*:\s*bool\b|Option<\s*bool\s*>)'
match: not_contains
---
