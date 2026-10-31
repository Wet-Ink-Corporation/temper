# Crate charter: `temper-domain`

## Purpose
temper's pure policy: the masking lexer and scanner rules, the manifest policy, config and glob matching, the verdict and fingerprint rules, the findings, baseline, ratchet and report models with their renderers, and every message text the hooks print (port plan §1.3). Empty until Phase 1 brings the scanner.

## Layer
domain.

## Allowed dependencies
- Workspace crates: none.
- Third-party, each added by the phase that first uses it (D10, wi-b03273): `regex`, `toml` (features `std`, `parse`, `preserve_order`), `sha2`, `serde` (with `derive`), `serde_json`, `thiserror`. Dev only: `proptest`, `insta`.

## Public surface
None yet. Phase 1 adds `Scanner`, `Hit` and `classify_path`; §1.3 of the port plan lists the rest.

## Invariants
- No `std::fs`, `std::process`, `std::env`, `SystemTime::now` or threads: every input arrives as a value (ARC-02).
- Output that 1.x writes (hook messages, JSON fields, report text) is byte-identical to 1.x unless §2.4 of the port plan lists the divergence (D16).

## Errors
Typed `thiserror` enums per module, added with the code that needs them (ERR-01, H-09).

## Test strategy
- Unit tests beside the code; property tests for the lexer and glob matcher; `insta` snapshots for rendered reports.
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
