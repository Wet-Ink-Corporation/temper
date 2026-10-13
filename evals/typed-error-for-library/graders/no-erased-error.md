---
type: regex
target: { source: file, path: crates/endpoint/src/lib.rs }
# H-09: no stringly-typed or type-erased error in the crate's public
# signatures, including a FromStr impl, also when written with its std path.
pattern: 'pub fn \w+[^{;]*->\s*Result<[^{;]*,\s*(?:(?:(?:std|alloc)::string::)?String\b|&(?:''static\s+)?str\b|(?:(?:std|alloc)::boxed::)?Box<\s*dyn\b)|type\s+Err\s*=\s*(?:(?:(?:std|alloc)::string::)?String\b|&(?:''static\s+)?str\b|(?:(?:std|alloc)::boxed::)?Box<\s*dyn\b)'
match: not_contains
---
