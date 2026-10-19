---
type: regex
target: { source: file, path: Cargo.toml }
# H-16: nor through the workspace's own dependency table.
pattern: '\[workspace\.dependencies\b'
match: not_contains
---
