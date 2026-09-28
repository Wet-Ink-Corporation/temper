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

| Where | Pattern |
|---|---|
| `crates/ledger-domain` | Validated newtypes (`AccountId`, `OwnerName`, `Cents`) with `FromStr` and private fields; checked money arithmetic; the Decider (`decide`, `evolve`, `fold`); a typed `Rejection`; property tests for invariants |
| `crates/ledger-app` | Ports owned by the consumer (`EventStore`, `Clock`); a use-case service generic over them; `AppError` separating rejections from store failures; hand-written fakes |
| `crates/ledger-store-jsonl` | Versioned DTOs, `TryFrom` into the domain, v1 upcasting, deterministic output, buffered and bounded I/O, optimistic concurrency under a file lock, an `insta` snapshot |
| `crates/ledger-cli` | Thin composition root: hand-parsed args with slice patterns, locked buffered stdout, named `tracing` events on stderr, `anyhow` context at the edge, exit codes from one exhaustive `match` |
| Root | The temper workspace template: shared `[workspace.lints]`, profiles, pinned toolchain, `clippy.toml`, `deny.toml`, CI, pre-commit hook, and a charter (`AGENTS.md`) per crate |

Each charter's decisions log records which rust-craft rules shaped that crate and why.

## Run the gate

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked
for p in ledger-domain ledger-app ledger-store-jsonl ledger-cli; do
  cargo check -p "$p" --locked                    # each crate builds alone
done
cargo +1.96 check --workspace --all-targets --locked   # MSRV
cargo deny check                                  # if cargo-deny is installed
node <temper>/scripts/temper.mjs scan --all       # escape-hatch scan
```

In Claude Code with the temper plugin, `/temper:gate` runs the fmt, manifest, clippy, test, doc and scan steps; CI runs the rest. Start reading at `AGENTS.md`, then each crate's `AGENTS.md`.
