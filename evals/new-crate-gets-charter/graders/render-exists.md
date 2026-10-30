---
type: regex
target: { source: file, path: crates/render/src/lib.rs }
pattern: 'pub fn \w+\s*(?:<[^>]*>)?\s*\([^)]*Report|impl\b[^{]*Display for'
---
