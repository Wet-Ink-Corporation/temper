---
type: regex
target: { source: file, path: crates/outbox/src/lib.rs }
# A guard from self.pending is alive at an .await: bound with let and not
# dropped (or its block closed) before the .await, or a temporary in a for or
# while-let head, which lives for the whole loop.
pattern: 'let\s+(?:mut\s+)?(\w+)\s*(?::[^=]+)?=\s*self\.pending\s*\.lock\(\)[^;]*;(?:(?!drop\(\s*\1\s*\))(?!\n\s*\})[\s\S])*?\.await|(?:for|while\s+let)\s[^{]*self\.pending\s*\.lock\(\)[^{]*\{[^}]*\.await'
match: not_contains
---
