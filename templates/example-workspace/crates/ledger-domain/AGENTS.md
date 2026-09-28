# Crate charter: `ledger-domain`

## Purpose
The rules of a bank account, as pure functions over data: validated value types (`AccountId`, `OwnerName`, `Cents`) and the account aggregate in Decider shape. It is its own crate because it has a different dependency profile from everything else (no I/O, no serialization) and changes only when the business rules change (ARC-07).

## Layer
domain. Pure: no I/O, no clock, no randomness, no environment, no async, no serialization, no logging.

## Allowed dependencies
- Workspace crates: none.
- Third-party: `thiserror` (typed rejection and validation errors, ERR-01). Dev only: `proptest` (invariant properties, TST-06).
- Never: `serde`, `tracing`, `tokio`, any `ledger-*` crate.

## Public surface
Re-exported from `lib.rs`: `Command`, `Event`, `State`, `Rejection`, `decide`, `evolve`, `fold`; `AccountId`/`AccountIdError`; `OwnerName`/`OwnerNameError`; `Cents`/`ParseCentsError`. Workspace-internal, not published.

## Invariants
- A `Cents` balance never goes below zero and never wraps: `decide` refuses overdrafts and overflowing deposits (`rejects_withdrawal_over_balance`, `rejects_deposit_that_would_overflow_the_balance`, property `balance_never_goes_below_zero`).
- `evolve` is total and infallible; replaying the events `decide` produced reproduces the state (property `replaying_decided_events_reproduces_state`). An event `decide` could not have produced in the current state leaves the state unchanged.
- Depositing then withdrawing the same amount restores the state (property `withdrawing_a_deposit_restores_the_state`).
- A closed account never reopens and accepts no command (`closed_account_rejects_every_command`).
- `AccountId` is 1 to 64 of `a-z`, `0-9`, `-`, not starting with `-`: no separators, dots or case ambiguity, so adapters can embed it in file names and URLs without escaping. `OwnerName` is trimmed, 1 to 80 characters, with no control characters. Both are only constructed through `FromStr` (API-03).

## Errors
- `Rejection`: why `decide` refused a command. Business outcomes only, never infrastructure (ERR-10). `#[non_exhaustive]`: new rules add variants. Callers usually report it rather than match variants.
- `AccountIdError`, `OwnerNameError`, `ParseCentsError`: why text is not a valid value, with the offending position, length or text as typed fields (ERR-03).

## Test strategy
- Unit: given/when/then over `decide` and `evolve` in `src/account/tests.rs`; validation cases beside each newtype.
- Property (`proptest`): balance conservation, replay, deposit/withdraw round trip, and `Cents` parse/display round trip.
- Doctests on `decide`, `AccountId` and `Cents` (TST-11).
- Run: `cargo test -p ledger-domain`.

## Do not
- Do not add `serde` derives here. Persisted shapes are DTOs in `ledger-store-jsonl`, converted with `TryFrom` (OUT-01, OUT-05).
- Do not read time, ids or randomness here; the application stamps events with its `Clock` port.
- Do not make `evolve` fallible. Validation belongs in `decide`; history is already true.
- Do not give newtypes public fields or unchecked constructors.

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-09-28 | Events are not the persisted contract: no `serde` in the domain; the store adapter owns versioned DTOs. | OUT-01, ARC-02 | example maintainers |
| 2026-09-28 | `Command`, `Event` and `State` are exhaustive so adding a variant breaks every `match` over them (the store's DTO mapping, the CLI's rendering) and forces review. Errors are `#[non_exhaustive]`. | API-05, CRF-03 | example maintainers |
| 2026-09-28 | `Cents` parses `12`, `12.5` and `12.50` so users never type raw cents; money stays `u64` cents inside. | API-03, ERR-09 | example maintainers |
| 2026-09-28 | `evolve` stays total and ignores events `decide` could not have produced; history consistency is not re-validated on load. The store validates each record's shape and values, and only `decide` creates events. | ARC-13 | example maintainers |
| 2026-09-28 | Add `proptest` as a dev-dependency for invariant properties. | H-16, TST-06 | example maintainers |
