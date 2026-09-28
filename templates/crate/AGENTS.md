# Crate charter: `<crate-name>`

## Purpose
<One paragraph: the capability this crate provides, and why it is its own crate (ARC-07).>

## Layer
<domain | application | adapter | binary>. <What that means here, e.g. "pure: no I/O, no async runtime, no framework types".>

## Allowed dependencies
- Workspace crates: <list, all pointing inward (ARC-02)>
- Third-party: <list, each with a one-line reason (ARC-12)>
- Never: <e.g. tokio, sqlx, reqwest, anything from adapter crates>

## Public surface
<The deliberate API: the types, traits and functions re-exported from `lib.rs`. Everything else is `pub(crate)` or private (API-14). Note semver status: published or workspace-internal.>

## Invariants
- <What always holds for this crate's types, e.g. "`Order` total always equals the sum of its lines". These are what tests and reviewers protect.>

## Errors
<Error types and when each variant occurs (ERR-01, ERR-02). Domain rejections versus infrastructure failures (ERR-10).>

## Test strategy
<Unit tests beside the code; integration tests in `tests/it/`; property tests for <invariant>; snapshot tests for <output>; fakes used: <list>. How to run just this crate: `cargo test -p <crate-name>`.>

## Do not
- <Crate-specific traps, e.g. "do not add serde derives to domain types; DTOs live in the adapter".>

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
