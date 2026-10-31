# Crate charter: `xtask`

## Purpose
temper's repository tooling, run as `cargo xtask <task>` (the alias is in `.cargo/config.toml`). Never published. Tasks:
- `check-licenses`: every crate under `crates/` holds `LICENSE-MIT` and `LICENSE-APACHE` equal to the root `LICENSE` and `LICENSE-APACHE`, because `cargo package` packs only files inside the crate (port plan §1.2).
- `versions --check [X]` and `versions --set X`: the version sites agree (port plan §4, "Versioning"). The sites are the workspace and sibling versions in `Cargo.toml`, the members in `Cargo.lock`, `package.json` and its six pins once it exists, `plugin.json` only once `package.json` exists (until the cutover the plugin ships 1.x), and the templates' `.claude/temper-version` once present. `--set` of a release (not a pre-release) needs a `## X` heading in `CHANGELOG.md`, and writes nothing without it.
- `check-npm-lock`: `package.json` and `package-lock.json` install exactly one engine safely (invariant 6): exact pins for the six packages, no `scripts` or `overrides`, lockfile format 2 or 3, one entry per package from the npm registry over HTTPS with a sha512 integrity, nothing else.
- `npm-pack X --bins DIR`: writes the six `@wet-ink/temper-<os>-<cpu>` packages under `target/npm/` from `DIR/<target>/temper[.exe]`, with both licences and the binary at mode 0755.

## Layer
tooling, `publish = false`.

## Allowed dependencies
- Workspace crates: none.
- Third-party (D10): `serde_json`; `anyhow` if a later task needs it. Dev only: `tempfile`.

## Public surface
None: a binary. Exit codes: `0` the task passed, `1` a check failed or a file could not be read, `2` usage error. Success goes to stdout, problems to stderr.

## Invariants
- The logic is pure, text in and text out (`licenses.rs`, `versions.rs`, `npm.rs`); the file system is touched only in `main.rs` and `pack.rs`.
- `versions --set` computes every new text before it writes any file, and spawns no process: it rewrites the members' `Cargo.lock` entries itself.
- Paths are found from `CARGO_MANIFEST_DIR`, so a task gives the same answer from any working directory.

## Errors
A private `XtaskError` (read, write, JSON, missing binary), each with its path; no `anyhow`.

## Test strategy
- Unit tests beside the pure logic; `tempfile` trees for `versions` and `npm-pack`.
- CI and `tests/ci.sh` run `cargo xtask check-licenses` and `cargo xtask versions --check` on the committed tree.
- Run: `cargo test -p xtask`.

## Do not
- Do not put anything here that the shipped binary needs.

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Hand-written release tooling in `xtask`, not cargo-dist (D7). | H-16 | two-way default (wi-f22cef) |
| 2026-10-02 | Crates carry plain copies of both licences, checked by `check-licenses`. | H-16 | owner (wi-4679b3) |
| 2026-10-31 | `plugin.json` joins the version check only once `package.json` exists (the cutover); a pre-release `--set` needs no CHANGELOG heading; `--set` rewrites `Cargo.lock` itself instead of running `cargo update`; packages' `package.json` keys come out sorted (serde_json without `preserve_order`), checked by an exact-text test instead of `insta`. | H-16 | two-way default (wi-654352) |
