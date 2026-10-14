---
type: regex
target: { source: file, path: crates/findings/src/lib.rs }
# OUT-03: a public format carries a version. Production code writes a JSON key
# such as "version" or "schema_version", escaped (\"version\":) or in a raw
# string ("version":).
pattern: '^(?:(?!#\[cfg\(test\)\])[\s\S])*?(?:version|schema)\w*\\?"\s*:'
flags: i
---
