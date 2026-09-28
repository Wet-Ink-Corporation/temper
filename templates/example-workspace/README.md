# Ledger: a rust-craft reference workspace

A small event-sourced bank-account ledger, written to the rust-craft standard that the temper plugin enforces. Copy its shapes, not its domain.

```console
$ ledger --store /tmp/ledger open acc-1 "Ada Lovelace"
acc-1: open, Ada Lovelace, balance 0.00 (version 1)
$ ledger --store /tmp/ledger deposit acc-1 20
acc-1: open, Ada Lovelace, balance 20.00 (version 2)
$ ledger --store /tmp/ledger withdraw acc-1 25
2026-09-28T14:42:35.771797Z  WARN ledger: command rejected rejection=insufficient funds: balance 20.00, requested 25.00
$ echo $?
3
```

## What it demonstrates

| Where | Pattern | Rules |
|---|---|---|
| `crates/ledger-domain` | Validated newtypes (`AccountId`, `OwnerName`, `Cents`) with `FromStr` and private fields; the Decider (`decide`, `evolve`, `fold`); a typed `Rejection`; property tests for invariants | API-02, API-03, ARC-13, ERR-09, TST-06 |
| `crates/ledger-app` | Ports owned by the consumer (`EventStore`, `Clock`); a use-case service generic over them; `AppError` separating rejections from store failures; hand-written fakes | ARC-03, ARC-04, ERR-10, TST-09 |
| `crates/ledger-store-jsonl` | Versioned DTOs, `TryFrom` into the domain, v1 upcasting, deterministic output, buffered and bounded I/O, optimistic concurrency under a file lock, an `insta` snapshot | OUT-01..05, PERF-07, H-13, TST-07 |
| `crates/ledger-cli` | Thin composition root: hand-parsed args with slice patterns, locked buffered stdout, `tracing` to stderr, `anyhow` context at the edge, documented exit codes | ARC-14, ERR-06, OUT-06, OUT-08 |
| Root | The temper workspace template: shared `[workspace.lints]`, pinned toolchain, `clippy.toml`, `deny.toml`, CI, pre-commit hook, and a charter (`AGENTS.md`) per crate | ARC-01, ARC-08, MOD-09 |

There are no `unwrap`, `expect`, `panic!`, `unsafe`, `as` casts, `#[allow]` or `#[expect]` in production code, and every `clone` carries its reason.

## Run the gate

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo deny check                                  # if cargo-deny is installed
node <temper>/scripts/temper.mjs scan --all       # escape-hatch scan
```

In Claude Code with the temper plugin, `/temper:gate` runs all of these. Start reading at `AGENTS.md`, then each crate's `AGENTS.md`.
