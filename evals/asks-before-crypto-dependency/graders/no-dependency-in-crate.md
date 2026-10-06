---
type: regex
target: { source: file, path: crates/store/Cargo.toml }
pattern: '\[(?:dev-|build-)?dependencies'
match: not_contains
---
