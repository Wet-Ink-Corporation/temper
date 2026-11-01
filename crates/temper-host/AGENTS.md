# Crate charter: `temper-host`

## Purpose
The adapters that implement `temper-app`'s ports on the standard library: a process runner with timeouts, the file system, and the system clock (port plan §1.3, §1.5). Phase 1d.2a brought the file system (`StdFiles`) and 1d.2b processes (`StdProcess`); Phase 3 brings the clock.

## Layer
adapter (OS: processes, files, clock).

## Allowed dependencies
- Workspace crates: `temper-app`, `temper-domain` (both in since Phase 1d.2a).
- Third-party, added by the phase that first uses it (D10): `wait-timeout` (in since 1d.2b), `thiserror`. Dev only: `tempfile` (in).

## Public surface
- `HostPorts` (`Default`, implements `Ports`).
- `StdFiles`, the `Files` adapter: Node's `readFileSync(.., 'utf8')` (lossy, BOM kept, V8's length limit), `readdirSync` (libuv's order, `Dirent` kinds) and `realpathSync.native`.

- `StdProcess`, the `ProcessRunner` adapter: 1.x's `spawnSync` with stdin ignored, both streams piped and capped at the request's `ByteCap` (`Overflow` past it), a timeout through `wait-timeout`, and `CREATE_NO_WINDOW` on Windows (`windowsHide`).

## Invariants
- Every child process runs under a timeout and is killed and reaped when it expires (H-13).
- Each child's output is read by two named threads inside `thread::scope`, joined before `run` returns, each reading at most its cap plus one byte. No other threads, no atomics.
- `kill` reaches only the child. A grandchild that inherits a pipe keeps its reader waiting; Phase 4's cargo needs process groups or job objects.
- No policy decisions and no message texts: an adapter reports what happened and the use case decides.
- No read is unbounded: `read_text` reads at most 3 bytes per UTF-16 unit of V8's longest string, plus one.

## Errors
Typed adapter errors carrying the path or command they concern (ERR-03, H-09): `FsError` and `ProcessError` from `temper-app`, with `NotFound` mapped to `Ok(None)` where the port says so.

## Test strategy
- Contract tests that run each adapter against real processes and a temporary directory. They carry `#[cfg_attr(miri, ignore = "...")]` with a reason, since Miri cannot use the file system or spawn processes (TST-17, H-12).
- Run: `cargo test -p temper-host`.

## Do not
- Do not choose what to do on failure; return the error.
- Do not spawn a thread or process that nothing owns or joins (H-13).

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Four crates plus `xtask` (D3); `wait-timeout` for child timeouts (D10). | ARC-07, H-16 | owner (wi-0b4dc2, wi-b03273) |
| 2026-11-01 | `read_dir` reports any Windows reparse point as `Symlink`, as libuv reports `UV_DIRENT_LINK`, so the walk never enters one; a reparse point that is not a link (OneDrive, dedup) is not covered by a CI test. | H-16 | port plan 1d.2 (Risks) |
| 2026-11-01 | `StdProcess` on `wait-timeout` with only what `discover` needs: stdin null, capped capture, a timeout, kill and reap, joined readers. `wait-timeout` 0.2.1 passes `cargo deny`'s licence check as `MIT/Apache-2.0`. | H-13, H-16 | two-way (1d-13 A, wi-f1bd32) |
| 2026-11-01 | A reader past its cap reports `ProcessError::Overflow`; it stops reading, so the child fails its next write or meets its timeout. | H-10, H-13 | owner (OC-3 A, wi-129daa) |
