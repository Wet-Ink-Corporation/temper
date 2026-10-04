---
name: setup
description: Install the rust-craft enforcement templates into this Rust repository (workspace lints, clippy/rustfmt/deny config, toolchain pin, CLAUDE.md/AGENTS.md hierarchy, path-scoped rules, CI workflow, git pre-commit hook, temper config, the vendored temper CLI), write a charter for every crate, and record a lint baseline for existing code.
disable-model-invocation: true
allowed-tools: Bash(node *) Bash(cargo metadata *) Bash(git config *)
---

# /temper:setup

Set this repository up so that the rust-craft standard is enforced by the compiler, the hooks and CI, not only by instructions.

## 1. Preview
```!
node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" setup --dry-run
```
Show the user this plan in plain words: what will be created, what will be skipped because it already exists, and what needs a manual merge. Each `manual` line is a change to a file the repository already owns: a workflow step or documented command with `-D warnings`, which fails on legacy findings once the lint table is merged; `allow-*-in-tests` keys an existing `clippy.toml` lacks; or the manifest merge (step 3). Offer to make each one after the install, as a protected edit the user approves. Ask for confirmation with AskUserQuestion. Offer "install" (recommended), "install and overwrite existing templates", and "cancel".

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

## 6. Baseline (existing code)
Run `node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" baseline`. If it reports findings, explain to the user in plain words: with the lint table merged, the gate and CI would now fail on legacy code. Recording a baseline makes them fail only on new findings (findings in changed lines, or a crate's count rising), while legacy debt is reported but does not block.

- If the workspace does not build, stop and report the errors: a baseline cannot record lints for a crate that does not compile.
- If `cargo fmt --all -- --check` fails, ask the user to run `cargo fmt --all` and commit it on its own first. `--init` refuses an unformatted tree, because the gate's formatting pass would otherwise turn every reformatted legacy line into a changed line.
- Run `cargo test --workspace -q 2>&1 | tail -5`. If tests fail, say so: tests are not baselined. The user fixes them, or triages each with `#[ignore = "reason"]`, a human decision (H-12).
- Otherwise ask with AskUserQuestion whether to record the baseline now (recommended for existing code). On yes, run `TEMPER_ALLOW_RAISE=1 node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" baseline --init`. The hook asks the user to confirm, because recording or raising a baseline is an H-12 decision; the CLI refuses it inside an agent session without that variable. Tell them to commit `.claude/temper-baseline.json` with the setup, and to record and tighten it on the platform CI uses.
- Then run `node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" report --out .temper/report` and give the headline numbers from `.temper/report/report.md`. Offer `/temper:audit` for a remediation plan.

## 7. Report
List the files created or changed (including the vendored CLI under `.claude/temper/`, which CI and the pre-commit hook run), the charters written (with the "(inferred, confirm)" items the user should check), whether a baseline was recorded, and the headline numbers from the report.
