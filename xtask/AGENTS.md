# Crate charter: `xtask`

## Purpose
temper's repository tooling, run as `cargo xtask <task>` (the alias is in `.cargo/config.toml`). Never published. Phase 0a has one task:
- `check-licenses`: every crate under `crates/` holds `LICENSE-MIT` and `LICENSE-APACHE` equal to the root `LICENSE` and `LICENSE-APACHE`, because `cargo package` packs only files inside the crate (port plan §1.2).

Phase 0b adds `versions`, `npm-pack`, `check-npm-lock` and `verify-install` (port plan §4).

## Layer
tooling, `publish = false`.

## Allowed dependencies
- Workspace crates: none.
- Third-party, added by the phase that first uses it (D10): `anyhow`, `serde_json`.

## Public surface
None: a binary. Exit codes: `0` the task passed, `1` a check failed or a file could not be read, `2` usage error. Success goes to stdout, problems to stderr.

## Invariants
- The comparison logic is pure (`src/licenses.rs`); the file system is read only in `main.rs`.
- Paths are found from `CARGO_MANIFEST_DIR`, so a task gives the same answer from any working directory.

## Errors
A private `XtaskError` with the path that could not be read; no `anyhow` until a phase adds it.

## Test strategy
- Unit tests beside the pure logic.
- CI and `tests/ci.sh` run `cargo xtask check-licenses` on the committed tree.
- Run: `cargo test -p xtask`.

## Do not
- Do not put anything here that the shipped binary needs.

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Hand-written release tooling in `xtask`, not cargo-dist (D7). | H-16 | two-way default (wi-f22cef) |
| 2026-10-02 | Crates carry plain copies of both licences, checked by `check-licenses`. | H-16 | owner (wi-4679b3) |
