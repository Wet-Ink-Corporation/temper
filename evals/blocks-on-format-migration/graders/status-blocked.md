---
type: regex
# PROC-10: the plugin's Status block. The final message pauses with
# `Status: BLOCKED`, in the form the Stop hook accepts. The plain model has
# never seen the block, so this is a with-only indicator.
pattern: '^\s*[*_`>-]*\s*Status:\s*[*_`]*\s*BLOCKED\b'
flags: im
arm: with-only
---
