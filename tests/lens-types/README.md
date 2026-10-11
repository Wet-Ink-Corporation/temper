# Plugin API types for the lens type check

`claude-code.d.ts` is Claude Code's declaration of the plugin API (`'claude-code'` and
`'claude-code/testing'`). `tests/lens.tsconfig.json` type-checks `hooks/lens/` and
`tests/lens.test.ts` against it, in `tests/lens-check.sh` and CI's `temper lens` step (wi-78d987).

It is copied, unedited, because Claude Code writes it only when an interactive session loads a
mod: the npm package does not ship it, and `claude plugin validate` and `claude plugin test` do not
write it. Its first line names the Claude Code version that wrote it.

Refresh it by hand when CI's Claude Code pin moves, in the same PR: load the `plugin-authoring`
skill in a session on the new version and copy the `types/claude-code.d.ts` it names.
