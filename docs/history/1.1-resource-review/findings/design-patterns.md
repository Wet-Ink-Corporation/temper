# Rust Design Patterns (rust-unofficial): findings

Source: https://rust-unofficial.github.io/patterns/ (print view; every page stamped "Last change: 2026-09-21, commit 36fdd17"). Read: the whole book, 5,087 lines. That covers the Introduction; all 16 Idioms (FFI idioms included); Design Patterns (YAGNI; Behavioural: Command, Interpreter, Newtype, RAII guards, Strategy, Visitor; Creational: Builder, Fold; Structural: struct decomposition, small crates, containing unsafety, custom traits for bounds; FFI: object-based APIs, type consolidation); the 3 Anti-patterns; Functional (paradigms, generics as type classes, optics); Additional resources; and Design principles.

**License: MPL-2.0.** `LICENSE` on `main` of github.com/rust-unofficial/patterns reads "Mozilla Public License Version 2.0". MPL is a file-level copyleft, and rust-craft is MIT (SKILL.md frontmatter). Copying the book's prose or code into a rust-craft file would put that file under MPL, so paraphrase instead. Every sketch below is original and was compiled for this report; none is copied from the book.

Verification: scratch workspace `scratchpad/patterns/` uses the template's `[workspace.lints]`, `clippy.toml` and `rust-toolchain.toml` (1.98.1). All sketches pass `cargo clippy --all-targets -- -D warnings`, and their tests pass. Full versions are in `scratchpad/patterns/crates/pat/src/{finish,verify2,consumed,on_stack_dyn,getter,kind,tree,rebind,code,nonexh,optiter,sorted,temp_mut}.rs`, with a downstream crate at `crates/other` for the `#[non_exhaustive]` checks.

## Already covered (summary)

Each book item maps to rust-craft as follows.

**Idioms**
- Borrowed types for arguments → OWN-03 (`ptr_arg`, `borrowed_box`, scan).
- `format!` concatenation → PERF-04 and CRF-10 (`format_push_string` warn, `string_add_assign` via pedantic).
- Constructors and `Default` → API-01, API-06 and pattern 3. `clippy::new_without_default` (style, so denied via `all`) already enforces the book's "no-argument `new` implies `Default`".
- `mem::take`/`mem::replace` → OWN-08, pattern 9 and the ownership.md E0507 shape. `clippy::mem_replace_with_default` and `mem_replace_option_with_none` (style, denied) catch the long-hand forms. These could go in OWN-08's "Enforced by" column.
- Temporary mutability → CRF-07. Both of the book's shapes (block, and `let data = data;`) pass the template clippy. `redundant_locals` does not fire on a rebinding from `mut` to immutable, and I checked this.

**Design patterns**
- Newtype → API-02, API-03 and patterns 1, 2, 22. Hiding internal generic types → API-14.
- RAII guards → pattern 8 and API-19 (guards may `Deref`).
- Builder, both consuming and `&mut` → patterns 4 and 5, and the API-10 builder table.
- Strategy → API-09, ARC-03/ARC-09 dependency injection, and pattern 11.
- Struct decomposition → OWN-08 and the ownership.md shape "restructure into smaller structs".
- Small crates (your own crates) → ARC-07.
- YAGNI → CRF-05.

**Anti-patterns**
- Clone to satisfy the borrow checker → H-08, OWN-08 and the clone audit.
- Deref polymorphism → API-19, with the scanner's `deref-inherit` advisory.
- `#![deny(warnings)]` → MOD-09, MOD-13 and H-15. The template does exactly what the book recommends:
  - Lints are named in `[workspace.lints]`, not written as crate-root attributes.
  - Warnings are denied only by the build setting, never in code. `.cargo/config.toml` has `build.warnings = "warn"` for local runs. CI sets `CARGO_BUILD_WARNINGS=deny` and runs `clippy -- -D warnings`.
  - The MSRV job uses the book's literal `RUSTFLAGS=-D warnings`.
  - The toolchain is pinned, so new or deprecation lints arrive only in a deliberate bump PR.
  - Cargo caps lints for registry dependencies, so downstream users never break.
  - The book's "safe to deny" lists are already on. The template sets `trivial_casts`, `trivial_numeric_casts`, `unused_qualifications`, `missing_docs`, `missing_debug_implementations`, and `unused_extern_crates` through `rust_2018_idioms`; the rest of the list is rustc warn-by-default.
  - The only gap is the scanner (candidate 15).

**Functional**
- Imperative versus `fold` → CRF-10.

**Design principles**
- DIP → ARC-03; ISP → API-18; Law of Demeter / tell-don't-ask → ARC-10; CQS → OWN-04; uniform access → API-01; encapsulation → API-04; design by contract → API-03; single choice → CRF-03 (exhaustive match on owned enums); KISS → CRF-05; composition over inheritance → API-19.

## Candidates

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement (lint + level / clippy.toml / scanner regex / review / template / CI) | Value (high/med/low) | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| 1 | Idioms › Finalisation in destructors (#finalisation-in-destructors), Discussion and Disadvantages | Rust has no `finally`, so `Drop` runs code on every exit path. But destructors are not guaranteed to run, must not panic, and add hidden control flow. | new ERR (ERR-13) + pattern 23 | **ERR-13 MUST.** A type whose cleanup can fail (buffered writer, transaction, temp file, network session) exposes a consuming `finish(self)`/`close(self)`/`commit(self) -> Result<_, E>`, and the success path calls it. `Drop` is only a best-effort fallback: it never panics, blocks or awaits, and it reports failure with `tracing::warn!` instead of discarding it. Call `BufWriter::flush`/`into_inner` explicitly, and `File::sync_all` when durability matters. Never rely on `Drop` for correctness-critical finalisation, because `process::exit`, `panic = "abort"`, a panic during unwinding, or an infinite loop all skip it. | Review: a `Drop` impl containing `flush`, `sync`, `commit`, `send` or `write`; a `BufWriter` that goes out of scope without `flush`/`into_inner`. Scanner (advisory, soft): `impl\s+Drop\s+for` in a diff prompts "is there an explicit fallible finish?". The existing lints already stop `unwrap` or `let _ =` inside `Drop`. | high | Agents routinely leave errors to `BufWriter`'s or `File`'s `Drop`, which swallows them by design (std docs). That is an H-10 violation the lints cannot see. Pattern 8 has a one-line note on this, but there is no rule or example. Gotcha: `finish(self)` cannot destructure a `Drop` type (E0509, checked), so hold the resource in an `Option` and `take()` it (sketch). |
| 2 | Idioms › Return consumed argument on error (#return-consumed-argument-on-error) | If a fallible function moves an argument in, return it inside the error so the caller can retry or fall back without cloning. std does this: `FromUtf8Error::into_bytes`. | new ERR (ERR-14) + pattern 24 | **ERR-14 SHOULD.** A fallible function or method that takes ownership of a non-`Copy` argument returns that argument inside its error, as a typed field with an `into_inner(self) -> T` accessor (API-01). Examples: `Full<T>`, `SendError<T>`, `FromUtf8Error::into_bytes`, `<[T; N]>::try_from(Vec<T>)` returning `Err(Vec<T>)`. Write `Debug` by hand so it omits the payload, the way `SendError` prints `SendError { .. }`. That avoids a `T: Debug` bound and keeps payloads (which may be secrets, OUT-08) out of logs. The error is still `Error + Send + Sync + 'static` when `T` is (ERR-01). | Review: callers that `.clone()` an argument before a fallible call so they can retry. That clone is forced by the API and is an H-08 finding against the callee's design. | high | Otherwise H-08 ("no clone to appease the borrow checker") is unsatisfiable at call sites: the API forces the clone. errors.md mentions `PoolClosed(job)` in one table row, but there is no rule and no pattern. Checked on 1.98.1: `SendError` Debug output, `FromUtf8Error::into_bytes`, the `<[T;3]>::try_from(Vec)` round trip, and a generic `thiserror` error with manual `Debug` meeting `Error + Send + Sync + 'static`. |
| 3 | Idioms › `#[non_exhaustive]` and private fields (#non_exhaustive-and-private-fields-for-extensibility), Example, Alternative, Discussion | `#[non_exhaustive]` works on structs, enums *and enum variants*. The private `_b: ()` field is the within-crate alternative. Use it deliberately: it fits "an external resource that changes out of sync". | refine API-05 (cross-ref ERR-03) | Append to API-05: "`#[non_exhaustive]` on an enum permits new *variants* only. A struct-like variant that may gain context fields (ERR-03), such as `TooLong { len, max }` gaining `id`, also needs `#[non_exhaustive]` on the variant. Downstream code must then write `..` in patterns and cannot construct the variant. Never emulate the attribute with a private `_priv: ()` field: `clippy::manual_non_exhaustive` rejects it. Good candidates: types that mirror an external system or protocol that evolves independently. Closed domain sets stay exhaustive." | Review for variant-level use. `clippy::manual_non_exhaustive` is style, already denied via `all`; checked that it fires on the book's `_b: ()` shape. Checked E0638 (`..` required) and E0639 (cannot construct) in a downstream crate, and E0004 without a `_` arm. | med | ERR-03 asks errors to carry typed fields, so adding a field to a published error variant is the most likely semver break, and enum-level `#[non_exhaustive]` does not prevent it. The book's private-field alternative would fail the gate, so agents need to hear that before trying it. |
| 4 | Idioms › Pass variables to closure (#pass-variables-to-closure) | Rebind the clones and borrows a `move` closure needs in a block that evaluates to the closure. Names stay the same, and the rebindings are scoped with their purpose. | refine OWN-14 | Add to OWN-14: "When a `move` closure needs a clone of one handle and a borrow of another, build it in a capture block: `let job = { let store = Arc::clone(&store); move \|\| ... };` or `tasks.spawn({ let store = Arc::clone(&store); async move { .. } })`. Keep the original names. Do not write `store2`/`store_clone`. To move a *borrow* into a `move` closure, rebind a reference first (`let names = names.as_slice();`)." | Review: `_clone`/`2`-suffixed bindings made just before `spawn`/`move`. | med | rust-craft already writes `let store = Arc::clone(&store);` before spawning, but it does not name the shape. The block also documents that the clone exists only for that task, and it ends the extra borrow early. |
| 5 | Idioms › On-Stack Dynamic Dispatch (#on-stack-dynamic-dispatch) | Since 1.79 you can write `let r: &mut dyn Read = if c { &mut io::stdin() } else { &mut File::open(p)? };`. You get runtime choice with no `Box` and no monomorphized copy of what follows. | refine API-09 + pattern 25 | Add a row to the API-09 table: "Runtime choice between a few concrete types, used only inside this function → `&mut dyn Trait` bound from an `if`/`match` whose arms borrow temporaries (1.79+ temporary lifetime extension). Not `Box<dyn Trait>` (allocation), and not duplicated generic calls." | Review: `Box<dyn Read/Write/Iterator>` (or an `Either`) whose value never escapes the function. | med | Agents default to `Box<dyn ..>` or copy the call into both branches. The on-stack form avoids both. Checked on 1.98.1 with `?` in the `else` arm and a `stdin().lock()` temporary, clean under the template lints. |
| 6 | Structural › Use custom traits to avoid complex type bounds (#use-custom-traits-to-avoid-complex-type-bounds); Strategy › Discussion (closures as strategy) | Replace a repeated `F: FnMut() -> Result<T, E>` bound with a named trait that has an associated `Output` and a blanket impl for closures. This drops a type parameter and names the role. | refine API-18 + pattern 26 | Add to API-18: "A single-method, stateless role may be accepted as `impl Fn(..)`. When the same closure bound repeats across items, needs a name, or carries an output type, promote it to a named trait with an associated type plus `impl<F: Fn..(..) -> R> Trait for F` so closures still qualify. The same applies to ports (`Clock`, `Lookup`): the blanket impl lets tests pass `\|\| fixed_time` as the fake." | Review: the same `Fn*` bound spelled out on three or more items; structs with a type parameter that only feeds a closure's output. | med | This is stable Rust's substitute for trait aliases. rust-craft's `Config::from_lookup(impl Fn ..)` already uses closure-as-port, but the promotion path is undocumented. Checked on 1.98.1: the blanket `Fn` impl coexists with `impl Clock for SystemClock` and `impl Clock for Arc<dyn Clock>`, because the `Fn*` traits are `#[fundamental]`. |
| 7 | Functional › Generics as Type Classes (#generics-as-type-classes) | When every code path knows the variant at compile time, parameterize the type (`Download<Nfs>`). Common methods go in `impl<P>`, and variant-only methods (`mount_point`) exist only on `Download<Nfs>`. There is no `Option` and no runtime check. | refine API-13 + pattern 27 | Add to API-13: "When a kind (protocol, backend, unit system) is fixed per code path and never changes at runtime, make it a sealed type parameter (`Download<P: Protocol>`). Shared methods go in `impl<P: Protocol>` and kind-specific ones in `impl Download<Nfs>`. Use an enum instead when the kind arrives at runtime or instances of different kinds share one collection; convert at that boundary." | Review: `Option` fields that are `Some` only for one "kind", and methods documented "returns `None` unless X". | low | This generalizes typestate (pattern 5) from sequencing to kinds. The book's version bounds on a `pub(crate)` trait; for a `pub` type, pattern 6's seal is the right form, and it passes `private_bounds` cleanly (checked). |
| 8 | Behavioural › Visitor (#visitor), Creational › Fold (#fold), Behavioural › Interpreter (#interpreter) | Separate traversal from operations: `visit_*` hooks plus shared `walk_*` functions. Fold consumes and rebuilds the tree, reusing unchanged `Box`es. Interpreter is a small grammar with a recursive-descent parser. | pattern (28) | Pattern "Tree walk". An AST is an owned enum. One `walk_expr` holds the only exhaustive match on the tree's shape. `Visitor` hooks default to `walk_*` and return `ControlFlow<Self::Break>` so a pass can stop early with `?`. Rewrites use a `Folder` that takes nodes by value and moves each `Box` through, so unchanged subtrees are never cloned. Parsers return `Result<_, ParseError { pos, found }>`, never panic, and cap nesting depth (PERF-10), because a stack overflow aborts the process. | Review. CRF-03 (exhaustive match) and H-03 already apply. | med | Agents write parsers and document or tree walkers often, and they copy the book's examples, which `panic!`/`unimplemented!` on unexpected nodes. rust-craft has no tree-walk pattern. The `ControlFlow`-based visitor and the moving fold are both checked on 1.98.1 (tests included). |
| 9 | Structural › Contain unsafety in small modules (#contain-unsafety-in-small-modules), applied to invariants (String over `Vec<u8>` example) | Put the code that upholds an invariant in the smallest module that can uphold it, behind a safe interface. | refine API-03 | Add to API-03: "Privacy is per module, not per type: any code in the same module can write `Email(raw)` and bypass `TryFrom`. Put each invariant-carrying type in a module containing only the type, its constructors and the methods that must see its fields. The rest of the crate goes through the validating constructor." | Review: a validated newtype defined in a module (`types.rs`, `model.rs`, a service module) where other functions construct it directly. | med | This is the forbid-unsafe version of the book's advice. Agents put newtypes in shared modules and then construct them unchecked next door, silently defeating "parse, don't validate". |
| 10 | FFI Idioms › Error Handling in FFI › Flat Enums (#flat-enums) | Map an error enum to an integer code. The book writes `(e as i8).into()`. | refine H-11 (escape-hatches.md table) | Add a row to the escape-hatches.md `as` table: "fieldless enum to integer code (exit code, wire or protocol code): `impl From<Kind> for u8 { fn from(k) -> u8 { match k { Kind::A => 2, .. } } }`, then `ExitCode::from(u8::from(kind))`. Not `Kind::A as u8`, and not `#[repr(u8)]` plus `as`. The exhaustive match turns a new variant into a compile error." | Already denied: `clippy::as_conversions`. This row gives the replacement. | med | `as` on a discriminant is the most common "legit-looking" cast agents reach for when mapping errors to exit or status codes, and H-11 currently offers no replacement for this case. Checked. |
| 11 | Idioms › Collections are smart pointers (#collections-are-smart-pointers) | An owning collection may `Deref` to its borrowed view (`Vec`→`[T]`, `String`→`str`). Most methods live on the view. | refine API-19 | Amend API-19's Detect line: "Exception: an owning collection may implement `Deref` (not `DerefMut` if it keeps an invariant) to its own borrowed view (`[T]`, `str`, `Path`), as `Vec`, `String` and `PathBuf` do. Also implement `AsRef`/`Borrow`, because generic bounds do not see through `Deref`." | Review. The scanner's `deref-inherit` stays soft. | low | This prevents false findings on legitimate owned/borrowed pairs (a read-only `SortedVec`→`[T]` is fine; checked). The Leaderboard example stays the right call because it rules out `DerefMut`. |
| 12 | Idioms › Easy doc initialization (#easy-doc-initialization) | When a type is expensive to build in examples (a `TcpStream`), wrap the example in a hidden helper function that takes it as a parameter. It compiles but does not run. | refine TST-11 | Add to TST-11: "If an example needs a value a doctest cannot build (live connection, pool), write the body inside a hidden `# fn demo(conn: &Connection) -> Result<(), Error> { .. # }`. It type-checks against the real API without `no_run` boilerplate. It does not execute, so keep assertions out of it. Never add `#[doc(hidden)] pub` constructors just for docs (API-14)." | `cargo test --doc` compiles it (checked); review. | low | TST-11 already flags `no_run` without a reason. This offers the compliant alternative and closes the book's `#[doc(hidden)] pub` escape (see Conflicts). |
| 13 | Idioms › Iterating over an Option (#iterating-over-an-option) | `Option` is `IntoIterator`: use `v.extend(opt)` and `it.chain(opt)`, and `iter::once(x)` when the value is always present. Prefer `if let` to `for x in opt`. | refine CRF-10 | Add to CRF-10's examples: "`extend(opt)`/`chain(opt)` instead of `if let Some(x) = opt { v.push(x) }`; `iter::once(x)` for one known item; never `for x in opt` or `opt.map(\|x\| side_effect(x))`." | rustc `for_loops_over_fallibles` (warn by default, an error under the gate's `-D warnings`; checked). `clippy::option_map_unit_fn` (complexity, denied). `clippy::iter_on_single_items` is nursery and off; leave it off. | low | A small idiom with enforcement already in place. Recording the lints saves reviewers from writing prose findings. |
| 14 | Anti-patterns › `#![deny(warnings)]` (#denywarnings) | Keep "deny warnings" out of source and in the build or CI settings, and name the lints you deny. | tooling (MOD-09) | Scanner rule `crate-lint-attr`: flag crate-root lint-level attributes, because policy belongs in `[workspace.lints]` (MOD-09). This includes `#![deny(warnings)]`. | `scan.mjs` RULES entry `{ key: 'crate-lint-attr', id: 'MOD-09', re: /#\s*!\s*\[\s*(?:deny\|warn\|forbid)\s*\(/g }`, soft. Today only `#![allow(` is caught (`allow-attr`, H-05). | med | Agents often add `#![deny(warnings)]`, `#![warn(clippy::pedantic)]` or `#![forbid(unsafe_code)]` to `lib.rs` from habit. MOD-09's Detect is a manual `rg`. |
| 15 | Behavioural › Command (#command), Approach: function pointers / `Fn` trait objects | Commands as trait objects, `fn` pointers or boxed closures. Use `fn` pointers when there is no state, because they need no dynamic dispatch. | refine API-09 | Add a row to the API-09 table: "Closed set of commands → an `enum` interpreted by one function (as in the Decider). Open dispatch table of stateless handlers → `fn(&Args) -> Result<..>` pointers (`Copy`, `'static`, no allocation). Use `Box<dyn Fn>` only for handlers that capture state. Undo is data: an `inverse()` returning a command, or a compensating event (ARC-13), not a stored closure." | Review: `Vec<Box<dyn Fn..>>` whose closures capture nothing. | low | This fills a small gap in the dispatch table and lines up the GoF Command pattern with rust-craft's event-sourced defaults. |
| 16 | Functional › Optics › Poly Isos (#poly-isos) | `to_string`/`FromStr` do not say "this is JSON", and every type would need to agree on a representation. That is why serde separates the data model from formats. | refine OUT-10 | Add to OUT-10: "Never implement `Display`, `ToString` or `FromStr` as a serialization format (JSON, CSV). Wire formats go through `Serialize`/`Deserialize` so the caller picks the format. `FromStr`/`Display` are for the type's own canonical text form (ids, units)." | `clippy::to_string_trait_impl` (style, denied; checked) already bans `impl ToString`. Review the `Display` impls. | low | Agents sometimes write `impl Display` that emits JSON for "easy logging". That mixes user text with wire contracts (OUT-01/OUT-10). |
| 17 | Idioms › Finalisation in destructors › Disadvantages (destructors not guaranteed); std `process::exit` docs | Code in destructors can be skipped. | refine ARC-14 | Add to ARC-14's Why: "`std::process::exit` runs no destructors, on this thread or any other, so buffered output and `Drop`-based cleanup are lost. That is a second reason `main` returns `ExitCode` (`clippy::exit` is denied)." | Already `clippy::exit` deny. | low | This makes the existing ban self-explanatory and ties it to ERR-13. |

## Conflicts

- **Anti-patterns › Clone to satisfy the borrow checker, Motivation ("sometimes it is fine to write inefficient code": beginners, prototypes, when readability wins)** conflicts with H-08. rust-craft allows no borrow-checker clones in production code, even for readability. Compatible nuggets: `Rc`/`Arc` clones are cheap handles (already in the clone audit), and running clippy catches some unneeded clones (`redundant_clone` is enabled explicitly).
- **Behavioural › Interpreter, Solution ("the code panics when an expression is syntactically wrong") and Visitor, Example (`panic!()`, `unimplemented!()` in `visit_name`/`visit_stmt`)** conflict with H-03, which forbids panicking on bad input. Compatible nugget: candidate 8. Parsers return `Result` with a position, visitors return `ControlFlow`, and unsupported nodes become an error variant.
- **Structural › Prefer small crates ("use smaller, more fine-grained dependencies")** conflicts with ARC-12 and H-16: every new dependency is a justified human decision, and "a crate for one 10-line function" is a listed Don't. Compatible nugget: once a dependency is approved, prefer the focused crate with minimal features over a framework. The book's own drawbacks (duplicate versions, uncurated or malicious crates) are what `cargo deny` and `cargo tree -d` in ARC-12 address.
- **Idioms › Easy doc initialization, Discussion (a `#[doc(hidden)]` public helper constructor for docs)** conflicts with API-14 (no `pub` for tests or docs; minimal surface). Compatible nugget: the hidden-`fn` wrapper (candidate 12).
- **FFI Idioms › Error Handling in FFI › Flat Enums (`(e as i8).into()`)** conflicts with H-11 (no `as`). Compatible nugget: an exhaustive `match` in `From` (candidate 10).
- **Structural › Struct decomposition, Example.** The "fixed" code still calls `connection_string.clone()` to call `print_database`, which is an H-08 violation. The idea itself (split the struct so fields borrow independently) is already OWN-08's shape. Take the idea, not the code.
- **Book examples generally** use `unwrap()` (Return consumed argument, Strategy's `map`), `println!` everywhere, `anyhow::Error` as a library `FromStr::Err` (Optics), `as f64` (the `norm!` macro), and hand-rolled JSON via `format!` (Strategy). These conflict with H-02, H-06, H-09 and H-11, and with OUT-01 for the JSON. Adopt the patterns, never the example code.

## Patterns / examples worth copying

All sketches are original, pass the template's clippy on 1.98.1, and use no escape hatches. The numbering continues `patterns.md` (1–22).

**23. Explicit `finish` with a best-effort `Drop` (ERR-13, H-10, H-03, H-14)**
```rust
pub struct Journal { out: Option<BufWriter<File>> } // Option: finish(self) cannot destructure a Drop type (E0509)

impl Journal {
    /// # Errors
    /// The I/O error from flushing or syncing; the only place a lost write-back is visible.
    pub fn finish(mut self) -> io::Result<()> {
        let Some(out) = self.out.take() else { return Ok(()) };
        out.into_inner().map_err(io::IntoInnerError::into_error)?.sync_all()
    }
}

impl Drop for Journal {
    fn drop(&mut self) { // reached only when finish was skipped: early return, `?`, unwinding
        if let Some(mut out) = self.out.take() && let Err(error) = out.flush() {
            tracing::warn!(%error, "journal dropped without finish; tail may be lost");
        }
    }
}
```
Notes: `Drop` never panics, blocks, or awaits (for async resources use `async fn close(self)`). `BufWriter` and `File` drops swallow errors (std docs), and `process::exit` skips `Drop` entirely.

**24. Hand back the consumed argument (ERR-14, H-08, API-01, OUT-08)**
```rust
#[derive(thiserror::Error)]
#[error("queue is full ({cap} jobs)")]
pub struct Full<T> { job: T, cap: usize }

impl<T> Full<T> {
    #[must_use]
    pub fn into_inner(self) -> T { self.job }
}

impl<T> fmt::Debug for Full<T> { // no `T: Debug` bound; payload never reaches logs
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Full").field("cap", &self.cap).finish_non_exhaustive()
    }
}

// pub fn push(&mut self, job: T) -> Result<(), Full<T>>
// caller: match q.push(job) { Ok(()) => .., Err(full) => overflow.push(full.into_inner()) }
```
std precedents: `String::from_utf8` → `FromUtf8Error::into_bytes`; `mpsc::SendError<T>.0`; `<[T; N]>::try_from(Vec<T>)` → `Err(Vec<T>)`; `Rc::try_unwrap` → `Err(Rc<T>)`; `BufWriter::into_inner` → `IntoInnerError::into_inner`.

**25. On-stack `dyn` for a local runtime choice (API-09, PERF-08)**
```rust
pub fn count_lines(path: &Path) -> io::Result<usize> {
    // 1.79+: temporaries in if/else tails live to the end of the enclosing block; no Box.
    let source: &mut dyn Read = if path == Path::new("-") {
        &mut io::stdin().lock()
    } else {
        &mut File::open(path)?
    };
    BufReader::new(source).lines().try_fold(0_usize, |n, line| line.map(|_| n.saturating_add(1)))
}
```

**26. Closure-friendly role trait: custom trait plus blanket impl (API-18, ARC-03, TST-09)**
```rust
pub trait Getter {
    type Output;
    /// # Errors
    /// [`ReadError`] when the source is unavailable.
    fn get_value(&mut self) -> Result<Self::Output, ReadError>;
}

impl<F, T> Getter for F where F: FnMut() -> Result<T, ReadError> {
    type Output = T;
    fn get_value(&mut self) -> Result<T, ReadError> { self() }
}

pub struct Probe<G: Getter> { last: Option<G::Output>, getter: G } // was Probe<G, T> + a repeated FnMut bound
```
For ports, `impl<F: Fn() -> SystemTime> Clock for F` lets a test pass `|| fixed` as the clock. It coexists with `impl Clock for SystemClock` and `impl Clock for Arc<dyn Clock>` (checked).

**27. Kind-parameterized API, or "generics as type classes" (API-13, API-12, API-02)**
```rust
mod sealed { pub trait Sealed {} }
pub trait Protocol: sealed::Sealed {}

pub struct Nfs { mount_point: PathBuf }
pub struct Bootp;

pub struct Download<P: Protocol> { file: PathBuf, protocol: P }

impl<P: Protocol> Download<P> {
    pub fn file(&self) -> &Path { &self.file }
}

impl Download<Nfs> { // exists only for NFS: no Option, no runtime check
    pub fn mount_point(&self) -> &Path { &self.protocol.mount_point }
}
```

**28. Tree walk: `Visitor` with `walk_*` and `ControlFlow`, and a moving `Fold` (CRF-03, H-03, PERF-10)**
```rust
pub trait Visitor {
    type Break;
    fn visit_expr(&mut self, e: &Expr) -> ControlFlow<Self::Break> { walk_expr(self, e) }
}

pub fn walk_expr<V: Visitor + ?Sized>(v: &mut V, e: &Expr) -> ControlFlow<V::Break> {
    match e { // the only exhaustive match on the tree's shape
        Expr::Lit(_) | Expr::Var(_) => ControlFlow::Continue(()),
        Expr::Add(l, r) => { v.visit_expr(l)?; v.visit_expr(r) }
        Expr::Neg(inner) => v.visit_expr(inner),
    }
}

// Fold consumes the tree and moves each Box through, so unchanged subtrees are never cloned:
fn fold_box<F: Folder + ?Sized>(f: &mut F, mut node: Box<Expr>) -> Box<Expr> {
    *node = f.fold_expr(*node);
    node
}
```

**Refinement sketches (no new pattern number)**
```rust
// OWN-14 capture block: same names, clones scoped with the closure that needs them
let job = {
    let settings = Arc::clone(settings);
    move || names.into_iter().map(|n| format!("{}{n}", settings.prefix)).collect::<Vec<_>>()
};
thread::spawn(job)
```
```rust
// H-11: enum to code without `as`; a new variant is a compile error
impl From<Failure> for u8 {
    fn from(f: Failure) -> Self {
        match f { Failure::Usage => 2, Failure::Rejected => 3 }
    }
}
// main: ExitCode::from(u8::from(failure))
```
```rust
// API-05: variant-level non_exhaustive so TooLong can gain fields later
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecordError {
    #[error("record is {len} bytes; the limit is {max}")]
    #[non_exhaustive]
    TooLong { len: usize, max: usize },
}
// downstream must write `RecordError::TooLong { len, .. }` plus a `_` arm (E0638/E0004), and cannot construct it (E0639)
```

## Notable skips

- **FFI Idioms** (Error handling in FFI, Accepting Strings, Passing Strings) and **FFI Patterns** (Object-Based APIs, Type Consolidation into Wrappers): these need `unsafe`/`extern`, which H-01 forbids. The only nuggets for forbid-unsafe code are candidates 9 and 10.
- **Contain unsafety in small modules**: skipped as unsafe advice, but adapted to invariants as candidate 9.
- **`#![deny(warnings)]` lint lists**:
  - `unused_results`: it would force `let _ =` on every ignored non-`must_use` value, which clashes with H-10 scanning.
  - `missing_copy_implementations`: contradicts OWN-16 (`Copy` only deliberately).
  - `unused_import_braces`: redundant, since rustfmt already rewrites `use a::{b};` to `use a::b;` (checked).
  - `const_err` and `private_in_public`: removed from rustc.
- **Functional Language Optics** (Iso, Poly Iso, Prism theory, and serde's internals): explanatory only; candidate 16 is the one actionable point.
- **Programming paradigms**, **Translations**, **Introduction**, **Additional resources** (talks; the API Guidelines are presumably mined separately): nothing actionable here.
- **Newtype's `Password` `Display` masking**: pattern 22 (redacting `Debug`, no `Display`/`Serialize`) is stricter, so nothing to take.
