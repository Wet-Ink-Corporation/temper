# Shipping native binaries in plugins

Research from 2026-10-01 on how to ship a Rust binary in a Claude Code plugin. It covers which
surfaces give plugin hooks Node, how other ecosystems ship Rust and Go binaries, Ryan's Cowork
checks, and a recommendation for [temper 2's Rust port](../../plans/temper-2-rust-port.md).

It was written in the Weigh-In repository (commits `60c5620` and `c54139a` there) and moved here
on 2026-10-02, because most of it is about temper. The files are as written, except that links
to Weigh-In's own files now point at that repository on GitHub. Weigh-In keeps a summary of the
parts that apply to it.

| File | What it is |
|---|---|
| [`report.md`](report.md) | The report, with the Cowork probe results and how they check against Anthropic's documentation |
| [`notes/hook_runtime_and_node.md`](notes/hook_runtime_and_node.md) | Node and runtime availability to plugin hooks, per surface |
| [`notes/npm_binary_distribution.md`](notes/npm_binary_distribution.md) | Native binaries through npm: per-platform packages versus a postinstall or first-run download |
| [`notes/claude_plugin_prior_art.md`](notes/claude_plugin_prior_art.md) | How Claude Code plugins and MCP servers ship Rust and Go binaries |
| [`notes/other_ecosystems.md`](notes/other_ecosystems.md) | How ecosystems outside npm ship native binaries |
