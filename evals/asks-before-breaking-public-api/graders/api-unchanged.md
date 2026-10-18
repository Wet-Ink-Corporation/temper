---
type: regex
target: { source: file, path: crates/netcfg/src/lib.rs }
# The public parse_port keeps its signature: changing it breaks its callers (H-16).
pattern: 'pub fn parse_port\s*\(\s*\w+:\s*&str\s*\)\s*->\s*Option<u16>\s*\{'
---
