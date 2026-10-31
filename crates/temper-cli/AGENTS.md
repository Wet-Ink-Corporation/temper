# Crate charter: `temper-cli`

## Purpose
The `temper` binary: the composition root. It parses arguments, wires the `temper-host` adapters into the `temper-app` use cases, runs one command or hook, writes the result and maps the outcome to an exit code. No logic beyond that lives here (ARC-14).

## Layer
binary.

## Allowed dependencies
- Workspace crates: all of them.
- Third-party, added by the phase that first uses it (D10): `anyhow`, `serde_json`. No `clap` (D10). Dev only: `tempfile`.

## Public surface
None: a binary. In Phase 0a it answers:
- `temper --version`: `temper <version>` on stdout, exit 0.
- `temper` with no arguments, or an empty first argument: the 1.x usage text on stdout, exit 0.
- anything else: the same usage text on stdout, exit 2, as 1.x does for an unknown subcommand. Each 1.x subcommand moves here when it graduates (port plan §3).

Exit codes: `0` success, `1` stdout could not be written, `2` usage error. When the reader of a stdout pipe has gone (`temper .. | head`), temper stops writing and exits with the command's own code (port plan §2.4, D16(b)); any other write failure exits 1. The 1.x subcommands bring their own codes with them, unchanged.

## Invariants
- Output goes through one locked stdout handle and is flushed before exit; no print macros (H-06, ERR-13).
- The usage text is the 1.x text, byte for byte, until the cutover.
- `main` returns an `ExitCode`; nothing calls `std::process::exit`.

## Errors
Phase 0a has no failure beyond the stdout write: a closed pipe keeps the command's code, anything else exits 1. Later phases add `anyhow` at this edge only (ERR-06).

## Test strategy
- Unit: argument parsing in `src/args.rs`.
- End to end: `tests/it/` runs the real binary (`CARGO_BIN_EXE_temper`) and asserts stdout, stderr and exit codes. Miri cannot spawn processes, so these tests carry `#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]` (TST-17, H-12).
- Run: `cargo test -p temper-cli`.

## Do not
- Do not put policy or message texts here; they belong in `temper-domain`.
- Do not add an argument-parsing dependency (D10).

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Hand-parse arguments; no `clap` (D10). | ARC-12, H-16 | owner (wi-b03273) |
| 2026-10-31 | `--version` prints the version and exits 0, where 1.x prints the usage text and exits 2; the port plan's Phase 0 test list asks for it, and the Node CLI never answered `--version`. | OUT-06 | port plan (Phase 0 test list) |
| 2026-10-31 | Exit codes come from a private fieldless `Exit` enum through one exhaustive `match`, so no code is a loose constant or an `as` cast. | H-11 | Phase 0a |
