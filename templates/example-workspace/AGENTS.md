# Ledger

An event-sourced bank-account ledger with a command-line front end. Accounts are opened, paid into, drawn from and closed; every change is an immutable event in an append-only JSON Lines file, and an account's state is a fold over its events. This workspace is the temper reference example: a small, complete project that follows rust-craft end to end.

## Crate map

| Crate | Layer | Purpose | Depends on |
|---|---|---|---|
| `crates/ledger-domain` | domain | Value types (`AccountId`, `OwnerName`, `Cents`) and the account Decider (`decide`, `evolve`, `fold`). Pure. | std, thiserror |
| `crates/ledger-app` | application | The `Ledger` use cases; owns the `EventStore` and `Clock` ports. | ledger-domain, thiserror |
| `crates/ledger-store-jsonl` | adapter | `EventStore` over append-only JSON Lines files, with versioned records and upcasting. | ledger-app, ledger-domain, serde, serde_json, thiserror |
| `crates/ledger-cli` | binary | `ledger`: argument parsing, wiring, output, exit codes. | all workspace crates, anyhow, thiserror, serde, serde_json, tracing, tracing-subscriber |

Dependencies point inward: domain, then application, then adapters, then binaries (ARC-02). Each crate's `AGENTS.md` is its charter (ARC-08).

## Commands

| Purpose | Command |
|---|---|
| Format | `cargo fmt --all` |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Test | `cargo nextest run --workspace` (or `cargo test --workspace`), plus `cargo test --doc --workspace` |
| Docs | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` |
| Supply chain | `cargo deny check` |
| Full gate (Claude Code) | `/temper:gate`, or `node "$TEMPER_CLI" gate` |
| Escape-hatch scan | `node "$TEMPER_CLI" scan --all` |
| Try it | `cargo run -q -p ledger-cli -- --store /tmp/ledger open acc-1 "Ada Lovelace"`, then `deposit acc-1 12.50`, `show acc-1` |

## Engineering standard (rust-craft)

All Rust here follows rust-craft. The full text, with rule IDs, examples, the review rubric and the process, is the `rust-craft` skill shipped in the temper plugin. The non-negotiables for production code (tests may use `unwrap`/`expect`/`assert!`):

- H-01 no `unsafe`; H-02 no `unwrap`/`expect`; H-03 no `panic!`/`todo!`/`unimplemented!`/`unreachable!`; H-04 no panicking indexing.
- H-05 no `#[allow]`; `#[expect(lint, reason)]` needs human approval; never weaken lint, CI or hook config.
- H-06 no `dbg!`, no print macros (use `tracing` or `writeln!` on a locked handle), no commented-out code, no untracked TODOs.
- H-07 no `Box::leak`/`mem::forget`/`transmute`/`static mut`/`'static` to silence lifetimes; H-08 no clones to appease the borrow checker.
- H-09 typed errors in library APIs (`thiserror`), with `anyhow` only in binaries; H-10 never discard failures; H-11 no lossy `as` casts.
- H-12 never weaken tests or checks; H-13 bounded and owned resources; H-14 no blocking or held locks across `.await`; H-15 zero warnings, documented public API.
- H-16 new dependencies, API breaks, lint exceptions, crate-boundary changes, migrations and test rewrites are human decisions.

Process: explore, frame, plan (`.temper/plans/`), red, green, refactor, self-review, gate, independent review, then report with a Status block (`DONE`, `WIP` or `BLOCKED`, plus the gate result, the review verdict, what changed, decisions needed and what was not verified).

## Decisions log

| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-09-28 | Four crates split by dependency direction: domain, application (ports), one adapter, one binary. | ARC-02, ARC-07 | example maintainers |
| 2026-09-28 | Events are not the persisted contract. The domain has no `serde`; the store adapter owns versioned DTOs and converts with `TryFrom`. | OUT-01, OUT-05 | example maintainers |
| 2026-09-28 | Dependencies: `thiserror` (typed errors), `serde` + `serde_json` (store records, CLI JSON output), `anyhow` (binary edge only), `tracing` + `tracing-subscriber` (binary diagnostics); dev: `proptest`, `insta`, `tempfile`. All MIT/Apache-2.0, all widely maintained. | H-16, ARC-12 | example maintainers |
| 2026-09-28 | No `clap`: the CLI hand-parses five commands and two options (see `crates/ledger-cli/AGENTS.md`). | ARC-12 | example maintainers |
| 2026-09-28 | Domain rejections and store failures stay distinct end to end: `AppError::{Rejected, Store}` and exit codes 3 and 4. | ERR-10, OUT-06 | example maintainers |
| 2026-09-28 | Optimistic concurrency with an expected `Version` on every append, enforced by the store under a `std::fs::File::lock` advisory lock. | ARC-03 | example maintainers |
| 2026-09-28 | Record schema v2 is current; v1 is frozen and upcast on read. Any schema change is a new version plus an upcaster, decided by a human. | OUT-03, H-16 | example maintainers |
| 2026-09-28 | MSRV 1.96 (tests use `std::assert_matches!`, stable since 1.96), toolchain pinned to 1.98.1. Release profile unchanged from the template. | MOD-01, PERF-09 | example maintainers |
| 2026-09-28 | No `#[expect]` or `#[allow]` anywhere; no lint exceptions requested. | H-05 | example maintainers |
