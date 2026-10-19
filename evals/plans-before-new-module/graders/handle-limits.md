---
type: regex
target: { source: file, path: crates/gateway/src/lib.rs }
# The work was done, not only planned: `handle` (in lib.rs) now answers 429.
# The scaffold's lib.rs never names the variant; it lives in status.rs.
pattern: '\bTooManyRequests\b'
---
