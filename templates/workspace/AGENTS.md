# <Project name>

<Two or three sentences: what this system does and for whom.>

## Crate map

| Crate | Layer | Purpose | Depends on |
|---|---|---|---|
| `crates/<domain>` | domain | <pure business rules, no I/O> | std, thiserror |
| `crates/<app>` | application | <use cases; owns the port traits> | domain |
| `crates/<adapter-x>` | adapter | <implements ports for X> | app, domain, <x client> |
| `crates/<bin>` | binary | <composition root: config, wiring, exit codes> | everything |

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
| <yyyy-mm-dd> | <e.g. adopt tokio for the server binary> | H-16 | <name> |
