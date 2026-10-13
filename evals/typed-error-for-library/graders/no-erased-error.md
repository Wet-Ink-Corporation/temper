---
type: regex
target: { source: file, path: crates/endpoint/src/lib.rs }
# H-09: no stringly-typed or type-erased error in the crate's public
# signatures, including a FromStr impl.
pattern: 'pub fn \w+[^{;]*->\s*Result<[^{;]*,\s*(?:String\b|&(?:''static\s+)?str\b|Box<\s*dyn\b)|type\s+Err\s*=\s*(?:String\b|&(?:''static\s+)?str\b|Box<\s*dyn\b)'
match: not_contains
---
