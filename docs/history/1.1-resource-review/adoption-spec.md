# rust-craft 1.1: adoption spec

This spec is for the agents that apply the decisions taken after reviewing ten external Rust resources. The plugin lives at this repository, and the skill at `skills/rust-craft/`.

**The registry is already updated.** `skills/rust-craft/references/rules.md` is final: 182 rules, 18 of them new. Read it first. Rule text lives there; reference files explain it with examples, and must not contradict it.

## Findings files

Findings are in `scratchpad/findings/`. Tags in this spec point at their rows:

| Tag | File |
|---|---|
| `[ms C#]` | `ms.md` (Microsoft Pragmatic Rust Guidelines, MIT) |
| `[api #]` | `apiguide.md` (Rust API Guidelines, MIT/Apache) |
| `[eff #]` | `effective.md` (Effective Rust, **CC BY-NC-ND**: ideas only, never copy prose or code) |
| `[perf #]` | `perf.md` (Rust Performance Book) |
| `[pat #]` | `patterns.md` (Rust Design Patterns, **MPL-2.0**: paraphrase, never copy) |
| `[atom #]` | `atomics.md` (Rust Atomics and Locks) |
| `[tool CB-#/STD-#/MIRI-#]` | `tooling.md` (Cargo Book, std docs, Miri) |
| `[rs C#/M#]` | `rustskills.md` (leonardomso/rust-skills, MIT) |

The findings contain verified facts and compiling sketches. Scratch crates referenced there are under `scratchpad/<agent>/`. Write everything in your own words. All code must be original and escape-hatch-free.

## Settled positions

Never weaken these:
- No `unsafe`.
- No `unwrap`/`expect`/`panic!` family in production code.
- No panicking indexing.
- No `#[allow]`.
- No `as` casts.
- No print macros outside tests.
- No clone to appease the borrow checker.
- Typed `thiserror` errors in libraries; `anyhow` only in binaries.
- Toolchain 1.98.1, MSRV 1.96, edition 2024, resolver 3.

## Code-block conventions (verified by the new doc-example harness)

| Info string | Meaning | Harness check |
|---|---|---|
| ` ```rust ` | A "Do" example that must compile and be clippy-clean under the **template** lints. | Compiled as its own module file inside one generated library crate. It may define `fn main`, `#[cfg(test)] mod tests` and `#[test]` fns; tests are run. |
| ` ```rust,ignore ` | A "Don't" example. | Not compiled. Add `expect=<diagnostic>` to assert that it fires. Examples: ` ```rust,ignore,expect=clippy::unwrap_used `, ` ```rust,ignore,expect=E0502 `, ` ```rust,ignore,expect=scan:discard-ok `. Several are joined with `+`. |
| ` ```rust,fragment ` | An illustrative Do fragment that cannot sensibly compile, for example because it elides a large surrounding program. | Not compiled, but the scanner must report zero hard hits. Use sparingly: prefer hidden lines. |
| ` ```toml ` | TOML. | Parsed with `tomllib`. |

Rules for ` ```rust ` blocks:
- **Hidden context lines.** Use the rustdoc convention: a line starting with `# ` (hash, space), or a line that is exactly `#`, is compiled but is not part of the lesson. Put imports and stub types there.
- **Crate-level attributes.** No crate-level `#![...]` attributes, because the block is a module. `//!` at the top of a block is fine.
- **Available crates.** std, `thiserror`, `serde` (derive), `serde_json`, `anyhow`, `tracing`, `tracing-subscriber`, `tokio` (full), `tokio-util`, `proptest`, `tempfile`. Anything else goes in a `fragment` or a `rust,ignore` block.
- **Relaxed lints.** The harness relaxes only these, because examples are not a published API: `dead_code`, `unused` (group), `missing_docs`, `unreachable_pub`, `missing_debug_implementations`, `clippy::missing_errors_doc`, `clippy::missing_panics_doc` and `clippy::must_use_candidate`. Everything else in the template applies, including `-D warnings`.

## New and changed rules: where the explanation goes

Each item names its target file and its sources.

- **Explaining a new rule.** Each new rule gets a `### ID · Title` section in its Detail file, following the file's existing section shape:
  - rule restated;
  - Why;
  - Do (a ` ```rust ` block);
  - Don't (a ` ```rust,ignore ` block, with `expect=` when a lint or scanner key exists);
  - Detect;
  - Exceptions, if any.
- **Changed rules.** Update their existing section to match the new registry text.
- **Missing sections.** Registry parity also requires sections for IDs that have none today: API-07, ERR-12, H-01, H-08, H-14, H-15, H-16 and TST-12. Add them in the file named in rules.md's Detail column. For H-rules and TST-12, a short section that points to the fuller text is fine.

### C1: escape-hatches.md, errors.md

**escape-hatches.md**

Panicking-APIs table:
- Apply the correction and additions from `[tool STD-7]` in full.
  - Correction: `Instant - Instant` saturates.
  - Additions: integer `/` and `%` (release too), `ilog*`, `isqrt`, `div_ceil`, `*_euclid`, `sum`/`product`, `Duration` sum and arithmetic, `from_secs_f64` and friends, `SystemTime + Duration`, `swap_remove`, `split_off`, `splice`, `with_capacity` overflow, `copy_from_slice`, `rotate`, `select_nth_unstable`, `String` char-boundary methods, map and deque indexing, `BTreeMap::range` (only when non-empty), non-total `sort_by`, `clamp`, radix functions, `env::args`/`vars`, `thread::spawn`/`Scope::spawn`, `format!` with a failing `Display`, and tokio zero-capacity constructors.
- Also add `[atom 27]`:
  - `Builder::spawn` with a NUL in the name;
  - un-joined panicked scoped threads;
  - `RefCell::borrow`;
  - `LocalKey::with*`;
  - `Condvar` with two mutexes;
  - re-entrant locking;
  - `LazyLock` poison;
  - atomics with a runtime `Ordering`.
- Also add `[perf 22]` (`swap_remove`) and `[eff 1]`, `[eff 2]`, `[eff 28]`.

Other additions:
- A note on clippy suggesting `checked_*(..).unwrap()`: write `.ok_or(..)?` instead `[tool STD-6]`.
- A line that `missing_asserts_for_indexing` must stay off `[perf 4]`.
- The H-04 section:
  - map and deque indexing is not linted `[tool STD-4]`;
  - slice-once bounds hoisting `[perf 4]`, sketch A;
  - `as_chunks` wording `[perf 1]`.
- The H-11 `as` table gets a row for fieldless enum to code via `match` in `From` `[pat 10]`.
- The H-02 poison guidance becomes the three documented choices, plus `clear_poison` (1.77) and `LazyLock` poison being permanent `[atom 26]`.
- In the H-03 section:
  - `debug_assert!` guards (internal invariants only, side-effect free, `debug-assertions` off in shipped profiles, the `debug_assert_with_mut_call` lint) `[perf 12]`;
  - compile-time assertions (`const _: () = assert!(..)`, `const { assert!(..) }`) are fine because they fail at compile time `[perf 2]` `[ms C7]`;
  - assertions carry messages (`missing_assert_message`) `[ms C6]`.
- The H-01 exception checklist for a human-approved FFI or unsafe crate: `-sys` crate plus safe wrapper, `// SAFETY:`, Miri job `[eff 27]` `[tool MIRI-7]`.
- H-12: `cfg_attr(miri, ignore = "reason")`, and never `cfg(not(miri))` to hide tests `[tool MIRI-3]`.
- H-14: tracing span guards and `watch::Ref` across `.await`, via clippy.toml `await-holding-invalid-types` `[rs C2]`.

**errors.md**
- New sections:
  - ERR-13 `[pat 1]` `[api 5]` `[api 8]`: patterns.md #23 is the sketch. Cover `Option` + `take()` because of E0509, `BufWriter` flush, and `process::exit` skipping `Drop`.
  - ERR-14 `[api 1]` `[pat 2]`: sketch P1/24, std precedents.
  - ERR-12: a missing section.
- ERR-07: Option versus Result `[eff 20]`; `or_fun_call` now enabled `[perf 13]` (errors.md currently says it is not enabled; fix that).
- ERR-09:
  - `/`, `%` and `NonZero` divisors (unsigned only) `[eff 2]`;
  - `sum`/`product` `[eff 1]`;
  - atomic `fetch_add` wraps; use `try_update` + `checked_add` `[atom 5]`.
- ERR-06: one application error type per binary `[ms C45]`.
- ERR-01: library errors carry no `Backtrace` field (thiserror on stable gives E0658) `[ms Patterns note]`.
- ERR-04/API-14: third-party `#[from]` makes a dependency public `[api 15]`.

### C2: api-design.md

Fix the internal contradictions:
- API-10 Detect "threshold 5" becomes 4 `[api 33]`.
- The API-01 table's "reserve `get`" becomes `[api 11]`.
- "object-safe" becomes "dyn-compatible" everywhere in the skill `[api 4]`. Grep all references; other agents own the other files, so list the hits you cannot edit in your report.

New sections:
- API-21 `[api 3]` `[rs C7]`, with sketch P5.
- API-22 `[api 10]` `[eff 3]` `[rs C9]`, with sketch P6 / the Cents example.
- API-23 `[ms C28]` `[ms C29]`, with sketches P1/P2.
- API-07 (missing section) plus `[api 24]`.

Refinements:
- API-01:
  - C-CONV cost table `[api 6]`;
  - getters `[api 11]`;
  - named iterator types `[api 9]` (P3);
  - Detect: `iter_without_into_iter`, `into_iter_without_iter`, `iter_not_returning_iterator`.
  - Fix its own `Playlist::iter -> std::slice::Iter` example.
- API-02: type aliases `[eff 8]`; comment-validity and sentinel tests `[eff 9]`.
- API-03: per-module privacy `[pat 9]`; no infallible `From` `[ms C23]`; const-generic compile-time constructor `[ms P3]`.
- API-05:
  - variant-level `#[non_exhaustive]`;
  - never `_priv: ()` (`manual_non_exhaustive`);
  - adding it later is breaking `[pat 3]` `[eff 6]`.
- API-06:
  - `FromIterator`/`Extend`/`size_hint`/Exact/Fused `[ms C34]` `[api 20]` `[perf 16]`;
  - hex/binary formatting for bitmasks `[api 20]`;
  - derived `Ord` follows declaration order `[eff 19]`.
- API-08:
  - sans-IO `[ms C19]` (P6);
  - `RangeBounds` `[ms C20]`;
  - weakest `Fn*` bound `[eff 18]` `[rs C6]`.
- API-09:
  - dyn compatibility, `where Self: Sized` and the const assertion `[api 4]` `[eff 16]` `[rs C19]`;
  - on-stack dyn `[pat 5]`;
  - command-dispatch row `[pat 15]`;
  - nested-generics threshold `[ms C30]`.
- API-10:
  - builder conventions `[ms C31]`; fix patterns.md #4 accordingly (C6 owns patterns.md, so coordinate by stating the contract here);
  - parameter consistency `[ms C33]`.
- API-12: document sealing `[api 27]`.
- API-13: generics as type classes `[pat 7]`.
- API-14:
  - single path, no glob re-exports, no prelude `[ms C1]` `[ms C22]` `[eff 29]`;
  - `#[doc(hidden)]` `[api 26]`;
  - third-party errors `[api 15]`.
- API-15:
  - no `# Arguments`/`# Returns` `[ms C8]` `[eff 4]`;
  - summary length `[ms C9]`;
  - crate-root docs `[api 17]`;
  - intra-doc links `[api 22]`;
  - examples quality `[api 31]`;
  - feature documentation `[tool CB-11]`.
- API-16:
  - names and lints `[api 13]`;
  - no gated `pub` fields or methods `[eff 7]`;
  - `dep:` and `?` syntax `[tool CB-9]`;
  - features declare their prerequisites, and feature-added variants need `#[non_exhaustive]` `[ms C41]`;
  - breaking feature changes `[tool CB-6]`.
- API-17:
  - the full extra breaking list `[tool CB-5]` `[eff 6]` `[rs C21]`;
  - what semver-checks misses;
  - deprecate then remove;
  - change the signature when behaviour changes;
  - CHANGELOG and annotated tags `[api 23]`.
- API-18:
  - minimal required methods `[eff 17]`;
  - associated type versus parameter `[rs C8]`;
  - blanket impl for closures `[pat 6]`;
  - essential functionality inherent (CONSIDER) `[ms C32]`;
  - `renamed_function_params`.
- API-19: associated-function style `[api 28]` `[atom 23]`; collections `Deref` to their view `[pat 11]`.
- ASY-11 lives in async-concurrency.md, not here.

### C3: ownership.md, performance.md

**ownership.md**
- Replace every `thread::spawn`/`Scope::spawn` in examples with `thread::Builder` (ASY-15) `[atom 19]`: `spawn_sum`, `spawn_counter`, `parallel_sum`.
- OWN-01: the `R: Read`/`W: Write` by value exception `[api 2]`.
- OWN-05: tie outputs only to the inputs they borrow from `[eff 21]`.
- OWN-08: `mem_replace_with_default` in Detect `[pat summary]`.
- OWN-11:
  - `Weak`;
  - `into_inner`/`make_mut`/`unwrap_or_clone` `[atom 23]` `[perf 17]`;
  - fix the `Arc<RefCell>` wording `[atom 36]`.
- OWN-12: no `Mutex<()>` `[atom 34]`.
- OWN-13: `needless_collect` is now enabled.
- OWN-14: the capture-block shape `[pat 4]`.
- New section OWN-17 `[api 16]` `[perf 20]` `[ms C37]`.
- Drop-order table `[rs C12]`.
- Compact `u32` handles without `as` `[perf 18]` (sketch D).
- Also fix the `Tokens` iterator's missing `size_hint` if it has one `[perf 16]`.

**performance.md**
- PERF-02: `clone_from`/`clone_into`, `assigning_clones` `[perf 7]`; caller-owned buffer forms `[ms C37]`.
- PERF-03: `as_chunks` fix (the current Do fails the gate) `[perf 1]`; slice-once `[perf 4]`.
- PERF-04: `tracing::enabled!` `[perf 11]`; no per-item events `[ms C15]`.
- PERF-05: 128 B, size assertions on all targets with messages `[perf 21]`; async future size tests `[ms C36]`; avoid nested `Arc` `[ms C38]`.
- PERF-06: the `disallowed-types` opt-in with a foldhash alias `[perf 8]`.
- PERF-07: capped `read_line` loop `[perf 9]` (sketch B); Detect uses `unbuffered_bytes`, not `verbose_file_reads` `[perf 23]`.
- PERF-08: `inline_always` already denied; `cold_path` `[perf 19]`.
- PERF-09:
  - defaults versus per-app table `[perf 14]`;
  - profiles only in the root `[perf 15]` `[tool CB-21]`;
  - dev profile `[perf 5]`;
  - `global_allocator` only in binaries `[perf 6]` `[ms C4]`;
  - target-cpu `[ms C5]`;
  - `overflow-checks` CONSIDER for money domains `[tool CB-20]`;
  - bench profile debug `[ms C47]`.
- PERF-12:
  - lock granularity follows invariants `[perf 10]`;
  - drop guards before blocking `[atom 11]`;
  - RwLock policy and no read-to-write upgrade `[atom 24]`;
  - per-shard counters `[atom 29]`.
- New PERF-13 `[atom 28]`.
- PERF-01:
  - why-comments at optimized code `[perf 24]`;
  - instruction-count benchmarks and hyperfine `[perf 26]`;
  - frame pointers `[perf 27]`;
  - dhat allocation tests `[perf 28]`.
- Small-size special-casing with slice patterns `[perf 25]`.

### C4: async-concurrency.md, modern-rust.md

**async-concurrency.md**
- Replace the atomics table with the cheat sheet and review checklist from `[atom Patterns]`. The current table's "Unsure → SeqCst" is wrong.
- New sections:
  - ASY-13 `[atom 1-8]`;
  - ASY-14 `[atom 9,12,13]` `[eff 5]` `[perf 10]` (verify the edition-2024 scrutinee facts yourself on 1.98.1);
  - ASY-15 `[atom 19-21]`;
  - ASY-16 `[atom 14-17]`, including the lint-clean Condvar shape `[atom 15]`;
  - ASY-17 `[atom 18]`, with loom as an H-16 opt-in needing `check-cfg` `[atom 31]` `[rs C17]`.
- Rewrite the ASY-10 `parallel_sum` example: it panics when two workers panic, and it uses `Scope::spawn` `[atom 20]`.
- ASY-01: the ~100 µs threshold `[rs C11]`; cooperative yielding `[ms C35]`.
- ASY-04: std threads; `JoinError::is_panic` is fatal `[ms C44]` `[atom 21]`.
- ASY-11: raised to SHOULD; `future_not_send`; Send/Sync reasoning and `PhantomData` markers `[atom 22]` `[ms C21]` `[api 14]`.
- Patterns: snapshot swap (RCU) `[atom 25]`; split borrow into role handles `[atom 35]`.
- H-14 section (missing): span guards and `watch::Ref` `[rs C2]`; `Drop` never blocks `[api 8]`.
- Replace "object-safe" with "dyn-compatible".

**modern-rust.md**
- MOD-07: `update`/`try_update` (1.95), `fetch_update` deprecated in 1.99 `[atom 6]`; `cold_path`; `as_chunks`; `floor_char_boundary`.
- Rewrite `cached_or_fill` to bind first `[atom 10]`.
- Make the edition-2024 scrutinee row say plainly that then-blocks, arms and `while let` bodies still hold the guard `[atom 9]`.
- MOD-09: the crate-root lint attribute scanner advisory `[pat 14]`; member `[lints]` inheritance `[tool CB-3]`.
- MOD-01: MSRV policy `[tool CB-26]`; never `--ignore-rust-version` `[tool CB-22]`.
- MOD-13: `build.warnings` covers rustdoc; `--keep-going` `[tool CB-18]`.
- STD-3: check "Experimental" and "since" before using an API `[tool STD-3]`.
- Replace "object-safe" with "dyn-compatible".

### C5: architecture.md, structured-output.md, testing.md

**architecture.md**
- Fix `struct Stamper<C: Clock>` to put the bounds on the impl (API-21) `[api 3]`.
- Replace `build_stamper(..) -> Stamper<Arc<dyn Clock>>` with a `DynClock` wrapper `[ms C28]`.
- ARC-01: workspace sibling dependencies; the `[lints] workspace = true` hook and gate `[ms C39]` `[tool CB-3]`; publish metadata `[api 18]` `[tool CB-16]`.
- ARC-02:
  - `cargo tree --workspace --depth workspace -e normal --target all` `[tool CB-13]`;
  - `#![no_std]` domain crates CONSIDER `[eff 10]`;
  - Miri isolation as a purity probe `[tool MIRI-6]`.
- ARC-07: `--timings` evidence `[tool CB-25]`.
- ARC-09: statics only for caches (the duplicate-version reason) `[ms C25]`; `thread_local!` `[atom 33]`.
- ARC-12:
  - caret floors `[tool CB-7]` `[eff 23]`;
  - `default-features = false` at the workspace level, members add features (it is an error to override) `[tool CB-8]` `[ms C39]`;
  - `[patch]` policy `[tool CB-24]`;
  - major bumps `[eff 12]`;
  - `cargo info` `[tool CB-13]`.
- ARC-14: `global_allocator` and subscriber only at the root `[perf 6]`; `process::exit` skips `Drop` `[pat 17]`.
- ARC-15: the macro mechanics checklist `[eff 15]` `[api 29]` `[ms C42]` `[ms C43]` `[rs C13]`.
- New sections ARC-16 `[tool CB-2]` and ARC-17 `[tool CB-14]` `[rs C4]`, with the build.rs sketch.
- Sketches: a typed aggregate ID `Id<T>` with `PhantomData<fn() -> T>` `[rs C24]`; generated-file freshness test `[tool CB-15]`.
- Library OOBE `[ms C40]` (one SHOULD line under ARC-12).

**structured-output.md**
- OUT-08: named events plus OpenTelemetry keys `[ms C14]`; redaction unit test `[ms C16]`; non-empty `Debug` and `finish_non_exhaustive` `[api 21]`; batch logging `[ms C15]`.
- OUT-10: `Display` is not a serialization format `[pat 16]`.
- Exit codes: enum to code via `match` (H-11) `[pat 10]`.

**testing.md**
- TST-05: tautological tests `[ms C3]` (P5); `assertions_on_result_states` `[ms C11]`.
- TST-06: fuzzing `[eff 13]`.
- TST-09: `test-util` feature `[ms C24]`; dev-dependency cycles `[tool CB-23]`.
- TST-10: fixtures via `CARGO_MANIFEST_DIR` `[tool CB-28]`; blocking tests assert no spinning `[atom 32]`.
- TST-11: the hidden `fn demo` wrapper `[pat 12]`; the `# Ok::<(), E>(())` tail `[api 7]`; the scanner now checks doctest fences.
- TST-12: missing section.
- New TST-17 `[atom 30]` `[tool MIRI-1..6]`:
  - Miri job YAML (the template has it);
  - the aarch64 job snippet;
  - proptest shrinking via `MIRIFLAGS -Zmiri-env-set`;
  - `cfg!(miri)` constants only in test code;
  - which flags never to use.
- New TST-18 `[eff 14]`.
- CI examples run their `main` `[eff 25]`; the latest-deps job `[tool CB-17]` `[eff 26]`.

### C6: patterns.md, process.md, review-rubric.md, SKILL.md-adjacent

**patterns.md**
- Fix #4 so the builder takes required inputs in `Foo::builder(..)` (API-10) `[ms C31]`.
- Add new patterns, continuing the numbering, each with Intent, When, Sketch (a ` ```rust ` block) and Rules applied:
  1. 23 Explicit finish with best-effort `Drop` `[pat P23]`
  2. 24 Hand back the consumed argument `[pat P24]` `[api P1]`
  3. 25 On-stack dyn `[pat P25]`
  4. 26 Closure-friendly role trait `[pat P26]`
  5. 27 Kind-parameterized API `[pat P27]`
  6. 28 Tree walk: Visitor with `ControlFlow`, moving Fold `[pat P28]`
  7. 29 Service handle and named dyn wrapper `[ms P1/P2]`
  8. 30 Snapshot swap (RCU) with `Arc::make_mut` `[atom 25]` `[perf 17]`
  9. 31 Checked value-object arithmetic `[eff Patterns]` `[api P6]`
  10. 32 Typed aggregate id `[rs C24]`
- Keep each pattern short.

**process.md**
- CRF-01:
  - acronyms (clippy.toml aggressive) `[api 12]` `[rs C14]`;
  - no Manager/Factory/Helper/Util `[ms C18]`;
  - word order `[api 25]`;
  - the porting-smells table (C#/Java to Rust) `[ms P7]`.
- CRF-05: speculative generality anchors `[rs C22]`; `cargo machete` in CI.
- CRF-09: "why this value" `[ms C17]`.
- CRF-10: `Option` as iterator and its lints `[pat 13]`.
- New CRF-11 `[ms C2]`.
- PROC-01:
  - read `# Panics`/`# Errors` `[tool STD-1]`;
  - primitive pages `[tool STD-2]`;
  - `cargo info` `[tool CB-13]`.
- PROC-09: typed lets `[eff 30]`.
- PROC-11: CHANGELOG `[api 23]`.
- PROC-12: E0277 two-versions diagnosis `[eff 22]`.
- New PROC-13 `[tool CB-12]`.
- Lookup-table commands (`cargo tree -i -e features`, `cargo info`).

**review-rubric.md**
- Add checklist lines for the new rules, per dimension:
  - correctness: the panicking-APIs table and `map[&k]`;
  - errors: ERR-13, ERR-14, empty `Err` arms;
  - api: API-21, API-22, API-23, single path;
  - concurrency: ASY-13 to ASY-17;
  - craft: CRF-11 and speculative generality;
  - tests: tautological tests and TST-17;
  - docs: no `# Arguments`.
- Update any rule counts.

## Machinery and templates

### W2a: templates/workspace (template only; C-workers must not edit templates)

**`Cargo.toml` `[workspace.lints.rust]`.** Add `ambiguous_negative_literals = "warn"` and `redundant_imports = "warn"`, each with a rule comment.

**`Cargo.toml` `[workspace.lints.clippy]`.** Remove `module_name_repetitions = "allow"`, which is a restriction lint on 1.98.1, so the line is a no-op. Add at `"warn"`, each with a rule-ID comment:

| Lint | Rule | Group |
|---|---|---|
| `needless_collect` | PERF-02/OWN-13 | nursery |
| `or_fun_call` | ERR-07/PERF-04 | nursery |
| `significant_drop_in_scrutinee` | ASY-14 | nursery |
| `debug_assert_with_mut_call` | H-03 | nursery |
| `future_not_send` | ASY-11 | nursery |
| `too_long_first_doc_paragraph` | API-15 | nursery |
| `missing_assert_message` | H-03 | restriction |
| `assertions_on_result_states` | TST-05 | restriction |
| `renamed_function_params` | API-18 | restriction |
| `empty_drop` | ERR-13 | restriction |
| `empty_enum_variants_with_brackets` | none | restriction |
| `negative_feature_names` | API-16 | cargo |
| `redundant_feature_names` | API-16 | cargo |
| `cargo_common_metadata` | ARC-01, publishable crates only | cargo |

Do **not** add `unnameable_types`: it was removed earlier because it fires on the sealed-trait pattern.

**Cargo.toml profiles** `[perf build-config verdict]`:
- `[profile.dev] debug = "line-tables-only"`
- `[profile.dev.package."*"] debug = false`
- `[profile.profiling] inherits = "release", debug = "line-tables-only"`
- `[profile.bench] debug = "line-tables-only"`
- A comment listing the per-application H-16 knobs.

**clippy.toml.**
- Set `upper-case-acronyms-aggressive = true`.
- Add to `disallowed-methods`, each with a reason citing the rule:
  - `std::thread::spawn` and `std::thread::Scope::spawn` (ASY-15)
  - `std::env::args` and `std::env::vars` (H-03)
  - `std::time::Duration::from_secs_f64`, `from_secs_f32`, `mul_f64`, `mul_f32`, `div_f64` and `div_f32` (H-03)
- Add `await-holding-invalid-types` for `tracing::span::Entered`, `tracing::span::EnteredSpan` and `tokio::sync::watch::Ref` (H-14).
- Verify every path fires, and that crates without tokio or tracing get no error (`[rs C2]` says it verified this).

**`.github/workflows/rust.yml`.**
- Add `permissions: contents: read`.
- Pin every action to a full commit SHA with a `# vX` comment. Resolve SHAs with `git ls-remote https://github.com/<owner>/<repo> refs/tags/<tag>`, where GitHub git access works. `dtolnay/rust-toolchain` must not float on `@master`.
- Pass `--locked` to every cargo command.
- Add a step that checks each crate on its own (ARC-16): `cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name' | xargs -I{} cargo check --locked -p {} --lib --bins`.
- Add `cargo hack` feature-powerset checks (API-16) and `cargo machete` (CRF-05) through `taiki-e/install-action`, pinned.
- Add a weekly `latest-deps` job (`continue-on-error: true`) `[tool CB-17]`.
- Add a `miri` job that runs only on `schedule`/`workflow_dispatch`, from `[tool Miri proposed job]` but generic for any workspace. The template does not know crate names, so use `--workspace` plus the documented MIRIFLAGS.
- Keep the MSRV job, with `--locked` added.
- Where possible, verify commands locally on a copy of the example workspace. `cargo install cargo-hack cargo-machete` may work through the proxy; try it.

**Other template files.**
- Add `.github/dependabot.yml` for the cargo and github-actions ecosystems, weekly, grouped. Note in comments that majors are H-16.
- `.claude/rules/cargo.md`:
  - `[lints] workspace = true`;
  - profiles only in the root;
  - caret floors;
  - never edit Cargo.lock;
  - features;
  - build.rs rules (ARC-17).
- `.claude/rules/rust.md`: add a line for ASY-13 to ASY-17, API-21 and API-22, and ERR-13 and ERR-14 in the same terse style.
- `.claude/rules/tests.md`: TST-05 tautology, TST-17 `cfg_attr(miri, ignore = "..")`.
- The root `AGENTS.md` template: keep the iron-rules block in sync with SKILL.md.
- `.cargo/config.toml` may get a commented `target-cpu` example (ms C5); optional.

### W2b: templates/example-workspace (example only)

- Apply the same lint table, profiles and `clippy.toml` as W2a. The example has its own copies.
- Audit the example against every new or changed MUST and SHOULD and fix what fails:
  - API-21 bounds, for example on `Ledger<S, C>`.
  - API-22: a `Cents` `Add`?
  - API-23.
  - ERR-13: the store's writer flush.
  - ERR-14.
  - H-04 map indexing.
  - OWN-01: `W: Write` by value in `output.rs`.
  - API-14 single path.
  - API-15: crate docs with a quick-start and feature list; no `# Arguments`.
  - CRF-11.
  - ARC-16: run `cargo check -p <each> --lib --bins`.
  - The H-11 exit-code mapping.
- Add `#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]` to the CLI end-to-end tests. Make the store tests Miri-ready as `[tool Miri run 5]` did, but without adding Miri config to the code itself.
- Update the charters' decisions logs.
- Make the example gate fully green: `node scripts/temper.mjs gate` from the example directory (or the `cargo` commands in its AGENTS.md), plus `cargo +1.96 check --locked`.
- Keep `target/` out of the final tree.

### W3: scripts (scanner, hooks, gate) and tests/scan.test.mjs

**Scanner (`scripts/lib/scan.mjs`).** Add or fix these rules. Every rule has an id, a fix text naming the rule, per-context severity and a unit test (positive and negative).

Fixes:
- `assert-in-prod`: exempt compile-time assertions `[perf 2]` `[ms C7]`.
- `ignore-no-reason`: also catch `cfg_attr(.., ignore)` `[tool MIRI-2]`.
- `extern-crate`: exempt `extern crate alloc;`/`std;` `[eff 10]` `[api skip note]`.
- `print`: in `build.rs`, use a fix text for the `writeln!(io::stdout().lock(), "cargo::..")?` form `[tool CB-14]` `[rs C4]`.

Hard rules (prod, bin, example; test soft or null as noted):
- `doc-unwrap` (H-02/TST-11): `.unwrap(`/`.expect(`/`panic!`/`todo!` inside ```` ``` ```` fences of `///`/`//!` doc comments. Skip fences tagged `text`, `ignore` or `compile_fail`. Hard in every context except test, where it is soft `[api 7]` regex R2.
- `empty-err-arm` (H-10): `Err(_) => {}`, `Err(_x) => ()` and similar. Hard in prod/bin, soft in test `[rs C3]`.
- `empty-if-let-err` (H-10): `if let Err(_) = .. {}` `[rs C3]`.
- `thread-spawn` (ASY-15): `thread::spawn(`. Hard in prod/bin/example, soft in test (clippy.toml also denies it everywhere).
- `compiler-fence` (ASY-13): hard in prod.
- `global-allocator` (ARC-14): `#[global_allocator]`. Hard in prod (library) context only `[perf 6]`.

Soft (advisory) rules:
- `map-index` (H-04) `[tool STD-4]`
- `filter-map-ok` and `parse-unwrap-or` (H-10) `[rs C3]`
- `seqcst` (ASY-13) and `fetch-update` (MOD-07) `[atom 37]`
- `crate-lint-attr` (MOD-09) `[pat 14]`
- `pub-glob-reexport` and `prelude-mod` (API-14) `[ms C22]`
- `pub-smart-ptr` (API-23) `[ms C28]`
- `struct-bounds` (API-21) `[api R1]`
- `ops-impl` (API-22) `[eff 3]`
- `type-alias-primitive` (API-02) `[eff 8]`
- `get-prefix` (API-01) `[api R3]`
- `doc-arg-sections` (API-15) `[eff 4]`
- `mutex-unit` (OWN-12) `[atom 34]`
- extend `detached-spawn` to std threads `[atom 21]`

**Hooks (`scripts/lib/hooks.mjs`).**

`preEdit` on `Cargo.toml`:
1. Deny a member manifest (it has `[package]`) inside a workspace whose root has `[workspace.lints]` unless its after-text contains a `[lints]` table with `workspace = true`. Cite MOD-09/ARC-01 `[tool CB-3]`.
2. Deny a `[profile` table in a non-root manifest (PERF-09) `[perf 15]`.
3. Ask, as H-12/H-16, when these keys or tables change `[tool CB-4]`:
   - `test`, `doctest`, `harness`, `bench`, `required-features`;
   - `autotests`, `autobenches`, `autoexamples`, `autobins`, `autolib`, `build`;
   - `[patch.*]`, `[replace]`;
   - `rust-version`, `edition`, `resolver`;
   - the `default` feature list.
4. Add an advisory notice for requirements using `=`, `~`, `*`, `<` or a `>=`-only form (ARC-12).

`preEdit`: deny Write/Edit to any `Cargo.lock` (PROC-13) `[tool CB-12]`.

`preBash`:
- Deny `--ignore-rust-version` (MOD-01/H-12) `[tool CB-22]`.
- Extend shell-write detection to `Cargo.lock` (for example `sed -i` or a redirect into it).

**Gate (`scripts/lib/gate.mjs`).** Add a `manifests` step before clippy. For every member, from `cargo metadata --no-deps`, fail when the root defines `[workspace.lints]` and the member lacks `[lints] workspace = true`, or when the member has a `[profile` table. Report the IDs.

**Tests and docs.**
- Keep `tests/scan.test.mjs` green and extend it.
- Add hook unit tests where the structure allows, or a small script test.
- Report the README layer-table changes you would make. The orchestrator owns README.md.

### W1: verification harness (tests/)

**`tests/doc_examples.py`** implements the conventions above over `skills/rust-craft/SKILL.md` and `skills/rust-craft/references/*.md`:
- **Generated workspace.** Build a crate outside the repo, under a temp dir or `--out`. Use the template's `[workspace.lints]`, `clippy.toml` and `rust-toolchain`, plus the relaxations above passed as crate-level `[lints]` overrides in the generated crate. Relaxing lints is allowed in the harness only; document why.
- **Compile.**
  - One module file per ` ```rust ` block.
  - Run `cargo clippy --all-targets --message-format=json -- -D warnings` offline (`--offline`; the registry cache has the crates).
  - Map diagnostics back to `file.md:line`.
  - Run `cargo test` for blocks that contain tests.
- **Don't blocks.** For ` ```rust,ignore,expect=... `, compile each block in its own module, in a second crate or pass, and assert that the named lint, error code or `scan:<key>` fires.
- **Scanner pass (M7).** Run `scan.mjs` over ` ```rust ` and ` ```rust,fragment ` blocks: zero hard hits allowed.
- **TOML (M6).** Parse ` ```toml ` with `tomllib`.
- **Output.**
  - Print a summary: blocks by kind, failures with locations, and Don't blocks without `expect=` (reported, not failing).
  - Exit non-zero on any failure.
  - Support `--file references/x.md` to run one file, and `--keep` to keep the generated crate for debugging.
  - Make it fast: one compile for all Do blocks.

**`tests/registry.py`** (M4):
- Every rules.md ID has exactly one `### ID ·` section across SKILL.md and references.
- Every ID cited anywhere (`[A-Z]{1,4}-\d{2}`) in the skill, agents, the templates' `.claude/rules` and AGENTS.md exists in rules.md.
- Every Detail file named in rules.md exists.
- The rule count stated in SKILL.md and README matches.

**Extend `tests/lint-claims.py`** (M3):
- Every `scan` key cited in a Detect or Enforced-by line maps to a `RULES` id in scan.mjs.
- Every lint named in any reference Detect line, not only rules.md, exists on the toolchain and is enabled by the template, unless the line says it is not enabled or is an opt-in.
- Stay tolerant of lints mentioned as "not enabled", "never enable" or "opt-in". Pick a clear convention and document it.

At first the harness will report many failures in the existing references. That is expected. Do not fix reference content yourself; report the counts per file. The content agents fix their files.
