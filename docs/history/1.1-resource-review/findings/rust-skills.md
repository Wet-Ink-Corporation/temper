# leonardomso/rust-skills: findings

Source: https://github.com/leonardomso/rust-skills (local clone `scratchpad/res/rust-skills`). License: **MIT** (c) 2025 Leonardo Maldonado. Copying text requires keeping the notice; every proposal below is rewritten, so no attribution is needed.
Read: README.md, SKILL.md (AGENTS.md and CLAUDE.md are symlinks to it), CHANGELOG.md, CONTRIBUTING.md, LICENSE, all 265 files in `rules/`, `.github/workflows/ci.yml`, and all of `checks/` (README, Cargo.toml, rust-toolchain.toml, src/lib.rs, check.sh, gen.py, analyze.py, validate.py, gen_index.py, baseline.txt). I also ran the `checks/` harness on 1.98.1 against a copy, and ran temper's `scan.mjs` over every example it extracted. Scratch crates for the checks below are in `scratchpad/rustskills/v1`–`v9`.

Status counts (primary status per file, 265 files): covered 195 · conflict 21 · skip 23 · refine 10 · tooling 8 · new 5 · pattern 3.
Counting secondary tags as well: conflict 26, refine 13, tooling 9. In 38 files the text is covered but the file's own examples break a settled position; these are flagged `(ex)` in the appendix.

## Already covered (summary)

About three quarters of rust-skills restates guidance that rust-craft already has, usually with stronger enforcement in rust-craft:

- **Ownership:** borrow over clone, `&str`/`&[T]`, Cow, `Arc` ladder, entry API, `mem::take`.
- **API:** naming (`as_`/`to_`/`into_`, no `get_`), newtypes, parse-don't-validate, `#[non_exhaustive]`, sealed traits, typestate/builder, `#[must_use]`, extension traits, Deref abuse.
- **Errors:** thiserror, context chains, lowercase messages, `?`, source chains.
- **Async:** bounded channels, JoinSet, cancel safety, CancellationToken, no lock across await, native async fn in traits.
- **Serde:** rename_all, deny_unknown_fields, tagging, TryFrom validation.
- **Performance:** with_capacity, buffered I/O, `Box` large variants, size asserts, profile tuning, measure first.
- **Modern Rust:** let-else, if-let chains, LazyLock, workspace lints, MSRV.
- **Tests:** cfg(test) modules, proptest, insta, criterion, descriptive names.
- **Observability:** tracing over log, no secrets, error chains.

rust-craft is missing a small set of **std APIs that panic**. Examples of **silent discard that the current lints and scanner do not catch** are also missing. So are a handful of **API-design CONSIDERs** (weakest `Fn` bound, bounds off struct definitions, associated type vs generic parameter, operator overloading) and **build.rs** guidance.

The `anti-*` rules as reviewer checks: 7 of the 15 are fully covered (clone-excessive, index-over-iter, premature-optimize, string-for-str, stringly-typed, type-erasure, vec-for-slice). 4 of the 15 add enforceable detail: empty-catch → C3, collect-intermediate → C5, lock-across-await → C2, over-abstraction → C22. The remaining 4 conflict with H-02 or H-03 (expect-lazy, panic-expected, unwrap-abuse) or are covered but have bad examples (format-hot-path).

## Candidates

Enforcement states refer to the template's `[workspace.lints]`: `all = deny`, `pedantic = warn`, plus the explicit list. Every lint named here was checked with `lintcheck.sh` on 1.98.1 and reported `ok`.

| # | Source | Guidance | Status | Proposed rule text | Enforcement | Value | Why it matters |
|---|---|---|---|---|---|---|---|
| C1 | num-float-compare, num-saturating-clamp, num-nonzero, anti-expect-lazy, doc-panics-section | Some std APIs panic on bad input | refine H-03 (escape-hatches.md "Panicking APIs" table + review checklist) | Add rows. **Integer `/`, `%`**: panic on a zero divisor, and on `MIN / -1` for signed types. Use `checked_div`/`checked_rem`, or take a `NonZeroU*` divisor (`u32 / NonZeroU32` is implemented and cannot panic; it exists for unsigned types only, and signed gives E0277). **`clamp(lo, hi)`**: panics if `lo > hi` (floats also on a NaN bound). Use bounds that are constants or validated at construction. **`sort_by`, `sort_unstable_by`**: "may panic" with a non-total comparator such as `partial_cmp(..).unwrap_or(Equal)`. Use `f64::total_cmp`. **`thread::spawn`, `Scope::spawn`**: panic if the OS refuses a thread. Use `thread::Builder::new().spawn(..)?` or `.spawn_scoped(s, ..)?`. **`Duration::from_secs_f64/f32`, `mul_f64/f32`, `div_f64/f32`**: panic on negative, overflowing or non-finite values. Use `Duration::try_from_secs_f64/f32`. | Mechanical: add to clippy.toml `disallowed-methods` the entries `std::thread::spawn`, `std::thread::Scope::spawn`, `std::time::Duration::{from_secs_f64, from_secs_f32, mul_f64, div_f64}`. `disallowed_methods` is style, so it is already deny via `all`. Verified in v8: all six fired; the compliant forms are clean on 1.98.1 and type-check on 1.96. Note that this also bans them in tests. For `/`, `%`, `clamp` and `sort_by`: review, plus a checklist line. Rejected: `arithmetic_side_effects` and `integer_division` (restriction, not enabled) flag every operator. | high | Under the template lints, all six panicking forms passed `clippy -D warnings` with zero diagnostics (v8). An agent that has been told to avoid `unwrap` writes `partial_cmp(..).unwrap_or(Equal)` or `a / b` and nothing stops it. |
| C2 | obs-instrument-spans, async-watch-latest | Span guards and `watch::Ref` held across `.await` | tooling (H-14, OUT-08) | Also add to H-14's Detect line: "`Span::enter()` guards and `watch::Ref` held across `.await`; instrument futures with `.instrument(span)` or `#[instrument]`." | clippy.toml: `await-holding-invalid-types = [{ path = "tracing::span::Entered", reason = "H-14: use .instrument(span)" }, { path = "tracing::span::EnteredSpan", reason = "…" }, { path = "tokio::sync::watch::Ref", reason = "H-14: clone or copy out, drop before .await" }]`. This drives `await_holding_invalid_type` (suspicious, already deny via `all`). Verified in v3/v4 that it fires, and that it gives no error in crates without tokio or tracing. | high | This is the most common async tracing bug: a span guard held across an await corrupts the span tree. A `watch::Ref` held across an await blocks the writer. `await_holding_lock` catches neither. Zero cost, and no new lint group. |
| C3 | anti-empty-catch | Silent discards that pass the lints | tooling (H-10, ERR-12) | Scanner rules: `empty-err-arm` is `Err(_) => {}` or `Err(_e) => ()`. `empty-if-let-err` is `if let Err(_) = x {}`. Both are hard in prod and bin, soft in tests, and the same class as `discard-ok`. `filter-map-ok` is `filter_map(Result::ok)` or `filter_map(\|x\| f(x).ok())`. `parse-unwrap-or` is `.parse::<T>().unwrap_or(..)` / `_or_else` / `_or_default`. Both of those are soft, telling the author to collect the errors, count and log them, or state why. | scan.mjs `RULES`. The four regexes were prototyped on masked code (`run/proto.mjs`) and matched exactly the intended lines. | high | Verified in v9: `match r { Ok(v) => .., Err(_) => {} }` makes clippy suggest `if let Ok` (still silent). `if let Err(_) = r {}` makes it suggest `.is_err()`. `if let Err(_e) = r { /* c */ }`, `filter_map(\|s\| s.parse().ok())` and `parse().unwrap_or(0)` all pass clean. The lints steer toward new forms of H-10, and the scanner only knows `.ok();`, `let _ =` and `unwrap_or_default`. |
| C4 | proj-build-rs-minimal | build.rs hygiene | new ARC-16 SHOULD + scanner fix | "ARC-16 SHOULD: Keep `build.rs` minimal and deterministic. Emit `cargo::rerun-if-changed` and `cargo::rerun-if-env-changed` for every input, write only under `OUT_DIR`, make no network calls and embed no timestamps, and probe capabilities rather than parsing `rustc --version`. Heavy code generation goes in an `xtask` or checked-in generated code. `main` returns `Result`; emit directives with `writeln!(io::stdout().lock(), ..)?`." | Review. Scanner: `classifyPath` maps `build.rs` to `bin`, where `print` is hard. Verified that clippy exempts build scripts from `print_stdout` but not from `unwrap_used`. Either add a `build` kind with `print: null`, or keep it and document the `writeln!` form (verified clean). | med-high | Without rerun directives Cargo re-runs the script on every build. Today temper would refuse the idiomatic `println!("cargo::…")` in build.rs, and rust-craft gives no guidance there. |
| C5 | anti-collect-intermediate | `collect()` then iterate or `len()` | tooling (PERF-02, OWN-13) | Add `needless_collect` to PERF-02's Enforced-by column. | `needless_collect = "warn"`: nursery, **not enabled, needs enabling**. `redundant_clone` is precedent for an explicitly enabled nursery lint. Verified that it fires on collect-then-len. | med | This turns an existing review-only SHOULD into a lint. |
| C6 | closure-fn-trait-bounds | Weakest closure trait | refine API-08 | "Bound callbacks by the weakest trait the body needs: `FnOnce` if called once, `FnMut` if called repeatedly by one caller, `Fn` only when shared or called concurrently." | review | med | Over-bounding (`Fn` where `FnOnce` suffices) forces callers to clone captured state, which H-08 then flags. Agents default to `Fn`. |
| C7 | type-generic-bounds | Bounds on impls, not types | new API-21 SHOULD | "Put trait bounds on `impl` blocks and functions, not on `struct` or `enum` definitions, unless a field needs them (`T::Assoc`, a `Drop` impl). Bounds on the type spread to every use site." | review (no lint on stable) | med | This is a C-STRUCT-BOUNDS API guideline. Bounds on definitions make every generic domain type (repositories, event envelopes) noisier. |
| C8 | trait-associated-type-vs-generic | Associated type vs type parameter | new API-22 CONSIDER | "Use an associated type when each implementor has exactly one natural choice (`Iterator::Item`, `Decider::Event`); use a type parameter when one type implements the trait many times (`From<T>`)." | review | low-med | This fits the owner's Decider shape (ARC-13): `type Command; type Event; type State;` on a Decider trait. |
| C9 | api-operator-overload | Operator overloading | new API-23 CONSIDER | "Implement `std::ops` only for arithmetic or set semantics. Implement the `OpAssign` and `&T` forms together. Operators cannot return `Result`, so a type that can overflow or be invalid exposes `checked_*` methods and does not implement the panicking operator (H-03)." | review. `suspicious_arithmetic_impl` and `suspicious_op_assign_impl` are suspicious, so already deny. | low-med | Money and quantity value objects in DDD code are the classic case, and `impl Add for Money` hides overflow. |
| C10 | api-impl-fromiterator | Collection traits | refine API-06 | "Collection-like types implement `FromIterator`, `Extend`, and `IntoIterator` for `T`, `&T` and `&mut T` (C-COLLECT)." | Cite `iter_without_into_iter` and `into_iter_without_iter` (pedantic, already warn) in Enforced-by. | low-med | Makes domain collections (event streams, aggregates' child lists) work with `for` and `collect`. |
| C11 | async-spawn-blocking | A number for "blocking" | refine ASY-01 | "Code that runs more than about 100 µs without reaching an `.await` is blocking: move it to `spawn_blocking` or `rayon`, and measure borderline work." | review | med | ASY-01 says "CPU-bound" but gives no threshold. The source says ">1 ms definitely, 10 µs–1 ms consider", which is looser than Tokio's own 10–100 µs guidance. The compatible nugget is to state a number. |
| C12 | mem-drop-order | Drop order | pattern (ownership.md) | Short table: struct fields drop in declaration order, tuple elements in order, locals and parameters in reverse. Order guard fields deliberately, e.g. transaction before connection. | review | low-med | RAII guard ordering (transaction, lock, span) is invisible in code review. rust-craft covers the 2024 temporary-scope changes but not these base rules. |
| C13 | macro-proc-error-spans | Proc-macro errors | pattern (ARC-15, H-02/H-03) | Entry fn: `parse_macro_input!` then `expand(input).unwrap_or_else(syn::Error::into_compile_error).into()`. The inner `fn expand(..) -> syn::Result<TokenStream2>` uses `?` and `syn::Error::new_spanned`. | review | med (only when proc macros exist; the dependency is H-16) | This is how a proc macro satisfies H-02/H-03: it panics nowhere and reports errors with spans. The source's own example calls `parse_macro_input!` inside a `Result` fn and uses `.expect`, so it would be refused. |
| C14 | name-acronym-word | `HttpServer`, not `HTTPServer` | tooling (CRF-01, API-01) | Enforced-by: `upper_case_acronyms` (aggressive). | clippy.toml `upper-case-acronyms-aggressive = true`. `upper_case_acronyms` is style, already deny. This needs `avoid-breaking-exported-api = false`, which the template already sets. Verified in v3 that it catches `HTTPServer`. | low-med | One line, and it enforces RFC 430 casing that is currently review-only. |
| C15 | obs-structured-fields | Fields, not interpolation | tooling (OUT-08) | Scanner soft rule `log-interpolation`: a `tracing` event macro (`trace!`…`error!`) whose message literal contains `{}` or `{name}` gets the advice "use fields: `info!(user_id = %id, \"login\")`". | scan.mjs; must match on raw text, because the lexer masks string contents. | med | Interpolated messages defeat structured querying, and OUT-08 is currently review-only. |
| C16 | obs-library-facade | Libraries don't install subscribers | tooling (OUT-08, ARC-12) | Gate or manifest check: a crate with a `lib` target and no `bin` target must not list `tracing-subscriber`, `env_logger`, `fern` or `simple_logger` in `[dependencies]`. `[dev-dependencies]` are fine. | new manifest check (gate or lint-claims-style test) | low-med | Holds the hexagonal boundary (only the composition root configures output) mechanically. |
| C17 | lint-cfg-check | Declare custom cfgs | tooling (MOD-09) | "When code uses a custom cfg, declare it: `unexpected_cfgs = { level = \"warn\", check-cfg = ['cfg(loom)'] }` in `[workspace.lints.rust]`." | `unexpected_cfgs` is rustc warn-by-default, so already on. This is a declaration, not a level change, but it is a lints-table edit and so asks the human. | low | Without it, the first `cfg(loom)` or `cfg(fuzzing)` fails the `-D warnings` gate and invites an `#[allow]`. |
| C18 | test-loom-concurrency | Model-check sync code | new TST CONSIDER (or ASY-10 note) | "If a crate implements its own synchronization over atomics (which ASY-10 discourages), model-check it with `loom` under `cfg(loom)`." | review; `loom` is a dev-dependency (H-16) | low | Rare under ASY-10, but it is the only real verification for that code. |
| C19 | trait-object-safety | Keep ports dyn-compatible | refine API-09 | "Gate generic or `Self`-returning methods with `where Self: Sized` to keep a trait dyn-compatible, and assert it: `const _: fn(&dyn Port) = \|_\| {};`." | compile-time assertion (verified in v1: E0038 when violated) | low-med | Ports (ARC-03) are often used as `dyn`. A generic method added later breaks every `Box<dyn Port>` far from the change. |
| C20 | own-rc-single-thread | Back-references | refine OWN-11 | "Parent or back references use `Weak`; an `Rc`/`Arc` cycle leaks (H-07)." | review | low | This fills a gap on the sharing ladder. |
| C21 | trait-blanket-impl | Blanket impls are breaking | refine API-17 | "Adding a blanket `impl<T: Bound> Trait for T` to a published trait is a breaking change (H-16)." | review; cargo-semver-checks | low | The break is non-obvious and easy to miss. |
| C22 | anti-over-abstraction | Speculative generality signs | refine CRF-05 (review-rubric checklist) | Checklist: "No single-impl trait that is not a port (a port with one adapter plus a test fake is fine, ARC-03). No method-less marker trait without a sealing or typestate purpose. No type-parameter soup or `where` clauses with 4+ bounds on internal code." | review | low-med | This gives the reviewer concrete anchors for CRF-05. The carve-out keeps it compatible with hexagonal ports; the source's "rule of three" would forbid them. |
| C23 | mem-clone-from | `clone_from` | refine PERF-02 | Enforced-by: add `assigning_clones`. | pedantic, already warn | low | The lint is already on, so this only adds the claim that lint-claims.py then checks. |
| C24 | type-phantom-marker | Typed IDs | pattern (API-02, ARC-13) | `Id<T>` with `PhantomData<fn() -> T>` and manual `Clone`/`Copy`/`Eq`/`Hash`/`Debug` (derives would add `T:` bounds). | review | low-med | One generic ID for every aggregate (`Id<Order>`, `Id<Customer>`). `fn() -> T` keeps the ID `Send + Sync + Copy` even for `Id<Rc<()>>` (verified in v7). |

Review-rubric checklist lines implied by C1, C3 and C22 (Correctness / Errors / Craft):
- "No panicking std API from the escape-hatches table (integer `/` `%`, `clamp`, non-total `sort_by`, `thread::spawn`, float `Duration`). (H-03)"
- "No empty `Err` arm, empty `if let Err`, `filter_map(.. .ok())` or `parse().unwrap_or(..)` without a stated reason. (H-10)"
- "No speculative generality: see C22. (CRF-05)"

## Conflicts

Settled positions: not weakened anywhere below. Each line gives the difference and then any compatible nugget.

- **conflict H-02** (err-no-unwrap-prod, err-expect-bugs-only, anti-expect-lazy, anti-unwrap-abuse): allows `expect("BUG: …")` or `unwrap` for "invariants", `#[allow(clippy::unwrap_used)]` in modules, and `unwrap_or(0)` as the fix. Nugget: anti-expect-lazy's `thread::spawn(..).expect(..)` case becomes C1 (`Builder::spawn`).
- **conflict H-03** (anti-panic-expected, err-result-over-panic, type-never-diverge, doc-panics-section, test-should-panic): `panic!` for violated preconditions, `# Panics` docs as the contract, `-> !` helpers, `#[should_panic]`. Nugget: doc-panics-section's list of panicking std APIs feeds C1. The scanner already flags `should_panic`.
- **conflict H-01** (7 `unsafe-*` files, doc-safety-section, lint-unsafe-doc, opt-bounds-check `get_unchecked`, test-fixture-raii `unsafe { env::set_var }`): unsafe guidance and unsafe examples. Nuggets: opt out of `Send`/`Sync` with a `PhantomData<*const ()>` field instead of `unsafe impl`. Tests inject configuration rather than mutating the environment (ARC-11, TST-10).
- **conflict H-05** (lint-pedantic-selective, lint-workspace-lints, lint-warn-style / -suspicious / -missing-docs examples): `allow` of pedantic lints (`missing_errors_doc`, `too_many_lines`), per-crate `[lints.clippy]` overrides, `#[allow]` examples. Nugget: a per-crate override on top of `lints.workspace = true` is **invalid Cargo** ("cannot override `workspace.lints`", verified), so under MOD-09 overrides are impossible, which supports H-05.
- **conflict H-11** (num-cast-try-from, type-newtype-ids): `as` after a manual range check, and `as i64` on IDs. rust-craft requires `TryFrom`, or `#[expect(clippy::as_conversions, reason)]` with approval.
- **conflict H-09 / anyhow only in binaries** (err-anyhow-app "internal library code: either"; err-thiserror-lib `#[error(transparent)] Other(#[from] anyhow::Error)`; err-result-over-panic `Result<_, &'static str>`; api-from-not-into and serde-try-from-validate with `String` errors).
- **conflict H-10** (anti-empty-catch recommends `let _ =` with a comment; async-oneshot/mpsc/broadcast `let _ = tx.send(..)`; const-vs-static `let _ = CELL.set(..)`; async-cancellation-token `.ok()`). Nugget: anti-empty-catch's list of empty-catch shapes becomes C3. rust-craft's reply idiom (`if tx.send(..).is_err() { /* receiver gone: why */ }`) is already in async-concurrency.md.
- **conflict H-06** (print macros throughout the examples; 158 `print` hits in extracted examples).
- **conflict ARC-06** (proj-mod-rs-dir): recommends `mod.rs` directories, and states `mod_module_files` backwards (the lint bans `mod.rs`). Not settled, but rust-craft's choice is enforced by the template.
- Example-level, non-settled contradictions (covered, marked `(ex)`): `#[async_trait]` in test-mock-traits (ASY-03/MOD-04), `#[doc(hidden)] pub mod` test helpers (API-14), `process::exit` in main in err-anyhow-app and others (ARC-14, `clippy::exit` deny), `once_cell` in anti-premature-optimize (MOD-03), `tests/common/mod.rs` vs `tests/it/` (TST-14), `#[error("… {0}")]` interpolating the source (ERR-05), `mockall` as a default (TST-09 limits mocks to external seams).

## Patterns / examples worth copying

All compile clean under the template lints (`clippy -D warnings`, 1.98.1) unless noted.

build.rs without `println!` (C4, verified in v5):
```rust
use std::io::{self, Write};
fn main() -> io::Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "cargo::rerun-if-changed=build.rs")?;
    writeln!(out, "cargo::rerun-if-changed=proto/events.proto")?;
    writeln!(out, "cargo::rerun-if-env-changed=LEDGER_SCHEMA_VERSION")
}
```

Non-panicking forms for the C1 table (verified in v8; type-checks on 1.96):
```rust
pub fn sort_scores(v: &mut [f64]) { v.sort_by(f64::total_cmp); }
pub fn per_item(total: u32, count: NonZeroU32) -> u32 { total / count }
pub fn ratio(a: i64, b: i64) -> Option<i64> { a.checked_div(b) }
pub fn spawn_worker() -> io::Result<JoinHandle<u32>> {
    thread::Builder::new().name("worker".into()).spawn(|| 1)
}
pub fn dur(x: f64) -> Result<Duration, TryFromFloatSecsError> { Duration::try_from_secs_f64(x) }
```

Typed aggregate ID (C24, verified in v7 with a test asserting `Id<Rc<()>>: Send + Sync + Copy`):
```rust
pub struct Id<T> { raw: u64, _entity: PhantomData<fn() -> T> }
impl<T> Clone for Id<T> { fn clone(&self) -> Self { *self } }
impl<T> Copy for Id<T> {}
impl<T> PartialEq for Id<T> { fn eq(&self, o: &Self) -> bool { self.raw == o.raw } }
impl<T> Eq for Id<T> {}
// Hash and Debug likewise; derives would demand T: Clone + PartialEq + ...
```

Dyn-compatibility assertion for a port (C19, verified in v1: E0038 when violated):
```rust
pub trait EventStore {
    fn append(&self, events: &[Event]) -> Result<(), StoreError>;
    fn load_as<T: From<Event>>(&self) -> Vec<T> where Self: Sized;
}
const _: fn(&dyn EventStore) = |_| {};
```

Proc-macro error reporting (C13, sketch; proc-macro crate not built here):
```rust
#[proc_macro_derive(Command)]
pub fn derive_command(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}
fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let Data::Enum(e) = &input.data else {
        return Err(syn::Error::new_spanned(input, "Command can only be derived for enums"));
    };
    /* ... */
}
```

Drop order (C12):
```rust
struct UnitOfWork {
    tx: Transaction,   // dropped first: rolls back if not committed
    conn: Connection,  // dropped second: the transaction never outlives it
}
```

## Notable skips

- **Miri / unsafe tooling** (unsafe-miri-ci, opt-simd-portable, type-repr-transparent, target-cpu, PGO): there is no unsafe code to check. `target-cpu=native` breaks reproducible artifacts.
- **Extra dependencies**: arrayvec, smallvec, compact_str, thin-vec, ahash as defaults. PERF-06 already requires measurement, and each is an H-16 decision.
- **Low-value or duplicate items**: `#[doc(hidden)]` macro helpers, fragment specifiers, `$crate` paths (lint `crate_in_macro_def` is already on), `@` bindings, prelude modules, numeric formatting, crate README and publish metadata (`clippy::cargo` group rejected as a whole), `-rs` crate names, `chain` micro-optimizations.
- **Nightly-only**: `collect_into`.
- **Thread-local state** (conc-thread-local): global state (ARC-09).
- **Lint suggestions rejected after testing**: `wildcard_enum_match_arm` (restriction) fires on the mandatory `_` arm of foreign `#[non_exhaustive]` enums such as `io::ErrorKind`. `tests_outside_test_module` (restriction) fires on integration tests in `tests/`. `unused_results` (rustc, allow-by-default) is noisy and pushes toward `let _ =`.

## Mechanism (Deliverable B)

### How rust-skills gets agents to apply it

- **Distribution.** `npx add-skill leonardomso/rust-skills` installs it as a standard Agent Skill. AGENTS.md and CLAUDE.md are symlinks to SKILL.md, so tools that read those names get the same file. The README's manual installs for Cursor, Windsurf, Copilot, Zed and Cline copy **SKILL.md only**, so every `rules/*.md` link is dead there. Those agents get the one-line index and nothing else.
- **SKILL.md (499 lines, 37.8 KB) is a full index**:
  - frontmatter description ("Use when writing, reviewing, or refactoring Rust… Invoke with /rust-skills")
  - "When to Apply"
  - a 26-category priority table (CRITICAL → REFERENCE)
  - a Quick Reference with one bullet per rule: `[id](rules/id.md) - summary`
  - a recommended Cargo profile
  - "How to Use" (4 steps)
  - a "Rule Application by Task" table mapping a task to rule prefixes
- **Rule files** are 1.8–5 KB with a fixed shape: `# id`, `> summary`, `## Why It Matters`, `## Bad`, `## Good`, `## See Also`. The `> summary` line is the only trigger. There is no "when to apply" field, and only 81 of 265 files have a "When…" section.
- **Nothing else.** There are no hooks, no gate, no reviewer, no required rule-ID citation, and no enforcement data: only 33 of 265 files name a lint, and none map to a lint configuration. Application depends entirely on the agent choosing to open rule files.

### How it verifies itself

- **`validate.py` (structure):**
  - `# id` matches the filename
  - a `>` summary appears in the first 6 lines
  - Why It Matters and See Also are present
  - every See Also link resolves
  - SKILL.md lists exactly the rule files
- **`gen_index.py --check` (index parity):** regenerates the SKILL.md Quick Reference tables and the README counts from the rule files, and CI fails on drift.
- **`checks/` (examples):**
  - `gen.py` extracts ```` ```rust ```` blocks and **skips every `## Bad` section**. It also skips `#![feature]` blocks, proc-macro blocks, placeholder crates and blocks with `...` pseudocode.
  - Fragments are wrapped in `async fn __ex() -> Result<(), Box<dyn Error>>` under an `#![allow(unused, dead_code, …)]` header, as `examples/` of one crate with about 30 dependencies (tokio, serde, anyhow, thiserror, mockall, proptest, insta, criterion, loom, syn…).
  - `check.sh` runs `cargo check --examples` only: **type-check, no clippy, no run, no tests**.
  - `analyze.py` buckets errors: FRAGMENT (only name-resolution codes such as E0412/E0425/E0432/E0433), ARTIFACT (harness wrapping), LOW (E0282/E0283 inference), and SUSPECT (anything else).
- **`baseline.txt`** holds 124 accepted SUSPECT signatures (`file :: section :: sorted error codes`). CI (`dtolnay/rust-toolchain` 1.95.0, `bash checks/check.sh`) fails only on new signatures. It is a ratchet, not a guarantee that examples compile.

**So, do they compile every example? No.**

My run on 1.98.1, with the toolchain pin removed on a copy, took about 3 minutes. Of 1131 ```rust blocks, gen.py extracted 823. Of those:
- 276 compile clean, which is 24% of all blocks
- 354 are FRAGMENT
- 67 are ARTIFACT
- 1 is LOW
- 125 are SUSPECT: the 124 baselined plus 1 new on 1.98.1, api-newtype-safety Good, E0308

All 241 Bad blocks are never compiled, so nobody checks that a Bad example actually shows the problem.

Two real bugs are hidden by the classifier:
- `use std::assert_matches::assert_matches;` in pat-matches-macro is E0432 and wrong. The correct import is `use std::assert_matches;`, which is what rust-craft ERR-11 says. The harness counted it as a harmless FRAGMENT.
- TOML is never parsed. Two invalid configs ship: a per-crate `[lints.clippy]` next to `workspace = true`, and `lto` in `[profile.release.package."*"]`. Both are Cargo errors (verified).

Running temper's `scan.mjs` over the 823 extracted examples (as prod code, harness header stripped) gives hard hits in 200 blocks (24%) across 123 rule files:

| Hit | Count |
|---|---|
| print | 158 |
| unwrap | 108 |
| unsafe | 48 |
| as-cast | 21 |
| panic | 12 |
| allow-attr | 7 |
| exit | 7 |
| discard-ok | 6 |
| ptr-param | 4 |
| erased-error | 1 |

The Good examples often break the guide's own neighbouring rules.

### Comparison with temper

| Aspect | rust-skills | temper |
|---|---|---|
| Getting applied | Skill description plus a big index. Nothing checks application. | SessionStart/UserPromptSubmit context; PreToolUse scan refuses hard hits on Write/Edit; Bash guard; Stop gate; independent reviewer with a validated verdict JSON. |
| Enforcement data | Mostly none; 33 of 265 files mention a lint. | Every rule has an "Enforced by" column. The template enables those lints. |
| Claims tested | `validate.py` checks structure and links; `gen_index --check` checks index parity. | `tests/lint-claims.py` checks that lints cited in rules.md are enabled by the template. `scan.test.mjs`, evals and e2e `smoke.sh` test the machinery. |
| Examples compiled | Good blocks only; type-check only; failures baselined; 24% of blocks proven. | **None.** 134 `rust` and 81 `rust,ignore` blocks plus 13 `toml` blocks in the references are never compiled, linted or parsed by temper's tests. A scratch prototype (`scratchpad/refcheck`) exists but is not in temper. |
| Registry integrity | Filename equals ID; index generated from files. | 164 IDs in rules.md, but only 156 have `### ID ·` sections (missing: API-07, ERR-12, H-01, H-08, H-14, H-15, H-16, TST-12). No test checks this. |

### Mechanisms temper should adopt

| # | Mechanism | Detail | Value |
|---|---|---|---|
| M1 | **Compile and lint every reference Do-block** | Put every ```` ```rust ```` block in references/ and SKILL.md into a generated workspace that uses the **template's** Cargo.toml, clippy.toml and rust-toolchain. Run `cargo clippy --all-targets -- -D warnings` (not just `check`) plus `cargo +1.96 check` for MSRV. Wrap fragments deliberately, via explicit `# ` hidden-line prelude lines as in rustdoc, instead of classifying errors after the fact. **Never** treat E0432/E0433 on `std`/`core`/`alloc` or known-dependency paths as "fragment"; that is exactly how rust-skills hid the `assert_matches` bug. | high |
| M2 | **Bad-block assertions** | Annotate each `rust,ignore` Don't block with what it must trigger, e.g. ```` ```rust,ignore,expect=clippy::unwrap_used ```` or `expect=scan:discard-ok` or `expect=E0502`. The harness compiles it and asserts that the named lint, scanner key or error code fires. This proves that the Detect lines and "Enforced by" claims are true, which rust-skills never checks. | high |
| M3 | **Extend `lint-claims.py`** | (a) Every "scan" in an Enforced-by cell maps to a `RULES` key with that ID in scan.mjs. (b) Every `RULES[].lints` entry is enabled by the template. (c) Every lint named in a Detect line exists on the pinned toolchain (the `lintcheck.sh` idea) and its group is recorded, so a nursery or restriction lint is flagged unless it is explicitly enabled. | med-high |
| M4 | **Registry parity** | Test that every rules.md ID has a `### ID ·` section in its Detail file (8 are missing today), every ID cited anywhere exists, and every Detail file named exists. Optionally generate SKILL.md §6 tables from rules.md with a `--check` mode, like `gen_index.py --check`. | med |
| M5 | **Toolchain-tied baseline** | The M1/M2 harness runs on the pinned 1.98.1 in CI. A toolchain bump regenerates the harness results in the same commit, like rust-skills' baseline, but with an **empty baseline as the target**: any accepted failure needs a reason line. | med |
| M6 | **Parse every ```` ```toml ```` block** | `tomllib` parse, plus `cargo metadata` for manifest-shaped blocks. This catches invalid config such as the two found in rust-skills. | med |
| M7 | **Scanner over examples** | Run `scan.mjs` on Do blocks (expect zero hard hits) and on Don't blocks (expect the tagged key). Cheap, because the scanner is pure JS. | med |
| M8 | **Keep what temper does better** | Detect lines and the Enforced-by column are tested claims; rust-skills has neither. rust-skills' one-line-per-rule index and task→prefix table are already equivalent to rust-craft SKILL.md §6. Don't copy the 499-line single-file index: its size is why single-file installs lose all detail. rust-craft's 140-line SKILL plus references is the better progressive disclosure. | (keep) |

### SKILL.md structure notes

rust-skills' progressive disclosure is:
1. frontmatter trigger
2. priority table
3. one-line index
4. rule file with Bad/Good

That is sound only when the file links resolve. Its strengths worth mirroring:
- **Every rule file has a Bad/Good pair.** rust-craft has Do/Don't for only about 105 and 81 of 164 IDs.
- **A task → rule-prefix routing table.**

Its weaknesses:
- no trigger field per rule
- no MUST/SHOULD strength
- no enforcement column
- no settled-position framing, so rules contradict each other; for example, anti-empty-catch recommends `let _ =` while other rules reject it
- a single-file fallback that silently drops 95% of the content

## Source inaccuracies (verified on 1.98.1)

- pat-matches-macro: `use std::assert_matches::assert_matches;` is E0432. Since 1.96 the macro is `std::assert_matches` at the crate root.
- lint-workspace-lints: `[lints] workspace = true` plus `[lints.clippy]` is a Cargo error ("cannot override `workspace.lints`").
- perf-release-profile: `lto` in `[profile.release.package."*"]` is a Cargo error ("`lto` may not be specified in a `package` profile").
- proj-mod-rs-dir: says `mod_module_files` enforces `mod.rs`. It is the reverse: the lint bans `mod.rs`.
- lint-clippy-nursery-selected: puts `redundant_else` in nursery. It is pedantic.
- opt-lto-release: describes `lto = "off"` as thin-local. `"off"` disables LTO; thin-local is `lto = false`.
- async-async-fn-bounds: implies `AsyncFn` bounds can require `+ Send` futures. They cannot name the future (rust-craft ASY-12 is right).
- anti-expect-lazy: presents `thread::spawn(..).expect(..)` as the fix. `thread::spawn` itself panics on OS failure.

## Appendix: rule file → rust-craft mapping

`(ex)` means the text is covered but the file's examples break a settled position (see Conflicts). `C#` refers to the Candidates table.

| File | rust-craft ID(s) | Status |
|---|---|---|
| anti-clone-excessive | H-08, OWN-01, OWN-10, PERF-11 | covered |
| anti-collect-intermediate | PERF-02, OWN-13 | tooling (C5) |
| anti-empty-catch | H-10, ERR-08, ERR-12 | tooling (C3); conflict H-10 |
| anti-expect-lazy | H-02 | conflict H-02; refine (C1: thread spawn) |
| anti-format-hot-path | PERF-04 | covered (ex) |
| anti-index-over-iter | H-04, PERF-03 | covered |
| anti-lock-across-await | H-14, ASY-06, ASY-09 | covered (ex) |
| anti-over-abstraction | CRF-05, API-09, ARC-03 | refine (C22) |
| anti-panic-expected | H-03 | conflict H-03 |
| anti-premature-optimize | PERF-01 | covered |
| anti-string-for-str | OWN-03 | covered |
| anti-stringly-typed | API-02, H-09 | covered |
| anti-type-erasure | API-09 | covered |
| anti-unwrap-abuse | H-02, H-05, H-10 | conflict H-02 |
| anti-vec-for-slice | OWN-03 | covered |
| api-builder-must-use | API-11 | covered |
| api-builder-pattern | API-10, API-13 | covered |
| api-common-traits | API-06, OUT-08 | covered |
| api-default-impl | API-06 | covered |
| api-extension-trait | API-18 | covered |
| api-from-not-into | API-07 | covered (ex) |
| api-impl-asref | OWN-01, API-08 | covered |
| api-impl-fromiterator | API-06 | refine (C10) |
| api-impl-into | OWN-02, API-08 | covered |
| api-must-use | API-11 | covered |
| api-newtype-safety | API-02 | covered |
| api-non-exhaustive | API-05 | covered |
| api-operator-overload | none | new API (C9) |
| api-parse-dont-validate | API-03 | covered |
| api-sealed-trait | API-12 | covered |
| api-serde-optional | API-16 | covered |
| api-typestate | API-13 | covered |
| async-async-fn-bounds | ASY-12 | covered (source wrong on Send) |
| async-bounded-channel | ASY-07, H-13 | covered |
| async-broadcast-pubsub | ASY-07 | covered (ex) |
| async-cancel-safety | ASY-05 | covered |
| async-cancellation-token | ASY-04, ASY-05 | covered (ex) |
| async-clone-before-await | OWN-14, H-14 | covered |
| async-fn-in-trait | ASY-03, MOD-04 | covered |
| async-join-parallel | ASY-04, PERF-10 | covered |
| async-joinset-structured | ASY-04 | covered |
| async-mpsc-queue | ASY-07, ASY-09 | covered (ex) |
| async-no-lock-await | H-14, ASY-09 | covered |
| async-oneshot-response | ASY-07, ASY-09, ERR-12 | covered (ex) |
| async-select-racing | ASY-05 | covered (ex) |
| async-spawn-blocking | ASY-01, H-14 | refine (C11) |
| async-tokio-fs | H-14, ASY-06 | covered |
| async-tokio-runtime | ASY-02 | covered (ex) |
| async-try-join | ASY-08 | covered |
| async-watch-latest | H-14, ASY-07 | tooling (C2) |
| closure-disjoint-capture | OWN-14 | covered |
| closure-fn-trait-bounds | API-08 | refine (C6) |
| closure-impl-fn-return | API-08, API-09 | covered |
| closure-move-capture | OWN-14 | covered (ex) |
| closure-static-vs-dyn | API-09, PERF-08 | covered |
| coll-binaryheap | PERF-06 | covered |
| coll-map-choice | PERF-06, OUT-04 | covered |
| coll-seq-choice | PERF-06 | covered (ex) |
| coll-set-membership | PERF-06 | covered |
| conc-atomic-ordering | ASY-10 | covered |
| conc-rayon-par-iter | ASY-01, ASY-10 | covered (ex) |
| conc-scoped-threads | ASY-10, OWN-07 | covered (ex) |
| conc-thread-local | ARC-09, H-07 | skip (static mut already forbidden, TLS is global state) |
| const-block | MOD-10, PERF-05 | covered |
| const-fn | MOD-10 | covered |
| const-generics | MOD-10 | covered |
| const-vs-static | MOD-03, H-07 | covered (ex) |
| conv-asmut-mutable | API-08 | covered |
| conv-fromstr-parsing | API-07 | covered |
| conv-tryfrom-fallible | API-07, API-03 | covered |
| doc-all-public | H-15, API-15 | covered |
| doc-cargo-metadata | API-17 | skip (publishing metadata) |
| doc-crate-readme | API-15 | skip (low value) |
| doc-errors-section | API-15 | covered |
| doc-examples-section | API-15, TST-11 | covered (ex) |
| doc-hidden-setup | TST-11 | covered |
| doc-intra-links | API-15 | covered |
| doc-link-types | API-15 | covered |
| doc-module-inner | H-15 | covered |
| doc-panics-section | H-03 | conflict H-03; refine (C1) |
| doc-question-mark | TST-11 | covered |
| doc-safety-section | H-01 | conflict H-01 |
| err-anyhow-app | ERR-06, H-09 | covered; conflict H-09 |
| err-context-chain | ERR-06 | covered |
| err-custom-type | ERR-01, ERR-02, ERR-03 | covered |
| err-doc-errors | API-15 | covered |
| err-expect-bugs-only | H-02 | conflict H-02 |
| err-from-impl | ERR-04, ERR-05 | covered (ex: interpolates {0} source, vs ERR-05) |
| err-lowercase-msg | ERR-05 | covered |
| err-no-unwrap-prod | H-02, H-05 | conflict H-02 |
| err-question-mark | ERR-07 | covered |
| err-result-over-panic | H-03, H-09 | conflict H-03 |
| err-source-chain | ERR-04 | covered |
| err-thiserror-lib | ERR-01, H-09 | covered; conflict H-09 |
| lint-cargo-metadata | ARC-12 | skip (publish-only, wildcards already in deny.toml) |
| lint-cfg-check | MOD-09 | tooling (C17) |
| lint-clippy-nursery-selected | MOD-09, PERF-12, H-08 | covered (source mislabels groups) |
| lint-deny-correctness | H-15, MOD-09 | covered |
| lint-missing-docs | H-15 | covered (ex) |
| lint-pedantic-selective | H-05, H-15 | conflict H-05 |
| lint-rustfmt-check | CRF-06 | covered |
| lint-unsafe-doc | H-01 | conflict H-01 |
| lint-warn-complexity | MOD-09 | covered |
| lint-warn-perf | MOD-09 | covered |
| lint-warn-style | MOD-09 | covered (ex) |
| lint-warn-suspicious | MOD-09 | covered (ex) |
| lint-workspace-lints | MOD-09, H-05 | conflict H-05 (invalid Cargo) |
| macro-export-crate-path | ARC-15 | skip (low) |
| macro-fragment-specifiers | ARC-15 | skip (low) |
| macro-prefer-functions | ARC-15 | covered |
| macro-private-helpers | API-14 | skip (low) |
| macro-proc-error-spans | H-02, H-03 | pattern (C13) |
| macro-proc-syn-quote | ARC-15 | skip (proc-macro deps are H-16) |
| macro-proc-two-crate | ARC-07 | skip |
| macro-rules-hygiene | ARC-15 | covered (lint crate_in_macro_def, already on) |
| mem-arena-allocator | OWN-11, PERF-02 | covered |
| mem-arrayvec | PERF-02, ARC-12 | skip (dependency) |
| mem-assert-type-size | PERF-05 | covered |
| mem-avoid-format | PERF-04 | covered (ex) |
| mem-box-large-variant | PERF-05 | covered |
| mem-boxed-slice | OWN-15 | covered |
| mem-clone-from | PERF-02 | refine (C23) |
| mem-compact-string | PERF-06, ARC-12 | skip (dependency) |
| mem-drop-order | none | pattern (C12) |
| mem-reuse-collections | PERF-02 | covered (ex) |
| mem-smaller-integers | PERF-05 | covered (ex) |
| mem-smallvec | PERF-02 | covered (ex) |
| mem-take-replace | OWN-08 | covered |
| mem-thinvec | PERF-05 | skip (dependency) |
| mem-with-capacity | PERF-02 | covered |
| mem-write-over-format | PERF-04 | covered (ex) |
| mem-zero-copy | OWN-10, PERF-11, OUT-09 | covered (ex) |
| name-acronym-word | CRF-01, API-01 | tooling (C14) |
| name-as-free | API-01 | covered |
| name-consts-screaming | CRF-01 | covered (ex) |
| name-crate-no-rs | none | skip |
| name-funcs-snake | CRF-01 | covered |
| name-into-ownership | API-01 | covered |
| name-is-has-bool | API-01 | covered |
| name-iter-convention | API-01 | covered |
| name-iter-method | API-01 | covered |
| name-iter-type-match | API-01 | covered |
| name-lifetime-short | OWN-05 | covered (rust-craft prefers meaningful names) |
| name-no-get-prefix | API-01 | covered |
| name-to-expensive | API-01 | covered |
| name-type-param-single | CRF-01 | covered |
| name-types-camel | CRF-01 | covered |
| name-variants-camel | CRF-01 | covered |
| num-cast-try-from | H-11 | conflict H-11 |
| num-float-compare | H-03 | refine (C1) |
| num-nonzero | H-03, API-02 | covered; refine (C1 note) |
| num-overflow-explicit | ERR-09 | covered |
| num-saturating-clamp | ERR-09, H-03 | refine (C1) |
| obs-error-chain | ERR-08, ERR-05 | covered |
| obs-instrument-spans | OUT-08, H-14 | tooling (C2) |
| obs-levels-filter | OUT-08 | covered (ex) |
| obs-library-facade | OUT-08, ARC-12 | tooling (C16) |
| obs-no-sensitive-data | OUT-08 | covered |
| obs-structured-fields | OUT-08 | tooling (C15) |
| obs-tracing-over-log | OUT-08, H-06 | covered |
| opt-bounds-check | H-04, PERF-03 | covered; conflict H-01 (get_unchecked) |
| opt-cache-friendly | PERF-05 | covered |
| opt-codegen-units | PERF-09 | covered |
| opt-cold-unlikely | PERF-08 | covered (ex) |
| opt-inline-always-rare | PERF-08 | covered |
| opt-inline-never-cold | PERF-08 | covered (ex) |
| opt-inline-small | PERF-08 | covered |
| opt-likely-hint | PERF-08 | covered |
| opt-lto-release | PERF-09 | covered (source wrong on lto="off") |
| opt-pgo-profile | PERF-09 | skip |
| opt-simd-portable | PERF-01, H-01 | skip (nightly/unsafe) |
| opt-target-cpu | PERF-09 | skip |
| own-arc-shared | OWN-11, PERF-11 | covered (ex) |
| own-borrow-over-clone | H-08, OWN-01 | covered |
| own-clone-explicit | OWN-16 | covered |
| own-copy-small | OWN-16 | covered |
| own-cow-conditional | OWN-10 | covered |
| own-lifetime-elision | OWN-05 | covered |
| own-move-large | PERF-05, OWN-16 | covered |
| own-mutex-interior | OWN-11, OWN-12 | covered (ex) |
| own-rc-single-thread | OWN-11 | refine (C20) |
| own-refcell-interior | OWN-12 | covered |
| own-rwlock-readers | PERF-12 | covered (ex) |
| own-slice-over-vec | OWN-03 | covered |
| pat-at-bindings | none | skip (low) |
| pat-exhaustive-enum | CRF-03 | covered (its lint suggestion rejected) |
| pat-if-let-chains | MOD-02 | covered |
| pat-let-else | MOD-02, ERR-07 | covered |
| pat-matches-macro | ERR-11, TST-05 | covered (source import path wrong) |
| perf-ahash | PERF-06 | covered |
| perf-black-box-bench | TST-16 | covered |
| perf-chain-avoid | PERF-01 | skip (micro) |
| perf-collect-into | PERF-02 | skip (nightly) |
| perf-collect-once | PERF-02 | covered |
| perf-drain-reuse | PERF-02 | covered |
| perf-entry-api | OWN-09 | covered |
| perf-extend-batch | PERF-03 | covered |
| perf-io-buffering | PERF-07 | covered |
| perf-iter-lazy | PERF-02, OWN-13 | covered |
| perf-iter-over-index | PERF-03 | covered |
| perf-profile-first | PERF-01 | covered |
| perf-release-profile | PERF-09 | covered (source has invalid Cargo) |
| proj-bin-dir | ARC-14 | covered |
| proj-build-rs-minimal | none | new ARC (C4); tooling |
| proj-feature-additive | API-16 | covered |
| proj-flat-small | ARC-06, ARC-07 | covered |
| proj-lib-main-split | ARC-14 | covered |
| proj-mod-by-feature | ARC-07 | covered (uses mod.rs, shared/) |
| proj-mod-rs-dir | ARC-06 | conflict ARC-06 (mod.rs, lint semantics reversed) |
| proj-msrv-declare | MOD-01, ARC-01 | covered |
| proj-prelude-module | API-14 | skip |
| proj-pub-crate-internal | API-14 | covered |
| proj-pub-super-parent | API-14 | covered |
| proj-pub-use-reexport | ARC-05, API-14 | covered |
| proj-workspace-deps | ARC-01, ARC-12 | covered |
| proj-workspace-large | ARC-01 | covered |
| serde-custom-with | OUT-01, OUT-02 | covered |
| serde-default-compat | OUT-02, OUT-03 | covered |
| serde-deny-unknown-fields | OUT-02 | covered |
| serde-enum-representation | OUT-02 | covered |
| serde-flatten | OUT-02 | covered |
| serde-rename-all | OUT-02 | covered |
| serde-skip-empty | OUT-02 | covered |
| serde-try-from-validate | OUT-05 | covered (ex: String error) |
| test-arrange-act-assert | TST-03, TST-05 | covered (ex: is_err-only asserts) |
| test-cfg-test-module | TST-02 | covered |
| test-criterion-bench | TST-16 | covered |
| test-descriptive-names | TST-03 | covered |
| test-doctest-examples | TST-11, TST-08 | covered (ex) |
| test-fixture-raii | TST-10, TST-14 | covered; conflict H-01 (unsafe set_var) |
| test-integration-dir | TST-02, TST-14 | covered (ex: tests/common/mod.rs, vs TST-14 tests/it) |
| test-loom-concurrency | ASY-10 | new TST (C18) |
| test-mock-traits | TST-09, ARC-03 | covered (ex: async_trait, vs ASY-03) |
| test-mockall-mocking | TST-09 | covered (TST-09 limits mocks to external seams) |
| test-proptest-properties | TST-06 | covered |
| test-should-panic | H-03, TST-05 | conflict H-03 |
| test-snapshot-testing | TST-07 | covered |
| test-tokio-async | TST-10 | covered |
| test-use-super | TST-02 | covered |
| trait-associated-type-vs-generic | none | new API (C8) |
| trait-blanket-impl | API-17, API-18 | refine (C21) |
| trait-coherence-newtype | API-02 | covered |
| trait-default-methods | API-18 | covered |
| trait-dyn-vs-generic | API-09 | covered |
| trait-object-safety | API-09 | refine (C19) |
| type-deref-coercion | API-19 | covered |
| type-display-vs-debug | OUT-10 | covered |
| type-enum-states | API-02 | covered |
| type-generic-bounds | none | new API (C7) |
| type-never-diverge | H-03 | conflict H-03 |
| type-newtype-ids | API-02, API-04 | covered (ex: pub field, as cast) |
| type-newtype-validated | API-03, OUT-05 | covered |
| type-no-stringly | API-02 | covered |
| type-numeric-fmt | API-06 | skip (low) |
| type-option-nullable | ERR-07 | covered |
| type-phantom-marker | API-13 | pattern (C24) |
| type-repr-transparent | H-01 | skip (FFI) |
| type-result-fallible | ERR-01 | covered |
| unsafe-extern-block | H-01 | conflict H-01 |
| unsafe-maybeuninit | H-01 | conflict H-01 |
| unsafe-minimize-scope | H-01 | conflict H-01 |
| unsafe-miri-ci | H-01 | skip (no unsafe to check) |
| unsafe-no-mangle-unsafe | H-01 | conflict H-01 |
| unsafe-safety-comment | H-01 | conflict H-01 |
| unsafe-send-sync-manual | H-01 | conflict H-01 (safe nugget: PhantomData opt-out) |
