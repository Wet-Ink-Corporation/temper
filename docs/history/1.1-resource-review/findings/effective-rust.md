# Effective Rust (David Drysdale): findings

Source: https://effective-rust.com/ (print view, converted to Markdown). Read: the preface, all six chapter intros, all 35 items and the afterword. The index was skipped. The book targets Rust 1.70 and edition 2018/2021. Every behavioural claim marked "verified" was checked on 1.98.1 in `scratchpad/effective/ws/`, a workspace that uses the temper template's `[workspace.lints]` and `clippy.toml`.

**License.** The site footer says: "Effective Rust by David Drysdale © 2024 Galloglass Consulting Limited", licensed CC BY-NC-ND 4.0. The license was checked on effective-rust.com and lurklurk.org, because the converted Markdown does not carry it. Because of the NoDerivatives term, rust-craft (MIT) must not copy or adapt the book's prose or code. Take the ideas only. All rule text and sketches below are original wording.

## Already covered (summary)

- **Item 1** → API-02, API-10, API-05. **Item 2** → API-09, OWN-04. **Item 3** → ERR-07, H-02, CRF-10, `collect::<Result<Vec<_>,_>>` in errors.md.
  - The combinator lints are already on: `manual_map`, `question_mark` and `manual_ok_or` (style, deny); `option_as_ref_deref` (complexity); `map_unwrap_or` and `single_match_else` (pedantic). All were verified by group lookup.
- **Item 4** → ERR-01..06, H-09.
- **Item 5** → API-06 table (From, never Into; TryFrom), API-07, H-11. `from_over_into` (style) is on.
- **Item 6** → API-02, API-03, CRF-09.
- **Item 7** → API-10/11 builder table, API-13, API-06 `Default` row.
- **Item 8** → OWN-10, OWN-11, OWN-12, API-19, the escape-hatches `RefCell` row.
- **Item 9** → PERF-03, CRF-10 ("for loop when side effects, early exits or several accumulators"), the H-10 `flatten`/`filter_map(Result::ok)` row. `cloned_instead_of_copied` and `needless_for_each` are pedantic and on.
- **Item 10** → API-06 table, OUT-10 (Debug vs Display), OUT-08 and pattern 22 (a secret has no `Clone`). `derived_hash_with_manual_eq`, `derive_ord_xor_partial_ord` (correctness) and `expl_impl_clone_on_copy` (pedantic) are on.
- **Item 11** → pattern 8 (RAII guard, including `finish(self) -> Result` because `Drop` cannot fail), PERF-12 plus `significant_drop_tightening`. Pattern 21 covers the explicit `flush` (BufWriter's `Drop` swallows errors).
- **Item 12** → API-09, API-08 (turbofish caveat), MOD-11 (upcasting is now stable).
- **Item 13** → API-12 (a defaulted method can still collide), API-18.
- **Item 14** → OWN-05, OWN-06, OWN-07. `'_` in paths comes from `elided_lifetimes_in_paths`, which is in `rust_2018_idioms`.
- **Item 15** → OWN-08 symptom table, ownership.md "Graphs and cyclic data" (arena, generational ids), OWN-10.
- **Item 16** → H-01. **Item 17** → ASY-09, ASY-10, ASY-11, PERF-12, OWN-11.
- **Item 18** → H-02, H-03, H-04, `disallowed-methods` for `catch_unwind`, the scanner's `assert-in-prod` and `should-panic` rules.
- **Item 19** → MOD-11, API-18. **Item 20** → PERF-01, OWN-06, OWN-16.
- **Item 21** → API-17, API-05. **Item 22** → API-14, API-04, `unreachable_pub`.
- **Item 23** → CRF-06, enforced by `clippy::wildcard_imports` (pedantic, on). Verified: it flags a private `use std::collections::*` but exempts `use super::*` in test modules and `pub use m::*`.
- **Item 24** → API-14 (`pub use bytes;`).
- **Item 25** → ARC-12, `deny.toml` (`wildcards = "deny"`, `multiple-versions = "warn"`), CRF-05 (`cargo machete`), architecture.md (Cargo.lock is committed).
- **Item 26** → API-16. **Item 27** → API-15, H-15, and rustdoc `-D warnings`, which covers `broken_intra_doc_links`.
- **Item 28** → ARC-15. **Item 29** → H-15 and the template lints.
- **Item 30** → TST-01, TST-02, TST-06 (includes a no-panic property), TST-11, TST-15 (nextest plus separate doctests), TST-16, PERF-01 (`black_box`).
- **Item 31** → testing.md (`llvm-cov`, `mutants`), API-17 (`semver-checks`).
- **Item 32** → template CI (fmt check, clippy `-D warnings`, nextest, doctests, rustdoc, `cargo deny`, MSRV job), MOD-01, MOD-13.
- **Item 33** → MOD-08, API-16 (positive `std` feature), PERF-10 (cap `with_capacity` from input).

In three places rust-craft is already sharper than the book, and the book should be ignored:

- **ERR-04 vs Item 4.** The book implements `From` for every sub-error. rust-craft uses `#[from]` sparingly so `?` never hides which step failed.
- **ERR-05 vs Item 4.** The book's `Display` interpolates the inner error and also returns it from `source()`, so the cause prints twice. ERR-05 forbids interpolating the source.
- **OWN-16 vs Item 10/20.** The book says to derive `Copy` whenever it is valid and cheap, and mentions the `missing_copy_implementations` lint. rust-craft keeps `Copy` for types that will stay small, because removing `Copy` later is a breaking change.

## Candidates

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement (lint + level / clippy.toml / scanner regex / review / template / CI) | Value (high/med/low) | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| 1 | Item 9 `iterators.html#iterator-consumers` (`sum`, `product`); the book's own example sums `x * x` | Iterator consumers such as `sum`/`product` replace loops. They apply `+`/`*` internally. | pattern (escape-hatches.md, Panicking APIs table), refine ERR-09 | Add a table row. Integer `Iterator::sum`/`product` overflow exactly like `+`/`*`: they panic in debug and wrap in release. On values not bounded by construction, use `try_fold(0, \|acc, &x\| acc.checked_add(x))` and map `None` to an error variant. Use `fold(0, u64::saturating_add)` only when a cap is the documented meaning. | Table row and review. Scanner advisory (prod, soft): `\.\s*(?:sum\|product)\s*(?:::\s*<[^>]*>)?\s*\(\s*\)` (tested: hits `.sum()`, `.sum::<u64>()` and `.product()`; misses `checksum()`). | high | Verified: `items.iter().sum()` on `&[u64]` passes the template gate (`clippy -D warnings`) and panics on `[u64::MAX, 1]` in a debug test. No enabled lint catches it; `arithmetic_side_effects` sees only operators. api-design.md already folds with `saturating_add` by habit, so make it a rule. |
| 2 | Item 18 `panic.html` ("harder to spot … `x / y` when `y` is zero") | Hidden panics include out-of-range indexing and division by zero. | pattern (Panicking APIs table), refine H-03/ERR-09 | Add a table row. Integer `/` and `%` panic on a zero divisor, and signed `MIN / -1` and `MIN % -1` panic on overflow. Divide by a runtime value only with `checked_div`/`checked_rem`, or type the divisor as `NonZeroU*`. `u64 / NonZeroU64` and `u64 % NonZeroU64` are infallible impls. No signed `NonZero` division impl exists, so signed code uses `checked_*`. | Table row; review. `clippy::integer_division` and `arithmetic_side_effects` (both restriction) are too noisy to enable workspace-wide. errors.md's per-crate opt-in stays as is. | high | Verified on 1.98.1: `i32::MIN / black_box(-1)` panics; `checked_div(0)` and `checked_div(-1)` on `MIN` return `None`; `u64 / NonZeroU64` compiles; `i64 / NonZeroI64` is E0277. The H-03 example already uses `NonZeroU32` but no rule says so, and the table lists only `+ - *`. |
| 3 | Item 10 `std-traits.html#operator-overloads` | Overload operators only for algebraic types, as a coherent set (`x - y` agrees with `x + -y`). Never hide expensive work behind an operator. | new API (API-21) | Implement `std::ops` traits only for types with algebraic meaning. Implement them as a consistent set: `Add`, `Sub`, `Neg` and the `*Assign` forms agree. An operator MUST NOT allocate, do I/O or do O(n) work. Operators cannot return `Result`. A type whose arithmetic can overflow or fail (money, integer quantities) MUST expose `checked_*` methods returning `Option`/`Result`. It implements the operator traits only when saturation is its documented meaning. `Self(self.0 + rhs.0)` is a latent panic (H-03, ERR-09). | Review. Scanner advisory (prod, soft): `\bimpl(?:\s*<[^>]*>)?\s+(?:(?:core\|std)\s*::\s*ops\s*::\s*)?(?:Add\|Sub\|Mul\|Div\|Rem\|Neg)(?:Assign)?\b(?:\s*<[^>]*>)?\s+for\b` (tested; misses `impl Addressable`). `suspicious_arithmetic_impl` and `suspicious_op_assign_impl` are already deny via `all`. | high | Verified: `impl Add for Cents { Self(self.0 + rhs.0) }` passes the template gate with zero warnings. DDD value objects (Money, Quantity) are exactly where agents write this, and the owner favours DDD. |
| 4 | Item 27 `documentation.html#what-not-to-document` | Don't repeat what the signature says, and don't describe callers ("X uses this to …"). Document the non-obvious: preconditions, invariants, errors, surprises. | refine API-15 | Doc comments state what the signature cannot: behaviour, preconditions, invariants, units, error conditions, surprises. They MUST NOT restate the signature, so no `# Arguments`/`# Parameters`/`# Returns` lists repeating names and types. They MUST NOT describe specific callers; state the reason instead. | Scanner advisory (prod, soft): `^\s*///\s*#{0,3}\s*(?:Arguments\|Parameters\|Params\|Returns)\s*:?\s*$` (tested; misses `/// Returns the id.` and `/// # Errors`). Review. | high | `missing_docs` (H-15) rewards filler. LLM agents default to docstring-style Args/Returns blocks, which go stale silently when a signature changes. API-15 says what to include but not what to leave out. |
| 5 | Item 17 `deadlock.html#deadlocks`, `#advice` | Rust prevents data races but not deadlocks. Put data that must stay consistent under one lock. Lock inversion across two locks deadlocks, and splitting locks breaks cross-structure consistency. Don't call closures with a lock held, and don't hand out guards. | new ASY (ASY-13), tooling | State that must stay mutually consistent lives behind a single lock. Code holds at most one lock at a time. If two are unavoidable, acquire them in one global order documented on the type. While a lock is held, never call a closure, callback or trait object. Never return a `MutexGuard`/`RwLock*Guard` from a `pub` fn. Split a lock for contention (PERF-12) only between fields that share no invariant. | Tooling: enable `clippy::significant_drop_in_scrutinee` (nursery, not currently on) at `warn`. Verified: it fires on `match m.lock()…first().copied() {..}` and the `if let` form, where the guard temporary lives through the arm that relocks. The enabled `significant_drop_tightening` did not fire. The template already enables nursery lints (`redundant_clone`, `significant_drop_tightening`). Review for the rest. | high | PERF-12 tells agents to split locks under contention, but nothing warns that split locks create lock-order inversion and torn cross-lock invariants. This is the one concurrency bug class the compiler leaves open. |
| 6 | Item 21 `semver.html#semver-for-crate-authors` | Grey-area breaks: adding `non_exhaustive`, losing object safety, a new blanket impl, a license change, a default-feature change. Make incompatible behaviour changes break the build, and deprecate before removing. | refine API-17 (+API-05) | Add to the hazard list: (a) adding `#[non_exhaustive]` to an existing public type is itself breaking, so decide it at first publication; (b) making a public trait no longer dyn-compatible; (c) adding a blanket impl of a public trait; (d) changing the license or the default feature set. (e) When behaviour changes incompatibly but the signature could stay the same, change the signature (a new type or name) so old callers fail to compile. (f) Remove API with `#[deprecated(since = "..", note = "use X")]` in a minor release, then delete it in the next major. | `cargo semver-checks` (catches some); review against the list. | med-high | API-05 says "decide deliberately" but not that deciding later is itself a break. (e) is the sharpest test: agents change semantics in place because it keeps the diff small. |
| 7 | Item 26 `features.html#features`; Item 29 (`negative_feature_names`, `redundant_feature_names`); Item 32 (every feature combination) | Feature unification means a crate cannot tell whether a feature is on. Don't feature-gate `pub` fields or `pub` trait methods. Optional dependencies create implicit features. Build all 2^N combinations. | refine API-16, tooling | Never put `#[cfg(feature = ..)]` on a `pub` struct field or on a `pub` trait method that implementors define or may override. Declare optional dependencies as `dep:name` so they do not become implicit features. CI checks every feature combination. | Tooling: enable `clippy::negative_feature_names` and `clippy::redundant_feature_names` individually at `warn`. They are in the `cargo` group, which is not part of `all`. Verified: they fire on features `no_std` and `use-fancy`. Don't enable the whole `cargo` group: `multiple_crate_versions` would fail the gate on any duplicate that `deny.toml` only warns about. `cargo_common_metadata` stayed silent on a `publish = false` crate. CI step: `cargo hack --feature-powerset --no-dev-deps check --workspace`. The template CI lacks it, although API-16 prescribes it. | med-high | API-16 forbids features that "remove or change" items. A gated `pub` field looks additive but breaks struct literals, which is exactly what agents do with `serde`/`schema` features. The lints mechanise API-16's "positive names" line. |
| 8 | Item 6 `newtype.html` (type aliases are "effectively just documentation") | A type alias does not stop pound-force-seconds being passed as newton-seconds. Only a newtype does. | refine API-02 | A `type` alias creates no new type. Never use `type UserId = u64;` or `type Cents = i64;` to distinguish ids, units or validated values; use a newtype (API-02/03). Aliases are only for abbreviating long types (`type Result<T> = Result<T, Error>`). | Scanner advisory (prod, soft), anchored at column 0 so indented associated types such as `type Output = u64;` don't match: `^(?:pub(?:\s*\([^)]*\))?\s+)?type\s+[A-Z][A-Za-z0-9]*\s*=\s*(?:[ui](?:8\|16\|32\|64\|128\|size)\|f(?:32\|64)\|bool\|String\|&'static\s+str)\s*;` (tested: hits `pub type UserId = u64;` and `pub(crate) type Name = String;`; misses `type Result<T> = ...` and `type Map = BTreeMap<..>`). | med-high | An alias looks like it satisfies API-02 in review ("it has a name") while giving no type safety. It is a common agent shortcut. |
| 9 | Item 1 `use-types.html#enums-with-fields`, `#optiont` | A comment explaining when a field is valid is a giveaway that an enum is needed. Never use in-band sentinels. `Option<Vec<T>>` only when "absent" and "empty" differ. | refine API-02 (Detect) | These are API-02 findings: (a) a comment stating when a field is valid or what it must hold ("only set when…", "must be 0 if…"); (b) in-band sentinels (`-1`, `0`, `""`, `u32::MAX`, `NaN`) for absence or failure, which become `Option`, `Result` or an enum; (c) `Option<Vec<T>>`/`Option<String>` unless absent and empty are different domain facts (ties to OUT-02 `default`). | Review. `clippy::option_option` (pedantic) already covers nested options. | med-high | Cheap, checkable review tests. Sentinels come in when agents port C/Go/SQL-shaped code, and API-02's Detect line misses them. |
| 10 | Item 33 `no-std.html#writing-code-for-no_std`, `#alloc` | Many library crates can be `no_std` + `alloc` with small changes (`BTreeMap` instead of `HashMap`). Check it in CI, because the compiler won't flag a dependency that pulls in `std`. | refine ARC-02 (CONSIDER), scanner fix | CONSIDER compiling pure domain crates as `#![no_std]` with `extern crate alloc;`, plus `#[cfg(test)] extern crate std;` for tests. The compiler then rejects `std::fs`, `std::net`, `std::env`, `SystemTime::now`, print macros and threads in the domain layer, which turns ARC-02 (no I/O) and injected time (TST-10) into compile errors. Use `BTreeMap`/`BTreeSet` (OUT-04), `core::error::Error` (MOD-08) and `thiserror = { .., default-features = false }`. | Charter/template. Scanner fix: the `extern-crate` rule (MOD-01 soft) must exempt `extern crate alloc;` and `extern crate std;`. Optional CI step: `cargo build -p <domain> --target thumbv7em-none-eabihf` to catch dependencies that pull in `std` (not verified: target not installed). | med | Verified: a `no_std` domain crate with a `thiserror` 2 error passes template `clippy --all-targets -D warnings` and rustdoc `-D warnings`. `std::time::SystemTime::now()` in a `no_std` crate is a compile error (E0433). This turns review-only rules into compile errors for event-sourced/hexagonal cores. |
| 11 | Item 32 `ci.html#public-ci-systems` (pin external scripts to known hashes; watch non-read access); Item 25 `dep-graph.html` (commit Cargo.lock, set up upgrades); Items 21/31 (Dependabot) | Pin external CI scripts to specific hashes. Commit the lock file and pair it with an upgrade process. | tooling | (a) Pin every `uses:` in `rust.yml` to a full commit SHA with a version comment. Today it uses tags and `dtolnay/rust-toolchain@master`, a moving branch. (b) Add top-level `permissions: contents: read`. (c) Make the first cargo step `cargo fetch --locked`, so a missing or stale `Cargo.lock` fails CI; architecture.md already requires committing it. (d) Add `.github/dependabot.yml` for the `cargo` and `github-actions` ecosystems, weekly. | template | med-high | This is a supply-chain hygiene gap in the shipped template. Without `--locked`, CI can silently re-resolve dependencies. |
| 12 | Item 21 `semver.html#semver-for-crate-users` | After a major-version upgrade, check that your use is still valid, because preconditions may have changed. Eventually follow majors for security fixes. | refine H-16 / ARC-12 | A dependency major-version bump, including a Dependabot PR, is a human decision (H-16). Read the upgrade notes for changed preconditions before merging: code that still compiles may now be wrong. | hook/review | med | H-16 lists new dependencies but not major upgrades, and it pairs with #11. |
| 13 | Item 30 `testing.html#fuzz-testing` | Fuzz code exposed to attackers with coverage-guided tools (cargo-fuzz). Panics are DoS. Don't fuzz per-PR; keep a corpus. | refine TST-06 | Code that parses attacker-controlled input (network protocols, file formats, decoders) SHOULD have a coverage-guided fuzz target in addition to the TST-06 no-panic property. Use `cargo fuzz`: nightly, in a separate `fuzz/` crate outside the gate. Run it on a schedule, not per PR, with the corpus committed. Every crash becomes a failing regression unit test before the fix (TST-01). | Review; scheduled workflow template. | med | Under H-03 any panic reachable from input is a defect. proptest with bounded strings explores far less than a fuzzer. Nothing in rust-craft mentions fuzzing. |
| 14 | Item 30 `testing.html#unit-tests` (final paragraph) | When code relies on specific behaviour from a dependency, add a small test of that behaviour as an early warning, separate from API changes. | new TST (TST-17) | When correctness depends on dependency behaviour that its signature doesn't express (sort stability, rounding, parser leniency, output ordering), pin that behaviour with a test in the consuming crate, so an upgrade that changes it fails CI. | review | med | Pairs with #11/#12: the lock file plus Dependabot makes upgrades routine, and this makes them safe. |
| 15 | Item 28 `macros.html#declarative-macros`, `#formatting-values`, `#advice-1` | A macro argument used twice is evaluated twice. Avoid hidden control flow, so return a `Result` the caller `?`s. Use `format_args!`. Don't insert implicit references. Macros are for keeping disparate code in sync and for boilerplate. Prefer derive macros and existing crates. | refine ARC-15, pattern | In macros: bind each `$e:expr` to a local once before use; use `ident`/`ty` fragments when an arbitrary expression is not meant. A macro that can fail expands to a `Result` expression the caller applies `?` to, never to a hidden `return`/`?`/`break`. Formatting macros forward to `format_args!`. References are taken at the call site (`m!(&x)`). Prefer a derive macro, or an existing crate (H-16), to a macro that emits a type. The legitimate uses are generating several code sites from one table and removing boilerplate that functions and generics can't. Debug with `cargo expand`. | review | med | ARC-15 says "never hide control flow" but gives no mechanics. Double evaluation and a hidden `return` are the two concrete failure modes. |
| 16 | Item 12 `generics.html#trait-object-safety` | A `Self`-returning or generic method bounded `where Self: Sized` doesn't stop the trait being used as a trait object. | refine API-09 | To keep a trait usable as `dyn` while offering a generic or `Self`-returning convenience method, bound that method `where Self: Sized`. It leaves the vtable and the trait stays dyn-compatible. Prefer this to splitting off a subtrait when only a few methods are affected. | review | med | Verified on 1.98.1: a trait with `fn duplicate(&self) -> Self where Self: Sized` and a generic defaulted method is used as `&dyn Trait`. API-09 offers only "split off a dyn-compatible subtrait". |
| 17 | Item 13 `default-impl.html` | Keep required methods minimal; add conveniences as default methods built on them. | refine API-18 | API-18's "about five" limit counts required methods. Keep required methods to the minimal primitives. Conveniences are provided methods written in terms of them, so implementors and hand-written fakes (TST-09) write only the primitives. | review | med | Directly reduces fake burden in ports-and-adapters designs, where every port trait gets a fake. |
| 18 | Item 2 `use-types-2.html#closures` | Accept the most general `Fn*` trait that works (`FnOnce` if called once), and prefer `Fn*` bounds to `fn` pointers. | refine API-08 / OWN-14 | Callback parameters take the least demanding bound the body allows: `FnOnce` when called at most once, `FnMut` when called repeatedly from one place, `Fn` only when shared or called through `&`. Accept closures generically (`impl FnMut(..)`), not as `fn` pointer types, which reject capturing closures. | review | med | Agents default to `F: Fn` or `Box<dyn Fn>`, which forces callers into clones or `Arc` (H-08). |
| 19 | Item 10 `std-traits.html#partialord-and-ord` | Derived ordering is lexicographic in field/variant declaration order; reorder fields or implement manually if that's wrong. Be wary of `PartialOrd` without `Ord`. | refine API-06 (Ord row) | Derived `PartialOrd`/`Ord` compare fields and enum variants in declaration order. Reordering fields or variants silently changes sort order, `BTreeMap` iteration and any sorted output (OUT-04). When order is part of the contract, pin it with a test or implement `Ord` by hand. Don't implement `PartialOrd` without `Ord` unless the type is genuinely partially ordered. | Review. `derive_ord_xor_partial_ord` (correctness) is already on. | med | A "harmless" field reorder in a refactor changes observable output. This is invisible in the diff unless the rule makes reviewers look. |
| 20 | Item 3 `transform.html` ("prefer `Result` to `Option`… lean toward `Result` if an error might communicate anything useful") | `Option` is for plain absence (`first()` on an empty slice); anything with a reason should be a `Result`. | refine ERR-07 | Return `Result` when a function can fail for more than one reason or the reason helps the caller. Return `Option` only for plain absence (lookup, first element, an optional field). A parse or validate function never returns `Option`. | review | med | ERR-07 covers converting `Option` to `Result` but not choosing the return type. Agents return `Option` from parsers, which loses the reason and pushes an ERR-07 conversion onto every caller. |
| 21 | Item 14 `lifetimes.html#algebra-of-lifetimes` | For `find(haystack, needle)` the output should borrow only from the haystack. The compiler's suggestion puts the same `'a` on both inputs. | refine OWN-05 | Tie an output lifetime only to the inputs it actually borrows from, e.g. `fn find<'h>(haystack: &'h str, needle: &str) -> Option<&'h str>`. The E0106 help suggests one `'a` on every input; don't apply it blindly, because it over-constrains callers and causes E0597 at call sites. | review | med | Agents apply compiler help verbatim. OWN-05's own example (`longer<'a>`) shows only the shared-lifetime case. |
| 22 | Item 24 `re-export.html` | Two semver-incompatible versions of a crate make "trait `RngCore` not implemented for `ThreadRng`" appear even though it visibly is. Fix with the re-exported crate. | refine PROC-12 / API-14 | Diagnostic rule: when rustc says "trait `X` is not implemented for `T`" and `T` visibly implements `X`, suspect two versions of one crate. Run `cargo tree -d` and `cargo tree -i <crate>`, then align versions or use the dependency's re-exported crate path. | process/review | med | Saves agents from rewriting correct code around a misleading E0277. PROC-09's two-attempts rule would otherwise burn cycles here. |
| 23 | Item 25 `dep-graph.html#version-specification`, `#what-to-depend-on` | Avoid exact `=` pins (use Cargo.lock) and `*`. Use a caret range with a minimum that includes the features you need. Build scripts and proc macros run code at build time. | refine ARC-12 | Version requirements are caret ranges whose lower bound is the oldest release that has every API you use: `"1.4"`, not `"1"`, if you call a 1.4 API. There are no `=` exact pins in `[workspace.dependencies]`: the committed Cargo.lock provides reproducibility, and `=` blocks unification with other crates. No `*`. In the dependency justification, count build scripts and proc macros as code that runs on developer and CI machines. | `deny.toml` `wildcards = "deny"` exists. CI grep: `rg -n '"=' Cargo.toml`. Published libraries: nightly `cargo update -Z direct-minimal-versions && cargo check` (not verified here). | med | The template's `version = "1"` style under-states minimums for published crates. Exact pins are a common agent reaction to breakage. |
| 24 | Item 32 `ci.html#ci-steps`; Item 25 (`cargo-udeps`) | Run dependency-hygiene tools in CI so regressions are caught. | tooling | Add a CI step: `taiki-e/install-action` with `cargo-machete`, then `cargo machete`. | template CI | med | CRF-05 names `cargo machete` as the detector, but neither the gate nor CI runs it. |
| 25 | Item 30 `testing.html#examples`; Item 32 (examples "may need to be explicitly triggered") | CI should build and run examples, and examples should use `?`, not `unwrap`. | tooling / refine TST-11 | Examples follow production rules, and their `main` returns `Result`/`ExitCode` using `?`. CI runs each example's `main` (`cargo run --example <name>`), or an integration test exercises the same flow. | template CI (a loop over the `cargo metadata` examples) | low-med | Verified: `cargo test --examples` builds examples and runs only their `#[test]` fns. A `main` that exits 3 still gives "test result: ok", so the book's claim that this runs examples is inaccurate. Today examples are only compile-checked (clippy `--all-targets`). |
| 26 | Item 32 `ci.html#ci-steps`, `#ci-principles` | For libraries, test without the lock file (latest deps). Diff regenerated generated files. Scan for committed secrets. When a process failure is found, add the CI check first. | tooling, refine PROC-11 | (a) A scheduled job runs `cargo update` then the test suite, catching upstream releases that break semver, which Cargo.lock hides. (b) Committed generated files (schemas, bindings, snapshots) are regenerated in CI, followed by `git diff --exit-code`. (c) A secret-scanning step. (d) When a process failure is discovered, add the check that detects it before fixing it. | template CI; process | low-med | (b) generalises OUT-07's snapshot approach. (d) is TST-01 applied to process. |
| 27 | Item 16 `unsafe.html`; Item 34 `ffi.html`; Item 35 `bindgen.html` | Wrap all unsafe in one layer, add SAFETY comments, keep blocks minimal, use `unsafe_op_in_unsafe_fn` and Miri. Generate bindings with bindgen into a `-sys` crate plus a safe wrapper crate. Free on the allocating side. Use RAII wrappers. | refine H-01 (exception path) | If a human approves FFI (H-16): raw bindings live in a `<lib>-sys` crate, generated by bindgen, regenerated and diffed in CI. A separate wrapper crate is the only other crate allowed `unsafe`, and it exposes only a safe API. Every `unsafe` block carries `// SAFETY:` naming the invariant. Memory is freed on the side that allocated it, through a `Drop` wrapper. Miri runs on the wrapper's tests. Every other crate keeps `unsafe_code = "forbid"`. | `clippy::undocumented_unsafe_blocks` (restriction; enable in those two crates only). `missing_safety_doc` (style) is already on. `unsafe_op_in_unsafe_fn` warns by default in edition 2024 (verified). Miri in CI for that crate. | low-med | escape-hatches.md says "FFI only in an approved -sys crate" without defining what approval requires. This gives the human a checklist. |
| 28 | Item 8 `references.html#smart-pointer-types` | `RefCell` borrowing panics at runtime unless the `try_` forms are used. | pattern (Panicking APIs table) | Extend the `RefCell` row: `borrow()` also panics while a mutable borrow is live, not only `borrow_mut()`. The fallible forms are `try_borrow`/`try_borrow_mut`, but prefer removing the `RefCell` (OWN-12). | table | low | Precision fix; the table lists only `borrow_mut`. |
| 29 | Item 22 `visibility.html#visibility-syntax`; Item 23 | A `pub` enum exposes all variants and their fields, and a `pub` trait all its methods. Glob re-exports from internal modules are tolerable. | refine API-14 | A `pub` enum's variant payloads and a `pub` trait's methods are API by default, so review them as such. In `lib.rs`, re-export items by name. `pub use module::*` exports whatever later becomes `pub` in that module, and `clippy::wildcard_imports` does not flag it. | review | low-med | Verified: `pub use inner::*` passes the pedantic `wildcard_imports` lint. ARC-05's table-of-contents `lib.rs` loses its purpose with globs. |
| 30 | Item 15 `borrows.html#local-code-refactoring` | Split a complicated expression into typed local variables to find which conversion the borrow checker rejects, then recombine. | refine PROC-09 | Add a step: split a failing chained expression into one `let` per step, each with an explicit type annotation, to find the exact step rustc rejects. Recombine after the fix. | process | low-med | A concrete, mechanical debugging move for the "stuck" protocol, better than guessing annotations. |

## Conflicts

- **Items 3 and 18 vs H-02/H-03.** The book treats `unwrap`/`expect`/`panic!` as a legitimate "ostrich" choice. It says panicking is fine "if you control `main`", for errors that are "very rare", or when "internal data is corrupted". It suggests paired infallible/fallible entry points, with the panicking one documented under `# Panics`.
  - rust-craft's settled position is no panics in production, with `main` returning `Result`/`ExitCode`.
  - Compatible nuggets: publish only the fallible form, named plainly (`x() -> Result`), because API-01 reserves `try_x` for a twin of an infallible `x`. The book's "machine vigilance" advice is to grep CI for panicking entry points, which temper's scanner already does.
- **Item 17 vs H-02.** The book says it is common to `.unwrap()` a mutex lock because poisoning is rare. rust-craft forbids that.
  - Compatible nugget: none new. escape-hatches.md already prescribes mapping `PoisonError`, or `into_inner` with a reason.
- **Items 15 and 20 vs H-07/H-08/OWN-11.** The book recommends `Rc<RefCell<T>>`/`Arc<Mutex<T>>` for interconnected data, says smart pointers are "not an admission of defeat", and quotes approval of "call `.clone()` when I need to". It back-links tree nodes with `Weak` and `upgrade().expect(..)`.
  - rust-craft requires an arena first and a written justification for shared mutability, and forbids clone-to-appease.
  - Compatible nuggets: (a) when `Rc`/`Arc` is justified, back-pointers are `Weak`, and a failed `upgrade()` is an error, not an `expect`. (b) Copying a small bounded slice into an owned struct that is stored (the TLV example in Item 20) is a legitimate H-08 ownership reason, because the callee stores it.
- **Item 16 vs H-01.** The book says to write `unsafe` only when forced, inside a wrapper layer. rust-craft forbids `unsafe` workspace-wide. Compatible nugget: row #27, the checklist for a human-approved FFI exception.
- **Item 34 vs `disallowed-methods` `catch_unwind`.** The book says to use `catch_unwind` to stop panics crossing an FFI boundary. This is relevant only inside an approved FFI crate (row #27). Everywhere else the ban stands.
- **Item 4 (`Result<_, String>` and `Box<dyn Error>` in early examples) and Item 30 (examples' `main` returning `Result<(), Box<dyn Error>>`) vs H-09.** H-09 governs library `pub` signatures only. An example binary's `main` returning `Box<dyn Error>` or `anyhow::Result` is compatible.

## Patterns / examples worth copying

All code below is original, not taken from the book (CC BY-NC-ND).

**Checked value-object arithmetic (row #3).** There is no `Add` impl, because overflow must be reportable.

```rust
/// An amount in cents. No `Add`: overflow must reach the caller (API-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cents(u64);

impl Cents {
    /// Sum of two amounts, or `None` if it does not fit.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }
}
```

**Overflow-aware total and non-zero divisor (rows #1 and #2).**

```rust
use std::num::NonZeroU64;

/// Total of `items`, or `None` when it overflows `u64` (never `.sum()`).
#[must_use]
pub fn total(items: &[u64]) -> Option<u64> {
    items.iter().try_fold(0_u64, |acc, &x| acc.checked_add(x))
}

/// Share per part; the divisor's type rules out division by zero.
#[must_use]
pub fn per_part(total: u64, parts: NonZeroU64) -> u64 {
    total / parts
}
```

**Result-producing macro (row #15).** The argument is evaluated once and control flow stays visible at the call site.

```rust
macro_rules! expect_status {
    ($status:expr, $want:expr) => {{
        let status = $status; // evaluated exactly once
        if status == $want { Ok(status) } else { Err(StatusError { status }) }
    }};
}
// Call site: `let code = expect_status!(resp.status(), 200)?;`: the `?` is visible here.
```

**One lock for co-invariant state (row #5).**

```rust
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Seats reference players, so both change under one lock (ASY-13).
#[derive(Debug, Default)]
pub struct Lobby {
    state: Mutex<LobbyState>,
}

#[derive(Debug, Default)]
struct LobbyState {
    players: BTreeMap<String, u32>,
    seats: BTreeMap<u32, Vec<String>>,
}
```

**Compiler-enforced pure domain crate (row #10).** Verified with the template lints.

```rust
//! Order domain: decisions and events only; no I/O, no clock (ARC-02).
#![no_std]

extern crate alloc;

#[cfg(test)]
extern crate std;

use alloc::collections::BTreeMap; // no HashMap in alloc, and OUT-04 wants BTreeMap anyway
```

**dyn-compatible trait with generic conveniences (row #16).** Verified.

```rust
/// A drawable shape; still usable as `&dyn Shape`.
pub trait Shape {
    /// Area in square units.
    fn area(&self) -> u64;

    /// Scaled area. `Self: Sized` keeps this generic method out of the vtable.
    fn scaled_area<F: Into<u64>>(&self, factor: F) -> u64
    where
        Self: Sized,
    {
        self.area().saturating_mul(factor.into())
    }
}
```

## Notable skips

- **Item 33, fallible allocation and `no_global_oom_handling`.** Kernel-style environments only. PERF-10 already caps `with_capacity` from input.
- **Items 34 and 35, FFI mechanics** (`#[repr(C)]`, `CString`/`CStr`, `#[no_mangle]` prefixes, `Box::into_raw`/`from_raw`, bindgen options, `cxx`). These are unsafe-only; the usable residue is row #27.
- **Item 16, Miri and SAFETY-comment details.** These apply only inside a row #27 exception.
- **Item 19, `type_name`/`TypeId`/`Any` mechanics.** MOD-11 covers downcasting. "Don't parse `type_name`" is subsumed by OUT-10's "never parse `Debug`".
- **Item 18, the `no_panic` crate.** It is a linker trick, fragile, and a new dependency. The scanner and lints already cover the practical ground.
- **Item 14, `const` promotion caveats and `Box::leak` for `'static`.** H-07 covers the latter.
- **Lint proposals rejected after verification:**
  - `clippy::panic_in_result_fn` (restriction) fires on `assert_eq!` inside `#[test] fn … -> Result`, which contradicts TST-04.
  - `clippy::fallible_impl_from` (nursery) is redundant with `unwrap_used`, `expect_used`, `panic` and `indexing_slicing`. It did not flag `assert!` inside a `From` impl.
  - `clippy::std_instead_of_core` and `std_instead_of_alloc` (restriction) fire on ordinary std crates and test modules; `#![no_std]` (row #10) does the job properly.
  - The `missing_copy_implementations` lint contradicts OWN-16's intent.
  - `clippy::derive_partial_eq_without_eq` (nursery) is optional at most, because API-06 already says to add `Eq` unless a float is inside.
- **Incidental inconsistency, not from the source.** api-design.md's API-10 Detect line says `too_many_arguments` has "threshold 5". `clippy.toml` sets `too-many-arguments-threshold = 4`, and rules.md says "max 4".
