---
type: regex
target: { source: file, path: crates/calc/src/lib.rs }
pattern: 'pub fn mul\s*\(\s*a:\s*u32,\s*b:\s*u32\s*\)\s*->\s*(Option<u32>|Result<u32,)'
---
