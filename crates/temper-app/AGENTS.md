# Crate charter: `temper-app`

## Purpose
temper's use cases: one per subcommand and hook handler, and the ports they reach the outside world through (`Files`, `ProcessRunner`, `Clock`), plus the git and cargo command wrappers built on `ProcessRunner` (port plan §1.3). Phase 1d.2 brings the first use case, `temper scan --all` and `temper scan FILES` (`Engine::scan`), with 1.x's project discovery (the git top level first, through `ProcessRunner`), crate lookup and path rules.

## Layer
application.

## Allowed dependencies
- Workspace crates: `temper-domain`.
- Third-party, added by the phase that first uses it (D10): `serde_json`, `thiserror` (both in since Phase 1d.2). Dev only: `proptest` (in), `insta`, `tempfile`.

## Public surface
Re-exported from `lib.rs`, one path each (API-14):
- `Engine` (`new(ports, env, scanner)`, `scan`), `RunEnv` (`cwd`, `style`), `ScanArgs` (`targets`, `format`), `ScanTargets` (`All`, `Files`), `OutputFormat` (`Text`, `Json`), `CommandOutput` (`stdout`, `stderr`, `exit`), `Exit` (`Ok`, `Fail`, `Usage`), `CommandError` (`Refused`, `Files`, `Json`).
- The ports: `Ports` (`files`, `process`), `Files` (`read_text`, `read_dir`, `real_path`), `DirEntry` (`name`, `kind`), `EntryKind` (`Dir`, `File`, `Symlink`, `Other`), `FsError` (`new`, `op`, `path`, `source`), `FsOp` (`Read`, `ReadDir`, `RealPath`).
- The process port: `ProcessRunner` (`run`), `ProcessRequest` (`program`, `args`, `cwd`, `stdout_cap`, `stderr_cap`, `timeout`; no `env` until Phase 4, OC-8 B), `ByteCap` (`new`, `get`), `ProcessOutput` (`status`, `stdout`, `stderr`), `ExitKind` (`Code`, `Signal`, `TimedOut`), `Stream` (`Stdout`, `Stderr`), `ProcessError` (`Spawn`, `Io`, `Overflow`).

Everything else is `pub(crate)`: the lexical port of Node's `path.posix` and `path.win32` in `paths` (OC-1 B), `project` (`discover`, `crate_of`, `exists`, `declares_package`), `relpath` (`rel_to`), `git` (`toplevel`), and `scan` (`walk`). The in-memory fakes in `fakes` are `#[cfg(test)]` until another crate's tests need them; then a `test-util` feature exposes them (TST-09).

## Invariants
- No `std::fs`, `std::process` or `std::env`: files, processes, the clock and the run environment come through ports or `RunEnv` (ARC-02).
- The ports are traits owned here; adapters implement them, never the other way round.
- Path text follows Node's rules for the `RunEnv`'s `PathStyle`, not the host's, so both styles are tested on every platform.

## Errors
A typed error per use case family, wrapping port and domain errors with `#[from]` (ERR-01, H-09): `CommandError` for the commands, with `Refused` carrying 1.x's stderr text and exit; `FsError` (operation, path, `io::Error`) for the `Files` port; `ProcessError` (program plus cause, or the stream and cap that overflowed) for `ProcessRunner`, where a timeout or a failing exit is an `ExitKind`, not an error. Each 1.x fallback that swallows a failure is an explicit `match` arm with its reason (H-10): an unreadable target or manifest is skipped, an unresolvable path keeps its lexical form, `exists` is false, and any failed git run means no git root.

## Test strategy
- Unit tests against the fakes; no real processes or files.
- Node's path functions are checked against values printed by Node 22, and as properties in both styles (TST-06).
- Tests that need real git, cargo or the file system live in `temper-cli/tests/it` and run through the composition root.
- Run: `cargo test -p temper-app`.

## Do not
- Do not decide policy that belongs in `temper-domain`, and do not hold message texts; an error's own `Display` is not one.
- Do not call the standard library's process, file or environment APIs directly.

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Four crates plus `xtask` (D3). | ARC-07, H-16 | owner (wi-0b4dc2) |
| 2026-11-01 | `Engine::new(ports, env, scanner)`; `rules` (the rule map) arrives with its reader in Phase 5. | API-14 | two-way (1d-15, wi-12a32e) |
| 2026-11-01 | Node's lexical path algebra is a `pub(crate)` port in `paths`, over `&str` with `PathStyle` as a value, not `std::path`: `components()` keeps `..` and compares case-sensitively on Windows. | ARC-05, API-14 | two-way (OC-1 B, wi-30e6a5) |
| 2026-11-01 | Phase 1d.2 is two PRs: 1d.2a without processes (`discover` walks Cargo manifests only), 1d.2b with `ProcessRunner` and the git top level. | ARC-05, PROC-05 | two-way (OC-5 B, wi-368b47) |
| 2026-11-01 | `Files` has only what `scan` reads: `read_text`, `read_dir` and `real_path`; `existsSync` is `real_path` answering `Ok(Some(_))`, because §1.3's `metadata` is lstat and a dangling link would read as present. | API-14 | two-way (OC-9 B, wi-bb98f7) |
| 2026-11-01 | `discover` asks git for its top level through `StdProcess` on `wait-timeout`, built with only what `discover` needs: stdin null, capped capture, a timeout, kill and reap. | H-13, H-16 | two-way (1d-13 A, wi-f1bd32) |
| 2026-11-01 | `ProcessRequest` has `stdout_cap: ByteCap` and no `env`; Phase 4 replaces `stdout_cap` with `StdoutSink` and adds `env`. | API-14 | two-way (OC-8 B, wi-b175fb) |
| 2026-11-01 | A stream past its `ByteCap` is `ProcessError::Overflow { program, stream, cap }`, as 1.x's `maxBuffer` fails the run; output is never silently truncated. | H-10, ERR-03 | owner (OC-3 A, wi-129daa) |
