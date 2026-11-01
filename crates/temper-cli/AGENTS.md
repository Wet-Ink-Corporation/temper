# Crate charter: `temper-cli`

## Purpose
The `temper` binary: the composition root. It parses arguments, wires the `temper-host` adapters into the `temper-app` use cases, runs one command or hook, writes the result and maps the outcome to an exit code. No logic beyond that lives here (ARC-14).

## Layer
binary.

## Allowed dependencies
- Workspace crates: all of them (`temper-app`, `temper-domain` and `temper-host` are in since Phase 1d.2a).
- Third-party, added by the phase that first uses it (D10): `anyhow`, `serde_json`. No `clap` (D10). Dev only: `tempfile` (in).

## Public surface
None: a binary. Nothing routes to it until a subcommand graduates (port plan §3); it answers:
- `temper --version`: `temper <version>` on stdout, exit 0.
- `temper` with no arguments, or an empty first argument: the 1.x usage text on stdout, exit 0.
- `temper scan --all [--json]` and `temper scan FILES.. [--json]` (Phase 1d.2a): 1.x's output, byte for byte, except that the text form sorts files by byte order (D16(l)). Inside a git repository, paths are relative to git's top level (1d.2b). As in 1.x, `--json` and `--all` are read anywhere, every other argument that does not start with `--` is a file, other `--` flags are ignored, and files win over `--all`.
- bare `temper scan` (1.x `scanChanged`, with or without `--json`): the usage text, exit 2, until Phase 4 ports it (1d-14 A).
- anything else: the same usage text on stdout, exit 2, as 1.x does for an unknown subcommand. Each 1.x subcommand moves here when it graduates (port plan §3).

Exit codes:
- `0` success.
- `1` a failed command (`scan` found a hard hit, or could not list a directory), or stdout or stderr could not be written.
- `2` a usage error, or a command that cannot run here (`scan` outside a Rust project, with 1.x's stderr line).

When the reader of a stdout pipe has gone (`temper .. | head`), temper stops writing and exits with the command's own code (port plan §2.4, D16(b)); any other write failure exits 1. The 1.x subcommands bring their own codes with them, unchanged.

## Invariants
- Output goes through one locked handle per stream and is flushed before exit; no print macros (H-06, ERR-13).
- The usage text is the 1.x text, byte for byte, until the cutover.
- `main` returns an `ExitCode`; nothing calls `std::process::exit`.

## Errors
A command's refusal (`CommandError::Refused`) prints its message and keeps its exit. Any other failure prints `temper: ` and the error with each of its causes joined by `: ` (anyhow's `{:#}` shape), and exits 1. The chain printer is hand-written: three error sites need no `.context()` yet (OC-6 B); `anyhow` stays allowed for when they do (ERR-06).

## Test strategy
- Unit: argument parsing in `src/args.rs`.
- End to end: `tests/it/` runs the real binary (`CARGO_BIN_EXE_temper`) and asserts stdout, stderr and exit codes; `tests/it/scan.rs` writes the 1d.2 fixture at test time and compares with 1.x's recorded output in `tests/it/oracle/`. Every run sets `GIT_CEILING_DIRECTORIES` to the system temporary directory, so a test tree is a git repository only when the test runs `git init` in it; the git cases need `git` on `PATH`, and the 8.3 short-name case runs only on Windows. Miri cannot spawn processes, so these tests carry `#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]` (TST-17, H-12).
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
| 2026-11-01 | Only `scan --all` and `scan FILES` are ported in Phase 1d.2; `scan` graduates in Phase 4, and the launcher is unchanged. | ARC-05 | two-way (1d-10 A, wi-ca29db) |
| 2026-11-01 | Bare `temper scan` prints the usage text and exits 2 until Phase 4 ports `scanChanged`. | OUT-06 | two-way (1d-14 A, wi-c027c1) |
| 2026-11-01 | A hand-written cause-chain printer instead of `anyhow` at this edge, for now; a deviation from ERR-06 (SHOULD), revisited in Phase 2. | ERR-06 | two-way (OC-6 B, wi-950997) |
