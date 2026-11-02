# Crate charter: `temper-app`

## Purpose
temper's use cases: one per subcommand and hook handler, and the ports they reach the outside world through (`Files`, `ProcessRunner`, `Clock`), plus the git and cargo command wrappers built on `ProcessRunner` (port plan §1.3). Phase 1d.2 brings the first use case, `temper scan --all` and `temper scan FILES` (`Engine::scan`), with 1.x's project discovery (the git top level first, through `ProcessRunner`), crate lookup and path rules. Phase 2b brings `Engine::hook` with the `post-tool` and `user-prompt` handlers, the skill markers, telemetry with rotation, and the state dir and config in discovery. Phase 2c brings the `pre-edit` handler, with Cargo's workspace-root rules for the manifest checks. Phase 2d brings the `pre-bash` handler's shell-independent checks.

## Layer
application.

## Allowed dependencies
- Workspace crates: `temper-domain`.
- Third-party, added by the phase that first uses it (D10): `serde_json`, `thiserror` (both in since Phase 1d.2). Dev only: `proptest` (in), `insta`, `tempfile`.

## Public surface
Re-exported from `lib.rs`, one path each (API-14):
- `Engine` (`new(ports, env, scanner)`, `scan`, `hook`), `RunEnv` (`cwd`, `style`, `plugin_data`, `temp_dir`, `remote_session`), `ScanArgs` (`targets`, `format`), `ScanTargets` (`All`, `Files`), `OutputFormat` (`Text`, `Json`), `CommandOutput` (`stdout`, `stderr`, `exit`), `Exit` (`Ok`, `Fail`, `Usage`), `CommandError` (`Refused`, `Files`, `Json`).
- The hooks (2b): `HookInput` (`parse`), `HookInputError`, `HookError` (`Files`, `NullInput`, `NotPorted`; since 2c `InputType` and `Glob`; since 2d `Setup`). `Engine::hook` answers `post-tool`, `user-prompt`, since 2c `pre-edit`, and since 2d `pre-bash` without its 2e and 2f branches.
- The ports: `Ports` (`files`, `process`, `clock`), `Files` (`read_text`, `read_dir`, `real_path`; since 2b `stat`, `write_atomic`, `append_line`, `create_dir_all`, `rename`), `FileMeta` (`len`), `DirEntry` (`name`, `kind`), `EntryKind` (`Dir`, `File`, `Symlink`, `Other`), `FsError` (`new`, `op`, `path`, `source`), `FsOp` (`Read`, `ReadDir`, `RealPath`, `Stat`, `Write`, `Append`, `CreateDir`, `Rename`), `Clock` (`now`).
- The process port: `ProcessRunner` (`run`), `ProcessRequest` (`program`, `args`, `cwd`, `stdout_cap`, `stderr_cap`, `timeout`; no `env` until Phase 4, OC-8 B), `ByteCap` (`new`, `get`), `ProcessOutput` (`status`, `stdout`, `stderr`), `ExitKind` (`Code`, `Signal`, `TimedOut`), `Stream` (`Stdout`, `Stderr`), `ProcessError` (`Spawn`, `Io`, `Overflow`).

Everything else is `pub(crate)`: the lexical port of Node's `path.posix` and `path.win32` in `paths` (OC-1 B), `paths::clean_rel` (2-26), `project` (`discover`, `discover_at` with `Workspace`, `crate_of`, `exists`, `declares_package`, `after_line_start`, `is_js_space`), `relpath` (`rel_to`), `git` (`toplevel`, `common_dir`), `scan` (`walk`), `manifests` (`workspace_root_for` with `RootFor`, `own_lints_member`; 2c), `hooks` (dispatch through `HookCall` with `workspace_for_file`, `input`, `skill` with `skill_loaded`, `pre_edit`, `pre_bash`, and `answered` and `logged`, which write a decision's telemetry row before its answer) and `telemetry` (`log`). The in-memory fakes in `fakes` are `#[cfg(test)]` until another crate's tests need them; then a `test-util` feature exposes them (TST-09).

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
| 2026-11-02 | `HookInput` wraps the domain's `JsValue`, not a typed DTO: an exception to OUT-05, because 1.x coerces any JSON type (`String(5)`, `?? {}`) and a DTO would reject input 1.x accepts (port plan 2-10 A). A `null` input is `HookError::NullInput` with V8's text (2-22). | OUT-05 | two-way (2-10 A, 2b) |
| 2026-11-02 | `discover_at` returns a `Workspace { root, state_dir, config }` beside `Project { root }`, so `scan`'s `discover` and its tests are unchanged; the state dir is git's common dir plus `temper`, else `<top>/.git/temper`, else `<root>/target/temper` (2-3 A). | API-14, ARC-05 | two-way (2b) |
| 2026-11-02 | A handler call borrows its ports, run environment and input as one `HookCall`, so `telemetry::log` and the handlers take one argument, not five. | API-10 | two-way (2b) |
| 2026-11-02 | `FileMeta` holds only `len`, the one field 2b reads; `write_atomic` takes `&str`, as every writer is text. | API-14 | two-way (2b) |
| 2026-11-02 | The skill marker's path is `resolve`d against the cwd, not `join`ed: the same file for every `fs` call 1.x makes with a relative `CLAUDE_PLUGIN_DATA`. The handlers check the marker once; 1.x's second `skillLoaded` call reads the same file and gives the same answer. | ARC-05 | two-way (2b) |
| 2026-11-02 | Telemetry and the hooks' failures that 1.x swallows (a failed append, a failed rotation, an unreadable config) are explicit `match` arms producing the value the code goes on with; `log` returns its `Result` and `post_tool` drops it in a commented arm. | H-10 | two-way (2b) |
| 2026-11-02 | `hooks::run` takes the `HookCall` and the engine's compiled `Scanner`, which `pre-edit` reads, instead of building the call itself. | API-10 | two-way (2c) |
| 2026-11-02 | A hook-input field of a type 1.x throws on (a non-string `file_path`, non-iterable `edits`, non-string content scanned as text) is `HookError::InputType` with V8's text and no value (D16(p)); a config glob that does not compile is `HookError::Glob`. Both reach the `failed:` notice, as 1.x's throws do. | ERR-01, ERR-03 | two-way (2c) |
| 2026-11-02 | `pre-edit` omits 1.x's `noteTree` (the `tree-*.json` record) until Phase 3 brings the fingerprint, and graduates then (2-2). Without git or a session id 1.x writes nothing there either, so the parity cases run without git. | ARC-05 | two-way (2-2, 2c) |
| 2026-11-02 | `workspace_root_for` returns `RootFor` (`Itself`, `Member(Manifest)`) without the root's path: the policy reads only the manifest and the relation. | API-14 | two-way (2c) |
| 2026-11-02 | `pre-bash` runs the domain's `BashPolicy` stages in 1.x's order and leaves the reviewer, copy, state-dir and protected-config branches out, with a comment at each place they go (2e, 2f); it stays out of `RUST_READY` until 2f. A pattern that fails to compile is `HookError::Setup`, through the `failed:` notice. `logged` moves from `pre_edit` to `hooks`, beside the new `answered`, so both handlers write a decision's row the same way. | ARC-05, ERR-01 | two-way (2d) |
