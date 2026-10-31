# Rule registry

This is the canonical list of rules. Reviewers cite these IDs in findings. Implementers cite them in plans, when deviating from a SHOULD, and in exception requests.

**Level.** MUST blocks merge. SHOULD is the default and needs a stated reason to deviate. CONSIDER is judgment.

**Enforced by.** `lint` means a compiler or clippy lint denied in the workspace template. `scan` means the temper pre-write scanner. `hook` means a temper lifecycle hook. `gate` means the completion gate. `review` means human or agent review.

**Review-only by design.** These MUST rules need judgement or describe how the work is done, so no tool can check them without false positives: API-04, ARC-03, TST-01, TST-02, TST-16, PERF-01, PERF-10, ASY-01, ASY-05, OUT-03, OUT-05, PROC-01, PROC-04, PROC-06, PROC-07, PROC-09 and PROC-11. They still block merge, through the reviewer's verdict (PROC-08), and the review rubric names each one.

**Advisory scans under MUST rules.** For API-02, API-19, API-21, ARC-09, ARC-11, TST-10, ASY-04, ASY-07, ASY-17, OUT-04 and CRF-04 (and `SeqCst` under ASY-13), the only tool is a scanner advisory: each pattern it looks for has legitimate uses that a blocking check would refuse, and the template enables no lint for it, because a new lint would fail code that adopted repositories already have. The gate lists the advisories a change adds under MUST rules, and the reviewer judges each one: fixed, justified in a comment at the code, or a finding under its rule.

**Detail** points to the reference file that explains the rule, with examples. `sources.md` records which external guidance each rule draws on, and what was deliberately not adopted.

**Production code** means everything except `#[cfg(test)]` items and `#[test]` functions. Test relaxations, including clippy's `allow-*-in-tests`, apply only inside those, so plain helper functions in `tests/` are not exempt. Benchmarks (`benches/`, compiled without `cfg(test)`), `examples/`, `build.rs` and doctest examples follow production rules.

## H: Iron rules (escape hatches)

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| H-01 | MUST | No `unsafe` code anywhere. | lint `unsafe_code=forbid`, scan | escape-hatches.md |
| H-02 | MUST | No `unwrap`/`expect`/`unwrap_unchecked`/`unwrap_err`/`expect_err` in production code, including doctest examples. | lint `clippy::unwrap_used`, `clippy::expect_used`, scan (code and doctest fences) | escape-hatches.md, errors.md |
| H-03 | MUST | No `panic!`/`todo!`/`unimplemented!`/`unreachable!` in production code, and no call to a std API that panics on input the code does not control (the escape-hatches.md table). | lint `clippy::panic`, `todo`, `unimplemented`, `unreachable`, `disallowed_methods` (thread spawn, `env::args`, float `Duration`); scan; review | escape-hatches.md |
| H-04 | MUST | No panicking index or slice on data whose bounds are not proven by construction. That includes `map[&k]` on `HashMap`/`BTreeMap` and `deque[i]` on `VecDeque`, which no lint catches. | lint `clippy::indexing_slicing`, `string_slice`; scan (advisory: map indexing); review | escape-hatches.md |
| H-05 | MUST | No `#[allow]`. `#[expect(..., reason = "...")]` only with human approval. Never weaken lint, format, CI or hook config. | lint `clippy::allow_attributes`, `allow_attributes_without_reason`, scan, hook (protected files) | escape-hatches.md |
| H-06 | MUST | No `dbg!`. No print macros in production code: libraries use `tracing`, binaries and build scripts use `writeln!` on a locked handle. No commented-out code. No untracked TODOs. | lint `clippy::dbg_macro`, `print_stdout`, `print_stderr`; scan (prints in `build.rs`, TODOs, commented-out code); review | escape-hatches.md |
| H-07 | MUST | No `Box::leak`, `mem::forget`, `transmute`, `static mut`, or `'static` added to silence lifetimes. Shared mutability must be justified. | lint `clippy::mem_forget`, `disallowed_methods`, `static_mut_refs`; scan; review | escape-hatches.md, ownership.md |
| H-08 | MUST | No clones to appease the borrow checker. Every clone has an ownership reason. A clone forced by a consuming fallible API is a finding against that API (ERR-14). | lint `clippy::redundant_clone`, `clone_on_ref_ptr`; hook (advisory on new clones); review | ownership.md |
| H-09 | MUST | No `String`, `Box<dyn Error>` or `anyhow`/`eyre` errors in public library signatures. | scan, review | escape-hatches.md, errors.md |
| H-10 | MUST | No silently discarded failures (`let _ =`, `.ok();`, empty `Err` arms, masking defaults). | lint `let_underscore_must_use`, `unused_must_use`; scan | errors.md |
| H-11 | MUST | No lossy `as` numeric casts, including enum discriminants: map a fieldless enum to a code with an exhaustive `match` in `From`. | lint `clippy::as_conversions`, `cast_*`; scan | escape-hatches.md |
| H-12 | MUST | Never weaken verification (tests, lints, hooks, CI) to get green. That includes manifest switches that stop tests running (`test`, `doctest`, `harness`, `autotests`), `#[ignore]` or `cfg_attr(.., ignore)` without a reason, and raising the lint baseline (`temper baseline --update`). | hook (test removal, manifest test switches, config edits and baseline raises prompt the human), scan, review | testing.md |
| H-13 | MUST | Bounded resources: channels, buffers, timeouts, owned tasks and threads (joined or aborted; `spawn_blocking` tasks awaited). | review; scanner advisories under ASY-04 and ASY-07 | escape-hatches.md, async-concurrency.md, performance.md |
| H-14 | MUST | No blocking in async code. No lock guard, `RefCell` borrow, entered tracing span or `watch::Ref` held across `.await`. `Drop` never blocks (ERR-13). | lint `clippy::await_holding_lock`, `await_holding_refcell_ref`, `await_holding_invalid_type`; review | async-concurrency.md |
| H-15 | MUST | Zero warnings. fmt, clippy, tests and docs are green. Public items are documented. | lint `missing_docs`, `clippy::missing_errors_doc`; hook (gate) | process.md |
| H-16 | MUST | New dependencies and major-version bumps, `[patch]` overrides, `rust-version` raises, API breaks, exceptions, crate boundaries, migrations and test rewrites are human decisions. | review, hook (asks on new dependencies, protected files and manifest keys; refuses `--ignore-rust-version`) | process.md |

## OWN: Ownership, borrowing, lifetimes

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| OWN-01 | MUST | Borrow in parameters, own in returns. Take `&T`, `&mut T`, `&str` or `&[T]` unless the function stores or consumes the value. Exception: generic I/O parameters are `R: Read`/`W: Write` by value, and callers pass `&mut reader` to keep it. | lint `clippy::needless_pass_by_value`, review | ownership.md |
| OWN-02 | SHOULD | Take ownership when the callee stores or transforms the value. Let the caller decide whether to clone. Use `impl Into<String>` only for constructors and builders. | review | ownership.md |
| OWN-03 | MUST | Never accept `&String`, `&Vec<T>`, `&Box<T>`, `&PathBuf` or `&OsString`. Use `&str`, `&[T]`, `&T`, `&Path` or `&OsStr`. | lint `clippy::ptr_arg`, `borrowed_box`; scan | ownership.md |
| OWN-04 | SHOULD | Choose the receiver deliberately: `&self` to query, `&mut self` to mutate, `self` to convert, consume or transition typestate. | lint `clippy::wrong_self_convention`, review | ownership.md |
| OWN-05 | SHOULD | Rely on lifetime elision. Name lifetimes only when elision cannot express the relationship, tie an output only to the inputs it borrows from, and give lifetimes meaningful names in types (`'src`, `'buf`). | lint `clippy::needless_lifetimes`, `elidable_lifetime_names`, review | ownership.md |
| OWN-06 | SHOULD | Borrowing structs (`struct View<'a>`) are for short-lived views, parsers and iterators. Long-lived domain data owns its data. Do not thread lifetimes through application layers. | review | ownership.md |
| OWN-07 | MUST | Do not add `'static` bounds, `Box::leak` or globals to silence lifetime errors. Fix the ownership model instead: move ownership, scope threads, or share an `Arc`. | review; the H-07 scanner rules | ownership.md |
| OWN-08 | MUST | Resolve borrow conflicts by restructuring, not cloning. Split borrows (disjoint fields, `split_at_mut`, `get_disjoint_mut`), shorten borrow scopes, compute then mutate, and use `mem::take`/`mem::replace` to move out of `&mut`. | lint `clippy::mem_replace_with_default`, review | ownership.md |
| OWN-09 | SHOULD | Use the entry API for insert-or-update. Never do a double lookup (`contains_key` then `insert`). Exception: with `String` keys looked up by `&str`, use `get_mut` and then `insert(key.to_owned())` on a miss, which avoids allocating a key on every call. | lint `clippy::map_entry`, review | ownership.md |
| OWN-10 | SHOULD | Return `Cow<'_, T>` when a function usually borrows but sometimes must allocate. | review | ownership.md |
| OWN-11 | MUST | Walk the sharing ladder: single owner, then borrow, then move into the task, then `Arc<T>`, then `Arc<Mutex<T>>` with a written justification. Use `Rc`/`RefCell` only for single-threaded shared graphs, and prefer an arena with index handles. Back-references are `Weak`. Take a value back out with `Arc::into_inner` and update shared snapshots with `Arc::make_mut`, not a clone. | lint `clippy::arc_with_non_send_sync`, review, scan (advisory) | ownership.md |
| OWN-12 | SHOULD | Interior mutability (`Cell`, `RefCell`, `Mutex`, atomics) is a documented design decision on the type. Never use it to patch a wrong receiver. A `Mutex` owns what it protects: never a free-standing `Mutex<()>`. | review, scan (advisory) | ownership.md |
| OWN-13 | SHOULD | Return iterators or slices instead of allocating a collection the caller only iterates. | lint `clippy::needless_collect`, review | ownership.md |
| OWN-14 | SHOULD | Closures borrow by default. Use `move` for threads, tasks and returned closures. Clone only the specific handle before the `move`, never the whole struct, and build the closure in a capture block that rebinds the clone under the same name. | review | ownership.md |
| OWN-15 | CONSIDER | Use `Box<str>`, `Box<[T]>`, `Arc<str>` or `Arc<[T]>` for immutable owned data that never grows. | review | ownership.md |
| OWN-16 | SHOULD | Derive `Copy` only for small value-like types that will stay small. Keep expensive clones explicit. | lint `clippy::trivially_copy_pass_by_ref`, `large_types_passed_by_value`, review | ownership.md |
| OWN-17 | SHOULD | Return results by value, as a tuple or named struct when there are several. A function that fills a caller-owned buffer is named `*_into` and documents whether it appends or clears. Never use a `&mut` parameter to return an extra value. | review | ownership.md |

## API: Public API design

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| API-01 | MUST | Follow standard naming. `as_` is free and borrowed, `to_` is costly or `Copy`-to-owned, `into_` consumes a non-`Copy` value. Getters are the field name (`x`, `x_mut`), with `get` only for keyed lookup or the single obvious value. `iter`/`iter_mut`/`into_iter` return iterator types named after the method. | lint `clippy::wrong_self_convention` (receivers only), `iter_without_into_iter`, `into_iter_without_iter`; scan (advisory: `get_` getters); review | api-design.md |
| API-02 | MUST | Make illegal states unrepresentable. Use enums instead of flags plus optional fields, and newtypes for identifiers, units and validated values. A `type` alias is not a newtype. Never use in-band sentinels (`-1`, `""`, `u32::MAX`) for absence or failure. | review, scan (advisory: primitive type aliases) | api-design.md |
| API-03 | MUST | Parse, don't validate. A validating constructor returns `Result<Self, E>` (`TryFrom`, `FromStr`), fields stay private, and the invariant is never re-checked. Each invariant-carrying type lives in its own module, because privacy is per module. No infallible `From<raw>` for a validated type. | review | api-design.md |
| API-04 | MUST | Public structs have private fields unless they are plain data with no invariants. | review | api-design.md |
| API-05 | SHOULD | In published crates, mark public enums and structs that may grow (errors, config, events) `#[non_exhaustive]`, deliberately and at first publication, since adding it later is breaking. Inside a workspace, enums stay exhaustive so matches break (CRF-03); Decider events are exhaustive. Struct-like variants that may gain fields get it too. Never emulate it with a private `_priv: ()` field. | lint `clippy::manual_non_exhaustive`, review | api-design.md |
| API-06 | SHOULD | Implement standard traits eagerly where they make sense: `Debug` on every public type, plus `Clone`, `PartialEq`/`Eq`/`Hash`, `Default`, `Display` for user-facing types, `From`/`TryFrom`, `FromStr`. Collections also implement `FromIterator`, `Extend` and `IntoIterator` for `&T` and `&mut T`. Keep public types `Send + Sync` unless intentionally not. | lint `missing_debug_implementations`, `clippy::iter_without_into_iter`, review | api-design.md |
| API-07 | SHOULD | Express conversions through `From`/`TryFrom`/`AsRef`/`FromStr` rather than ad-hoc `from_x` functions, and put each conversion on the more specific type (`TryFrom<&str> for Email`), never on `str` through an extension trait. | lint `clippy::should_implement_trait`, `from_over_into`, review | api-design.md |
| API-08 | SHOULD | Accept generic inputs at boundaries (`impl AsRef<Path>`, `impl IntoIterator`, `impl RangeBounds<T>`) and return concrete types, or `impl Trait` when the type is unnameable or private. Parsers and codecs take `impl BufRead`/`impl Read`/`impl Write`, not paths (sans-IO). Callbacks take the weakest `Fn*` bound the body needs. | review | api-design.md |
| API-09 | SHOULD | Default to static dispatch. Choose `dyn Trait` deliberately, for heterogeneity, runtime plug-ins or code-size control. A public trait used as `dyn` stays dyn-compatible (`where Self: Sized` on generic methods) and says so with `const _: Option<&dyn Trait> = None;`. For a runtime choice local to one function, use on-stack `&mut dyn Trait`, not a `Box`. | review | api-design.md |
| API-10 | MUST | At most one `bool` parameter, and only where it reads naturally at the call site (`set_visible(true)`). Never `Option<bool>`. More than four parameters, or optional configuration, means a params struct or a builder: `FooBuilder`, created by `Foo::builder(required..)`, with setters named after fields and validation in `build()`. Recurring parameters keep one order across a crate. | lint `clippy::fn_params_excessive_bools` (max 1), `too_many_arguments` (max 4), review | api-design.md |
| API-11 | SHOULD | Mark values that must be used `#[must_use]`: pure constructors, builders, iterators and computed results. | lint `clippy::must_use_candidate`, `return_self_not_must_use` | api-design.md |
| API-12 | SHOULD | Seal traits that are not meant to be implemented downstream, and say so in the trait's docs. | review | api-design.md |
| API-13 | CONSIDER | Use a typestate or builder when misuse (wrong call order, incomplete configuration) should be a compile error, and a sealed type parameter when a kind is fixed per code path. Prove it with a compile-fail test. | review | api-design.md |
| API-14 | MUST | Keep the surface minimal. Private by default, `pub(crate)` for internals, `pub` only for the deliberate API. No `pub` to make tests easier. Every public item has exactly one public path: no glob re-exports (except `#[cfg]`-selected platform modules) and no `prelude` module. `#[doc(hidden)]` only for macro or sealing internals. Re-export a dependency only when it is part of your contract, and remember that a third-party error in a public variant makes that dependency public. | lint `unreachable_pub`, scan (advisory: glob re-exports, `prelude`), review | api-design.md |
| API-15 | MUST | Document every public item: purpose in a one-line summary, an `# Examples` doctest for primary entry points, and `# Errors` for fallible functions. Name parameters in prose, never in `# Arguments`/`# Returns` sections. Library crate roots have `//!` docs with a quick-start doctest and their feature list. Link with intra-doc paths. | lint `missing_docs`, `clippy::missing_errors_doc`, `too_long_first_doc_paragraph`, `rustdoc::broken_intra_doc_links`; scan (advisory: parameter sections) | api-design.md |
| API-16 | SHOULD | Cargo features are additive, named for what they enable (`serde`, never `use-serde` or `no-std`), and never change the meaning of a type or gate a `pub` field or trait method. Optional dependencies are `dep:` features, and a feature that needs another declares it. Keep default features lean; CI checks feature combinations. | lint `clippy::negative_feature_names`, `redundant_feature_names`; CI (`cargo hack`); review | api-design.md |
| API-17 | MUST | Published crates follow semver. Run `cargo semver-checks`, check by hand what it misses, and deprecate before removing. Breaking changes are H-16 decisions. | review | api-design.md |
| API-18 | SHOULD | Traits are small, cohesive role interfaces. Keep required methods minimal and add conveniences as provided methods. Use an associated type when each implementor has one natural choice. Use extension traits to add behaviour to foreign types, and a named trait with a blanket impl for closures instead of a repeated `Fn` bound. | lint `clippy::renamed_function_params`, review | api-design.md |
| API-19 | MUST | Do not implement `Deref` to emulate inheritance. `Deref` is for smart pointers, guards and an owning collection's own borrowed view. A pointer's or guard's own operations are associated functions (`Guard::release(this)`), so they never shadow the target's methods. | review, scan (advisory) | api-design.md |
| API-20 | CONSIDER | Use `#[diagnostic::on_unimplemented]` on public traits users are likely to misimplement. | review | api-design.md |
| API-21 | MUST | Generic `struct` and `enum` definitions carry no trait bounds; put them on the `impl` blocks that need them. Exceptions: a field names an associated type, the parameter is `?Sized`, or a `Drop` impl needs the bound. | review, scan (advisory) | api-design.md |
| API-22 | MUST | Implement `std::ops` traits only where the operator has its usual algebraic meaning, as a consistent set, with no allocation, I/O or O(n) work. Never implement an operator whose result can overflow or be invalid: expose `checked_*` returning `Option`/`Result`, or document wrapping or saturating semantics. | lint `clippy::suspicious_arithmetic_impl`, `suspicious_op_assign_impl`; scan (advisory); review | api-design.md |
| API-23 | SHOULD | Public signatures do not expose `Arc`, `Rc`, `RefCell`, `Mutex` or `RwLock`. Hide sharing in a handle type with a cheap `Clone` over `Arc<Inner>`, and wrap a runtime-chosen implementation in a named type (`DynClock`) instead of exposing `Arc<dyn Clock>`. | scan (advisory), review | api-design.md, patterns.md |

## ERR: Error handling

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| ERR-01 | MUST | Library crates expose typed errors that implement `std::error::Error + Send + Sync + 'static`, usually derived with `thiserror`. | review; the H-09 scanner rule | errors.md |
| ERR-02 | SHOULD | Scope error types to an operation or module, with a closed set of variants callers can act on. Avoid one crate-wide `Error` with dozens of variants. | review | errors.md |
| ERR-03 | MUST | Variants carry diagnostic context (path, id, offending value) as typed fields, not preformatted strings. | review | errors.md |
| ERR-04 | SHOULD | Preserve causes with `#[source]`. Use `#[from]` only when exactly one operation can produce that source, so `?` never hides which step failed. | review | errors.md |
| ERR-05 | SHOULD | Error messages are lowercase, have no trailing punctuation, and describe this layer's failure. The reporter prints the cause chain. | review | errors.md |
| ERR-06 | SHOULD | Binaries use one application error type (`anyhow`, or `color-eyre`) at the edge, with `.context()`/`.with_context()` at every boundary crossing. `main` returns `Result` or `ExitCode`. | review | errors.md |
| ERR-07 | MUST | Convert `Option` to an error explicitly (`ok_or`, `ok_or_else`, `let ... else`). Return `Option` only for plain absence: a parse or validate function returns `Result`. | lint (H-02), `clippy::or_fun_call` | errors.md |
| ERR-08 | MUST | Handle or propagate, never both. Do not log an error and then return it. Log once, where it is handled. | review | errors.md |
| ERR-09 | MUST | Arithmetic on external input uses `checked_*`, `saturating_*` or `wrapping_*` deliberately. That includes `/` and `%` (or a `NonZero` divisor), `Iterator::sum`/`product`, and atomic `fetch_add`/`fetch_sub`, which wrap. Overflow is an error, documented saturation, or wrapping that the specification defines. | review; see errors.md for opting a crate into the arithmetic lint (an H-05 decision) | errors.md |
| ERR-10 | SHOULD | Separate domain rejections (invalid command, conflict, not found) from infrastructure failures (I/O, network), using distinct types or variants. | review | errors.md |
| ERR-11 | SHOULD | Tests assert error variants structurally (`assert!(matches!(...))`, or `assert_matches!` on 1.96+, imported with `use std::assert_matches;`), not by comparing `Display` strings. | review | errors.md |
| ERR-12 | MUST | Never silence `#[must_use]`. `let _ =` on a `Result` is rejected by the lint. When a failure is genuinely irrelevant, match it explicitly (`if let Err(e) = ... { tracing::debug!(...) }`) with a comment saying why. | lint `let_underscore_must_use` | errors.md |
| ERR-13 | MUST | A type whose teardown can fail (flush, commit, sync, a final frame) exposes `close(self)`/`finish(self) -> Result`, and the success path calls it. `Drop` is only a fallback: it never panics, blocks or awaits, and it logs that `finish` was skipped instead of failing silently. | lint `clippy::empty_drop`, review | errors.md |
| ERR-14 | SHOULD | A fallible function that takes a non-`Copy` argument by value returns it inside its error (`into_inner(self) -> T`), so callers never clone in case of failure. The error's `Debug` omits the payload. | review | errors.md |

## ARC: Architecture, crates, modules, coupling

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| ARC-01 | MUST | Use a workspace with a virtual manifest and members under `crates/`. Share `[workspace.package]`, `[workspace.dependencies]` and `[workspace.lints]`: every member declares `[lints] workspace = true` and uses sibling crates through workspace dependencies. Use edition 2024, `resolver = "3"`, and set `rust-version`. | hook, gate, template, review | architecture.md |
| ARC-02 | MUST | Dependencies point inward: domain (pure) before application (use cases, ports) before adapters (I/O) before binaries (composition root). Domain crates have no I/O, no runtime and no framework dependencies. | review (with `cargo tree`; which crate is domain is per workspace, so no template check) | architecture.md |
| ARC-03 | MUST | Ports are traits owned by the consumer (the application layer) and implemented by adapters. Adapter types (DB rows, HTTP types) never leak inward. | review | architecture.md |
| ARC-04 | SHOULD | Functional core, imperative shell. Decisions are pure functions over data, and effects happen at the edges. | review | architecture.md |
| ARC-05 | MUST | One concept per module, with a small surface. `lib.rs` is a table of contents (module declarations and re-exports), not an implementation dump. | review | architecture.md |
| ARC-06 | SHOULD | Use `foo.rs` plus `foo/bar.rs`, not `mod.rs`. Never use `#[path]`. Aim for about 400 lines per file and about 50 per function; clippy enforces a hard limit of 60 per function. | lint `clippy::mod_module_files`, `too_many_lines`; scan | architecture.md |
| ARC-07 | SHOULD | Split a crate for a distinct dependency profile, a distinct reason to change, a reuse boundary or measured compile time (`cargo build --timings`). Never split by type category. No `utils`, `common`, `helpers` or `models` dumping grounds. | review | architecture.md |
| ARC-08 | MUST | Every crate has a charter (`AGENTS.md`, with `CLAUDE.md` importing it) covering purpose, layer, allowed dependencies, public surface, invariants, error types, test strategy, a do-not list and a decisions log. Keep it current in the same change. | gate (a crate new since the base, or whose charter was deleted, without `AGENTS.md`), review, template | architecture.md |
| ARC-09 | MUST | Keep coupling low. Depend on the narrowest abstraction. No cyclic module dependencies. No global mutable state; inject dependencies through constructors. Statics and `thread_local!` hold only caches and immutable data. | review, scan (advisory) | architecture.md |
| ARC-10 | SHOULD | Keep cohesion high. Behavior lives with the data whose invariants it maintains ("tell, don't ask"). | review | architecture.md |
| ARC-11 | MUST | Parse configuration once at startup into validated typed structs and pass it down. Never read environment variables deep inside library code. | review, scan (advisory) | architecture.md |
| ARC-12 | MUST | Justify every dependency (maintenance, size, license, `cargo deny`). Declare it in `[workspace.dependencies]` as a caret floor at the oldest version whose API you use, never `=` (except a re-exported proc-macro companion), `~`, `*` or an upper bound, with `default-features = false` plus baseline features; members add what they use. Adding one is H-16, except those on the pre-approved list in `.claude/temper.json` (the crates this standard itself prescribes). | review, hook (asks on new dependencies; advisory on non-caret requirements), `cargo deny` | architecture.md |
| ARC-13 | SHOULD | Event-sourced domains use the Decider shape: `decide(&Command, &State) -> Result<Vec<Event>, Rejection>` and `evolve(State, &Event) -> State`. Events are past-tense, immutable and versioned. `evolve` has no wildcard arm on the event, so a new event fails to compile there. | review | architecture.md, patterns.md |
| ARC-14 | SHOULD | Binaries are thin: parse input, build the object graph (composition root), run, and map errors to exit codes. Only the composition root sets a `#[global_allocator]` or installs a log subscriber. | review, scan | architecture.md |
| ARC-15 | SHOULD | Write a macro only when functions, generics or traits cannot express the idea. Evaluate each argument once, and never hide control flow: a fallible macro expands to a `Result` the caller applies `?` to. | review | architecture.md |
| ARC-16 | MUST | Every crate builds on its own: its manifest enables every dependency feature its code uses, without relying on feature unification from sibling crates or dev-dependencies. CI checks each member separately. | CI (the template's `cargo check -p` per member), review | architecture.md |
| ARC-17 | MUST | Build scripts write only under `OUT_DIR`, declare every input with `cargo::rerun-if-changed`, declare custom cfgs with `cargo::rustc-check-cfg`, read target facts from `CARGO_CFG_*`, and return errors from `main`. No network access and no timestamps. Checked-in generated code comes from an xtask with a freshness test. | review; `build.rs` is scanned as production code under the H-rules | architecture.md |

## TST: Testing and TDD

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| TST-01 | MUST | Red, green, refactor. Each new behavior gets a failing test before its implementation. Each bug gets a regression test that fails first. | review (the plan's Progress section records each red run: test name and failure line) | testing.md |
| TST-02 | MUST | Test through the narrowest real seam. Unit tests live in `#[cfg(test)] mod tests` beside the code. Integration tests in `tests/` use the public API only. Never widen visibility for a test. | review | testing.md |
| TST-03 | SHOULD | Test names state behavior (`rejects_withdrawal_over_balance`). One behavior per test, laid out Arrange, Act, Assert. | review | testing.md |
| TST-04 | SHOULD | Tests return `Result` and use `?` for setup. Prefer `expect("reason")` to bare `unwrap`. | review | testing.md |
| TST-05 | MUST | Assert exact values and exact error variants. Never write `assert!(r.is_ok())` alone; compare whole values with `assert_eq!`. Never write a test whose expected value restates a constant or mirrors the implementation's branches: assert a property. | lint `clippy::assertions_on_result_states`; scan (`weak-assert`, `should-panic`); review | testing.md |
| TST-06 | SHOULD | Use property tests (`proptest`) for invariants, round-trips and algebraic laws in parsers, codecs and domain rules. Parsers of attacker-controlled input also get a coverage-guided fuzz target, run on a schedule. | review | testing.md |
| TST-07 | SHOULD | Use snapshot tests (`insta`) for structured or textual output, and review snapshots as code. | review | testing.md |
| TST-08 | CONSIDER | Prove type-level guarantees (typestate, sealing, deliberate `!Send`) with compile-fail tests (`trybuild` or `compile_fail` doctests). | review | testing.md |
| TST-09 | MUST | Test doubles are hand-written fakes of port traits. No test-only branches in production code. Use mocking crates only at external seams. Cross-crate fakes live behind a `test-util` feature. | scan (`test-branch`, new code only), hook and gate (a mocking crate added to `[dependencies]`), review | testing.md |
| TST-10 | MUST | Tests are deterministic: no sleeps, no wall clock, no real network, no shared global state, no order dependence. Inject the clock and RNG seeds, use paused time for tokio, and locate fixtures through `env!("CARGO_MANIFEST_DIR")`. | review, scan (advisory) | testing.md |
| TST-11 | SHOULD | Public entry points have runnable doctests that use `?`, not `unwrap`. | the H-02 doctest-fence check (`doc-unwrap`), review | testing.md |
| TST-12 | MUST | Never weaken tests (H-12). Removing tests or changing expectations requires a human decision. | hook (ask), review | testing.md |
| TST-13 | SHOULD | Value test strength over coverage. Use `cargo mutants` on critical changed code and kill surviving mutants. Treat coverage as a signal, not a target. | review | testing.md |
| TST-14 | SHOULD | Build test data with builder or fixture functions. Compile integration tests as one binary (`tests/it/main.rs` plus modules), with shared helpers declared `#[cfg(test)] mod support;` so the test lint relaxations apply, and fixtures that return `Result`. | review | testing.md |
| TST-15 | SHOULD | Keep feedback fast: use `cargo nextest` when available, keep unit tests in milliseconds, and isolate slow tests. | review | testing.md |
| TST-16 | MUST | Every performance claim ships with a benchmark (`criterion` or `divan`). | review | testing.md, performance.md |
| TST-17 | MUST | Crates with non-`Relaxed` atomics, `Condvar`s or hand-written thread coordination run their tests under Miri (with `-Zmiri-many-seeds`) and on an aarch64 runner in CI. A test Miri cannot run carries `#[cfg_attr(miri, ignore = "reason")]`. | CI, review; the H-12 scanner rule rejects `ignore` without a reason | testing.md |
| TST-18 | SHOULD | When correctness depends on dependency behaviour its signature does not express (sort stability, rounding, parser leniency, output order), pin that behaviour with a test in the consuming crate. | review | testing.md |

## PERF: Performance and resources

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| PERF-01 | MUST | Measure before optimizing. Optimizations cite a benchmark or profile, in the plan and in a why-comment at the optimized code. | review | performance.md |
| PERF-02 | SHOULD | Allocate deliberately. Pre-size with `with_capacity`/`reserve`, reuse buffers across iterations (`clone_from`, `clone_into`, caller-owned buffers per OWN-17), and do not `collect()` only to iterate again. | lint `clippy::needless_collect`, `assigning_clones`, review | performance.md |
| PERF-03 | SHOULD | Prefer iterators and adapters (`extend`, `retain`, `drain`, `windows`, `as_chunks::<N>()`) to index loops. Remove bounds checks by slicing once with a fallible form (`get(..n)`, `split_at_checked`, `first_chunk`), never with `assert!`. | lint `clippy::needless_range_loop`, `chunks_exact_to_as_chunks`, review | performance.md |
| PERF-04 | SHOULD | Avoid needless string building. `write!` into existing buffers, have `Display` write directly, keep `to_string()`/`format!` off hot paths, and gate log-only computation with `tracing::enabled!`. | lint `clippy::format_push_string`, `or_fun_call`, review | performance.md |
| PERF-05 | SHOULD | Keep hot types small: at or below 128 bytes on 64-bit targets, pinned with a `const` size assertion that holds on every target. Box large, rare enum variants. Prefer `Vec<T>` over `Vec<Box<T>>`. | lint `clippy::large_enum_variant`, `vec_box`, `box_collection` | performance.md |
| PERF-06 | SHOULD | Choose collections by access pattern. Use faster hashers only for keys an attacker cannot control, only with measurement, and ban the std type with `disallowed-types` once adopted. | review | performance.md |
| PERF-07 | MUST | Buffer I/O (`BufReader`/`BufWriter`), lock stdout once for bulk output, and stream large inputs rather than slurping them, reading lines into one reused, length-capped buffer. | lint `clippy::unbuffered_bytes`, review | performance.md |
| PERF-08 | SHOULD | Hot generic code uses static dispatch. Use `#[inline]` only on small cross-crate functions with measured benefit; `#[inline(always)]` is an H-05 request. Mark rare branches with `std::hint::cold_path()`. | lint `clippy::inline_always`, review | performance.md |
| PERF-09 | CONSIDER | Keep the template's release, profiling, bench and dev profiles. `lto = "fat"`, `opt-level`, `panic = "abort"`, `strip`, a global allocator, PGO and `target-cpu` are per-application H-16 decisions with a benchmark. Profiles live only in the root manifest, and shipped profiles keep `debug-assertions = false`. | hook and gate (member profiles), review | performance.md |
| PERF-10 | MUST | Bound everything that grows with input: sizes, collections, recursion depth, concurrency (`Semaphore`), queue lengths (H-13). | review | performance.md |
| PERF-11 | SHOULD | Make refcount clones explicit with `Arc::clone(&x)`. Share large immutable data with `Arc<[T]>`, `Arc<str>` or `bytes::Bytes`. | lint `clippy::clone_on_ref_ptr` | performance.md |
| PERF-12 | SHOULD | Keep critical sections short: drop every guard before I/O, sleeps, joins, blocking channel operations or calls into unknown code. Default to `Mutex`, and use `RwLock` only when a benchmark favours it. Shard or message-pass under contention, but keep fields that share an invariant under one lock (ASY-14). When an async resource must be serialized across `.await`, prefer an actor, with `tokio::sync::Mutex` as the fallback (ASY-09). | lint `clippy::significant_drop_tightening`, review | performance.md, async-concurrency.md |
| PERF-13 | SHOULD | When a benchmark shows contention on independent atomics or small locks stored side by side, pad each to its own cache line (`#[repr(align(128))]`) and assert the alignment at compile time. Never pad without a benchmark. | review | performance.md |

## ASY: Async and concurrency

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| ASY-01 | MUST | Use async only for I/O-bound concurrency. CPU-bound work (anything that runs more than about 100 µs without reaching an `.await`) goes to threads, `rayon` or `spawn_blocking`. | review | async-concurrency.md |
| ASY-02 | SHOULD | Libraries stay runtime-agnostic where feasible. The binary chooses and configures the runtime. | review | async-concurrency.md |
| ASY-03 | SHOULD | Use native `async fn` in traits, not `#[async_trait]`. For public traits that need `Send` futures, write `fn op(&self) -> impl Future<Output = T> + Send`. For `dyn` use, return a boxed future through a named alias. | review; scanner advisory under MOD-04 | async-concurrency.md |
| ASY-04 | MUST | Structured concurrency. Every spawned task and thread is owned (`JoinSet`, a stored `JoinHandle`, `TaskTracker`) and is joined or aborted on shutdown. A task that panicked (`JoinError::is_panic`) is a bug: log it and shut down in order, never respawn and continue. | review, scan (advisory) | async-concurrency.md |
| ASY-05 | MUST | Make cancellation explicit and cooperative (`CancellationToken`). Every `.await` inside `select!` is cancel-safe or documented. | review | async-concurrency.md |
| ASY-06 | MUST | Never block in async code: no `std::thread::sleep`, blocking I/O, or heavy CPU, and no lock guard across `.await` (H-14). | lint `clippy::await_holding_lock`, review | async-concurrency.md |
| ASY-07 | MUST | Use bounded channels. Pick the channel by pattern: `mpsc` for work queues, `broadcast` for fan-out, `watch` for latest value, `oneshot` for replies. | review, scan (advisory) | async-concurrency.md |
| ASY-08 | MUST | Put timeouts on all external I/O. Retries are bounded, with backoff and jitter. | review | async-concurrency.md |
| ASY-09 | SHOULD | Share state by message passing (an actor owns it) before reaching for `Arc<Mutex<_>>`. Use `std::sync::Mutex` for short critical sections, and `tokio::sync::Mutex` only when holding across `.await` is unavoidable. | review | async-concurrency.md |
| ASY-10 | SHOULD | For sync parallelism use `std::thread::scope` (borrowing) or `rayon` (data parallel). Inside a scope, join every handle before inspecting any result. Atomics follow ASY-13. | review | async-concurrency.md |
| ASY-11 | SHOULD | Keep public types, and the futures public `async fn`s return, `Send` (types also `Sync`), and assert it at compile time. Opt out deliberately with a `PhantomData` marker and a compile-fail test. | lint `clippy::future_not_send`, review | async-concurrency.md |
| ASY-12 | SHOULD | Use async closures (`async \|x\| ...`, `AsyncFn*` bounds) for borrowed, non-spawned callbacks. Work that must be spawned keeps `F: Fn() -> Fut, Fut: Future + Send`, because `AsyncFn*` bounds cannot require a `Send` future. | review | async-concurrency.md, modern-rust.md |
| ASY-13 | MUST | Every atomic ordering is justified in a comment: `Relaxed` says why no other memory depends on the value, `Acquire`/`Release`/`AcqRel` name the operation they pair with, and `SeqCst` names the cross-variable total order it needs. Orderings are literal constants. Use `update`/`try_update` over hand-written CAS loops, `_weak` inside retry loops, and rule out ABA. Unsure means a `Mutex`, not `SeqCst`. | scan (`compiler_fence`; advisory: `SeqCst`, `fetch_update`), review | async-concurrency.md |
| ASY-14 | MUST | Lock discipline. Never leave a guard as a temporary in a `match`/`if let`/`while let` scrutinee whose body does more than read. Never lock what the thread already holds. Acquire several locks in one documented order. Keep state that shares an invariant under one lock. Never call callbacks or `dyn` code, or return a guard from a `pub fn`, while holding one. | lint `clippy::significant_drop_in_scrutinee` (misses let chains and `if let` guards), `significant_drop_tightening`; review | async-concurrency.md |
| ASY-15 | MUST | Spawn threads with `thread::Builder` (`spawn`, `spawn_scoped`), named, and propagate the `io::Error`: `thread::spawn` and `Scope::spawn` panic when the OS refuses. Keep every `JoinHandle` and join it on the shutdown path. | lint `clippy::disallowed_methods`; scan; review | async-concurrency.md |
| ASY-16 | MUST | Wait, don't spin. Wait on a `Condvar` only through `wait_while`/`wait_timeout_while`, with one `Mutex` per `Condvar`, and change state under the lock before notifying. No busy-wait loops on atomics, `try_lock` or `yield_now`: block on a primitive. | lint `clippy::missing_spin_loop` (partial: its `spin_loop()` fix does not comply), review | async-concurrency.md |
| ASY-17 | MUST | Do not build synchronization primitives (locks, once-cells, semaphores, channels) from atomics. Use std's; `parking_lot`, `crossbeam` and similar are H-16 decisions with a benchmark. | scan (advisory: `spin-lock`), review | async-concurrency.md |

## OUT: Structured output, serde, observability

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| OUT-01 | SHOULD | Wire types (DTOs) are separate from domain types when the external shape differs or must version independently. Convert with `From`/`TryFrom`. | review | structured-output.md |
| OUT-02 | MUST | Public contracts pin serde conventions: `rename_all` at the type level, explicit enum tagging, `deny_unknown_fields` on inbound commands and config, and `default` only where absence has meaning. | review | structured-output.md |
| OUT-03 | MUST | Persisted and public schemas carry a version. Evolve them additively and upcast old versions on read. | review | structured-output.md |
| OUT-04 | MUST | Output is deterministic: stable field order, `BTreeMap` for maps, sorted collections, and injected time and ids. | scan (advisory: `serde-hashmap`, `ambient-now`), review | structured-output.md |
| OUT-05 | MUST | Validate on the way in. Deserialize into a raw DTO, then `TryFrom` it into the domain type. Deserialized data is never trusted as validated. | review | structured-output.md |
| OUT-06 | SHOULD | CLIs offer a machine mode (`--format json`): data to stdout, diagnostics to stderr, meaningful `ExitCode`s, and JSON Lines for streams. Binaries write through `writeln!` on a locked, buffered stdout, never `println!`. | review | structured-output.md |
| OUT-07 | SHOULD | Generate JSON Schemas for public contracts with `schemars` and snapshot them to catch breaking changes. | review | structured-output.md |
| OUT-08 | MUST | Log with `tracing`: structured fields, spans (`#[instrument(skip_all, fields(...))]`), no `println!` in libraries, and no secrets or PII. Operator-facing events carry a stable `name:` and standard field names. Hot loops log per batch, not per item. Secret types get a redacting, non-empty `Debug` with a unit test proving the secret never prints, no `Serialize`, and no `PartialEq`; compare them in constant time. | lint (H-06), review | structured-output.md |
| OUT-09 | CONSIDER | Use zero-copy deserialization (`&'de str`, `Cow<'de, str>`, `#[serde(borrow)]`) for large or hot inputs. | review | structured-output.md |
| OUT-10 | SHOULD | `Display` is for users and `Debug` is for developers. Never parse `Debug` output. `Display` and `FromStr` are the type's own text form, never a serialization format. | lint `clippy::to_string_trait_impl`, review | structured-output.md |

## MOD: Modern Rust (edition 2024, current stable)

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| MOD-01 | MUST | Use edition 2024 everywhere. Pin the toolchain in `rust-toolchain.toml`, declare `rust-version`, and document the MSRV policy. Never pass `--ignore-rust-version`. | template, hook (Bash), scan (advisory), review | modern-rust.md |
| MOD-02 | SHOULD | Use `let ... else` for early exits, `if let` chains instead of nested `if let`, and `if let` guards in match arms (1.95+). | lint `clippy::manual_let_else`, `collapsible_if`, review | modern-rust.md |
| MOD-03 | MUST | Use `std::sync::LazyLock`/`OnceLock` (and `LazyCell`/`OnceCell`), not `lazy_static!` or `once_cell`. | scan, lint `disallowed_macros` | modern-rust.md |
| MOD-04 | SHOULD | Use native `async fn` in traits and async closures, not `async-trait` or boxed-closure workarounds. | scan (advisory) | modern-rust.md |
| MOD-05 | MUST | Use `#[expect(lint, reason = "...")]`, never `#[allow]`, and only under H-05. | lint `clippy::allow_attributes` | modern-rust.md |
| MOD-06 | SHOULD | Know the 2024 capture rules: return-position `impl Trait` captures all in-scope lifetimes. Narrow it with `+ use<'a, T>` when needed. | review | modern-rust.md |
| MOD-07 | SHOULD | Prefer modern std APIs to hand-rolled equivalents: `is_some_and`, `is_none_or`, `inspect_err`, `take_if`, `repeat_n`, `chunk_by`, `as_chunks`, `array_windows`, `extract_if`, `pop_if`, `div_ceil`, `isqrt`, `floor_char_boundary`, atomic `update`/`try_update` (not `fetch_update`, deprecated from 1.99), `hint::cold_path`, and `assert_matches!` in tests. Use the slice form of `get_disjoint_mut`, which returns a `Result`. APIs newer than `rust-version` are rejected by clippy (`incompatible_msrv`). | lint (various `manual_*`), scan (advisory: `fetch_update`), review | modern-rust.md |
| MOD-08 | CONSIDER | Use `core::error::Error` for `no_std`-compatible libraries, and `std::fmt::from_fn` for one-off `Display` adapters. | review | modern-rust.md |
| MOD-09 | MUST | Configure lints in the workspace `[lints]` table with member inheritance (`[lints] workspace = true` in every member), not as crate-root attributes. | template, hook, gate, scan (advisory: crate-root lint attributes), review | modern-rust.md |
| MOD-10 | CONSIDER | Use const generics, `const fn` and inline `const { }` where compile-time evaluation clarifies intent. | review | modern-rust.md |
| MOD-11 | SHOULD | Use trait upcasting (`&dyn Sub` to `&dyn Super`), not `as_any` hacks. No `Any` downcasting for control flow. The one sanctioned downcast is classifying errors at the binary edge (`anyhow::Error::downcast_ref`) to pick an exit code. | review, scan (advisory) | modern-rust.md |
| MOD-12 | CONSIDER | Use `cfg_select!` (1.95+) instead of the `cfg-if` crate in new code. | review | modern-rust.md |
| MOD-13 | SHOULD | In CI, set `build.warnings = "deny"` (Cargo 1.97+) rather than `RUSTFLAGS=-Dwarnings`, which invalidates the build cache. | template | modern-rust.md |

## CRF: Craftsmanship and readability

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| CRF-01 | MUST | Names reveal intent in the domain's own language. Types are nouns, functions are verbs. Use only conventional abbreviations (`id`, `buf`, `ctx`). Acronyms are one word (`HttpClient`, `Uuid`). No `Manager`, `Factory`, `Helper` or `Util` types. Names of one kind share a word order (`ParseAddrError`). | lint `clippy::upper_case_acronyms` (aggressive), review | process.md |
| CRF-02 | SHOULD | A function does one thing at one level of abstraction. Aim for about 50 lines (clippy's hard limit is 60) and at most 4 parameters (API-10). Extract a block rather than explaining it with a comment. | lint `clippy::too_many_lines`, `too_many_arguments`, `cognitive_complexity` | process.md |
| CRF-03 | SHOULD | Prefer early returns (`?`, `let-else`) to nesting. Match exhaustively on enums you own, with no `_ =>` arm, so new variants force review. A `_` arm on a `#[non_exhaustive]` enum defined outside the workspace is required and fine. | review | process.md |
| CRF-04 | MUST | Comments explain why, doc comments explain the contract, there is no commented-out code, and `TODO(#issue)` carries a reference. | scan (advisory), review | process.md |
| CRF-05 | MUST | No dead code, unused dependencies (`cargo machete`), unused features or speculative generality: no single-implementation trait that is not a port, no marker trait without a sealing or typestate purpose. | lint `dead_code`, `unused_*`; CI (`cargo machete`); review | process.md |
| CRF-06 | MUST | Format with `rustfmt` using the repository config; its output is authoritative. Grouping imports std, then external, then crate is a SHOULD, because stable rustfmt cannot enforce it. | hook (gate), CI; review (grouping) | process.md |
| CRF-07 | SHOULD | Bindings are immutable by default. Keep mutable state local. Shadowing a value through a transformation pipeline is fine. | review | process.md |
| CRF-08 | SHOULD | Leave the code cleaner within the change's scope. Unrelated refactors go in a separate change. | review | process.md |
| CRF-09 | SHOULD | Replace magic numbers with named constants or unit-bearing types (`Duration`, newtypes). A constant's doc says why it has this value and what breaks if it changes. | review | process.md |
| CRF-10 | SHOULD | Write expression-oriented code (`let x = match ...`) and iterator pipelines when they read more clearly than loops, but do not code-golf. | lint `for_loops_over_fallibles`, `clippy::option_map_unit_fn`, review | process.md |
| CRF-11 | MUST | Rustdoc and READMEs describe the current contract and behaviour. A `//` why-comment may justify the current design in the present tense. Design history, narratives of alternatives tried, and rule-compliance tables belong in the plan or the charter's decisions log. | review | process.md |

## PROC: Process

| ID | Level | Rule | Enforced by | Detail |
|---|---|---|---|---|
| PROC-01 | MUST | Explore before editing: crate charter, public API, tests, call sites. Check dependency APIs and features against the locked version (`cargo info`), and read the `# Panics` and `# Errors` sections of every API you call on data you do not control. Do not write from memory. | review | process.md |
| PROC-02 | SHOULD | Delegate wide searches to an exploration subagent and keep conclusions only. Keep tool output short (`-q`, `--message-format short`, filtered test output). | review | process.md |
| PROC-03 | MUST | Plan non-trivial changes: goal, invariants, signatures, placement, errors, test list, risks, HITL decisions. | hook (context), review | process.md |
| PROC-04 | MUST | Surface HITL decisions (H-16) explicitly. Never assume them. | review | process.md |
| PROC-05 | SHOULD | Take small steps: one behavior per red-green-refactor cycle, build and test each step, and commit when green if commits are in scope. | review | process.md |
| PROC-06 | MUST | For multi-step work, keep the plan file's checklist current so compaction or handoff loses nothing. | review | process.md |
| PROC-07 | MUST | Self-review the diff against the checklist before requesting review, and fix what you find. | review | process.md, review-rubric.md |
| PROC-08 | MUST | Every change that reaches "done" gets an independent fresh-context review. | hook (Stop gate) | process.md |
| PROC-09 | MUST | After two failed attempts at the same compiler error, stop and re-model ownership before trying again. Split a failing chain into typed `let` bindings to find the rejected step. | review | process.md, ownership.md |
| PROC-10 | MUST | Report with the Status block. State honestly what was not done or not verified. | hook (Stop) | process.md |
| PROC-11 | MUST | Keep docs in sync in the same change: crate charter, README, rustdoc, CHANGELOG if present (published crates keep an `Unreleased` section with breaking changes flagged). | review | process.md |
| PROC-12 | SHOULD | Read the whole diagnostic, including notes, help and `rustc --explain`, before changing code. An E0277 for a trait the type visibly implements means two versions of one crate: check `cargo tree -d`. | review | process.md |
| PROC-13 | MUST | Never edit `Cargo.lock` by hand. Change it with cargo (`cargo update -p <crate> --precise <version>`), resolve lockfile conflicts by re-running cargo, and run CI with `--locked`. | hook (denied), CI | process.md |
