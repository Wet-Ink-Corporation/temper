# Rust API Guidelines: findings

Source: https://rust-lang.github.io/api-guidelines/ (checklist: https://rust-lang.github.io/api-guidelines/checklist.html). Read: the whole print-view book (all 2,432 lines of `res/api-guidelines.md`): the intro, the checklist, all 11 topical chapters, and all 54 `C-*` items, plus External links. rust-craft side: all of `SKILL.md`, `rules.md` and `api-design.md`, the relevant parts of the other references, the template `Cargo.toml`, `clippy.toml`, `deny.toml`, `rust.yml` and `.cargo/config.toml`, and the `scan.mjs` RULES table.

Verification: every lint named below printed `ok` from `lintcheck.sh` on 1.98.1 (101 names; a fake name correctly printed UNKNOWN). Lint groups and defaults come from `clippy-driver -W help` on 1.98.1. Behavioural claims marked "(verified)" were checked in scratch crates under `scratchpad/apiguide/` that use the template's exact `[lints]` table and `clippy.toml`. The pattern sketches compile clean with `cargo clippy --all-targets -- -D warnings` in `scratchpad/apiguide/pat/src/`.

Lint-state shorthand: **deny(all)** means it is in style, correctness, suspicious, complexity or perf, so the template denies it through `all = deny`. **warn(pedantic)** means it is on through `pedantic = warn`; the gate denies warnings. **explicit** means it is listed by name in the template. **off: needs enabling** means restriction, nursery, cargo or an allow-by-default rustc lint.

## Already covered (summary)

- C-COMMON-TRAITS → API-06 (a richer table than the source), OWN-16, and `new_without_default` deny(all). Do not enable rustc `missing_copy_implementations`, because it would push `Copy` against OWN-16.
- C-CONV-TRAITS → API-07. Enforced by `from_over_into`, `should_implement_trait` and `infallible_try_from`, all deny(all).
- C-SERDE → API-06, API-16 (already the modern `dep:` form) and OUT-01.
- C-GOOD-ERR → ERR-01 (`Error + Send + Sync + 'static`, with a const assertion), ERR-05 (lowercase, no trailing punctuation) and H-09. `result_unit_err` deny(all) rejects `Result<_, ()>` (verified).
- C-FAILURE (the Errors part) → H-15 and API-15. `missing_errors_doc` warn(pedantic) also fires on fallible trait method declarations, so port traits are covered (verified).
- C-METHOD → ARC-10.
- C-DEREF → API-19, plus the scanner's `deref-inherit` rule.
- C-CTOR → the API-01 table (`new`, `with_`, `from_`). Enforced by `new_ret_no_self`, `new_without_default` and `self_named_constructors`, all deny(all).
- C-CALLER-CONTROL → OWN-01, OWN-02 and H-08. Enforced by `needless_pass_by_value` and `unnecessary_box_returns` warn(pedantic), `unnecessary_to_owned` deny(all), and `redundant_clone` explicit.
- C-GENERIC → API-08, with `implicit_hasher` warn(pedantic).
- C-NEWTYPE → API-02.
- C-CUSTOM-TYPE → API-02 and API-10. Enforced by `fn_params_excessive_bools` (threshold 1), `struct_excessive_bools` (threshold 2) and `option_option`, all warn(pedantic).
- C-BUILDER → API-10, API-13 and patterns #4 and #5. The divergence is noted under Conflicts.
- C-VALIDATE (the static-enforcement part) → API-03.
- C-DEBUG → API-06, with `missing_debug_implementations` explicit.
- C-STRUCT-PRIVATE → API-04.
- C-SEALED → API-12 and pattern #6.
- C-ITER → API-01. Enforced by `iter_without_into_iter`, `into_iter_without_iter` and `iter_not_returning_iterator`, all warn(pedantic). API-01's Detect line names only the first.
- C-SEND-SYNC → API-06, ASY-11 and API-17. The source's `#[test] fn` form is weaker than rust-craft's const assertion. Row 14 refines the level.

## Candidates

Scanner regexes that contain `|` are listed after the table so they do not break it.

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement (lint + level / clippy.toml / scanner regex / review / template / CI) | Value (high/med/low) | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| 1 | Flexibility, C-INTERMEDIATE (`flexibility.html#c-intermediate`) | Expose data you computed anyway: `binary_search` returns the insertion point, `HashMap::insert` returns the old value, and `String::from_utf8`'s error "hands back ownership of the input bytes". | new ERR-13 | **ERR-13 SHOULD.** A function that takes an argument by value and can fail returns that argument, unconsumed, inside its error, with an `into_inner(self) -> T` accessor, so callers never clone "in case it fails". A function that computes a by-product the caller may need (insertion index, replaced value, valid-prefix length) returns it instead of a bare `bool` or `()`. | Review. Add to the H-08 checklist: "a `.clone()` made only to call a consuming fallible function is a finding against that function's API". See pattern P1. | high | This is the API-side cure for H-08. Agents clone before `push`, `send` or `try_from(owned)` because the failure path eats the value. Today it appears only as a table cell (`errors.md:474`, `PoolClosed(job)`) and a pattern note (`patterns.md:652`), not as a rule. |
| 2 | Interoperability, C-RW-VALUE (`interoperability.html#c-rw-value`) | Generic reader and writer functions take `R: Read` and `W: Write` by value. `&mut R` also implements `Read`, so callers pass `&mut f` to keep the value. Say so in the docs. | refine OWN-01 | **OWN-01 exception (SHOULD).** Generic I/O parameters are `R: Read` and `W: Write` (or `impl Read` and `impl Write`) by value, not `&mut W`. Their docs say "pass `&mut reader` to keep using it". Fix `escape-hatches.md:286` (`write_report<W: Write>(out: &mut W, ..)`) and `patterns.md:1058` (`announce(out: &mut impl Write, ..)`). | Review. There is no lint conflict: `needless_pass_by_value` warn(pedantic) does not fire on `fn f<R: Read>(mut r: R)` (verified). | high | OWN-01 ("borrow in parameters") steers agents to `&mut W`, and rust-craft's own examples teach that form. `fn f<W: Write>(out: &mut W)` rejects `&mut dyn Write` with E0277, because `W: Sized` is implicit. The by-value form accepts both owned writers and `&mut dyn Write` (verified). |
| 3 | Future proofing, C-STRUCT-BOUNDS (`future-proofing.html#c-struct-bounds`) | Do not put derivable trait bounds on generic data structures; `derive` already scopes each impl. Adding a bound later is breaking. Three exceptions: an associated type, `?Sized`, and a `Drop` impl that needs the bound. | new API-21 | **API-21 MUST.** A generic `struct` or `enum` definition carries no bound on `Clone`, `Debug`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Default`, `Display`, `Error`, `Serialize`, `Deserialize` or `DeserializeOwned`. Put bounds on the `impl` blocks that use them. **SHOULD:** put port bounds on the impls too (`struct Lookup<S>` plus `impl<S: Store> Lookup<S>`). The exceptions are a field that names an associated type (`S::Error`), `?Sized`, or a `Drop` impl that needs the bound (E0367, verified). | Review, plus a soft scanner rule (regex R1 below). No clippy lint exists: `struct Repo<S: Store + Clone + Debug>` passes the template (verified). | high | Agents copy bounds onto structs by reflex. Each bound forces every user, and every test fake (TST-09), to satisfy it. `architecture.md:669` recommends `struct Stamper<C: Clock>`, which should become `struct Stamper<C>` plus `impl<C: Clock>`. See pattern P5. |
| 4 | Flexibility, C-OBJECT (`flexibility.html#c-object`) | Decide early whether a trait will be used as an object. If it will, exclude generic methods from the vtable with `where Self: Sized` (the `Iterator` technique). | refine API-09, API-17 | **API-09 (SHOULD).** Record in the docs of every public trait whether it is used as `dyn`. If it is (plug-ins, `Arc<dyn Port>`), keep it dyn-compatible. Mark generic, `async fn` and `-> impl Trait` methods `where Self: Sized`, and put `const _: Option<&dyn Trait> = None;` beside the trait. **API-17:** add "a public trait stops being dyn-compatible" to the list of breaking changes. **Terminology:** say "dyn-compatible" everywhere; replace "object-safe" at `async-concurrency.md:79` and `:84` and `modern-rust.md:288`. | Compiler, through the const assertion: adding `fn flush_to<W: Write>(&mut self, out: W)` without `Self: Sized` fails E0038 at the const, in-crate (verified). `where Self: Sized` works on generic, `async fn` and RPITIT methods on 1.98.1 (verified). | high | Without the assertion, E0038 appears only at a downstream `dyn` use, often in another crate. rustc 1.98.1 says "the trait `X` is not dyn compatible" (verified). ASY-03's boxed "twin" stays the way to make async methods callable through `dyn`; `Self: Sized` only removes them from the vtable. See pattern P4. |
| 5 | Dependability, C-DTOR-FAIL (`dependability.html#c-dtor-fail`) | A destructor that panics during unwinding aborts the process. Provide a `close` method that returns `Result`. If it is not called, `Drop` tears down and ignores or logs errors. | new ERR-14 | **ERR-14 MUST.** `Drop` never panics and is never the only path for a teardown that can fail. A type whose teardown can fail (flush, commit, rename, final frame) exposes `fn close(self) -> Result<_, E>` (or `finish`/`into_inner`), documented as the only way to observe the failure. Its `Drop` does best-effort teardown and logs a failure once with `tracing::warn!` (H-10: never discard silently). Callers call `flush()`/`close()` explicitly on `BufWriter`-like types. | Review; no lint. Promote the note in `patterns.md` #8 ("`Drop` cannot fail ... add `fn finish(self)`") to a registry rule. See pattern P2. | high | Losing errors in a flush on drop is a silent failure (H-10). The owner's event-sourced designs append and commit through writers where this is exactly the risk. Today it is only a pattern note plus the #21 `BufWriter` note. |
| 6 | Naming, C-CONV (`naming.html#c-conv`) | `as_` is free, borrowed to borrowed. `to_` is expensive, or owned to owned for `Copy` types (`f64::to_radians`). `into_` is owned to owned for non-`Copy` types. Single-value wrappers expose `into_inner()`. `mut` goes where it sits in the return type (`as_mut_slice`). | refine API-01 | **API-01 table additions.** (a) `to_x(self)` on `Copy` receivers. (b) `into_x` only on non-`Copy` receivers; on `Copy` types use `to_x`. (c) `as_x` is O(1) and never allocates or validates (`Path::to_str` is `to_` because it checks UTF-8). (d) A wrapper around one value exposes it through `into_inner(self)`. (e) `as_mut_slice`, not `as_slice_mut`. | `wrong_self_convention` deny(all) checks receivers only. It catches `to_*(&self)` on `Copy`, `into_*(&self)` and `to_*_mut(&self)`, and misses `as_string(&self) -> String` and `into_raw(self)` on `Copy` (verified). Cost is review only; say so in the Detect line. | high | Agents name allocating or validating conversions `as_`. The lint gives false assurance because it never checks cost. |
| 7 | Documentation, C-QUESTION-MARK (`documentation.html#c-question-mark`) | "Example code is often copied verbatim." Use `?`, not `unwrap`. | tooling (scanner) | Scanner rule `doc-unwrap` (H-02/TST-11): inside code fences of `///` and `//!` comments, excluding fences tagged `text` or `compile_fail`, flag `.unwrap()`, `.expect(`, `panic!` and `todo!` as **hard** in prod, bin and example. Also accept the shorter tail `# Ok::<(), MyError>(())` as a sanctioned doctest form (verified to compile and run on 1.98.1). | Scanner. Today `scan.mjs` skips doc comments entirely (`if (c.doc) continue;`). Each `///` line is its own comment entry, so fences need a toggle across consecutive doc lines (regex R2). Clippy does not lint doctests: `v.unwrap()` in a crate-level example passed the template with `unwrap_used = deny` (verified). | high | TST-11 and API-15 forbid `unwrap` in doctests, but the only detector is a review grep. Doctests are the code users copy. |
| 8 | Dependability, C-DTOR-BLOCK (`dependability.html#c-dtor-block`) | Destructors should not block. Provide a separate method that prepares for a teardown that neither fails nor blocks. | refine H-14 / ASY-06 | **H-14 / ASY-06 addition (MUST).** `Drop` never blocks: no `JoinHandle::join`, blocking I/O, `block_on`, `recv()` or waiting on a contended lock. Types that own threads, tasks or connections expose `fn shutdown(self) -> Result<..>` or `async fn close(self)`. `Drop` only signals: cancel a token, abort, close a channel. | Review; no lint. A `Drop` that joins a thread passes the template (verified). | med-high | `Drop` runs wherever the value dies, including on an async worker thread, so a blocking `Drop` is a hidden H-14 violation. ASY-04 requires tasks to be joined, and agents put that join in `Drop`. |
| 9 | Naming, C-ITER-TY (`naming.html#c-iter-ty`); Future proofing, C-NEWTYPE-HIDE (`future-proofing.html#c-newtype-hide`) | A method named `into_iter` returns `IntoIter` (likewise `Iter`, `Keys`), named with its module (`vec::IntoIter`). A newtype hides adapter types so the representation can change. | refine API-01, API-08 | **API-01 / API-08 (SHOULD).** `iter`, `iter_mut`, `into_iter`, `keys`, `values` and `drain` on a public type return a crate-owned iterator named after the method (`Iter`, `IterMut`, `IntoIter`, `Keys`, ...). The type is exported from the owning module, marked `#[must_use = "iterators are lazy and do nothing unless consumed"]`, and implements every extra iterator trait it can: `DoubleEndedIterator`, `ExactSizeIterator`, `FusedIterator`, `Clone`, `Debug`. `impl Iterator` is fine where the type never needs naming. Never expose `std::slice::Iter` as `IntoIterator::IntoIter`, because it pins the storage. | Review. `must_use_candidate` warn(pedantic) fires on `iter()` returning a newtype that lacks `#[must_use]` (verified), which pushes the attribute. `unnameable_types` (row 19) catches a newtype that was not exported. Add `into_iter_without_iter` and `iter_not_returning_iterator` to API-01 Detect. See pattern P3. | med-high | `IntoIterator for &T` needs a nameable `IntoIter`, because `impl Trait` in associated types is still unstable on 1.98.1 (E0658, verified). rust-craft's own API-01 example (`Playlist::iter -> std::slice::Iter`) and `ownership.md:129` leak `Vec` storage into the public type. |
| 10 | Predictability, C-OVERLOAD (`predictability.html#c-overload`) | Implement `Mul` only for something like multiplication, with the expected properties such as associativity. The same goes for the other operators. | new API-22 | **API-22 MUST.** Implement a `std::ops` trait only when the operation has that operator's meaning for the type: `Add` for same-unit quantities, never for "append" or "merge". Never implement an arithmetic operator whose body can overflow or fail (`Self(self.0 + rhs.0)` on a bounded integer). Provide `checked_add(self, rhs) -> Option<Self>` (or `Result`) instead, or document wrapping or saturating semantics on the impl (ERR-09). | `suspicious_arithmetic_impl` and `suspicious_op_assign_impl` deny(all) catch only a mismatched operator. `impl Add for Cents { Self(self.0 + rhs.0) }` passes the template (verified). The overflow check is `arithmetic_side_effects` (off: needs enabling; restriction, and an H-05 decision per ERR-09). See pattern P6. | med-high | Operators cannot return `Result`, so an overflowing `+` panics in debug builds (in effect an H-03 violation) and wraps silently in release. Agents write `impl Add for Money` by reflex. |
| 11 | Naming, C-GETTER (`naming.html#c-getter`) | No `get_` prefix. `first_mut`, not `get_first_mut` or `mut_first`. `get` only when there is "a single and obvious thing" (`Cell::get`). `get_mut` for wrappers (`Cursor::get_mut`). | refine API-01 | **API-01 (MUST).** A getter is the field name, and its mutable twin is `x_mut`. A no-argument `get()` is allowed only for the single obvious contained value (`Cell::get`, `NonZeroU32::get`, a newtype's `get()`). `get(key) -> Option<&V>` is for lookup. `get_ref` and `get_mut` are for wrappers exposing the wrapped value. Fix the table cell "Reserve `get` for keyed or indexed lookup", which contradicts rust-craft's own `StoreId::get`. | Scanner, soft (regex R3). No clippy lint exists: `get_inner(&self)` passes (verified). `misnamed_getters` deny(all) checks only the body. | med | `get_` prefixes are the most common naming habit carried over from Java and C#. Today this is review-only. |
| 12 | Naming, C-CASE (`naming.html#c-case`) | In CamelCase, acronyms count as one word (`Uuid`, `Usize`, `Stdin`); in snake_case they are lowercase (`is_xid_start`). | tooling (clippy.toml) | Add `upper-case-acronyms-aggressive = true` to `clippy.toml`. Add to CRF-01: "Acronyms are one word: `Uuid`, `HttpClient`, `Stdin`; `is_xid_start`." | clippy.toml. The aggressive setting flags `HTTPClient`, `XMLHttpRequest` and a variant `TCP`, with no false positives on `HttpClient`, `IoError` or `UdpV4` (verified). By default `upper_case_acronyms` deny(all) flags only fully-capitalized names (`UUID`), and on `pub` items only because the template sets `avoid-breaking-exported-api = false` (verified: with the default, `pub` items are skipped). | med | A config change with zero noise that makes a style rule mechanical. |
| 13 | Naming, C-FEATURE (`naming.html#c-feature`) | Feature names carry no placeholder words (`use-abc`, `with-abc`). Use `std`, never `use-std`. A negative name like `no-abc` is "practically never correct". | tooling (template lints) + refine API-16 | **API-16 addition (MUST).** Name a feature after the capability or optional dependency it enables (`std`, `serde`, `tokio`), never `use-x`, `with-x`, `x-support` or `no-x`. Template: `redundant_feature_names = "warn"` and `negative_feature_names = "warn"` in `[workspace.lints.clippy]`. | Both are off: needs enabling (cargo group). They fire on `use-std` (suggest `std`), `serde-support` (suggest `serde`) and `no-alloc` (suggest `alloc`) (verified). | med | Cheap, with zero noise. API-16 already says "positive names" but has nothing that checks it. |
| 14 | Interoperability, C-SEND-SYNC (`interoperability.html#c-send-sync`) | Watch that `Send` and `Sync` reflect thread safety. Tests "can help catch unintentional regressions". | refine ASY-11 | **ASY-11: raise CONSIDER to SHOULD in library crates.** Every public type and public error type has a const `assert_send_sync::<T>()` beside its definition, unless its docs state that it is deliberately `!Send` or `!Sync` (with a compile-fail proof, TST-08). Use the const form, not the source's `#[test] fn`, because it fails at `cargo check`. API-06 and API-17 already depend on this, so the CONSIDER level is inconsistent. | Compiler (const item); review for presence. Optional: `future_not_send` (off: needs enabling; nursery; trial first) to require `Send` futures from public `async fn` in libraries. | med | Level mismatch: API-06 is SHOULD, API-17 says "Assert them at compile time", and ASY-11 is only CONSIDER. |
| 15 | Necessities, C-STABLE (`necessities.html#c-stable`) | Public dependencies can "sneak in". The example `impl From<other_crate::Error> for Error` makes `other_crate` public even though the repr is private. | refine API-14, ERR-04 | **API-14 / ERR-04 addition (SHOULD).** A public error's variant fields and every `#[from]`-generated `From<dep::Error>` impl are public API, so a third-party error type there makes that crate's major version part of your semver. In published crates, keep third-party causes in a private repr (ERR-02's opaque shape), and do not put `#[from]` for a third-party error on a public enum unless the dependency is deliberately public and re-exported. | Review. rustc `exported_private_dependencies` exists but is inert on stable: Cargo prints "ignoring `public` on dependency ... pass `-Zpublic-dependency`" (verified). `cargo semver-checks` does not detect this. | med | thiserror's `#[from]` makes this leak one attribute away. Value is lower for the template default `publish = false`. |
| 16 | Predictability, C-NO-OUT (`predictability.html#c-no-out`) | Return several values as a tuple or struct, not through `&mut` out-parameters. Exception: modifying caller-owned data, for example a reused buffer (`read(&mut buf)`). | new OWN-17 | **OWN-17 SHOULD.** Return results by value, as a tuple or a named struct when there are several. Use a `&mut` parameter only to modify caller-owned data in place, or to fill a caller-supplied buffer for reuse (`read(&mut buf)`, `encode_into(.., &mut String)`, with an `_into` suffix). Never use one to return an extra value next to the return type. | Review. | med | Out-parameter habits from C, C# and Go appear in agent code. OWN-01 says "own in returns" but does not name this. The rule is consistent with `patterns.md:447` (`encode_into`) and PERF-02. |
| 17 | Documentation, C-CRATE-DOC (`documentation.html#c-crate-doc`) | Crate-level docs are thorough and include examples (RFC 1687). | refine API-15 | **API-15 addition (MUST).** Every library crate root has `//!` docs with a one-sentence purpose, a runnable quick-start doctest that uses `?`, and a list of its Cargo features. | `missing_docs` (explicit) checks only that crate docs exist; the content is review. `rustdoc::missing_crate_level_docs` adds nothing beyond `missing_docs`. | med | ARC-05 makes `lib.rs` a table of contents, so the crate doc is the landing page. API-15 requires examples only on "primary entry points" and never names the crate root. |
| 18 | Documentation, C-METADATA (`documentation.html#c-metadata`) | `[package]` includes authors, description, license, repository, keywords and categories. Set `documentation` only when the docs are not on docs.rs, and `homepage` only for a distinct site. | tooling (template lints) + refine ARC-01 | Template: `cargo_common_metadata = "warn"` in `[workspace.lints.clippy]`. **ARC-01 addition (MUST for publishable crates).** Set `description`, `license`, `repository`, `readme`, `keywords` and `categories`, inherited through `[workspace.package]`. Set `documentation` only if the docs are not on docs.rs, and `homepage` only for a distinct site. Omit `authors` (RFC 3052 made it optional). | Off: needs enabling (cargo group). It requires description, repository, readme, keywords and categories, does not check `authors`, and skips `publish = false` crates, so it adds no noise to the template default (verified). | med-low | Makes the metadata requirement mechanical at the moment a crate flips to publishable. |
| 19 | Naming, C-ITER-TY and C-NEWTYPE-HIDE (support); C-HIDDEN | The returned iterator or newtype must be a real, nameable public type. | tooling (template lints) | Template: `unnameable_types = "warn"` in `[workspace.lints.rust]`. | Off: needs enabling (rustc, allow by default). It flags a `pub struct` in a private module returned from a `pub fn` ("reachable but cannot be named"), a case `unreachable_pub` does not flag (verified: silent). | med | Closes an API-14 blind spot. Callers cannot store, name or bound a type they cannot name. |
| 20 | Interoperability, C-COLLECT (`interoperability.html#c-collect`) and C-NUM-FMT (`interoperability.html#c-num-fmt`) | Collections implement `FromIterator` and `Extend`. Types used for bit operations implement `LowerHex`, `UpperHex`, `Octal` and `Binary`; quantities like `Nanoseconds(u64)` do not. | refine API-06 | **API-06 table rows.** (a) `FromIterator` and `Extend`: collection-like types whose invariant holds for any sequence of items (a sorted container sorts on insert). Not for invariants an arbitrary iterator can break (`NonEmpty`). (b) `LowerHex`, `UpperHex`, `Octal` and `Binary`: bit-set and bitmask newtypes you would combine with `\|` or `&`. Not for quantities. | Review. | low-med | Domain collections (`EventLog`, `Playlist`) otherwise cannot be `collect()`ed, so agents write `from_vec` helpers. |
| 21 | Debuggability, C-DEBUG-NONEMPTY (`debuggability.html#c-debug-nonempty`) | `Debug` output is never empty, even for conceptually empty values. | refine OUT-08 / API-06 | **OUT-08 addition (MUST).** A hand-written `Debug` never prints an empty string. Redacting impls use `debug_struct(..).field(<visible fields>).finish_non_exhaustive()` or a placeholder (`Secret([redacted])`). | `missing_fields_in_debug` warn(pedantic) flags a manual `Debug` that drops fields unless it ends with `finish_non_exhaustive()` (both verified). Nothing flags an empty `Debug` (verified), so that part is review. | low-med | Redaction (OUT-08) is where agents write hand-rolled `Debug`. `finish_non_exhaustive()` is the lint-clean redaction idiom and signals that fields are hidden. |
| 22 | Documentation, C-LINK (`documentation.html#c-link`) | Prose links to related items (RFC 1574, "link all the things"). The source's syntax is HTML paths (`trait.Deserialize.html`, `#method.x`). | refine API-15 | **API-15 addition (MUST).** Link with intra-doc paths (``[`Store::load`]``). Never write `*.html` or `#method.` link targets by hand, because rustdoc cannot check them. | `rustdoc::broken_intra_doc_links` (default warn; denied by `RUSTDOCFLAGS=-D warnings` in CI) checks intra-doc links only. `doc_markdown` warn(pedantic). Scanner, soft (regex R4). | low-med | Agents trained on old docs copy HTML-path links, which rot silently. |
| 23 | Documentation, C-RELNOTES (`documentation.html#c-relnotes`) | Release notes cover every significant change and flag breaking changes clearly. Every published release gets a matching annotated tag. | refine PROC-11 / API-17 | **PROC-11 / API-17 addition (MUST for published crates).** Keep `CHANGELOG.md` with an `Unreleased` section updated in the same change, and list breaking changes under a `Breaking` heading. Give every published version an annotated tag (`git tag -a vX.Y.Z`), which is also the `cargo semver-checks --baseline-rev`. | Review. | low-med | PROC-11 says only "CHANGELOG if present". API-17 already assumes a release tag exists. |
| 24 | Predictability, C-CONV-SPECIFIC (`predictability.html#c-conv-specific`) | Conversions live on the more specific type, the one with the extra invariant (`str::from_utf8`, `str::as_bytes`), so general types are not "polluted with endless conversion methods". | refine API-07 / API-18 | **API-07 addition (SHOULD).** Put a conversion on the more specific type: `impl TryFrom<&str> for EmailAddress` and `EmailAddress::as_str`. Never use a `StrExt::to_email()` extension method or a free `fn to_email(&str)`. **API-18:** extension traits add behaviour to foreign types, not conversions into your types. | Review. | low-med | API-18 encourages extension traits, and agents then write `str::to_email()`. |
| 25 | Naming, C-WORD-ORDER (`naming.html#c-word-order`) | Names of the same kind use one word order, consistent within the crate and with similar std items (`ParseIntError`, so `ParseAddrError`). | new CRF-11 | **CRF-11 SHOULD.** Names of the same kind (error types, iterator types, constructors) use one word order across the crate, matching std where std has a pattern: verb-object-`Error` (`ParseAddrError`, `LoadConfigError`) and iterator types named after their method (row 9). | Review. | low-med | rust-craft's own examples mix `LoadConfigError` (verb-object) with `EmailError`, `PairError` and `StoreError` (object only). |
| 26 | Documentation, C-HIDDEN (`documentation.html#c-hidden`) | Hide unhelpful impls with `#[doc(hidden)]`. `pub(crate)` removes implementation details. | refine API-14 | **API-14 addition (SHOULD).** Use `#[doc(hidden)]` only on items that must be `pub` for macro expansion or sealed-trait internals; use `pub(crate)` for everything else. An impl that mentions a private type (`From<PrivateError> for PublicError`) needs no attribute, because rustdoc omits it. | Review. `unreachable_pub` is explicit. rustdoc 1.98.1 omits the source's example impl without `#[doc(hidden)]` (verified). | low | Stops agents copying an outdated idiom. |
| 27 | Future proofing, C-SEALED (`future-proofing.html#c-sealed`) | Document in rustdoc that a trait is sealed, "to avoid frustrated users". | refine API-12 | **API-12 addition (MUST).** A sealed trait's rustdoc states that it is sealed and cannot be implemented outside the crate. | Review. | low | The small missing piece in pattern #6. |
| 28 | Predictability, C-SMART-PTR (`predictability.html#c-smart-ptr`) | Smart pointers do not add inherent methods (`Box::into_raw(b)`), so calls are not confused with methods of the target. | refine API-19 | **API-19 addition (SHOULD).** A type that implements `Deref` (a pointer or guard) exposes its own operations as associated functions (`Guard::release(this)`), not `self` methods, so they never shadow the target's methods. | Review. `same_name_method` (off: restriction) covers only part of this. | low | Completes API-19 for the legitimate `Deref` case. |
| 29 | Macros, C-EVOCATIVE, C-MACRO-ATTR, C-ANYWHERE, C-MACRO-VIS, C-MACRO-TY (`macros.html`) | An item macro mirrors Rust syntax, accepts attributes and a visibility on each generated item, works at module and function scope, and accepts every form of `$t:ty`. | refine ARC-15 | **ARC-15 addition (MUST when an item-generating `macro_rules!` is justified).** Accept `$(#[$meta:meta])*` per item and `$vis:vis`. Mirror Rust syntax (`struct Name`, `const X = ..;`). Accept any `$t:ty`: primitives, relative, absolute and `super::` paths, and generics. Test invocations at module scope and inside a function. | Review, plus the two scope tests. | low | ARC-15 makes macros rare. When one is written, this is its checklist. |
| 30 | Necessities, C-PERMISSIVE (`necessities.html#c-permissive`) | Dual MIT/Apache-2.0 with `LICENSE-MIT` and `LICENSE-APACHE` files. A permissively licensed crate should depend only on permissively licensed crates. | tooling (deny.toml) / refine ARC-12 | Record an explicit decision on `MPL-2.0` in `deny.toml`'s allow-list, because it is file-level copyleft and not permissive in the source's sense: keep it for binaries, review it for published libraries. Published crates ship `LICENSE-MIT` and `LICENSE-APACHE` files matching the template's `MIT OR Apache-2.0`. | cargo-deny `[licenses]`. | low | The template's `license` field matches, but the allow-list goes beyond "permissive". |
| 31 | Documentation, C-EXAMPLE (`documentation.html#c-example`) | Every public item has an example "within reason". Examples show why to use the item, not just how. Linking to a shared example is fine. | refine API-15 | **API-15 addition (SHOULD).** An `# Examples` block shows a realistic reason to call the item, not only its call syntax. Link to a shared example instead of duplicating one. | Review only: `rustdoc::missing_doc_code_examples` is still unstable on 1.98.1 (verified, "unknown lint ... is unstable"). | low | rust-craft's narrower scope ("primary entry points") is reasonable. This row adds only the quality bar. |
| 32 | Naming, C-CASE (incidental template check) | n/a | tooling (template hygiene) | Remove `module_name_repetitions = "allow"` from the template, or mark it as documentation only. On 1.98.1 the lint is in `restriction` and allowed by default, so the line does nothing. | Template. | low | Template accuracy. |
| 33 | Type safety, C-CUSTOM-TYPE (incidental doc check) | n/a | refine API-10 (doc fix) | The API-10 Detect line in `api-design.md:471` says "`too_many_arguments` (threshold 5)". `clippy.toml` and `rules.md` say 4. Change it to 4. | Doc. | low | Internal contradiction. |

### Scanner regex sketches (JS, run over masked code or doc-comment text as noted)

- **R1** (row 3, soft, API-21). Runs on masked code: `/\b(?:struct|enum)\s+\w+\s*<[^>{;]*?:\s*[^>{;]*?\b(?:Clone|Debug|PartialEq|Eq|PartialOrd|Ord|Hash|Default|Display|Serialize|Deserialize)\b/g`. It also needs a second pass for `where` clauses between the name and `{` or `;`.
- **R2** (row 7, hard, H-02/TST-11). Runs on doc comments in order. Strip `^\s*//[/!]\s?`. Toggle `inFence` on `/^```/`, and record the fence info string. When `inFence` is true and the info string matches neither `text` nor `compile_fail`, test `/\.\s*(?:unwrap|expect)\s*\(|\b(?:panic|todo|unimplemented)\s*!/`.
- **R3** (row 11, soft, API-01). Runs on masked code: `/\bfn\s+get_(?!mut\b|ref\b|pin_mut\b|unchecked)\w+\s*\(\s*&\s*(?:mut\s+)?self\s*\)/g`. It matches getters whose only parameter is the receiver.
- **R4** (row 22, soft, API-15). Runs on doc comments: `/\]\(\s*[\w./-]*\.html(?:#[\w.-]+)?\s*\)|^\s*\[[^\]]+\]:\s*[\w./-]*\.html/`.

## Conflicts

- **C-GETTER `_unchecked` variants** (`naming.html#c-getter`) conflict with **H-01**. The source suggests `unsafe fn get_unchecked(&self, index) -> &V`, and rust-craft forbids `unsafe` everywhere. Compatible nugget: the safe half, `get(&self, k) -> Option<&V>` and `get_mut`, is already rust-craft's form (H-04). Nothing else to take.
- **C-VALIDATE dynamic enforcement** (`dependability.html#c-validate`) conflicts with **H-03**, **H-01** and **API-03**. The source lists `panic!` as a valid failure mechanism and "opt-out" `_unchecked` functions or a `raw` submodule; rust-craft forbids panics and unvalidated constructors of invariant types. Compatible nugget: the source's order of preference (static enforcement first, then dynamic) matches API-03. `debug_assert!` for internal sanity checks is already the scanner's suggested fix.
- **C-FAILURE `# Panics` and `# Safety` sections** (`documentation.html#c-failure`) conflict with **H-03**, **H-01** and API-15. The source says to document panic and safety conditions; rust-craft says production code never panics and never uses `unsafe`, so these sections should never exist. Compatible nugget: `missing_panics_doc` is already on through pedantic, and it fires on `assert!` in a `pub fn` (verified). It works as a panicking-path detector, so keep it at warn (denied by the gate).
- **Divergence, not a settled-position conflict: C-BUILDER** (`type-safety.html#c-builder`). The source marks non-consuming builders "(preferred)". rust-craft's API-10 table makes consuming builders the default, because a non-consuming `build(&self)` must clone its stored parts, which is the H-08 concern. Both forms are documented (patterns #4 and #5). Leave it as is and cite H-08 as the reason.

## Patterns / examples worth copying

All compile clean with `cargo clippy --all-targets -- -D warnings` under the template lints. The full files are in `scratchpad/apiguide/pat/src/`.

**P1. Hand-back error** (row 1, ERR-13). The full file adds `Display`, `impl<T: Debug> Error` and a doctest that ends with `# Ok::<(), OutboxFull<&str>>(())`.
```rust
pub struct OutboxFull<T> { item: T, capacity: usize }
impl<T> OutboxFull<T> {
    /// Recovers the item that was not enqueued, without a clone.
    pub fn into_inner(self) -> T { self.item }
}
pub fn push(&mut self, item: T) -> Result<(), OutboxFull<T>> {
    if self.items.len() >= self.capacity {
        return Err(OutboxFull { item, capacity: self.capacity });
    }
    self.items.push(item);
    Ok(())
}
```

**P2. Explicit `close`, best-effort `Drop`** (rows 5 and 8, ERR-14). `Journal<W: Write>` keeps the bound on the struct because the `Drop` impl needs it. That is exception 3 of C-STRUCT-BOUNDS: removing it gives E0367 (verified).
```rust
pub fn close(mut self) -> io::Result<W> {
    let sink = self.sink.take().ok_or(io::ErrorKind::BrokenPipe)?;
    sink.into_inner().map_err(io::IntoInnerError::into_error)
}
impl<W: Write> Drop for Journal<W> {
    fn drop(&mut self) {
        if let Some(mut sink) = self.sink.take() && let Err(error) = sink.flush() {
            tracing::warn!(%error, "journal dropped without close; flush failed");
        }
    }
}
```

**P3. Named iterator newtype** (row 9, API-01 and API-08).
```rust
/// Iterator over a [`Playlist`]'s tracks, returned by [`Playlist::iter`].
#[derive(Debug, Clone)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Iter<'a>(slice::Iter<'a, String>);
impl<'a> Iterator for Iter<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> { self.0.next().map(String::as_str) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.0.size_hint() }
}
// + DoubleEndedIterator, ExactSizeIterator, and IntoIterator for &Playlist with type IntoIter = Iter<'a>
```

**P4. Dyn-compatibility guard** (row 4, API-09).
```rust
pub trait EventSink {
    fn record(&mut self, event: &str);
    fn record_all<I>(&mut self, events: I)
    where I: IntoIterator, I::Item: AsRef<str>, Self: Sized
    { for event in events { self.record(event.as_ref()); } }
}
// Fails here (E0038), in this crate, if a method breaks dyn compatibility.
const _: Option<&dyn EventSink> = None;
```

**P5. Bounds on impls, not data** (row 3, API-21).
```rust
#[derive(Debug, Clone)]
pub struct Lookup<S> { store: S }          // no `S: Store` here
impl<S: Store> Lookup<S> {
    pub fn new(store: S) -> Self { Self { store } }
    #[must_use]
    pub fn len_of(&self, id: u64) -> Option<usize> { self.store.load(id).map(|r| r.len()) }
}
```

**P6. Checked operation instead of an overflowing operator** (row 10, API-22).
```rust
impl Cents {
    /// `None` on overflow. Chosen over `impl Add`, whose `+` panics in debug and wraps in release.
    #[must_use]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) { Some(sum) => Some(Self(sum)), None => None }
    }
}
```

**P7. Generic writer by value** (row 2): `pub fn write_report<W: Write>(mut out: W, ..)`. Callers keep the writer with `write_report(&mut w, ..)` or `write_report(&mut *dyn_sink, ..)`.

## Notable skips

- **C-BITFLAG** (`type-safety.html#c-bitflag`): it needs the `bitflags` crate, a new dependency (H-16) that is not on the pre-approved list. Row 20 covers formatting for hand-written bitmask newtypes.
- **C-SEND-SYNC raw-pointer and `unsafe impl Send` concerns**: unsafe-only (H-01).
- **C-FEATURE `#![no_std]` with `extern crate std;`**: no_std-only. Side note: the scanner's `extern-crate` rule (MOD-01, soft) would flag this canonical line in a no_std crate with a `std` feature. Exempt `extern crate std;` and `extern crate alloc;` if no_std crates are ever in scope.
- **C-PERMISSIVE README licence boilerplate**: a legal and process matter, not engineering guidance.
- **C-CASE "crate names must not use `-rs` or `-rust`"**: low value for workspace member crates. Worth one line in ARC-07 at most.

## Outdated relative to edition 2024 and Rust 1.98

1. **C-OBJECT.** "Object safety" is now **dyn compatibility**: rustc 1.98.1's E0038 reads "the trait `X` is not dyn compatible" (verified). The chapter predates `async fn` in traits and RPITIT (1.75), which are not dyn-compatible unless marked `where Self: Sized` (verified). It also predates trait upcasting (1.86, MOD-11). rust-craft itself still says "object-safe" in 3 places (row 4).
2. **C-QUESTION-MARK.** It mentions `try!`, but `try` has been a reserved keyword since edition 2018 (only `r#try!` works). Doctests can now end with `# Ok::<(), E>(())` instead of a hidden `fn main` (verified). The source's hidden-main example uses `Box<dyn Error>`, which is fine in a doctest; H-09 governs only `pub` signatures.
3. **C-GOOD-ERR.** It cites `error-chain`, which is unmaintained and banned in the template's `deny.toml`. It warns against `Error::description()`, deprecated since 1.42, and now irrelevant because thiserror never generates it. It predates `core::error::Error` (1.81).
4. **C-LINK.** Its link targets are HTML paths (`trait.Deserialize.html`, `#method.x`). Intra-doc links (stable since 1.48) replace them, and rustdoc checks only intra-doc links.
5. **C-METADATA.** It lists `authors`, which RFC 3052 made optional. Clippy's `cargo_common_metadata` does not check `authors` and does check `readme`, which the source omits (verified).
6. **C-HIDDEN.** Its example (`#[doc(hidden)] impl From<PrivateError> for PublicError`) is unnecessary: rustdoc 1.98.1 omits impls that involve private types (verified).
7. **C-SERDE / C-FEATURE.** They rely on implicit optional-dependency features. `dep:` syntax (Cargo 1.60, `serde = ["dep:serde"]`) supersedes them, and rust-craft already uses it. "Features: unclear" now has lints (row 13).
8. **C-NEWTYPE-HIDE.** "Rust 1.26 introduces `impl Trait`". Edition 2024 return-position `impl Trait` captures all in-scope lifetimes (MOD-06). `impl Trait` in associated types is still unstable on 1.98.1 (verified), so named newtypes are still required for `IntoIterator::IntoIter`.
9. **C-BUILDER.** The consuming example uses `Box<io::Write + Send>`, a bare trait object, which is a hard error since edition 2021 (`dyn` is required).
10. **C-SEND-SYNC.** Its `#[test] fn test_send()` assertions are superseded by a const-item assertion (trait bounds on `const fn` are stable since 1.61), which rust-craft already uses and which fails at `cargo check`.
11. **C-EXAMPLE.** `rustdoc::missing_doc_code_examples` exists but is still unstable on 1.98.1 (verified), so there is no stable lint.
12. **C-STABLE.** The concept is still right, but its would-be lint `exported_private_dependencies` is inert on stable: Cargo ignores `public = false` without `-Zpublic-dependency` (verified).

## Appendix: C-ID mapping (54 items)

Lint column: the lint that enforces the item, if any, and whether the template enables it.

| C-ID | rust-craft rule(s) | Status | Lint enforcement (template state) |
|---|---|---|---|
| C-CASE | CRF-01, API-01 (partial) | tooling (row 12), hygiene (row 32) | rustc `nonstandard_style` (default warn; gate denies). `upper_case_acronyms` deny(all), fully-capitalized names only unless the aggressive setting is on. `enum_variant_names` deny(all). |
| C-CONV | API-01, OWN-04 | refine (row 6) | `wrong_self_convention` deny(all), receivers only; cost is unchecked |
| C-GETTER | API-01 | refine (row 11) + conflict (`_unchecked`) | none for `get_`; `misnamed_getters` deny(all) checks bodies only |
| C-ITER | API-01 | covered (Detect line refined in row 9) | `iter_without_into_iter`, `into_iter_without_iter`, `iter_not_returning_iterator` warn(pedantic) |
| C-ITER-TY | none | refine (row 9) | none; `unnameable_types` off: needs enabling (row 19) |
| C-FEATURE | API-16 | tooling + refine (row 13) | `redundant_feature_names`, `negative_feature_names` off: needs enabling (cargo) |
| C-WORD-ORDER | CRF-01 (general only) | new CRF-11 (row 25) | none |
| C-COMMON-TRAITS | API-06, OWN-16 | covered | `missing_debug_implementations` explicit; `new_without_default` deny(all); `missing_copy_implementations` off (keep off, OWN-16) |
| C-CONV-TRAITS | API-07 | covered | `from_over_into`, `should_implement_trait`, `infallible_try_from` deny(all) |
| C-COLLECT | none | refine API-06 (row 20) | none |
| C-SERDE | API-06, API-16, OUT-01 | covered | none |
| C-SEND-SYNC | API-06, ASY-11, API-17 | refine (row 14) | none (const assertion); `future_not_send` off: needs enabling (nursery); `arc_with_non_send_sync` deny(all) (adjacent) |
| C-GOOD-ERR | ERR-01, ERR-05, H-09 | covered | `result_unit_err` deny(all); `error_impl_error` off (restriction, not needed) |
| C-NUM-FMT | none | refine API-06 (row 20) | none |
| C-RW-VALUE | OWN-01 (contradicted by examples) | refine (row 2) | none; `needless_pass_by_value` warn(pedantic) does not fire on this form (verified) |
| C-EVOCATIVE | ARC-15 | refine (row 29) | none |
| C-MACRO-ATTR | ARC-15 | refine (row 29) | none |
| C-ANYWHERE | ARC-15 | refine (row 29) | none |
| C-MACRO-VIS | ARC-15 | refine (row 29) | none |
| C-MACRO-TY | ARC-15 | refine (row 29) | none |
| C-CRATE-DOC | API-15, ARC-05 | refine (row 17) | `missing_docs` explicit (presence only) |
| C-EXAMPLE | API-15, TST-11 | refine (row 31) | none on stable (`rustdoc::missing_doc_code_examples` is unstable) |
| C-QUESTION-MARK | API-15, TST-11 | tooling (row 7) | none: clippy skips doctests and the scanner skips doc comments |
| C-FAILURE | H-15, API-15 (+ H-01, H-03) | covered (Errors) + conflict (Panics/Safety) | `missing_errors_doc` warn(pedantic), includes trait methods; `missing_panics_doc` warn(pedantic); `missing_safety_doc` deny(all), moot under H-01 |
| C-LINK | API-15 | refine (row 22) | `rustdoc::broken_intra_doc_links` (default warn; `-D warnings` in CI); `doc_markdown` warn(pedantic) |
| C-METADATA | ARC-01 (partial) | tooling + refine (row 18) | `cargo_common_metadata` off: needs enabling (cargo); skips `publish = false` |
| C-RELNOTES | PROC-11, API-17 | refine (row 23) | none |
| C-HIDDEN | API-14 | refine (row 26) | `unreachable_pub` explicit |
| C-SMART-PTR | API-19 | refine (row 28) | none; `same_name_method` off (restriction) |
| C-CONV-SPECIFIC | API-07, API-18 | refine (row 24) | none |
| C-METHOD | ARC-10 | covered | none |
| C-NO-OUT | none (PERF-02 covers buffer reuse only) | new OWN-17 (row 16) | none |
| C-OVERLOAD | ERR-09 (partial) | new API-22 (row 10) | `suspicious_arithmetic_impl`, `suspicious_op_assign_impl` deny(all), mismatched operator only; `arithmetic_side_effects` off: needs enabling (restriction; H-05 decision) |
| C-DEREF | API-19 | covered | none; scanner `deref-inherit` (soft) |
| C-CTOR | API-01 | covered | `new_ret_no_self`, `new_without_default`, `self_named_constructors`, `should_implement_trait` deny(all) |
| C-INTERMEDIATE | none as a rule (examples only: `errors.md:474`, `patterns.md:652`) | new ERR-13 (row 1) | none |
| C-CALLER-CONTROL | OWN-01, OWN-02, H-08 | covered | `needless_pass_by_value`, `unnecessary_box_returns` warn(pedantic); `unnecessary_to_owned` deny(all); `redundant_clone` explicit |
| C-GENERIC | API-08 | covered | `implicit_hasher` warn(pedantic) |
| C-OBJECT | API-09, API-17, ASY-03, MOD-11 | refine (row 4) | none (rustc E0038 at use sites; a const assertion makes it local) |
| C-NEWTYPE | API-02 | covered | none |
| C-CUSTOM-TYPE | API-02, API-10 | covered (doc fix in row 33) | `fn_params_excessive_bools`, `struct_excessive_bools`, `option_option` warn(pedantic); `too_many_arguments` deny(all) at 4 |
| C-BITFLAG | none | skip | none |
| C-BUILDER | API-10, API-13, patterns #4/#5 | covered (divergence noted) | `must_use_candidate`, `return_self_not_must_use` warn(pedantic) |
| C-VALIDATE | API-03 | covered (static) + conflict (panic, unchecked) | none |
| C-DTOR-FAIL | patterns.md #8 note only | new ERR-14 (row 5) | none |
| C-DTOR-BLOCK | H-14, ASY-06 (async code only) | refine (row 8) | none |
| C-DEBUG | API-06 | covered | `missing_debug_implementations` explicit |
| C-DEBUG-NONEMPTY | OUT-08 (example is non-empty) | refine (row 21) | `missing_fields_in_debug` warn(pedantic) (partial); nothing for empty output |
| C-SEALED | API-12, pattern #6 | covered + refine (row 27) | none |
| C-STRUCT-PRIVATE | API-04 | covered | `exhaustive_structs`, `partial_pub_fields` off (restriction; api-design.md explains why they are not enabled) |
| C-NEWTYPE-HIDE | API-08, pattern #13 | refine (row 9) | `must_use_candidate` warn(pedantic) (pushes `#[must_use]` onto newtype iterators) |
| C-STRUCT-BOUNDS | none (`architecture.md:669` contradicts it) | new API-21 (row 3) | none (verified) |
| C-STABLE | API-14 | refine (row 15) | `exported_private_dependencies` inert on stable |
| C-PERMISSIVE | ARC-12, `deny.toml`, template `license` | tooling / refine (row 30) | cargo-deny `[licenses]` (explicit in CI) |
