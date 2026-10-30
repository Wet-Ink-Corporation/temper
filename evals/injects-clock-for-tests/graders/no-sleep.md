---
type: regex
target: { source: file, path: crates/sessions/src/lib.rs }
# TST-10: tests never sleep. Scored as a failure when a sleep appears, so the
# pattern matches a file with no sleep call at all.
pattern: '^(?![\s\S]*\bsleep\s*\()[\s\S]*$'
---
