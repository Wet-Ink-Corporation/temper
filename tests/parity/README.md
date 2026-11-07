# tests/parity: the 1.x-against-Rust differential harness

The 2.0 port (`docs/plans/temper-2-rust-port.md` §2.2 item 2) holds the Rust engine to the 1.x Node engine's behaviour, byte for byte, except for the divergences D16 lists. This directory is where that is checked. It is test tooling: Node 22 built-ins and the Python 3.11 standard library only, outside the Cargo workspace. **It is deleted at the cutover, together with the Node code.**

## What is here

| Path | What |
|---|---|
| `lib/engines.mjs` | The hermetic environment, `runEngine` (with optional stdin and extra environment), and the two engines (`node scripts/temper.mjs` and `TEMPER_BIN`) |
| `lib/fixtures.mjs` | `SCAN_FIXTURE` (1d.2's oracle tree plus a BOM, invalid UTF-8 and a long hit line), the "large file", `HOOK_FIXTURES` (the hook cases' trees), tree copies and snapshots |
| `lib/cases.mjs` | One parity-case run: an engine in its own fresh copy, plus the files it wrote; `runHookCase` runs a hook case's steps with stdin, environment, a scratch temp dir and a plugin data dir, and returns what it wrote |
| `lib/normalise.mjs` | The only output rewrites, each citing its decision |
| `lib/diff.mjs` | How a mismatch is explained: the first differing byte, and the hits only one side has |
| `lib/corpus.mjs` | The corpus comparison and the D16(k) per-file bound, with the run and the walk injected |
| `harness.test.mjs` | Tests of `lib/` and of `doc_examples.py --extract`, with stub engines; no binary needed |
| `scan.test.mjs` | The `temper scan` parity cases S1 to S14; needs `TEMPER_BIN` |
| `hooks.test.mjs`, `cases/hooks/*.json` | The `temper hook` parity cases, one JSON list per area (`protocol`, `post-tool`, `user-prompt`, `pre-edit`, `pre-bash`); needs `TEMPER_BIN` |
| `status.test.mjs`, `cases/cli/status.json` | The `temper status` parity cases: the fingerprint corpus, one case per tree shape the fingerprint reads, and the `--json` cases (records, baselines, a running gate, sessions); both engines get `plugin.json`'s version as `TEMPER_PLUGIN_VERSION`; needs `TEMPER_BIN` |
| `corpus.mjs` | The scanner corpus driver (a script, not a node:test file) |
| `test-map.json`, `check_test_map.py` | 1.x engine tests mapped to their Rust tests, one source per 1.x test file: all of `tests/scan.test.mjs`, and the ported part of `tests/toml.test.mjs`, `tests/hooks.test.mjs`, `tests/shell.test.mjs`, `tests/telemetry.test.mjs`, `tests/hooks-registry.test.mjs`, `tests/state-retention.test.mjs`, `tests/handlers.test.mjs`, `tests/status-json.test.mjs` and `tests/gate-progress.test.mjs`, whose other tests a `pending` note assigns to later phases (Phase 8 maps every engine test) |

## Running it

```bash
node --test tests/parity/harness.test.mjs
python3 tests/parity/check_test_map.py --self-test
cargo build -p temper-cli && TEMPER_BIN=target/debug/temper node --test tests/parity/scan.test.mjs
TEMPER_BIN=target/debug/temper node --test tests/parity/hooks.test.mjs
TEMPER_BIN=target/debug/temper node --test tests/parity/status.test.mjs
python3 tests/parity/check_test_map.py

cargo build --release -p temper-cli
python3 tests/doc_examples.py --extract /tmp/doc-blocks
node tests/parity/corpus.mjs --bin target/release/temper \
  --lock Cargo.toml --lock templates/example-workspace/Cargo.toml \
  --root templates/example-workspace --root /tmp/doc-blocks --report /tmp/corpus-report
```

`tests/ci.sh` runs the first two in both modes and the next four in full mode, under the required `Checks` job. `.github/workflows/corpus.yml` runs the scan, hook and status parity cases and the corpus on Linux, Windows and macOS; a hook graduates into the launcher's `RUST_READY` when its cases pass there (OC-5 A, 2-20). That job is advisory (wi-68036c). It runs weekly, on `release/**`, by hand, and on PRs that touch the engines, the scanner's inputs or this directory (wi-295cf1). On failure it uploads `report.json` and `summary.md` for 14 days.

## Rules

- **No silent skip** (H-12). A missing `TEMPER_BIN` or binary, a spawn failure, a timeout, a stream past its buffer, output that is not the JSON it should be, an empty corpus root or a failed `cargo` call fails the test file, or makes `corpus.mjs` exit 2. Nothing is reported as skipped.
- **Same inputs for both engines.** Each case runs each engine in its own fresh copy, with the same arguments, the same cwd and the same hermetic environment: `PATH`, `HOME`, the temp variables, `LANG` and, on Windows, `SystemRoot`, `PATHEXT` and `COMSPEC`, plus `GIT_CONFIG_NOSYSTEM=1` and a `GIT_CEILING_DIRECTORIES` above the root. The tree's snapshot must not change.
- **Bytes decide.** The hit diff only explains a mismatch (1e-6, wi-9817a6). The normalisers are the whole list:
  - `maskRoots`: the temp root, its real path and, on Windows, its other spellings become `<TMP>` (§2.2).
  - `byteOrderHits`: D16(l) (wi-ecb9dd). It re-sorts 1.x's text-form hits as Rust sorts them (by file in UTF-8 byte order, then line, within each severity) and leaves Rust's output alone, so any other order difference still fails. It applies only to text-form cases whose fixture has paths that differ only in case.
  - `dropLoneSurrogateTail`: D16(e). It applies to 1.x's side only, and its count is in the report.
  - `maskTelemetryTimes`: a telemetry row's leading `"t"` time becomes `<T>` (§2.2, "`at` … fields are masked"). The rest of the row is compared as text, so key order is checked (2-14).
  - `maskMarkerTime`: a skill marker's content, an ISO time, becomes `<T>` (§2.2).
  - In `status.test.mjs` only, the detail of a `not valid JSON (...)` message becomes `<detail>`, in the text form and inside a `--json` string alike: D16(a), serde_json's wording against V8's.
  - `maskFailureText`: the text after `temper hook <event> failed: ` becomes `<MSG>` (OC-3 (p)), only in the cases that set `maskFailure` to the expected count; both sides must mask the same number.
- **Hook and CLI cases.** Each case is `{ name, fixture, git?, cwd?, pluginData?, env?, setup?, steps | event + stdin, maskFailure?, stdout?, wrote? }`. A step with `args` runs `temper <args>` instead of a hook (the status cases have only such steps). The steps run in order in one fresh copy, so a marker one writes is read by the next. Each engine gets its own scratch directory whose `tmp/` is `TMPDIR`, `TMP` and `TEMP`, and `pluginData: true` points `CLAUDE_PLUGIN_DATA` at a fresh `data/` there (a string points it at a path in the copy). `setup` actions shape the copy the same way for both engines before the snapshot: `{ git: [args] }` (a fixed identity, no signing, `core.autocrlf` off and author and committer dates one second apart per git call, so both copies get the same commit ids), `{ write: { rel: text | { base64 } } }`, `{ remove: [rel] }` and `{ age: { rel: ms } }`. The placeholders, in `stdin`, `env`, `args` and setup strings: `<ROOT>` is the copy's path (JSON-escaped in `stdin`), `<PID>` this harness's pid, `<DEADPID>` a pid no process has, and `<ISO:-n>` and `<MS:-n>` the case's one `now` minus `n` ms. The comparison covers each step's exit, stdout and stderr, the copy's changed files and every file in the scratch directory, with their contents. `stdout` and `wrote` pin the agreed result. Every Rust step must take under 2 s (OC-4 A).
- **The corpus is a function of the commit and the OS** (1e-2, wi-37a444). On Windows, Python writes the extracted code blocks with CRLF line ends; both engines read the same bytes, so parity is unaffected. It is the registry packages of both lockfiles, the example workspace, the skill's code blocks and the large file, never whatever the registry cache happens to hold.
- **The D16(k) bound** (wi-825595, measured as 1e-5 decided, wi-a24829). A root whose Rust run takes under 10 s bounds every file in it. A root at or over the bound is timed file by file, and a file fails at `ms >= 10_000`. The bound is absolute, with no retry.
