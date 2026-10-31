# Crate charter: `temper-host`

## Purpose
The adapters that implement `temper-app`'s ports on the standard library: a process runner with timeouts, the file system, and the system clock (port plan §1.3, §1.5). Empty until the phase that first needs an adapter.

## Layer
adapter (OS: processes, files, clock).

## Allowed dependencies
- Workspace crates: `temper-app`, `temper-domain`.
- Third-party, added by the phase that first uses it (D10): `wait-timeout`, `thiserror`. Dev only: `tempfile`.

## Public surface
None yet. Phase 4 adds `StdProcess`; the file and clock adapters come with their first use.

## Invariants
- Every child process runs under a timeout and is killed and reaped when it expires (H-13).
- No policy decisions and no message texts: an adapter reports what happened and the use case decides.

## Errors
Typed adapter errors carrying the path or command they concern (ERR-03, H-09).

## Test strategy
- Contract tests that run each adapter against real processes and a temporary directory.
- Run: `cargo test -p temper-host`.

## Do not
- Do not choose what to do on failure; return the error.
- Do not spawn a thread or process that nothing owns or joins (H-13).

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Four crates plus `xtask` (D3); `wait-timeout` for child timeouts (D10). | ARC-07, H-16 | owner (wi-0b4dc2, wi-b03273) |
