# Plugin API types for the lens type check

`claude-code.d.ts` is Claude Code's declaration of the plugin API (`'claude-code'` and
`'claude-code/testing'`). `tests/lens.tsconfig.json` type-checks `hooks/lens/` and
`tests/lens.test.ts` against it, in `tests/lens-check.sh` and CI's `temper lens` step (wi-78d987).

It is copied, unedited, because Claude Code writes it only when an interactive session loads a
mod: the npm package does not ship it, and `claude plugin validate` and `claude plugin test` do not
write it. Its first line names the Claude Code version that wrote it: 2.1.296 for now, newer than
CI's 2.1.286 pin. So tsc could accept a call that 2.1.286 lacks; the lens's plugin tests, which run
on the pinned CLI, are what catch that.

Refresh it by hand when CI's Claude Code pin moves, in the same PR: load the `plugin-authoring`
skill in a session on the new version and copy the `types/claude-code.d.ts` it names.
