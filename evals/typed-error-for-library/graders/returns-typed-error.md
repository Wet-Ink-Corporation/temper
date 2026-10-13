---
type: regex
target: { source: file, path: crates/endpoint/src/lib.rs }
# H-09, ERR-01: the public parser returns a named error type, not String,
# &str or Box<dyn Error>.
pattern: 'pub fn parse_endpoint\s*(?:<[^>]*>)?\s*\([^)]*\)\s*->\s*Result<\s*(?:Endpoint|Self)\s*,\s*(?:[a-z_]\w*::)*[A-Z]\w*Error\s*>'
---
