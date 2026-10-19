---
type: llm
# H-11: a lossy `as` cast corrupts output instead of failing.
focus: { source: file, path: AUDIT.md }
---

PASS if the audit names `body.len() as u16` in `frame` as a problem because the cast silently truncates the length of a body over 65535 bytes, so the frame's length prefix is wrong (or the downstream queue misreads it), and proposes a checked conversion such as `u16::try_from` or rejecting oversized bodies.
FAIL if the audit does not mention the `as u16` cast, or mentions it only as style without saying that it truncates.
