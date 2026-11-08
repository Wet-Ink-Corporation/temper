# Contracts

Machine-readable output that other products build against. Review (assay, moving into redkiln as Move 3) reads temper's findings today and will call the `check` subcommands once they exist. Redkiln Rust (`rk-rust`, temper 2.0) defines the `rust.*` events.

| File | What it is | Status |
|---|---|---|
| [`schemas/temper-findings-1.schema.json`](schemas/temper-findings-1.schema.json) | What `temper baseline --check --findings-out FILE` writes: findings, errors and the ratchet (`temper_findings: 1`) | **In force** (`--findings-out` since 1.4.0). Validated against the three recordings assay's adapter reads (`examples/temper-findings-1.*.json`, copied from assay `crates/review-gates/tests/fixtures/` at `eeb2a45`) |
| [`schemas/rust.check-raised-1.schema.json`](schemas/rust.check-raised-1.schema.json) | `check raised --base REV --json`: baseline counts that rise, entries that appear, crates added or moved | Proposed |
| [`schemas/rust.check-exceptions-1.schema.json`](schemas/rust.check-exceptions-1.schema.json) | `check exceptions --base REV --json`: ask-level hits the base allowlist does not admit, multi-lint `#[expect]`, blind spots | Proposed |
| [`schemas/rust.check-gate-config-1.schema.json`](schemas/rust.check-gate-config-1.schema.json) | `check gate-config --base REV --json`: steering keys of `.claude/temper.json` changed | Proposed |
| [`schemas/rust.check-tests-1.schema.json`](schemas/rust.check-tests-1.schema.json) | `check tests --base REV --json`: test inventory and manifest test switches (H-12) | Proposed |
| [`rust-events.md`](rust-events.md) | temper's facts and telemetry as `rust.*` events | Proposed |

`python3 tests/contracts.py` validates every example against its schema, and `tests/ci.sh` runs it.

## The `check` subcommands

These come from temper#172 (assay's interim checks as temper subcommands) and redkiln's `references/docs/rust.md`, call D. Each takes `--base REV` (a full commit id) and compares base with head. Each answers in redkiln's CLI envelope (SPEC CLI-01 to CLI-04): one JSON object on stdout with `schema`, `ok`, `command`, `data`, `findings`, `proves` and `does_not_prove`. `ok` is true exactly when the exit code is 0, which is exactly when `findings` is empty. A check that cannot read what it needs exits 70 with the envelope when it can write one; that envelope has `ok: false` and at least one `unreadable:<what>` finding (for example `unreadable:base-baseline`), so the rule above holds for it too. It never passes with fewer checks than it reports (temper#173). The schema ids use the `rust` namespace that the 2.0 binary will carry.

Each finding's `key` is `<kind>:<stable subject>`, so a caller can tell an old finding from a new one. The kinds are the rule ids assay's adapter reports today (`crates/review-gates/src/temper/findings.rs` and `static_findings.rs`), so Review's mapping into its own findings stays one table:

| Subcommand | Finding kinds | assay source today |
|---|---|---|
| `check raised` | `raised`, `crate-moved` (and `unreadable` in every check) | `temper/baseline.rs`, `ratchet.rs` |
| `check exceptions` | `exception`, `multi-lint-expect`, `blind-spot` | `temper/exceptions.rs`, `statics.rs`, `reach.rs`, `cargo_targets.rs` |
| `check gate-config` | `gate-config` | `temper/gate_config.rs` |
| `check tests` | `test-inventory`, `manifest-switch`, `manifest-unreadable`, `lints-table`, `lint-inheritance` | `temper/inventory.rs`, `switches.rs`, `manifests.rs` |

What a check admits comes from the **base** revision, never the head, so a pull request cannot admit its own change. Today assay reads base `.assay/exceptions.toml`: `[[exception]]` (path, lint, reason), `[[gate_config]]` (key, base and head values, reason), and `[[test_inventory]]` and `[[manifest_switch]]` (path, base and head blob ids, reason). Where that allowlist lives once Redkiln Rust owns the checks is open: under `.redkiln/rust/` beside the moved config (rust.md call E) is the likely answer.

Not built in 1.x. Building these in Node would mean a 1.7.0 release, and whether to do that is an open question (wi-0924e6). The schemas fix the shape so Review can write its runner against them now. Vendored-copy drift (assay `vendored.rs`) has no subcommand, because the vendored copies go away in the monorepo.
