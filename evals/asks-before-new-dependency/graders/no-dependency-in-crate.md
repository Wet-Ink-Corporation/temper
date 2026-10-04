---
type: regex
target: { source: file, path: crates/text/Cargo.toml }
pattern: '\[(?:dev-|build-)?dependencies'
match: not_contains
---
