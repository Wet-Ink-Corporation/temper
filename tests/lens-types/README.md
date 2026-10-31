# Plugin API types for the lens type check

`claude-code.d.ts` is Claude Code's declaration of the plugin API (`'claude-code'` and
`'claude-code/testing'`). `tests/lens.tsconfig.json` type-checks the temper-lens plugin's
`plugins/temper-lens/hooks/lens/` and `plugins/temper-lens/tests/` against it, in `tests/lens-check.sh` and CI's `temper lens` step (wi-78d987).

It is copied, unedited, because Claude Code writes it only in a session: as the `plugin-authoring`
skill loads, and when the engine loads a mod. The npm package does not ship it, and
`claude plugin validate` and `claude plugin test` do not write it. Its first line names the Claude Code version that wrote it: 2.1.296 for now, newer than
CI's 2.1.286 pin. So tsc could accept a call that 2.1.286 lacks; the lens's plugin tests, which run
on the pinned CLI, are what catch that.

Refresh it by hand when CI's Claude Code pin moves, in the same PR, from a session on the new
version. Either copy works; both carry the same declarations:
- load the `plugin-authoring` skill and copy the `types/claude-code.d.ts` it names, under the
  skill's own folder (written as the skill loads, before any mod has);
- or, once the engine has loaded a mod, copy `<mod folder>/.claude-plugin/types/claude-code/index.d.ts`.
  That is the path the engine writes; `.claude-plugin/types/` there also holds the built-in tools'
  and MCP tools' declarations, which the lens check does not use.
