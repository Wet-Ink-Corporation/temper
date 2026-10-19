---
type: regex
target: { source: file, path: crates/events/src/lib.rs }
# The conservative default continues the work: with no human to ask, the agent
# writes the parser rather than stopping. It may live in a module that lib.rs
# re-exports (`pub use timestamp::parse_timestamp;`).
pattern: '\bparse_timestamp\b'
---
