# Crate charter: `temper-app`

## Purpose
temper's use cases: one per subcommand and hook handler, and the ports they reach the outside world through (`Files`, `ProcessRunner`, `Clock`), plus the git and cargo command wrappers built on `ProcessRunner` (port plan §1.3). Empty until Phase 1.

## Layer
application.

## Allowed dependencies
- Workspace crates: `temper-domain`.
- Third-party, added by the phase that first uses it (D10): `serde_json`, `thiserror`. Dev only: `proptest`, `insta`, `tempfile`.

## Public surface
None yet. A `test-util` feature will expose the in-memory fakes of the ports to other crates' tests (TST-09).

## Invariants
- No `std::fs`, `std::process` or `std::env`: files, processes, the clock and the run environment come through ports or `RunEnv` (ARC-02).
- The ports are traits owned here; adapters implement them, never the other way round.

## Errors
A typed `AppError` per use case family, wrapping domain errors with `#[from]` (ERR-01, H-09).

## Test strategy
- Unit tests against the fakes; no real processes or files.
- Tests that need real git, cargo or the file system live in `temper-cli/tests/it` and run through the composition root.
- Run: `cargo test -p temper-app`.

## Do not
- Do not decide policy that belongs in `temper-domain`, and do not hold message texts.
- Do not call the standard library's process, file or environment APIs directly.

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Four crates plus `xtask` (D3). | ARC-07, H-16 | owner (wi-0b4dc2) |
