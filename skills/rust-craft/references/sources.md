# Sources

rust-craft is written in its own words. This file records which outside guidance informed which rules, and which advice was deliberately not adopted. Use it to see why a rule exists, or to see why a well-known recommendation is absent. It is not needed for day-to-day work.

Every claim taken from a source was re-checked on the pinned toolchain (1.98.1, MSRV 1.96) before adoption. Where a source and a settled rust-craft position disagree, the settled position wins: no `unsafe`, no panics or `unwrap` in production code, no `#[allow]`, no lossy `as`, no clone-to-compile, typed library errors.

| Source | License | Use made of it |
|---|---|---|
| [Microsoft Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/agents/all.txt) | MIT | Ideas, paraphrased |
| [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) | MIT / Apache-2.0 | Ideas, paraphrased; the `C-*` checklist is mapped item by item |
| [Effective Rust](https://effective-rust.com/) (David Drysdale) | CC BY-NC-ND 4.0 | Ideas only; no prose or code copied |
| [Rust Design Patterns](https://rust-unofficial.github.io/patterns/) | MPL-2.0 | Ideas only; no prose or code copied |
| [The Rust Performance Book](https://nnethercote.github.io/perf-book/) | MIT / Apache-2.0 | Ideas, paraphrased |
| [Rust Atomics and Locks](https://mara.nl/atomics/) (Mara Bos, O'Reilly) | All rights reserved; free to read online | Ideas only, paraphrased |
| [The Cargo Book](https://doc.rust-lang.org/cargo/) | MIT / Apache-2.0 | Ideas and command lines |
| [The std documentation](https://doc.rust-lang.org/std/) | MIT / Apache-2.0 | `# Panics` sections verified against the 1.98.1 source |
| [Miri](https://github.com/rust-lang/miri) | MIT / Apache-2.0 | CI practice; run against the example workspace |
| [leonardomso/rust-skills](https://github.com/leonardomso/rust-skills) | MIT | Its verification mechanism, and a few gaps |

## What each source contributed

**Microsoft Pragmatic Rust Guidelines**
- One public path per item, no glob re-exports and no preludes (API-14).
- No `Arc`/`Mutex` in public signatures; service handles and a named `Dyn*` wrapper (API-23, patterns 29).
- No design journals or rule tables in docs (CRF-11).
- No tautological tests (TST-05).
- Sans-IO parsers and `RangeBounds` parameters (API-08).
- Builder conventions and parameter order (API-10).
- The nested-generics threshold (API-09).
- Named tracing events, OpenTelemetry keys, and a redaction test (OUT-08).
- A why-this-value doc on constants (CRF-09).
- Cooperative yielding (ASY-01).
- Treating `JoinError::is_panic` as fatal (ASY-04).
- `global_allocator` only in binaries (ARC-14).
- Lint additions:
  - `ambiguous_negative_literals`
  - `redundant_imports`
  - `assertions_on_result_states`
  - `renamed_function_params`
  - `missing_assert_message`
  - `future_not_send`
  - `too_long_first_doc_paragraph`
  - the two feature-name lints

**Rust API Guidelines**
- Bounds on impls, not on type definitions (API-21, C-STRUCT-BOUNDS).
- Operators with their algebraic meaning and no overflow (API-22, C-OVERLOAD).
- Hand the consumed argument back in the error (ERR-14, C-INTERMEDIATE).
- Fallible teardown through `close(self)` (ERR-13, C-DTOR-FAIL and C-DTOR-BLOCK).
- Readers and writers by value (OWN-01, C-RW-VALUE).
- No out-parameters (OWN-17, C-NO-OUT).
- The `as_`/`to_`/`into_` cost table, getters, and named iterator types (API-01).
- The dyn-compatibility guard (API-09, C-OBJECT).
- Crate-root docs and intra-doc links (API-15).
- `upper-case-acronyms-aggressive` (CRF-01).
- `cargo_common_metadata` for crates that are published.

**Effective Rust**
- Integer `sum`/`product` and `/`/`%` panics, and `NonZero` divisors (ERR-09 and the escape-hatches table).
- One lock per invariant, lock order, and no callbacks under a lock (ASY-14).
- No `# Arguments` doc sections (API-15).
- A type alias is not a newtype, and no in-band sentinels (API-02).
- No feature-gated `pub` fields (API-16).
- The semver grey areas (API-17).
- CONSIDER `#![no_std]` domain crates (ARC-02).
- Fuzzing attacker-facing parsers (TST-06).
- Pinning dependency behaviour with tests (TST-18).
- Macro mechanics (ARC-15).
- CI hygiene: pinned action SHAs, `permissions`, and Dependabot.

**Rust Design Patterns**
- Explicit `finish` with a best-effort `Drop`, and returning the consumed argument (ERR-13, ERR-14; patterns 23 and 24).
- Variant-level `#[non_exhaustive]` (API-05).
- Capture blocks for `move` closures (OWN-14).
- On-stack `dyn` (API-09, pattern 25).
- A named trait with a blanket closure impl (API-18, pattern 26).
- Generics as type classes (API-13, pattern 27).
- The visitor and fold tree walk (pattern 28).
- Per-module privacy for invariants (API-03).
- An enum-to-code `match` instead of `as` (H-11).
- The crate-root lint-attribute scanner advisory (MOD-09).

**The Rust Performance Book**
- `as_chunks` in place of `chunks_exact` for constant sizes, and slice-once bounds hoisting (PERF-03).
- `clone_from` and `clone_into` (PERF-02).
- Gating log-only work (PERF-04).
- The 128-byte guide and portable size assertions (PERF-05).
- Profile defaults versus per-application choices, and the dev and profiling profiles (PERF-09).
- `cold_path` (PERF-08).
- A capped `read_line` loop (PERF-07).
- Exempting compile-time assertions from the scanner.
- Enabling the lints rust-craft already cited: `needless_collect`, `or_fun_call` and `debug_assert_with_mut_call`.

**Rust Atomics and Locks**
- The memory-ordering cheat sheet and justification comments (ASY-13).
- Guard temporaries in scrutinees, which edition 2024 did not fix; no re-entrant locking; lock order (ASY-14 and `significant_drop_in_scrutinee`).
- `thread::Builder` instead of the panicking `thread::spawn`, and joining every scoped handle (ASY-10, ASY-15).
- `Condvar::wait_while` and no spinning (ASY-16).
- No home-made primitives (ASY-17).
- Poison handling with `clear_poison` (H-02).
- Cache-line padding (PERF-13).
- Snapshot swap (pattern 30).
- Miri and aarch64 CI for ordering-sensitive crates (TST-17).

**The Cargo Book**
- Crates must build alone despite feature unification (ARC-16).
- Build-script rules (ARC-17).
- Never hand-edit `Cargo.lock`, and `--locked` in CI (PROC-13).
- Member lint inheritance, enforced by a hook and the gate (ARC-01, MOD-09).
- Manifest switches that silence tests (H-12).
- `[patch]` and `rust-version` as human decisions (H-16).
- Caret floors, and `default-features` at the workspace level (ARC-12).
- The full semver catalogue (API-17).
- `cargo info` and `cargo tree` recipes (PROC-01, ARC-02).

**std documentation**
- About 25 panicking APIs that no enabled lint catches (the escape-hatches table and `disallowed-methods`), plus one correction: `Instant - Instant` saturates.
- Map and deque indexing panic without a lint (H-04 scanner advisory).
- Read `# Panics` and `# Errors` before calling (PROC-01).

**Miri**
- The scheduled CI job, `cfg_attr(miri, ignore = "reason")`, proptest shrinking through `MIRIFLAGS`, and the list of flags never to use (TST-17).
- The example workspace passes under Miri with no UB, no leaks and no deadlocks.

**rust-skills**
- The idea of compiling every example. temper's `tests/doc_examples.py` goes further: it clippy-checks every Do block under the template lints, asserts that each annotated Don't block fires its diagnostic, and scans every block.
- Registry parity (`tests/registry.py`).
- Scanner rules for silent `Err` arms (H-10).
- `await-holding-invalid-types` for tracing spans and `watch::Ref` (H-14).
- A typed aggregate id (pattern 32).

## Deliberately not adopted

| Advice | Source | Why not |
|---|---|---|
| Panic on detected programmer errors; `expect("invariant")` | Microsoft, Effective Rust, rust-skills, Atomics | H-02/H-03: encode the invariant in a type, or return an error. Compile-time checks (`const { assert! }`) are the compatible form. |
| `catch_unwind` per request in servers | Microsoft | H-03. Tokio already isolates task panics; treat them as fatal (ASY-04). |
| `anyhow`/`eyre` in internal libraries | Microsoft, rust-skills | H-09. Typed errors in every library crate. |
| An enum with a test-only `Mocked` variant for I/O | Microsoft | TST-09: no test-only branches in production code. Use port traits with fakes. |
| `assert!` to hoist bounds checks; `get_unchecked` | Performance Book | H-03, H-04, H-01. Slice once with `get(..n)`, `first_chunk` or `as_chunks`. |
| Clone for readability or in prototypes | Design Patterns, Effective Rust | H-08. |
| "Prefer small, focused dependencies" | Design Patterns | ARC-12 and H-16: every dependency is a justified decision. |
| `#[doc(hidden)] pub` helpers for doc examples | Design Patterns | API-14. Use a hidden `fn` wrapper instead (TST-11). |
| `_unchecked` getters, and `# Panics`/`# Safety` doc sections | API Guidelines | H-01/H-03: those sections should never be needed. |
| Non-consuming builders as the default | API Guidelines | Consuming builders avoid clones in `build()` (H-08); both forms are documented. |
| `-A deprecated` alongside `-D warnings` | Cargo Book | H-05. Migrate off deprecated APIs in the bump that introduces them. |
| `println!`/`unwrap` in build scripts | Cargo Book | H-02/H-06. Use `writeln!` on a locked stdout, and have `main` return `Result` (ARC-17). |
| `#[cfg_attr(miri, ignore)]` without a reason | Miri README | H-12. The reason string is required and scanned. |
| `unsafe` with `// SAFETY:` comments as normal practice | rust-skills | H-01. The approved-exception checklist is in escape-hatches.md. |
| `as` after a manual range check | rust-skills | H-11: `TryFrom` for integers. Float-to-integer (no `TryFrom<f64>`) goes through the H-05 exception with the range check, as escape-hatches.md shows. |
| `let _ = tx.send(..)` | rust-skills | H-10: match the error and say why it is irrelevant. |
| `mod.rs` directories | rust-skills | ARC-06. |
| Lints: `unnameable_types`, the whole `clippy::cargo` group, `unneeded_field_pattern`, `panic_in_result_fn`, `missing_copy_implementations`, `same_name_method`, `missing_asserts_for_indexing`, workspace-wide `arithmetic_side_effects`, `empty_enum_variants_with_brackets` | several | Each was tested on the template: false positives on sealed traits, tests or serde enums; pressure towards `Copy` or `..` patterns; or the lint automates a conflicting practice. |
