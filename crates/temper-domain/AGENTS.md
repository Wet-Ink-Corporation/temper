# Crate charter: `temper-domain`

## Purpose
temper's pure policy: the masking lexer and scanner rules, the manifest policy, config and glob matching, the verdict and fingerprint rules, the findings, baseline, ratchet and report models with their renderers, and every message text the hooks print (port plan §1.3). Phase 1a brought the scanner's lexer, test regions, counts and first six rules, Phase 1b 40 more rules with their look-around and keep predicates, and Phase 1c the finders and the last regex rules (58 of 60); the rest arrives phase by phase.

## Layer
domain.

## Allowed dependencies
- Workspace crates: none.
- Third-party, each added by the phase that first uses it (D10, wi-b03273): `regex`, `toml` (features `std`, `parse`, `preserve_order`), `sha2`, `serde` (with `derive`), `serde_json`, `thiserror`. Dev only: `proptest`, `insta`.

## Public surface
Re-exported from `lib.rs`, one path each (API-14):
- `Scanner` (`new`, `scan`), `ScanOptions` (`kind`, `file`, `forbid`), `ScanReport` (`hits`, `into_hits`, `tests`, `ignores`, `clones`), `ScannerSetupError` (`rule`, `source`).
- `Hit` (`key`, `id`, `severity`, `line`, `text`, `msg`, `fix`, `exception`), `Severity`, `RuleKey`, `RuleId`, `LineNo`.
- `CodeKind`, `FileRole`, `is_build_script`.

Everything else is `pub(crate)`: the JS helpers in `js` (`js_trim`, `truncate_utf16`, `utf16_window_start`, `translate`), the lexer, regions, counts, rule table, predicates and finders under `scan`. 1d adds `classify_path`, `new_hits` and `format_hit`, and Phase 4 `Scanner::rules()` with `RuleInfo` (wi-ca74b3); §1.3 of the port plan lists the rest.

## Invariants
- No `std::fs`, `std::process`, `std::env`, `SystemTime::now` or threads: every input arrives as a value (ARC-02).
- Output that 1.x writes (hook messages, JSON fields, report text) is byte-identical to 1.x unless §2.4 of the port plan lists the divergence (D16).

## Errors
Typed `thiserror` enums per module, added with the code that needs them (ERR-01, H-09).

## Test strategy
- Unit tests beside the code; property tests (proptest, in use since Phase 1a) for the lexer, the scanner and the glob matcher; `insta` snapshots for rendered reports. Commit `proptest-regressions/` when proptest writes one.
- Scanner-level tests port `tests/scan.test.mjs` in its order, one module per 1.x test group (`src/scan/tests/`, support in `src/scan/tests.rs`); lexer, region and count tests use values computed by running 1.x on the same input.
- Parity with 1.x is checked by the port's harness (§2.2), not here.
- Run: `cargo test -p temper-domain`.

## Do not
- Do not read files, spawn processes or read the clock or the environment; take the value as an argument.
- Do not add a dependency outside the list above without a decision (H-16).

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Four crates plus `xtask`; this is the innermost (D3). | ARC-07, H-16 | owner (wi-0b4dc2) |
| 2026-10-02 | Port the masking lexer and the regex rules faithfully, no `syn` (D8); `regex` plus explicit predicates for the look-around patterns (D9). | H-16 | two-way defaults (wi-2f31ee, wi-e9386f) |
| 2026-10-31 | Phase 1a ships a vertical slice: lexer, test regions and counts with the §1.3 `Scanner` and six rules as their consumer, so no `pub(crate)` helper lands without a caller (no `dead_code` exception, no early `pub`). | ARC-05, H-05, H-16 | two-way (wi-a5a813) |
| 2026-10-31 | `regex` added (D10), with its default features spelled out (`std`, `perf`, `unicode`), so test and release builds compile the same patterns. | ARC-12, H-16 | owner (wi-b03273); features two-way (wi-466849) |
| 2026-10-31 | `thiserror` added for `ScannerSetupError` and every later domain error. | ERR-01, H-16 | pre-approved; two-way (wi-5eed96) |
| 2026-10-31 | `proptest` added as a dev-dependency with `std` only (no `fork`, `timeout`). | TST-06, H-16 | pre-approved; two-way (wi-c0f25b) |
| 2026-10-31 | The rule table keeps 1.x's JS regex sources verbatim and `js::translate` converts them at `Scanner::new`; the JS helpers live in a crate-level `js` module. | CRF-09, ARC-05 | two-way (wi-962170, wi-41c92b) |
| 2026-11-01 | The 1b/1c cut: 1b ports 40 more regex rules; `allow-attr`, `discard-ok`, `empty-err-arm` (with `otherArmBindsErr`) and `ScanOptions::forbid` move to 1c, so each 1.x test is ported whole and `forbid` lands with its only reader. | ARC-05, H-16 | two-way (wi-13609d) |
| 2026-11-01 | Where §1.6's predicate table is not faithful, the look-around rewrites reproduce JS backtracking: `ignore-no-reason` consumes `[\],)]` and a keep trims it, `extern-crate` holds on two or more trailing spaces, and `compiler-fence` drops the redundant `fn` alternative. A rewritten pattern carries its 1.x source in a comment (D8). | CRF-09, H-16 | two-way (wi-31f653) |
| 2026-11-01 | `js::translate` is fallible: it rejects JS constructs that `regex` would read with another meaning, and `ScannerSetupError`'s private source becomes a private `SetupCause` (`Js` or `Regex`). The public surface is unchanged; `source()` no longer downcasts to `regex::Error`. | ERR-01, H-09 | two-way (wi-ef9d9d) |
| 2026-11-01 | The rule data lives in `scan/rules/table.rs`, one ordered slice in 1.x order with a positional `ctx(prod, bin, example, test)`, over ARC-06's soft ~400 lines, so it diffs 1:1 against `scan.mjs`. | ARC-06 | two-way (wi-cbeff6) |
| 2026-11-01 | The 1c/1d cut: `doc-unwrap`, `doc-arg-sections`, doc blocks, `Env::doc` and `is_rust_fence` move to 1d with the 1.x tests that need the whole table; 1c lands 58 of the 60 rules. | ARC-05, H-16 | two-way (wi-9f78ea) |
| 2026-11-01 | `Scanner::rules()` and `RuleInfo` wait for their first non-test reader (the gate's new-code-only set, Phase 4); 1d ports the rule-table test against the in-crate table. | API-14, H-16 | two-way (wi-ca74b3) |
| 2026-11-01 | The "large file" test keeps its hit assertion; its `ms < 10_000` wall-clock bound moves to Phase 1e's corpus CI job as a D16 retired assertion. | TST-10, H-12 | owner (wi-825595) |
| 2026-11-01 | `struct-bounds`' run-time regexes (1.x `new RegExp(esc(name))`) are hand-written matchers, so `scan` compiles nothing and stays total. | H-03, CRF-09 | two-way (wi-5842d0) |
| 2026-11-01 | Comment records carry `kind: CommentKind { Plain, Doc }`, not a `doc: bool`. | API-10 | two-way (wi-91ace5) |
| 2026-11-01 | The scanner tests split into one module per 1.x test group, and `key_cases` into three, bodies unchanged. | ARC-06 | two-way (wi-bd9fd8) |
| 2026-11-01 | `ScanOptions::forbid` is `Option<&'a BTreeSet<String>>`, as §1.3 writes it: the caller lends one set per crate to many scans. | OWN-01, API-14 | two-way (wi-2d7b05) |
| 2026-11-01 | `other_arm_binds_err` lives in `finders/empty_errs.rs` beside `empty-if-let-err`; its patterns are owned by `PredicatePatterns`, which `empty-err-arm`'s keep receives. | ARC-05 | two-way (wi-5f034a) |
