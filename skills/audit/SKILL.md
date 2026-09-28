---
name: audit
description: Audit an existing Rust codebase against the rust-craft standard and produce a prioritized remediation plan (escape hatches, lint debt, architecture direction, test gaps), saved as .temper/audit.md.
argument-hint: "[crate or path to focus on]"
disable-model-invocation: true
allowed-tools: Bash(node *) Bash(cargo *) Bash(git *)
---

# /temper:audit

Focus: $ARGUMENTS (the whole workspace if empty)

## 1. Collect the evidence (run these, keeping the output short)
- `node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" report --out .temper/report`: findings from clippy, rustdoc and the scanner, mapped to rule IDs, per crate, with the baseline comparison. Read `.temper/report/report.json` for the numbers (severity, rules, files, crates) rather than re-running the tools. If the workspace lints are not installed yet, run `/temper:setup` first; without them the report measures only clippy's defaults and the scanner.
- `cargo tree -e normal --workspace --depth 1`: check dependency direction against ARC-02.
- `cargo test --workspace -q 2>&1 | tail -5`: do the tests pass?
- `cargo metadata --no-deps --format-version 1`: which crates have no `AGENTS.md` charter (ARC-08)?

Use an Explore subagent for any wide reading, such as finding `Arc<Mutex<` hot spots or god modules over 400 lines.

## 2. Analyze
Group the findings into:
1. **Safety and correctness risks**: H-01, H-02, H-03, H-04 and H-10 in code paths that handle external input.
2. **Architecture**: dependency direction, layering leaks, global state, and crates without a charter.
3. **Error and API design**: erased errors in library APIs, `pub` surface creep.
4. **Test gaps**: untested public behavior, nondeterministic tests.
5. **Idiom debt**: legacy crates to replace (MOD-03, MOD-04), lint debt by rule.

## 3. Plan
Write `.temper/audit.md` with an executive summary (5 lines), a table of counts by rule and crate (from the report), the top 10 remediation items ordered by risk reduced per unit of effort (each with its scope, rule IDs and a lower-bound size estimate), and the ratchet strategy. The ratchet is: the lint baseline (`.claude/temper-baseline.json`) and the hooks block new violations now, and legacy code is paid down crate by crate, each crate's cleanup being its own reviewed change followed by `temper baseline --tighten`. If no baseline is recorded yet, say that the gate and CI will fail on legacy debt until one is (`/temper:setup` step 6).

Report the file path and the executive summary.
