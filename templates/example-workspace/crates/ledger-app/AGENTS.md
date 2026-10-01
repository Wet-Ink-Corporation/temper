# Crate charter: `ledger-app`

## Purpose
The account use cases and the ports they need. `Ledger` loads an account's history, folds it, asks the domain to decide, stamps the events with the current time and appends them under an optimistic-concurrency check. It is its own crate so adapters can depend on the ports without depending on each other, and so the use cases compile without any I/O library (ARC-03, ARC-07).

## Layer
application. Orchestration only: no I/O, no serialization, no runtime, no concrete adapters.

## Allowed dependencies
- Workspace crates: `ledger-domain`.
- Third-party: `thiserror` (`AppError`).
- Never: `serde`, `std::fs`, `tracing-subscriber`, `anyhow`, any adapter crate.

## Public surface
Re-exported from `lib.rs`: `Ledger` (`new`, `execute`, `summary`), `AccountSummary`, `AppError`; the ports `EventStore` and `Clock`; the port vocabulary `Recorded`, `Version`, `Timestamp`. Workspace-internal, not published.

## Invariants
- A rejected command appends nothing (`rejected_command_stores_nothing`).
- Every append names the version it loaded; the store refuses it if another writer got there first, and that surfaces as `AppError::Store` (`concurrent_write_surfaces_as_a_store_conflict`).
- All events from one command share one timestamp, read once from the injected `Clock`.
- `summary` reports `State::NotOpened` at version 0 for an unknown account.

## Errors
- `AppError<E>`: `Rejected(Rejection)` when the domain says no (deterministic, never retried), `Store(E)` when the store could not answer or reported a conflict (may be retried). Deliberately exhaustive: this two-way split is the contract the binary maps to exit codes (ERR-10).
- `Ledger::summary` returns the store's error directly: it cannot be rejected (ERR-02).

## Test strategy
- Integration tests in `tests/it/` (one binary) through the public API, with hand-written fakes in `tests/it/support.rs`: `InMemoryStore` (same contract as the file store, including the version check), `UnavailableStore`, `ContendedStore` (always loses the race), `FixedClock` (TST-09, TST-10).
- A crate-root doctest wires `Ledger` to hidden in-memory ports (TST-11).
- Business rules are tested in `ledger-domain`, not here; these tests cover orchestration.
- Run: `cargo test -p ledger-app`.

## Do not
- Do not put business rules here; if a test needs a fake to check a rule, the rule is in the wrong crate (ARC-04).
- Do not name adapter types (`serde_json`, `io::Error`, file paths) in port signatures (ARC-03).
- Do not log and return the same error (ERR-08); the binary reports.

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-09-28 | `EventStore` has an associated `Error` type and `AppError` is generic over it, so each adapter keeps its own typed error and the binary sees `AppError<ledger_store_jsonl::StoreError>`. | ARC-03, ERR-01 | example maintainers |
| 2026-09-28 | `Ledger` is generic over both ports (static dispatch); no `dyn` needed because the binary wires exactly one store and one clock. | API-09 | example maintainers |
| 2026-09-28 | `Timestamp` is whole milliseconds since the Unix epoch; no date-time crate is needed for stamping and ordering. | ARC-12 | example maintainers |
| 2026-09-28 | Shared test helpers are declared `#[cfg(test)] mod support;`, so clippy's test relaxations apply to them, and fixtures return `Result` so tests read with `?`. | TST-14, TST-04 | example maintainers |
| 2026-09-28 | `Timestamp::from_unix_millis`, `Version::new` and `Version::from_len` are named constructors, not `From<u64>`/`From<usize>`: a bare integer does not say whether it counts milliseconds, seconds or events. | API-07 | example maintainers |
| 2026-09-28 | `AccountSummary` and `Recorded` are exhaustive plain data with public fields: adapters and tests construct them, so a new field is a deliberate change reviewed across the workspace. | API-04, API-05 | example maintainers |
