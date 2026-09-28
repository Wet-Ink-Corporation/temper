---
name: setup
description: Install the rust-craft enforcement templates into this Rust repository (workspace lints, clippy/rustfmt/deny config, toolchain pin, CLAUDE.md/AGENTS.md hierarchy, path-scoped rules, CI workflow, git pre-commit hook, temper config) and write a charter for every crate.
disable-model-invocation: true
allowed-tools: Bash(node *) Bash(cargo metadata *) Bash(git config *)
---

# /temper:setup

Set this repository up so that the rust-craft standard is enforced by the compiler, the hooks and CI, not only by instructions.

## 1. Preview
```!
node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" setup --dry-run
```
Show the user this plan in plain words: what will be created, what will be skipped because it already exists, and what needs a manual merge. Ask for confirmation with AskUserQuestion. Offer "install" (recommended), "install and overwrite existing templates", and "cancel".

## 2. Install
Run `node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" setup`, adding `--force` only if the user chose to overwrite.

## 3. Merge the workspace manifest
If the root `Cargo.toml` lacks `[workspace.lints]`, merge the `[workspace.package]`, `[workspace.lints.rust]` and `[workspace.lints.clippy]` tables from `${CLAUDE_PLUGIN_ROOT}/templates/workspace/Cargo.toml` into it, keeping the repository's existing values where they are stricter. Then add `[lints]\nworkspace = true` to every member crate's manifest. These are protected edits, so the user will be asked to approve each one. That approval is the point.

For a single-crate repository without a `[workspace]` table, put the same tables under `[lints.rust]` and `[lints.clippy]` in the package manifest instead.

## 4. Crate charters (ARC-08)
List the crates with `cargo metadata --no-deps --format-version 1`. For each crate that has no `AGENTS.md` in its root:
1. Read its manifest, `lib.rs`/`main.rs`, its module tree and its tests.
2. Fill in `${CLAUDE_PLUGIN_ROOT}/templates/crate/AGENTS.md` from what the code actually does. Mark anything you inferred rather than read with "(inferred, confirm)".
3. Write it to `<crate>/AGENTS.md`, and copy `${CLAUDE_PLUGIN_ROOT}/templates/crate/CLAUDE.md` next to it. That file imports the charter, so Claude Code loads it whenever files in that crate are read.

Fill in the crate map in the root `AGENTS.md` the same way.

## 5. Git hook
If the user agrees, run `git config core.hooksPath .githooks` so the pre-commit gate runs locally.

## 6. Baseline
Run `node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" scan --all` and `cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -40`. Report the counts per rule. On an existing codebase the numbers will be large, and that is expected: the hooks only block *new* violations (a ratchet), so legacy code does not stop anyone working. Offer `/temper:audit` for a remediation plan.

## 7. Report
List the files created or changed, the charters written (with the "(inferred, confirm)" items the user should check), and the baseline numbers.
