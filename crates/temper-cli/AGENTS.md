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
- `temper hook post-tool` and `temper hook user-prompt` (Phase 2b), `temper hook pre-edit` (Phase 2c; since Phase 3e it records the tree, 1.x's `noteTree`, before the skill gate), `temper hook pre-bash` (Phase 2d, its shell-independent checks; the rest in 2e and 2f), `temper hook subagent-start` (Phase 3e), `temper hook pre-handback` and `temper hook subagent-stop` (Phase 3f; the implementer branch of `subagent-stop` fails as not ported until Phase 4) and `temper hook session-start` (Phase 3d; the composition root reads `CLAUDE_PLUGIN_ROOT`, else `TEMPER_PLUGIN_ROOT`, `CLAUDE_ENV_FILE`, `TEMPER_RUNTIME_WARNINGS` (one warning per non-empty line, from the launcher) and `current_exe` for D17's `TEMPER_BIN`): 1.x's hook protocol (port plan §2.1). Stdin is read whole, up to 256 MiB, and lossily decoded; empty input reads as `{}`. Input that is not JSON, or is over the cap, prints the "could not parse the hook input as JSON; nothing was checked" notice (D16(n)); `TEMPER_DISABLE=1` gives silence; a handler error prints `temper hook <event> failed: <the error chain>` (OC-3 (p)). All exit 0. An unknown event, or a missing one (`undefined`), writes `temper: unknown hook event X` on stderr and exits 0 without reading stdin.
- `temper status [--session ID]` (Phase 3b, the text form): 1.x's nine lines, byte for byte, except that an unreadable config is named on the `config:` line (D16(d), OC-5 A); `TEMPER_BASE` and `--session` choose the base as in 1.x, with 1.x's one stderr note for a `TEMPER_BASE` that is no commit here. `--session` without an id, or followed by a flag, prints the status usage line on stderr, exit 2. `--json`, read anywhere (Phase 3c), prints 1.x's JSON document, with `version` from `TEMPER_PLUGIN_VERSION` (else this binary's), `disabled` from `TEMPER_DISABLE`, and a running gate whose pid the probe sees alive (OC-2 A). A recorded verdict or a progress start 1.x crashes on prints `temper: <error>`, exit 1. The launcher routes `status` here since 3c.
- the five other 1.x hook events (`session-start`, `subagent-start`, `pre-handback`, `stop`, `subagent-stop`): the usage text, exit 2, as an unported subcommand, until each is ported (2b-1).
- anything else: the same usage text on stdout, exit 2, as 1.x does for an unknown subcommand. Each 1.x subcommand moves here when it graduates (port plan §3).

Exit codes:
- `0` success.
- `1` a failed command (`scan` found a hard hit, or could not list a directory), or stdout or stderr could not be written.
- `2` a usage error, or a command that cannot run here (`scan` outside a Rust project, with 1.x's stderr line).

When the reader of a stdout pipe has gone (`temper .. | head`), temper stops writing and exits with the command's own code (port plan §2.4, D16(b)); any other write failure exits 1. The 1.x subcommands bring their own codes with them, unchanged.

## Invariants
- Output goes through one locked handle per stream and is flushed before exit, stderr first, so a note leads a merged stream as 1.x's `console.error` does (since 3c, #207 review F1); no print macros (H-06, ERR-13).
- The usage text is the 1.x text, byte for byte, until the cutover.
- `main` returns an `ExitCode`; nothing calls `std::process::exit`.

## Errors
A command's refusal (`CommandError::Refused`) prints its message and keeps its exit. Any other failure prints `temper: ` and the error with each of its causes joined by `: ` (anyhow's `{:#}` shape), and exits 1. The chain printer is hand-written: three error sites need no `.context()` yet (OC-6 B); `anyhow` stays allowed for when they do (ERR-06).

## Test strategy
- Unit: argument parsing in `src/args.rs`, the stdin reader in `src/hook_io.rs`, and Node's `os.tmpdir()` in `src/wire.rs` against values Node prints.
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
| 2026-11-02 | The hook events not yet ported answer as unported subcommands (usage text, exit 2), not with 2-13 A's `failed: not in this build yet` notice, so the existing `hook stop` expectations stay; the engine still has `HookError::NotPorted` for them, and the launcher never routes them. | OUT-06, H-12 | two-way (2b-1) |
| 2026-11-02 | `RunEnv`'s temp dir reproduces Node's `os.tmpdir()` from `TMPDIR`, `TMP` and `TEMP` (Windows: `TEMP`, `TMP`, then `%SystemRoot%\temp`), since `std::env::temp_dir` ignores `TMP` and `TEMP` on Unix (2-24); an empty `CLAUDE_PLUGIN_DATA` or `CLAUDE_CODE_REMOTE_SESSION_ID` reads as unset, as `||` does. | OUT-06 | two-way (2-24, 2b) |
| 2026-11-02 | The stdin reader keeps no cause for a failed read: 1.x's notice names none. | ERR-03 | two-way (2b) |
| 2026-11-02 | `hook pre-edit` routes to the engine. The `args` test's example of an unported event, which was `pre-edit`, is now `pre-bash`, and `pre-edit` joins the ported rows: the behaviour pinned (an unported event is an unported subcommand) is unchanged. | H-12, OUT-06 | two-way (2c), flagged for the reviewer |
| 2026-11-02 | `hook pre-bash` routes to the engine. The `args` test's example of an unported event, which was `pre-bash`, is now `pre-handback`, and `pre-bash` joins the ported rows, as in 2c: the behaviour pinned is unchanged. | H-12, OUT-06 | two-way (2d), flagged for the reviewer |
| 2026-11-07 | `temper status` (text) answers here, through the same `command` helper as `scan`, so `main.rs` stays within 100 lines; `--json` waits for 3c. | ARC-05 | two-way (wi-46d916, wi-fe3a19) |
| 2026-11-07 | `status --json` answers here and `status` enters the launcher's `RUST_READY`. The `args` test row that pinned `--json` as unported now pins the JSON form: a changed expectation, as 3b's row was a placeholder for this PR. `emit` writes stderr before stdout (#207 review F1). | H-12, OUT-06 | two-way (wi-8a012b, wi-c30895), flagged for the reviewer |
| 2026-11-08 | `hook session-start` routes to the engine and enters the launcher's `RUST_READY`; the `args` test's list of ported events gains it. `RunEnv`'s plugin root prefers `CLAUDE_PLUGIN_ROOT` as 1.x's `pluginRoot()` does, then the launcher's `TEMPER_PLUGIN_ROOT`; a `current_exe` that fails leaves `TEMPER_BIN` and `PATH` unwritten. | ARC-05 | owner (wi-168ff8); two-way (wi-0a77a1, wi-9771ab) |
