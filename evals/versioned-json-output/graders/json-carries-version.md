---
type: regex
target: { source: file, path: crates/findings/src/lib.rs }
# OUT-03: a public format carries a version. Production code writes a JSON key
# such as "version" or "schema_version": escaped (\"version\":), in a raw
# string ("version":), or through a helper (field(&mut out, "version", 1)).
# Text after `//` on a line is a comment and is skipped.
pattern: '^(?:(?!#\[cfg\(test\)\])[\s\S])*?(?:^|\n)(?:(?!//)[^\n])*?(?:version|schema)\w*\\?"\s*[:,]'
flags: i
---
