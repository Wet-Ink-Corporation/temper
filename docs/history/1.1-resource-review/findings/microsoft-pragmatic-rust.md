# Microsoft Pragmatic Rust Guidelines (agents/all.txt): findings

Source: https://microsoft.github.io/rust-guidelines/agents/all.txt (local copy `res/ms-guidelines.txt`, 3,529 lines). Read: the whole file, all 90 `M-*` guidelines in 13 chapters (AI, Application, Correctness, Documentation, FFI, Macros, Performance, Project, Universal, Libraries/Building, Libraries/Interop, Libraries/Resilience, Libraries/UX; the "Library Guidelines" and "Safety" chapter headers are empty). Line numbers below (`L…`) refer to all.txt. Three IDs are referenced but never defined in this text: `M-RUNTIME-ABSTRACTED`, `M-ABSTRACTIONS-DONT-NEST` (the anchor used for M-SIMPLE-ABSTRACTIONS) and `M-DOC-FIRST-SENTENCE` (the anchor used for M-FIRST-DOC-SENTENCE).

**License.** Line 1: `Copyright (c) Microsoft Corporation. Licensed under the MIT license.` Paraphrasing and short quotes with attribution are fine; copying larger blocks needs the MIT notice.

Checked against: rust-craft `SKILL.md`, `rules.md`, `api-design.md` (in full), and the relevant sections of `errors.md`, `architecture.md`, `testing.md`, `performance.md`, `structured-output.md`, `async-concurrency.md`, `escape-hatches.md`, `modern-rust.md`, `process.md`, `patterns.md`; the template `Cargo.toml`, `clippy.toml`, `deny.toml`, `rust.yml`, `.cargo/config.toml`; and `scan.mjs`.

Toolchain facts were checked on 1.98.1 in `scratchpad/ms/` (a copy of the template workspace with its lints, plus the candidate lints). Every lint named in this file prints `ok` with `lintcheck.sh`.

## Already covered (summary)

- M-DESIGN-FOR-AI -> API-01, API-02, API-15, TST-09, TST-11, TST-13.
- M-LINT-OVERRIDE-EXPECT -> H-05, MOD-05. rust-craft is stricter: `allow_attributes` is denied, and `#[expect]` needs human approval.
- M-LOG-NOT-PRINT -> H-06.
- M-PUBLIC-DISPLAY -> API-06, OUT-10.
- M-DONT-LEAK-TYPES -> API-14, ARC-03.
- M-IMPL-ASREF -> API-08, OWN-01, OWN-02. OWN-02's owned storage already rules out `struct User<T: AsRef<str>>`.
- M-ASYNC-FN -> ASY-03, plus `clippy::manual_async_fn` (style, already denied through `all`).
- M-BALANCED-MODULES -> ARC-05, ARC-06 ("never by kind (`types.rs`, `traits.rs`)"), API-14.
- M-INIT-CASCADED -> API-10 (`too_many_arguments` 4), API-02.
- M-BUILD-RESULT -> patterns.md #4 (infallible setters, `build() -> Result`).
- M-FAST-HASHER -> PERF-06.
- M-INITIAL-CAPACITY -> PERF-02.
- M-SHRINK-TO-FIT -> PERF-02 note.
- M-BOX-DST -> OWN-15 and `clippy::rc_buffer`. MS's thresholds (thousands of instances, immutable, not user-visible) could go into OWN-15's text.
- M-HOTPATH -> PERF-01, TST-16, PERF-09 `[profile.profiling]`. See C47 for the bench profile.
- M-LATEST-EDITION -> MOD-01, ARC-01.
- M-CRATES-FLAT-FOLDER -> ARC-01 (members under `crates/`).
- M-SMALLER-CRATES -> ARC-07, which already lists compile time and visibility as split criteria.
- M-MACRO-LAST-RESORT -> ARC-15.
- M-STRONG-TYPES -> OWN-03, API-08 (`Path` for OS paths).
- M-FFI-TRANSLATES (principle) -> ARC-03.
- M-ERRORS-CANONICAL-STRUCTS -> ERR-01, ERR-02 (opaque struct plus `kind()`, the `std::io::Error` shape), ERR-03. See Notable skips for the parts that differ.
- M-FROM-ERROR -> ERR-04. rust-craft is deliberately stricter: `map_err` names the step when two calls share a source type.
- M-UPSTREAM-GUIDELINES:
  - C-CONV and C-GETTER -> API-01.
  - C-COMMON-TRAITS -> API-06.
  - C-CTOR -> API-01 `new`, plus `clippy::new_without_default`.
  - C-FEATURE -> the lint in C12.
- M-PANIC-IS-STOP ("panics never communicate errors; never assume they are caught") -> H-03. Its list of valid panics is a conflict; see Conflicts.
- M-DI-HIERARCHY ("generics before `dyn`", "narrow traits plus a supertrait") -> API-09, API-18, and the architecture.md DI section.

## Candidates

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement (lint + level / clippy.toml / scanner regex / review / template / CI) | Value (high/med/low) | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| C1 | AI / `#M-SINGLE-ITEM-PATH` (L104-134) | A public item is reachable through one path only. Agents leave old re-exports behind when they refactor: `pub mod db` plus `pub use db::Connection` gives two paths. | refine API-14 | MUST: every public item has exactly one public path. A module whose items are re-exported is private (`mod x;` or `pub(crate) mod x;`), never `pub mod x;` plus `pub use x::Item;`. A change that moves an item removes the old path in the same diff (an H-16 decision for published crates). | Review. Scanner advisory (a two-pattern check per file): `pub mod (\w+);` and `pub use (?:self::\|crate::)?\1::` in the same file -> soft. Symptom in `cargo doc`: crate-local items listed under "Re-exports". Verified: rustdoc inlines re-exports from private and `pub(crate)` modules automatically, and lists only re-exports from a `pub mod`. No clippy lint catches it; the probe crate with both paths was lint-clean. | high | ARC-05 flags only "`pub mod` without a reason". Duplicate paths after iterative refactors are a failure agents repeat. |
| C2 | AI / `#M-NO-META-DESIGN-DOCUMENTATION` (L57-76) | Document the end state, not the design journey. Agents append "why X over Y" essays and tables of the rules they applied. | new PROC (or extend CRF-04) | MUST: rustdoc, READMEs and code comments describe current behavior, contracts and lasting design goals. Design history, rejected alternatives and rule-compliance self-reports (tables of rule IDs "applied") go in the plan (`.temper/plans/`) or the charter's decisions log (ARC-08), never in rustdoc, READMEs or comments. A comment cites a rule ID only inside a why-comment. | Review (rubric docs dimension). Optional scanner advisory on comments: `\|\s*(?:Rule\|Guideline)\s*\|` or `✅`. | high | SKILL.md tells implementers to cite rule IDs in plans and self-review notes. Without this rule, agents leak those citations and narratives into shipped docs, and MS's example of this failure uses MS's own rule IDs. |
| C3 | AI / `#M-TAUTOLOGICAL-TESTS` (L138-157) | A test must not restate a constant or mirror the implementation's branches. Test a property instead. If such a test exists only to kill a mutant, skip the mutant. | refine TST-05 (plus a TST-13 note and a testing.md smells row) | MUST NOT: write a test whose expected value copies the constant or the implementation's own branches (`assert_eq!(CHECKPOINTS, [0, 90, 180, 270])`). Assert a property the value must have (evenly spaced, monotonic, round-trips) or the behavior it drives. When only such a test could kill a mutant, the mutant is definitional: exclude it with a reason (TST-13) rather than writing the test. | Review; add a row to the smells table. | high | The smells table covers "expected value computed by the same logic" but not literal restatement. Agents chasing coverage or mutants generate these tests. |
| C4 | Application / `#M-MIMALLOC-APPS` (L198-219) | Applications set mimalloc as the global allocator. MS reports up to 25% gains on allocation-heavy benchmarks. | new PERF (CONSIDER) | CONSIDER: a binary with allocation-heavy hot paths benchmarks `mimalloc` as its `#[global_allocator]` (two lines in `main.rs`) and adopts it when the benchmark shows a win. Libraries never set a global allocator. Adding it is an H-16 dependency decision. | Review; the PERF-01/TST-16 benchmark. | med | Verified: `#[global_allocator] static A: std::alloc::System = std::alloc::System;` compiles under `unsafe_code = "forbid"`, so the swap needs no H-01 exception; mimalloc's own `unsafe` lives in the dependency. Not verified offline: the crate is MIT (passes deny.toml), and libmimalloc-sys compiles vendored C through `cc`. |
| C5 | Application / `#M-TARGET-CPU` (L223-241) | Server applications compile for the highest `target-cpu` the fleet guarantees, such as `x86-64-v3`. | new PERF (CONSIDER) | CONSIDER: a server binary deployed only to known hardware sets `rustflags = ["-C", "target-cpu=<level>"]` for that target in `.cargo/config.toml`. The CPU level is a deployment decision (H-16). Never do this for distributed CLIs or libraries. | Template: a commented example in `.cargo/config.toml`. | low | Too high a level crashes older hosts with SIGILL, so this stays a human call. |
| C6 | Correctness / `#M-PANIC-MESSAGE` (L316-332) | Every intentional panic or assert carries a message with the relevant values. Tests don't need one. | refine H-03 (escape-hatches.md) plus tooling | SHOULD: the assertions rust-craft still allows in production code (`debug_assert!`, inline `const { assert!(..) }`, `const _: () = assert!(..)`) carry a message naming the violated condition, and the values for runtime ones. | `clippy::missing_assert_message` (restriction, not on): enable at `warn`. Verified: fires on `debug_assert!(x > 0)` and on `const _: () = assert!(..)`; skips `#[test]` functions and `#[cfg(test)]` modules. | low | Cheap. performance.md's own `const _: () = assert!(size_of::<Message>() <= 16);` would need a message. |
| C7 | Correctness / `#M-PANIC-ON-BUG` "Correct by Construction" tip (L370-374) and `#M-STRONG-TYPES-GUARD` "Why `const`?" (L2751-2758) | Surface contract violations at compile time rather than at run time. | tooling (scanner) | The scanner's `assert-in-prod` rule must not fire on compile-time assertions: skip matches in `const { … }` blocks and in `const _: () = assert!(…)` items. | scan.mjs: exempt a match preceded by `const\s*\{\s*` or `const\s+_\s*:\s*\(\)\s*=\s*`, or skip matches inside const blocks with the masked-code index. | med-high | Verified by running `scanSource`: it soft-flags both `const _: () = assert!(size_of::<u64>() <= 16);` and `const { assert!(N > 0, "…") };`. Those are exactly the idioms PERF-05 and MOD-10 prescribe, so agents get contradictory feedback. |
| C8 | Documentation / `#M-CANONICAL-DOCS` (L527-577) | Use the canonical sections (summary, Examples, Errors, Panics, Safety, Abort). No parameter tables: explain parameters in prose. | refine API-15 | MUST NOT: write `# Arguments`, `# Parameters` or `# Returns` sections, or bullet lists of parameters. Name parameters in the prose ("Copies a file from `src` to `dst`."). The allowed headings are `# Examples`, `# Errors`, `# Panics` (never needed under H-03) and `# Safety` (never needed under H-01). | Scanner advisory on doc comments: `^\s*//[/!]\s*#\s*(?:Arguments\|Parameters\|Params\|Returns)\b` -> soft. Review. | med | Agents trained on Python, Java or JSDoc emit `# Arguments` tables. API-15 says what to include, not what to leave out. |
| C9 | Documentation / `#M-FIRST-DOC-SENTENCE` (L615-640) and `#M-MODULE-DOCS` (L644-676) | The summary sentence fits on one line, about 15 words. Every public module has `//!` docs covering its contents, when to use it (and when not), examples and side effects. | refine API-15 | SHOULD: the first sentence is one line of at most about 15 words, because it becomes the module-index summary. Every `pub` module and the crate root open with `//!` saying what the module holds and when to use it. | `clippy::too_long_first_doc_paragraph` (nursery, not on): enable at `warn` as a coarse backstop. Verified: it fired only on a first paragraph wrapped over three lines (~230 chars); it stayed silent on a single-line ~240-char paragraph and on shorter two-line ones, so it doesn't count words. `missing_docs` already requires the `//!`; the 15-word check is review. | med | API-15 says "one sentence" with no length. Agents write paragraph-long summaries. |
| C10 | Universal / `#M-STATIC-VERIFICATION` compiler lints (L1919-1933) | Enable 7 rustc lints (details in the lint diff below). | tooling | Add to `[workspace.lints.rust]`: `ambiguous_negative_literals = "warn"` and `redundant_imports = "warn"`. The other five are already on or moot. | template | med | Both are allow-by-default and free. Verified: `-1i32.abs()` and `use std::option::Option;` are flagged. |
| C11 | Universal / `#M-STATIC-VERIFICATION` restriction lints (L1951-1974) | Enable a list of restriction and nursery lints for consistency. | tooling | Add to `[workspace.lints.clippy]` at `warn`: `assertions_on_result_states`, `renamed_function_params`, `redundant_type_annotations`, `empty_drop`, `empty_enum_variants_with_brackets`. Skip the rest (reasons in the lint diff below). | template | med (`assertions_on_result_states`), low (the others) | `assertions_on_result_states` turns TST-05's MUST (never a bare `assert!(r.is_ok())`) into a gate failure. Verified: it fires inside `#[test]` and suggests `.unwrap()`, which tests may use. The scanner's `weak-assert` soft rule stays, for `is_some`/`is_none`. |
| C12 | Universal / `#M-STATIC-VERIFICATION` `cargo` group (L1942); C-FEATURE (L2007); `#M-FEATURES-ADDITIVE` (L2051) | Enable the `cargo` lint group. Feature names carry no placeholder words, and there is no `no-std` feature. | tooling | Add `negative_feature_names = "warn"` and `redundant_feature_names = "warn"`. Optionally add `cargo_common_metadata = "warn"`. Do not enable the whole `cargo` group. | Template. Verified: `no-std` and `use-serde` feature names are flagged; `cargo_common_metadata` stays silent for `publish = false` crates. | med | API-16 says "a `std` feature, never `no_std`" but nothing enforces it. `multiple_crate_versions` would fail the `-D warnings` gate on transitive duplicates the author can't fix; cargo-deny already warns on those. `wildcard_dependencies` duplicates deny.toml `wildcards = "deny"`. |
| C13 | Universal / `#M-STATIC-VERIFICATION` tool list (L1908-1917); `#M-FEATURES-ADDITIVE` "any combination must work" (L2049) | Run cargo-audit, cargo-hack (all feature combinations) and cargo-udeps as check-in gates. | tooling (CI) | Add CI steps: `cargo hack check --workspace --feature-powerset --no-dev-deps` (use `--depth 2` for crates with many features), and `cargo machete`. | `rust.yml` | med | API-16 already recommends `cargo hack --feature-powerset` and CRF-05 names `cargo machete`, but the CI template runs neither. cargo-audit is already covered by `cargo deny check` advisories. Keep machete rather than cargo-udeps, which needs nightly. |
| C14 | Universal / `#M-LOG-STRUCTURED` (L1636-1748) | Give events names in `<component>.<operation>.<state>` form, use OpenTelemetry semantic-convention attribute names, don't format strings, and redact. | refine OUT-08 | SHOULD: events that operators query carry a stable `name:` in `<component>.<operation>.<outcome>` form, for example `info!(name: "order.place.rejected", order.id = %id, "order rejected")`. Fields use the OpenTelemetry semantic-convention key where one exists (`http.request.method`, `http.response.status_code`, `url.path`, `db.system.name`, `db.operation.name`, `file.path`, `error.type`). | Review | med | OUT-08 already covers fields over interpolation and secrets. Named events and standard keys make dashboards portable. Verified: the tracing 0.1.44 level macros accept `name:`. Do not adopt MS's `{{field}}` message-template syntax: it prints literal braces unless the log backend expands templates. |
| C15 | Performance / `#M-LOG-OVERHEAD` (L1287-1311) | Assume library telemetry is always on. Keep hot inner loops free of per-item events. | refine OUT-08 / PERF-04 | SHOULD: no `tracing` event per element inside a hot loop. Record one event per batch (batch id, count, failures), and never put a `format!` result in a field. | Review | low-med | Agents instrument every iteration. |
| C16 | Universal / `#M-PUBLIC-DEBUG` (L1752-1786) | A type holding sensitive data implements `Debug` by hand, and unit tests prove the data doesn't leak. | refine OUT-08 | MUST: every hand-written redacting `Debug` (and `Display`, if any) has a unit test that formats a value built from a known secret and asserts the secret substring is absent. | Review. Add the test to patterns.md #22. | med | Redaction breaks silently when someone later switches to `#[derive(Debug)]`. Sketch below; it passes the template lints and runs. |
| C17 | Universal / `#M-DOCUMENTED-MAGIC` (L1581-1612) | A magic value's comment says why this value, what breaks if it changes, and which external systems depend on it. | refine CRF-09 | SHOULD: a named constant's doc says why this value, what breaks if it changes, and which external system or spec it tracks, for example `/// Matches api.foo.com's 24 h request timeout; lower values abort valid requests.` | Review | low-med | CRF-09's own example documents what the constant is, not why it has that value. |
| C18 | Universal / `#M-WEASEL-WORDS` (L2012-2035), `#M-REGULAR-FN` (L1806-1847); AI / `#M-RUST-SHAPED` (L80-100) | Avoid `Service`/`Manager`/`Factory` in names; a factory is a builder; take `impl Fn() -> T` instead of a factory parameter; associated functions are for construction only; don't port C#/Java constructs one to one. | refine CRF-01 (plus a "porting smells" table in process.md) | SHOULD: type names avoid `Manager`, `Factory`, `Helper` and `Util(s)`; name the role (`Bookings`, `BookingDispatcher`). A factory is a `FooBuilder`. A parameter that must create values repeatedly is `impl Fn() -> Foo`. Associated functions without a receiver construct `Self`; other helpers are free functions in the module. `Service` stays allowed as DDD vocabulary for application and domain services. | Scanner advisory: `\b(?:struct\|enum\|trait)\s+[A-Z]\w*(?:Manager\|Factory\|Helper\|Helpers\|Util\|Utils)\b` -> soft. Review. | med | CRF-01 already bans `manager`/`helper`/`util` as general names. MS adds Factory -> Builder, `impl Fn` over factory types, and the static-method smell. See the porting table under Patterns. |
| C19 | Interop / `#M-IMPL-IO` (L2291-2320) | Sans-IO: a function that needs one-shot I/O accepts `impl Read`/`impl Write`, not `File` or a path. | refine API-08 | SHOULD: parsers, decoders and encoders take `impl BufRead`, `impl Read` or `impl Write` (or `&[u8]`/`&str`), never `&Path` or `File`. Opening the file is the caller's job; a thin `from_path` convenience may wrap it. | Review | med | Today this is only a buffering remark (performance.md L318). An agent's `fn parse(path: &Path)` makes the parser untestable without the filesystem (TST-10). |
| C20 | Interop / `#M-IMPL-RANGEBOUNDS` (L2324-2348) | Range parameters use `impl RangeBounds<T>` or `Range<T>`, never `(lo, hi)`. | new API (SHOULD) | SHOULD: a parameter that denotes a range is `impl RangeBounds<T>` (any range form) or `Range<T>`/`RangeInclusive<T>`, never two scalars or a tuple. | Review | low | A cheap API polish that agents miss. |
| C21 | Interop / `#M-TYPES-SEND` (L2352-2445) | Public types are `Send`, and so is every future they return. Assert it. | refine ASY-11 plus tooling | SHOULD (library crates): futures returned by public `async fn`s are `Send`. | `clippy::future_not_send` (nursery, not on): enable at `warn`. Verified: it flags an `Rc` held across `.await`, and does not flag `async fn f<T>(x: T)` without a `Send` bound (low false positives). | med | ASY-11 asserts `Send` for types, but an unnameable future can't go in a const assertion. The lint closes that gap. A deliberately `!Send` future then needs an `#[expect]` (H-05), which is the right friction. |
| C22 | Resilience / `#M-NO-GLOB-REEXPORTS` (L2687-2716); UX / `#M-NO-PRELUDE` (L3352-3376) | No `pub use foo::*` (except `cfg`-selected platform modules), and no crate preludes. | new API (SHOULD) | SHOULD: `pub use` names each item. Glob re-exports are only for `#[cfg]`-selected platform modules. Do not define a `prelude` module: if one seems needed, the module layout is wrong (ARC-05). | Scanner advisory: `\bpub(?:\s*\([^)]*\))?\s+use\s+[^;]*::\s*\*\s*;` and `\bpub\s+mod\s+prelude\b` -> soft. Verified: `clippy::wildcard_imports` (pedantic, on) does not fire on `pub use inner::*`. | med | CRF-06 covers glob imports but not globs in the public surface. A glob re-export silently publishes every future item (API-14, API-17). |
| C23 | Resilience / `#M-STRONG-TYPES-GUARD` (L2720-2758) | A newtype with an invariant has a fallible constructor, conversions from weaker types are fallible, and it has no infallible `From`. | refine API-03 | Add to Detect: `impl From<raw> for Validated` on a type with an invariant is a finding; the only ways in are `TryFrom`, `FromStr` and a fallible `new`. For values known at compile time, use a const-generic constructor with an inline `const { assert! }` (Pattern P3) instead of MS's panicking `const fn new`. | Review. `clippy::fallible_impl_from` (nursery) catches only panicking `From`s, which H-02/H-03 already block; it doesn't catch a `From` that silently skips validation. | med | Verified: `Month::of::<14>()` fails the build with E0080 ("evaluation panicked: month must be in 1..=12"), and `Month::of::<3>()` passes the template lints. |
| C24 | Resilience / `#M-TEST-UTIL` (L2783-2805) | Gate testing functionality behind a single feature named `test-util`: mocks, inspection of sensitive data, safety-check overrides, fake data. | refine TST-09 | SHOULD: cross-crate fakes and test-only capabilities live behind one feature named `test-util` (tokio's convention), enabled only from `[dev-dependencies]`. Anything that bypasses a check or exposes a secret exists only under that feature. | Review | low-med | testing.md L252 uses the name `test-support`. The ecosystem name saves agents a guess, and MS's list of what must be gated is sharper. |
| C25 | Resilience / `#M-AVOID-STATICS` (L2454-2512) | Statics are duplicated when two semver-incompatible versions of a crate are linked. Use them only for caches that don't affect correctness. | refine ARC-09 | SHOULD: a `static` or `thread_local!` holds only state whose value never affects correctness: caches, interned constants, `LazyLock` of immutable data. | Review; the scanner's `global-mut` advisory already exists. | low | Adds the duplicate-version reason, which agents don't know. |
| C26 | Resilience / `#M-INTEGRATION-TESTS` (L2543-2551) | A test that touches only the public API belongs under `tests/`. Prefer integration tests when both kinds would work. | refine TST-02 | SHOULD: a test that uses only public items goes in `tests/it/`; `mod tests` is for tests that need private items. | Review | low | Keeps `src/` files lean. It is a shift of emphasis only; TST-02's "narrowest seam" stays. |
| C27 | Resilience / `#M-MOCKABLE-SYSCALLS` (L2561-2683) | Anything non-deterministic or external can be mocked: files, network, clocks, entropy and seeds, environment. Libraries don't offer a `Default` constructor that does real I/O. Memory-hungry code taking external input offers bounded or chunked operations. | refine TST-10 / ARC-03 | MUST: every source of non-determinism (filesystem, network, clock, RNG or seed, environment, hardware) sits behind a port. A library type that does I/O takes its port in the constructor and does not implement `Default`. | Review | med | "No `Default` that opens real I/O" is a crisp, checkable rule. The enum mock core is a conflict; see Conflicts. |
| C28 | UX / `#M-AVOID-WRAPPERS` (L2835-2863); `#M-DI-HIERARCHY` custom wrapper (L2950-2971) | No `Rc`/`Arc`/`Box`/`RefCell` in public APIs. When `dyn` is needed, wrap `Arc<dyn Trait>` in a named type. | new API (SHOULD) | SHOULD: public signatures take and return `&T`, `&mut T` or `T`, not `Arc`, `Rc`, `Box`, `RefCell`, `Mutex` or `Arc<Mutex<_>>`. Hide sharing in a handle type the crate owns (P1). When a public type must hold a runtime-chosen implementation, wrap it: `pub struct DynClock(Arc<dyn Clock>)` implementing `Clock` (P2), not `Stamper<Arc<dyn Clock>>`. Exceptions: the pointer is the product (a container), or a benchmark justifies it. | Scanner advisory: `\bpub\s+(?:async\s+)?fn\b[^{;]*\b(?:Rc\|Arc\|RefCell\|Mutex\|RwLock)\s*<` -> soft. Review. | high | Agents return `Arc<Mutex<State>>` and accept `Rc<RefCell<Config>>`. OWN-11 governs internal sharing; nothing governs the public surface. It also fixes architecture.md's own DI example, `build_stamper(..) -> Stamper<Arc<dyn Clock>>`. |
| C29 | UX / `#M-SERVICES-CLONE` (L3406-3462) | Heavyweight service types implement a cheap `Clone` through `Arc<Inner>` and forward their methods. | pattern | See P1. | patterns.md | med | Gives agents a sanctioned clone (H-08's "cheap handle"), satisfies C28, and fits the hexagonal adapters (repository, HTTP client) the owner builds. |
| C30 | UX / `#M-SIMPLE-ABSTRACTIONS` (L3466-3525) | Service-like types don't expose nested generics such as `Service<Backend<Store>>`; one level at most. | refine API-09 / architecture.md DI | SHOULD: a service type that users name has at most one level of generic nesting and about two type parameters. Beyond that, erase the types at the composition root behind a named `Dyn*` wrapper (C28, P2). | Review | med | Generic ports (`App<R, C>`) drift into `App<Retry<Cached<PgRepo>>>`. This gives the threshold rust-craft lacks. |
| C31 | UX / `#M-INIT-BUILDER` (L3189-3312), with `#M-BUILD-RESULT` (L2516-2539) | Provide a builder beyond two optional parameters (four or more permutations). Name it `FooBuilder` with a `Foo::builder()` shortcut and no public `FooBuilder::new()`. Setters are `x()`, not `set_x()`, and chainable, ending in `.build()`. Required parameters go to `builder(..)`. Validation happens in `.build()`. | refine API-10 | SHOULD: builders are named `FooBuilder`. The entry point is `Foo::builder(required..)` and takes every required input, so a missing one can't compile. There is no public `FooBuilder::new`. Setters are named after the field (`timeout()`, never `set_timeout()`) and are infallible. Cross-field validation happens only in `build() -> Result`. | Review | med | patterns.md #4 takes the required URL through a setter and fails at run time with `BuildError::MissingUrl`. That contradicts API-10's own `Request::builder(url)` example and API-02. Fix the pattern too. |
| C32 | UX / `#M-ESSENTIAL-FN-INHERENT` (L3113-3155) | A type's core functionality is inherent; trait impls forward to it. | new API (CONSIDER) | CONSIDER: a library type's essential operations are inherent methods, and a trait impl for the type forwards to them, so callers need no trait import. Ports (ARC-03) are exempt, because adapters are used through the port. | Review. Do not enable `clippy::same_name_method` (restriction), which flags exactly this forwarding. | low-med | Discoverability for callers and agents. |
| C33 | UX / `#M-PARAMETER-CONSISTENCY` (L3380-3402) | The same conceptual parameters appear in the same order everywhere. Ubiquitous ones (logger, ctx) go last, and so do closures; at most one closure per function. | new API (SHOULD) | SHOULD: parameters that recur across a crate's functions keep one order; call-specific ones come first, ubiquitous context (`&Logger`, `&Ctx`) last, and a closure last. A function takes at most one closure. | Review | low-med | Swapped `(tenant_id, user_id)` pairs compile when both are the same type. Newtypes (API-02) catch most cases; ordering covers the rest. |
| C34 | UX / `#M-COLLECTION-TRAITS` (L2883-2899) | A custom collection provides `Iter`, `IterMut` and `IntoIter` types, `IntoIterator` for `T`, `&T` and `&mut T`, `FromIterator`, `Extend`, a truthful `size_hint`, and `ExactSizeIterator`/`DoubleEndedIterator` where they apply. | refine API-06 | Add rows to the API-06 table: `FromIterator` and `Extend` for collection types, a truthful `size_hint`, `ExactSizeIterator`/`DoubleEndedIterator` when true. | `iter_without_into_iter` and `into_iter_without_iter` (pedantic, already on). | low | Completes the existing `IntoIterator` row. |
| C35 | Performance / `#M-YIELD-POINTS` (L1419-1459), `#M-THROUGHPUT` (L1388-1415) | Long CPU work in async code yields every 10-100 µs, or queries the runtime budget. Prefer batch APIs and never hot-spin. | refine ASY-06 | SHOULD: CPU work that stays on an async task (short per-item work over many items) yields cooperatively, with `tokio::task::yield_now().await` every ~10-100 µs of work or `tokio::task::coop::consume_budget().await` per item. Anything longer goes to `spawn_blocking` (ASY-01). APIs that process items expose a batch form. | Review | med | Gives "heavy CPU" in ASY-06 a number. Verified in tokio 1.53.1: `task::yield_now`, `task::coop::consume_budget` and `task::coop::has_budget_remaining` are public. |
| C36 | Performance / `#M-ASYNC-STACK-SIZE` (L1040-1114) | Track hot future sizes with a `size_of_val` test. Shrink what is held across `.await`. Return `impl Future` with a synchronous prologue for large arguments. | refine PERF-05 | SHOULD: an `async fn` on a hot path (thousands of concurrent instances) gets a test pinning `size_of_val(&hot(args))`. Values not needed after an `.await` are dropped or scoped before it. Large arguments are processed in the synchronous prologue of a `fn … -> impl Future`. | Test. `large_futures` (explicit `warn`) catches only big futures: verified it fires at the await site for a 20,002-byte future. | low-med | PERF-05's `const` size assertion can't name an async fn's future, so a test is the only pin. |
| C37 | Performance / `#M-MEM-REUSE` (L1315-1361) | APIs let callers reuse allocations (`get_in(id, &mut value)`); the allocating convenience form is secondary. | refine PERF-02 | SHOULD: a hot-path API that produces a buffer offers a caller-owned form (`read_into(&mut Vec<u8>)`, like `BufRead::read_line(&mut String)`). The allocating form is a thin wrapper over it. | Review | low-med | PERF-02 covers reuse inside your own code, not API shape. |
| C38 | Performance / `#M-AVOID-INDIRECTION` (L1118-1160) | Hot types avoid nested `Arc` layers; lift hot fields up. | refine PERF-05 | SHOULD: no `Arc` inside a type that is itself shared through `Arc` unless both levels are shared independently. Copy a hot flag into the owning struct rather than reading it through two pointers. | Review | low | A C#-port smell that agents reproduce. |
| C39 | Project / `#M-CARGO-WORKSPACE` (L1469-1475), `#M-CRATES-IN-WORKSPACE` (L1525-1543) | Workspace dependency definitions enable no features beyond basic ones; members opt in. Sibling crates are declared in the workspace with path and version. | refine ARC-12 / ARC-01 | SHOULD: `[workspace.dependencies]` entries set `default-features = false` with at most baseline features (`std`), and each member enables what it uses (`tokio = { workspace = true, features = ["net"] }`). Workspace members are declared there too (`ledger-domain = { path = "crates/ledger-domain", version = "0.1.0" }`), and members never write `path = "../x"`. | Review | low-med | The architecture.md L850 example enables `derive`, `rt-multi-thread`, `macros` and `net` at workspace level, which hides which crate needs what (for example, `cargo check -p domain` gets them anyway). The example workspace already routes siblings through workspace deps; ARC-01 doesn't state it as a rule. |
| C40 | Building / `#M-OOBE` (L2064-2127) | Libraries build with only `cargo` and `rustc` on every Tier-1 target. Code from external generators ships with the crate. Platform-specific dependencies sit behind `cfg` or a feature. | new ARC (SHOULD) | SHOULD: a library builds on every Tier-1 target with only `cargo` and a C toolchain. Build-time generators (`protoc`, `flatc`) run ahead of time and their output is committed, or a pure-Rust generator is used. Platform-specific dependencies sit under `[target.'cfg(..)'.dependencies]` or an opt-in feature. | Review; an optional CI OS matrix. | low-med | Relevant to gRPC/protobuf services. rust-craft is silent on it. |
| C41 | Building / `#M-FEATURES-ADDITIVE` (L2045-2061) | Features never disable or modify public items. A feature that needs another declares it rather than expecting the user to enable it. Enum variants added under a feature need `#[non_exhaustive]`. | refine API-16 | Add: a feature that needs another declares it (`tls = ["dep:rustls", "std"]`), and enum variants added under a feature require `#[non_exhaustive]` on the enum. | The lints in C12; the CI step in C13. | low-med | Adds the two sub-rules API-16 lacks. |
| C42 | Macros / `#M-MACROS-DONT-LIE` (L922-944), `#M-EXAMPLE-OVER-PROC` (L815-831), `#M-PROC-IMPLIED-ITEMS` (L981-1030) | Macros don't change an item's kind, signature or `async`-ness. Prefer macros by example. Don't emit hidden types. | refine ARC-15 | MUST: a macro, ours or a dependency's attribute macro, does not change an item's kind, signature, visibility or `async`-ness, and does not emit hidden public items. Prefer `macro_rules!` when it suffices. | Review; this also guides dependency choice (`#[async_trait]` rewrites signatures, MOD-04). | low-med | ARC-15 covers hidden control flow, not signature rewriting. |
| C43 | Macros / `#M-PROC-IMPL` (L948-977), `#M-MACRO-VERSION-PIN` (L898-918), `#M-MACRO-HELPERS` (L835-856) | Put proc-macro logic in a separate `_impl` library crate with snapshot tests, and add trybuild UI tests. Pin companion crates `=x.y.z`. Macros reach third-party items through a hidden `_private` module. | new ARC (CONSIDER) | When a proc-macro crate is approved (H-16): the logic lives in `foo_macros_impl`, a plain library over `proc_macro2` with `insta` snapshots of its expansions; `foo_macros` is a shim; the facade re-exports it and pins it to `=x.y.z`; expansions reference dependencies through `#[doc(hidden)] pub mod __private` in the facade. | Review | low | Rare in app work, but correct when it happens. |
| C44 | Correctness / `#M-PANIC-CONTINUATION` (L251-281) | `catch_unwind` is a last resort. After a per-request panic, the server should restart. | refine ASY-04 | MUST: a joined task whose `JoinError::is_panic()` is true is a bug, not a retryable error. Log it at `error` with the task's identity, and start orderly shutdown so the supervisor restarts the process. Never respawn the task and continue. | Review | med | Tokio already does the per-task catch MS allows. Pattern 17 counts panicked tasks but never says what to do next. This is compatible with H-03. |
| C45 | Application / `#M-APP-ERROR` (L167-194), "don't mix" (L189-190) | Pick one application error crate and don't mix error types. | refine ERR-06 | SHOULD: a binary uses exactly one application error type (`anyhow::Error` or `eyre::Report`), never both. | Review | low | The relaxation for library crates in the same guideline is a conflict; see Conflicts. |
| C46 | Project / `#M-MSRV` (L1557-1565) | Raising the MSRV is a minor-version change, and the MSRV stays a few releases behind stable. | refine API-17 | Replace "raising the MSRV (by policy)" in api-design.md L742 with: raising `rust-version` is a minor-version change with a CHANGELOG note. | Review | low | This only affects published crates. It is a divergence from the owner's policy: flag it for them, don't change it silently. |
| C47 | Performance / `#M-HOTPATH` (L1212-1252) | Enable debug symbols for benchmarks (`[profile.bench] debug = 1`). | tooling | Add `[profile.bench] debug = "line-tables-only"` to the template so profilers show source lines for `cargo bench` runs. | template | low | PERF-09's `[profile.profiling]` covers binaries, not benches. |

### M-STATIC-VERIFICATION lint-by-lint diff (source L1924-1981; template `[workspace.lints]`)

The template has `clippy::all = deny`, which covers correctness, suspicious, style, complexity and perf, and `pedantic = warn`. MS sets all seven groups to `warn`; the template is stricter on the five it shares, and adds `pedantic` at the same level. "Verified" means observed in the probe crate on 1.98.1.

| Lint | MS level | Group / default | Template status | Verdict |
|---|---|---|---|---|
| `ambiguous_negative_literals` | warn | rustc, allow | missing | **Enable at `warn`**. Verified it flags `-1i32.abs()`. |
| `missing_debug_implementations` | warn | rustc, allow | on (`warn`) | Already on. |
| `redundant_imports` | warn | rustc, allow | missing | **Enable at `warn`**. Verified it flags `use std::option::Option;`, and it catches imports of 2024-prelude items. |
| `redundant_lifetimes` | warn | rustc, allow | on | Already on. |
| `trivial_numeric_casts` | warn | rustc, allow | on | Already on. |
| `unsafe_op_in_unsafe_fn` | warn | rustc (warn in edition 2024) | missing | Not worth it: moot under `unsafe_code = "forbid"`. |
| `unused_lifetimes` | warn | rustc, allow | on | Already on. |
| `clippy::cargo` (group) | warn | cargo | missing | Not the group; enable two members instead (next two rows). `multiple_crate_versions` would fail the gate on transitive duplicates and duplicates cargo-deny's `multiple-versions = "warn"`. `wildcard_dependencies` duplicates deny.toml `wildcards = "deny"`. `cargo_common_metadata` is optional (verified silent while `publish = false`). |
| `clippy::negative_feature_names` | via cargo | cargo | missing | **Enable at `warn`** (verified). Enforces API-16's "no `no_std` feature". |
| `clippy::redundant_feature_names` | via cargo | cargo | missing | **Enable at `warn`** (verified on `use-serde`). Enforces C-FEATURE. |
| `complexity`, `correctness`, `perf`, `style`, `suspicious` | warn | groups | on (`deny` through `all`) | Already on, stricter. |
| `pedantic` | warn | group | on (`warn`) | Already on. |
| `nursery` (group) | optional | group | off | Not the group; cherry-pick `future_not_send` (C21) and `too_long_first_doc_paragraph` (C9). |
| `allow_attributes_without_reason` | warn | restriction | on (`deny`) | Already on, stricter; `allow_attributes` is also denied. |
| `as_pointer_underscore` | warn | restriction | missing | Not worth it: `as_conversions = "deny"` already rejects every `as`, and raw pointers are moot under H-01. |
| `assertions_on_result_states` | warn | restriction | missing | **Enable at `warn`** (C11; verified in `#[test]`). |
| `clone_on_ref_ptr` | warn | restriction | on (`warn`) | Already on. |
| `deref_by_slicing` | warn | restriction | missing | Not worth it. Verified: it rewrites `&v[..]` on a `&Vec` into `&**v`, which reads worse. `redundant_slicing` (complexity, denied) already catches `&slice[..]`, and `ptr_arg` catches `&Vec` parameters. |
| `disallowed_script_idents` | warn | restriction | missing | Not worth it: `non_ascii_idents = "deny"` is strictly stronger. |
| `empty_drop` | warn | restriction | missing | **Enable at `warn`** (low value; verified). |
| `empty_enum_variants_with_brackets` | warn | restriction | missing | **Enable at `warn`**, to pair with `empty_structs_with_brackets`, which is already on. Verified: it fires on `B {}`, and did not fire on `A()` in 1.98.1. |
| `empty_structs_with_brackets` | warn | restriction | on (`warn`) | Already on. |
| `fn_to_numeric_cast_any` | warn | restriction | missing | Not worth it: subsumed by `as_conversions = "deny"`. |
| `if_then_some_else_none` | warn | restriction | missing | Not worth it: pure style (`then_some`/`then`) with contested readability (CRF-10). |
| `map_err_ignore` | warn | restriction | on (`warn`) | Already on. |
| `redundant_type_annotations` | warn | restriction | missing | **Enable at `warn`** (low; verified on `let s: String = String::new();`). |
| `renamed_function_params` | warn | restriction | missing | **Enable at `warn`**. Verified: it flags a trait impl that renames `new_name` to `name`, and ignores `_`-prefixed parameters and `From::from(raw)`. |
| `semicolon_outside_block` | warn | restriction | missing | Not worth it: style only, and it matters mainly for `unsafe {}` blocks (H-01). |
| `undocumented_unsafe_blocks`, `unnecessary_safety_comment`, `unnecessary_safety_doc` | warn | restriction | missing | Not worth it: moot under `unsafe_code = "forbid"`. |
| `unneeded_field_pattern` | warn | restriction | missing | **Do not enable.** Verified: it flags the deliberate exhaustive destructure `let Pair { a, b: _ } = p;` and pushes toward `..`, which stops new fields from forcing review (the spirit of CRF-03). |
| `unused_result_ok` | warn | restriction | on (`deny`) | Already on, stricter. |
| `too_long_first_doc_paragraph` | warn | nursery | missing | **Enable at `warn`** as a coarse backstop (C9); it is not a 15-word check. |
| `literal_string_with_formatting_args` | allow | nursery | off (nursery not enabled) | Nothing to do: it is already off. |

Lints from other MS guidelines: `clippy::missing_assert_message` (restriction, C6) and `clippy::future_not_send` (nursery, C21) are **enable at `warn`**. `clippy::same_name_method` (restriction) should **not** be enabled (C32).

## Conflicts

1. **M-PANIC-ON-BUG (L336-376) and M-PANIC-IS-STOP "valid reasons to panic" (L305-312) vs H-02 and H-03.** MS says to panic on detected programming bugs and contract violations, and allows `x.expect("must never happen")`, `unwrap()` on a poisoned lock, and panicking constructors. rust-craft forbids all of these in production code. Compatible nuggets:
   - "Never use panics to communicate errors, and never assume they are caught" matches H-03.
   - The "Correct by Construction" tip (L370-374) is H-03's own remedy.
   - Const-context validation becomes `const { assert! }` (MOD-10, P3), with the scanner fix in C7.
   - Poisoned locks map to the existing `PoisonError::into_inner` or `Poisoned` error guidance.
2. **M-PANIC-CONTINUATION (L251-281) vs H-03 (`catch_unwind` is in `disallowed-methods`).** MS permits per-request `catch_unwind` in servers. Keep the ban. Compatible nugget: tokio already catches panics per task, so treat `JoinError::is_panic()` as fatal and start an orderly shutdown (C44).
3. **M-APP-ERROR (L173-174) vs H-09 ("anyhow only in binaries").** MS lets "crates in your own repository exclusively used from your application" use anyhow or eyre. rust-craft keeps library crates on typed `thiserror` errors. Compatible nugget: one application error type per binary, never mixed (C45).
4. **M-MOCKABLE-SYSCALLS enum core (L2598-2670) and M-DI-HIERARCHY "enum for test-only implementations" (L2930-2931, L2973-2983) vs TST-09 and ARC-03 (ports and adapters, settled).** MS dispatches I/O through a private enum with a `#[cfg(feature = "test-util")] Mocked(MockCtrl)` variant, which is a test-only branch in production code. It also prefers concrete types over traits for dependencies. Compatible nuggets:
   - The checklist of what must be injectable, and "no `Default` constructor that does real I/O" (C27).
   - Narrow role traits combined through a supertrait (API-18).
   - Generics before `dyn` (API-09).
   - A named wrapper around `Arc<dyn Port>` instead of exposing it (C28, P2).
   - When a fake needs an inspection handle, the constructor returns `(fake, handle)` rather than accepting a shared controller (L2672-2681), which avoids state ambiguity between instances.
5. **M-STRONG-TYPES-GUARD "Additional panicking constructors are allowed (e.g., `new`), and should preferably be `const`" (L2751) vs H-03.** Compatible nugget: the const-generic constructor with an inline `const { assert! }` gives the same compile-time guarantee with no runtime panic path (P3, verified).

## Patterns / examples worth copying

All Rust sketches below were compiled and tested in `scratchpad/ms/ws/crates/pat` under the template lints plus the lints proposed above. The result was clean, and the 4 tests pass.

**P1 · Service handle (M-SERVICES-CLONE):** cheap `Clone` over `Arc<Inner>`, so the clone H-08 sanctions is visible, and no `Arc` appears in the API (C28).
```rust
#[derive(Debug)]
struct Inner { endpoint: String }

/// A service handle: `Clone` copies one pointer, never the service.
#[derive(Debug, Clone)]
pub struct Directory { inner: Arc<Inner> }

impl Directory {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { inner: Arc::new(Inner { endpoint: endpoint.into() }) }
    }
    #[must_use]
    pub fn endpoint(&self) -> &str { &self.inner.endpoint }
}
```

**P2 · Named `dyn` wrapper (M-DI-HIERARCHY L2950-2971, M-AVOID-WRAPPERS).** This replaces `Stamper<Arc<dyn Clock>>` in the architecture.md DI example. The core stays generic (`Stamper<C: Clock>`), and the root builds `Stamper<DynClock>`.
```rust
#[derive(Clone)]
pub struct DynClock(Arc<dyn Clock>);

impl DynClock {
    pub fn new(clock: impl Clock + 'static) -> Self { Self(Arc::new(clock)) }
}
impl Clock for DynClock {
    fn now(&self) -> SystemTime { self.0.now() }
}
// `Debug` is hand-written (`f.write_str("DynClock")`) because `dyn Clock` is not `Debug`.
```

**P3 · Compile-time validated newtype constant** (M-STRONG-TYPES-GUARD's `const` intent without H-03's panic path). Verified: `Month::of::<14>()` fails the build with E0080 "evaluation panicked: month must be in 1..=12". Pair it with a fallible `TryFrom<u8>` for runtime values.
```rust
impl Month {
    /// Builds a month checked at compile time.
    #[must_use]
    pub const fn of<const N: u8>() -> Self {
        const { assert!(N >= 1 && N <= 12, "month must be in 1..=12") };
        Self(N)
    }
}
pub const MARCH: Month = Month::of::<3>();
```

**P4 · Redaction test (M-PUBLIC-DEBUG L1777-1785)**, for patterns.md #22 and OUT-08:
```rust
#[test]
fn debug_never_prints_the_token() {
    let raw = "552d3454-d0d5-445d-ab9f-ef2ae3a8896a";
    let rendered = format!("{:?}", Token::new(raw.to_owned()));
    assert!(!rendered.contains(raw), "Debug leaked the token: {rendered}");
    assert_eq!(rendered, "Token([redacted])");
}
```

**P5 · Property instead of tautology (M-TAUTOLOGICAL-TESTS)**, for the testing.md smells table:
```rust
// Bad: assert_eq!(CHECKPOINTS, [0, 90, 180, 270]);  // passes by construction
#[test]
fn checkpoints_are_evenly_spaced_and_increasing() {
    let steps: Vec<u32> = CHECKPOINTS.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(steps.iter().all(|&s| s == steps[0] && s > 0), "uneven: {steps:?}");
}
```

**P6 · Sans-IO with a reused buffer (M-IMPL-IO, M-MEM-REUSE):**
```rust
pub fn count_non_blank(mut input: impl BufRead) -> io::Result<u64> {
    let mut line = String::new();
    let mut count: u64 = 0;
    while input.read_line(&mut line)? != 0 {
        if !line.trim().is_empty() { count = count.saturating_add(1); }
        line.clear();
    }
    Ok(count)
}
// Test with `count_non_blank(&b"a\n\n b\n"[..])?`: no filesystem needed (TST-10).
```

**P7 · Porting smells table** (M-RUST-SHAPED, M-WEASEL-WORDS, M-REGULAR-FN, M-DI-HIERARCHY), for process.md:

| From C#/Java | Rust shape |
|---|---|
| `throw_if_null(x)`, null guards | `Option<T>` in the type, `?`/`let … else` |
| `IFoo` interface per class | No trait until a second implementation exists (CRF-05), or a consumer-owned port (ARC-03) |
| Static utility class, `Foo::helper(x)` | A free function in the module (M-REGULAR-FN) |
| `FooManager`, `FooFactory` | The role noun (`Foos`, `FooDispatcher`), `FooBuilder`, or an `impl Fn() -> Foo` parameter |
| `Arc` on every nested field | Owned fields, and one `Arc<Inner>` handle at the service level (P1, M-AVOID-INDIRECTION) |
| Getter/setter pairs | Behavior on the type (ARC-10) |
| Exceptions for control flow | `Result` and typed errors (ERR-*) |

**Escape-hatch/errors note (from M-ERRORS-CANONICAL-STRUCTS, verified):** with thiserror 2.0.21 on stable 1.98.1, `#[derive(thiserror::Error)]` on a struct with a field typed `std::backtrace::Backtrace` fails with **E0658 `error_generic_member_access`**, because thiserror generates `Error::provide`. MS's canonical error struct therefore doesn't compile as written under rust-craft's `thiserror` default. Hiding the field behind a type alias compiles, but it games the macro. Recommended line for errors.md: "Library errors carry no `Backtrace` field; the binary edge (`anyhow`, `RUST_BACKTRACE=1`) captures one."

## Notable skips

- **M-UNSAFE, M-UNSAFE-IMPLIES-UB, M-UNSOUND:** `unsafe`-only (H-01).
- **M-FFI-NAMING, M-ISOLATE-DLL-STATE, M-SYS-CRATES, M-ESCAPE-HATCHES (native-handle `unsafe` conversions):** FFI and `unsafe`-only. They apply only if a human grants an H-01 exception for a `-sys` crate.
- **M-MACRO-MAIN-CRATE:** proc-macro packaging detail; negligible for this standard.
- **M-DOC-INLINE:** moot. Verified with `cargo doc`: rustdoc already inlines `pub use` re-exports from private and `pub(crate)` modules, including glob re-exports, and lists only re-exports from a `pub mod` under "Re-exports". `#[doc(inline)]` is therefore needed only in the double-path situation C1 forbids. MS's own example (`pub(crate) mod foo`) doesn't need it.
- **M-LATEST-EDITION "the `resolver` field is generally not needed":** wrong for virtual manifests. Verified: a virtual workspace without `resolver` warns "virtual workspace defaulting to `resolver = "1"` despite … edition 2024 which implies `resolver = "3"`". Keep the template's explicit `resolver = "3"`.
- **Divergences from rust-craft that I would not adopt:**
  - **M-FROM-ERROR:** ERR-04 is deliberately stricter (`#[from]` only when one step can produce the cause).
  - **M-FOREIGN-REEXPORTS (L2230-2240):** conflicts with API-14's "re-export the crate (`pub use bytes;`) so users get the matching version". Only nugget: never re-export individual foreign items under your own paths.
  - **M-SHORT-NAMES "`foo::Id` over `foo::FooId`":** the template sets `module_name_repetitions = "allow"` on purpose ("`order::OrderError` is idiomatic"). The "at most about 2-3 compound words" heuristic could go into CRF-01 as review guidance.
  - **M-ERRORS-CANONICAL-STRUCTS "Display prints the backtrace and upstream cause" (L3080-3091):** contradicts ERR-05 ("do not interpolate `{source}`; the reporter prints the chain"). The `Backtrace` field also fails with thiserror on stable (see Patterns). Exposing `is_xxx()` instead of a `#[non_exhaustive]` `ErrorKind` is equivalent to ERR-02's `kind()`.
  - **M-SMALLER-CRATES "if in doubt, split":** ARC-07's criteria plus H-16 for new crates are deliberate.
  - **M-STRONG-TYPES "numeric APIs return plain numbers, not `NonZero`" (L2772-2773):** keep H-03's `NonZero` parameters where they remove a panic path. The MS point applies to getters such as `window_size()` only.
- **M-LOG-STRUCTURED:** the `{{property}}` message-template syntax (needs a template-aware backend) and the `data_privacy` crate (a new dependency, H-16).
- **M-FAST-HASHER `foldhash`:** PERF-06 already covers faster hashers with measurement. The crate choice is H-16.
