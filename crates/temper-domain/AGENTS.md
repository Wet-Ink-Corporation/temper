# Crate charter: `temper-domain`

## Purpose
temper's pure policy: the masking lexer and scanner rules, the manifest policy, config and glob matching, the verdict and fingerprint rules, the findings, baseline, ratchet and report models with their renderers, and every message text the hooks print (port plan §1.3). Phase 1a brought the scanner's lexer, test regions, counts and first six rules; the rest arrives phase by phase.

## Layer
domain.

## Allowed dependencies
- Workspace crates: none.
- Third-party, each added by the phase that first uses it (D10, wi-b03273): `regex`, `toml` (features `std`, `parse`, `preserve_order`), `sha2`, `serde` (with `derive`), `serde_json`, `thiserror`. Dev only: `proptest`, `insta`.

## Public surface
Re-exported from `lib.rs`, one path each (API-14):
- `Scanner` (`new`, `scan`), `ScanOptions` (`kind`, `file`), `ScanReport` (`hits`, `into_hits`, `tests`, `ignores`, `clones`), `ScannerSetupError` (`rule`, `source`).
- `Hit` (`key`, `id`, `severity`, `line`, `text`, `msg`, `fix`, `exception`), `Severity`, `RuleKey`, `RuleId`, `LineNo`.
- `CodeKind`, `FileRole`, `is_build_script`.

Everything else is `pub(crate)`: the JS helpers in `js` (`js_trim`, `truncate_utf16`, `translate`), the lexer, regions, counts, rule table and finders under `scan`. 1b adds `ScanOptions::forbid`, 1c `Scanner::rules()`, 1d `classify_path`, `new_hits` and `format_hit`; §1.3 of the port plan lists the rest.

## Invariants
- No `std::fs`, `std::process`, `std::env`, `SystemTime::now` or threads: every input arrives as a value (ARC-02).
- Output that 1.x writes (hook messages, JSON fields, report text) is byte-identical to 1.x unless §2.4 of the port plan lists the divergence (D16).

## Errors
Typed `thiserror` enums per module, added with the code that needs them (ERR-01, H-09).

## Test strategy
- Unit tests beside the code; property tests (proptest, in use since Phase 1a) for the lexer, the scanner and the glob matcher; `insta` snapshots for rendered reports. Commit `proptest-regressions/` when proptest writes one.
- Scanner-level tests port `tests/scan.test.mjs` in its order (`src/scan/tests.rs`); lexer, region and count tests use values computed by running 1.x on the same input.
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
