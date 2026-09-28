# Crate charter: `ledger-cli`

## Purpose
The `ledger` binary: the composition root. It parses arguments into validated domain values, wires `JsonlStore` and the system clock into `Ledger`, runs one request, writes the result, and maps the outcome to an exit code. No business logic lives here (ARC-14).

## Layer
binary.

## Allowed dependencies
- Workspace crates: all of them.
- Third-party: `anyhow` (context chains at the edge, ERR-06), `thiserror` (the typed `UsageError`), `serde` and `serde_json` (`--format json` output), `tracing` and `tracing-subscriber` (diagnostics on stderr; the subscriber is installed only here, OUT-08). Dev only: `tempfile`.

## Public surface
None: a binary. Usage: `ledger [--store <dir>] [--format human|json] <open|deposit|withdraw|close|show> <account> [..]`, default store `./ledger-data`.

Exit codes: `0` success, `1` stdout could not be written, `2` usage error, `3` the ledger rejected the command (nothing recorded), `4` the event store failed (I/O, corrupt record, or concurrent write), `5` the process could not set itself up (log subscriber). Fatal setup errors go straight to stderr, never through `tracing`.

## Invariants
- Data goes to stdout through one locked `BufWriter`; diagnostics go to stderr through `tracing` (`RUST_LOG`, default `warn`). Nothing uses print macros (H-06, OUT-06).
- `--format json` writes exactly one JSON object per line with a fixed field order.
- Each failure is logged once, in `main`, where it becomes an exit code (ERR-08).
- A rejection writes nothing to stdout and exits 3.

## Errors
- `UsageError` (private): why the command line is invalid; exit 2.
- Store failures travel as `anyhow::Error` with the account as context and are printed with `{:#}` (the whole cause chain on one line).

## Test strategy
- Unit: argument parsing (`src/args.rs`) and rendering (`src/output.rs`) beside the code.
- End to end: `tests/it/` runs the real binary (`CARGO_BIN_EXE_ledger`) against a temporary store and asserts stdout and exit codes, with `RUST_LOG` removed. The JSON test checks the timestamp's type, not its value, because it comes from the real clock.
- Run: `cargo test -p ledger-cli`.

## Do not
- Do not decide anything about money here; call `Ledger`.
- Do not call `std::process::exit`; return an `ExitCode` from `main`.
- Do not add a dependency for argument parsing without a decision (see below).

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-09-28 | Hand-parse arguments with slice patterns instead of `clap`: five commands and two options fit in one small, tested function, and it keeps a proc-macro dependency out of the build. Revisit with `clap` derive if commands gain options or help text grows. | ARC-12, H-16 | example maintainers |
| 2026-09-28 | `SystemClock` lives here as a private module: it is ten lines, and a separate crate for it would not meet any split criterion. | ARC-07 | example maintainers |
| 2026-09-28 | Classify failures with a private `Failure` enum built from the typed `AppError`, rather than downcasting `anyhow::Error`. | MOD-11, ERR-06 | example maintainers |
| 2026-09-28 | Add `anyhow`, `tracing`, `tracing-subscriber` (features `fmt`, `std`, `env-filter`). | H-16, ARC-12 | example maintainers |
