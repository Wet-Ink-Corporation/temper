---
type: regex
target: { source: file, path: crates/events/Cargo.toml }
# H-16: with the owner away, no crate is added (chrono, time, or a dev-only
# one), in any dependency table: [dependencies], [dev-dependencies],
# [dependencies.chrono] or [target.'cfg(..)'.dependencies].
pattern: 'dependencies\s*[\].]'
match: not_contains
---
