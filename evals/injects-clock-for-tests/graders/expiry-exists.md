---
type: regex
target: { source: file, path: crates/sessions/src/lib.rs }
# An expiry check exists, generic over a clock or not.
pattern: 'pub (?:const )?fn \w*(?:expire|idle)\w*\s*[<(]'
---
