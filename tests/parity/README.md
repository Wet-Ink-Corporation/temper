# tests/parity: the 1.x-against-Rust differential harness

The 2.0 port (`docs/plans/temper-2-rust-port.md` §2.2 item 2) holds the Rust engine to the 1.x Node engine's behaviour, byte for byte, except for the divergences D16 lists. This directory is where that is checked. It is test tooling: Node 22 built-ins and the Python 3.11 standard library only, outside the Cargo workspace. **It is deleted at the cutover, together with the Node code.**

## What is here

| Path | What |
|---|---|
| `lib/engines.mjs` | The hermetic environment, `runEngine`, and the two engines (`node scripts/temper.mjs` and `TEMPER_BIN`) |
| `lib/fixtures.mjs` | `SCAN_FIXTURE` (1d.2's oracle tree plus a BOM, invalid UTF-8 and a long hit line), the "large file", tree copies and snapshots |
| `lib/cases.mjs` | One parity-case run: an engine in its own fresh copy, plus the files it wrote |
| `lib/normalise.mjs` | The only output rewrites, each citing its decision |
| `lib/diff.mjs` | How a mismatch is explained: the first differing byte, and the hits only one side has |
| `lib/corpus.mjs` | The corpus comparison and the D16(k) per-file bound, with the run and the walk injected |
| `harness.test.mjs` | Tests of `lib/` and of `doc_examples.py --extract`, with stub engines; no binary needed |
| `scan.test.mjs` | The `temper scan` parity cases S1 to S13; needs `TEMPER_BIN` |
| `corpus.mjs` | The scanner corpus driver (a script, not a node:test file) |
| `test-map.json`, `check_test_map.py` | Every `tests/scan.test.mjs` test mapped to its Rust tests (Phase 8 adds the other engine tests) |

## Running it

```bash
node --test tests/parity/harness.test.mjs
python3 tests/parity/check_test_map.py --self-test
cargo build -p temper-cli && TEMPER_BIN=target/debug/temper node --test tests/parity/scan.test.mjs
python3 tests/parity/check_test_map.py

cargo build --release -p temper-cli
python3 tests/doc_examples.py --extract /tmp/doc-blocks
node tests/parity/corpus.mjs --bin target/release/temper \
  --lock Cargo.toml --lock templates/example-workspace/Cargo.toml \
  --root templates/example-workspace --root /tmp/doc-blocks --report /tmp/corpus-report
```

`tests/ci.sh` runs the first two in both modes and the next two in full mode, under the required `Checks` job. `.github/workflows/corpus.yml` runs the parity cases and the corpus on Linux, Windows and macOS. That job is advisory (wi-68036c). It runs weekly, on `release/**`, by hand, and on PRs that touch the engines, the scanner's inputs or this directory (wi-295cf1). On failure it uploads `report.json` and `summary.md` for 14 days.

## Rules

- **No silent skip** (H-12). A missing `TEMPER_BIN` or binary, a spawn failure, a timeout, a stream past its buffer, output that is not the JSON it should be, an empty corpus root or a failed `cargo` call fails the test file, or makes `corpus.mjs` exit 2. Nothing is reported as skipped.
- **Same inputs for both engines.** Each case runs each engine in its own fresh copy, with the same arguments, the same cwd and the same hermetic environment: `PATH`, `HOME`, the temp variables, `LANG` and, on Windows, `SystemRoot`, `PATHEXT` and `COMSPEC`, plus `GIT_CONFIG_NOSYSTEM=1` and a `GIT_CEILING_DIRECTORIES` above the root. The tree's snapshot must not change.
- **Bytes decide.** The hit diff only explains a mismatch (1e-6, wi-9817a6). The normalisers are the whole list:
  - `maskRoots`: the temp root, its real path and, on Windows, its other spellings become `<TMP>` (§2.2).
  - `sortHitLines`: D16(l) (wi-ecb9dd). It applies only to text-form cases whose fixture has paths that differ only in case.
  - `dropLoneSurrogateTail`: D16(e). It applies to 1.x's side only, and its count is in the report.
- **The corpus is a function of the commit** (1e-2, wi-37a444). It is the registry packages of both lockfiles, the example workspace, the skill's code blocks and the large file, never whatever the registry cache happens to hold.
- **The D16(k) bound** (wi-825595, measured as 1e-5 decided, wi-a24829). A root whose Rust run takes under 10 s bounds every file in it. A root at or over the bound is timed file by file, and a file fails at `ms >= 10_000`. The bound is absolute, with no retry.
