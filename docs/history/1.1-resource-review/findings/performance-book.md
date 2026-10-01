# The Rust Performance Book (Nethercote): findings

Source: https://nnethercote.github.io/perf-book/. Read: the whole print view (res/perf-book.md, 2053 lines): Introduction, Benchmarking, Build Configuration, Linting, Profiling, Inlining, Hashing, Heap Allocations, Type Sizes, Standard Library Types, Iterators, Bounds Checks, I/O, Logging and Debugging, Wrapper Types, Machine Code, Parallelism, General Tips, Compile Times.

Verification: toolchain 1.98.1. Every lint was checked with lintcheck.sh (all `ok`), and its group was read from `clippy-driver -W help`. Claims were tested in a scratch workspace that copies the template's `[workspace.lints]` and `clippy.toml` verbatim (`scratchpad/perf/ws`). Bounds-check claims were checked in asm (`scratchpad/perf/bc.rs`). The scanner was probed through `scanSource` (`scratchpad/perf/scan-probe.mjs`). Build times were measured on a copy of `templates/example-workspace` (a 2-core box, so treat the timings as noisy). The compiling sketches are in `scratchpad/perf/sketches.rs`: clippy-clean under the template plus the three proposed nursery lints, 6 tests pass, and rustdoc is clean.

Not in the source: the brief lists `Option::as_deref`, `split_at_checked` and `as_chunks`, but the book never mentions them. They appear below only as verified compliant alternatives, not as book content. `swap_remove`, `clone_from`, `Rc::make_mut`, `chunks_exact`, `extend` and `SmallVec` are in the book.

## Already covered (summary)

- **Build configuration**
  - Release vs dev builds, `codegen-units = 1` and `lto = "thin"`: template, PERF-09.
  - Custom `profiling` profile with `debug = "line-tables-only"`: performance.md PERF-09.
  - `cargo build --timings` and `cargo llvm-lines`: PERF-01 tool table.
  - Generic function with a non-generic inner fn (the `std::fs::read` shape): PERF-08, `read_config`.
- **Linting**
  - The clippy perf group: `all = deny`.
  - `ptr_arg`: OWN-03.
- **Profiling**
  - Profilers and DHAT with `dhat` behind a feature: PERF-01.
- **Inlining**
  - `#[inline]` only with evidence, `#[inline(always)]` almost never, `#[cold]` measured: PERF-08.
- **Hashing**
  - SipHash vs fast hashers only for non-attacker keys, with a benchmark and H-16: PERF-06.
- **Heap allocations**
  - Box, Rc and Arc sharing costs: OWN-11.
  - Vec growth, `with_capacity`/`reserve` and `shrink_to_fit`: PERF-02.
  - `SmallVec`/`ArrayVec` as a dependency plus a benchmark: PERF-06 table.
  - `format!` allocating: PERF-04.
  - Unneeded `clone`: H-08 and `redundant_clone` (nursery, enabled explicitly).
  - `to_owned` vs a borrowing struct: OWN-06.
  - `Cow`: OWN-10 and patterns.md §14.
  - Reusing a scratch buffer in a loop: PERF-02.
  - `lines()` vs `read_line`: PERF-07 "Also".
- **Type sizes**
  - Boxing large variants, the `size_of` const assertion, field reordering, `u32` indices with measurement: PERF-05.
  - Boxed slices: OWN-15 and the PERF-06 table.
- **Standard library types**
  - `vec![0; n]`: `slow_vector_initialization` (perf, on).
  - `retain`: PERF-02/03.
  - `ok_or_else`: ERR-07 and errors.md.
  - `parking_lot` as a dependency plus a benchmark: PERF-12.
- **Iterators**
  - Avoid `collect` then iterate, return `impl Iterator`, `extend`: PERF-02, OWN-13.
- **Bounds checks**
  - Iterators instead of index loops: PERF-03, H-04.
- **I/O**
  - Lock stdout, `BufWriter` with an explicit `flush`, buffered stdout: PERF-07, H-06, OUT-06.
- **Logging**
  - `tracing` fields format lazily: PERF-04 "Also".
- **Parallelism**
  - rayon and scoped threads: ASY-10.
- **General tips**
  - Measure first and optimize only hot code: PERF-01.
  - Algorithms and data structures first: PERF-06.
  - Caches are bounded: PERF-10.

## Candidates

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement (lint + level / clippy.toml / scanner regex / review / template / CI) | Value (high/med/low) | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| 1 | Iterators #chunks | Use `chunks_exact` (plus `remainder`) instead of `chunks` when the size divides. | refine PERF-03 (and escape-hatches H-04 "Exceptions") | For a **constant** chunk size, MUST use `slice.as_chunks::<N>()` (1.88), which returns `(&[[T; N]], &[T])`. Use `chunks_exact(n)` plus `.remainder()` only for a runtime `n`. | `clippy::chunks_exact_to_as_chunks` is **style**, so it is already denied by `all`. Fix the PERF-03 "Do" text (`xs.chunks_exact(4)`) and the escape-hatches line 203 wording. | high | Verified: the PERF-03 advice as written, `bytes.chunks_exact(4)`, **fails the gate** under the template lints. `as_chunks` gives arrays, so `u32::from_le_bytes(*w)` needs no `try_into`. |
| 2 | Type Sizes #avoiding-regressions-1; Logging #logging-and-debugging (assert vs debug_assert) | Pin hot type sizes with a static assertion. `assert!` always runs. | tooling (scanner) | Scanner `assert-in-prod` MUST NOT flag compile-time assertions (`const _: () = assert!(..)`, `const { assert!(..) }`). | In scan.mjs, replace the regex with `/(?<!\bconst\s*(?:_\s*:\s*\(\s*\)\s*=\s*\|\{\s*))\b(?:assert\|assert_eq\|assert_ne)\s*!/g` (tested: exempts both const forms and still hits a runtime `assert!`; `debug_assert!` is already not matched thanks to `\b`). | high | Verified: PERF-05's own example (performance.md:191) and MOD-10's `const { assert!(N > 0) }` (modern-rust.md:312) raise a soft H-03 hit. A failing const assert is `error[E0080]` at compile time, never a runtime panic, so it is fully compatible with H-03. Clippy does not flag it (`assertions_on_constants` is silent in const context). |
| 3 | Iterators #collect-and-extend | Don't `collect` if the collection is only iterated again. | refine PERF-02 / OWN-13 (enforcement) | Enable `clippy::needless_collect`. PERF-02 and OWN-13 already name it as their Detect. | `needless_collect = "warn"` in `[workspace.lints.clippy]`. It is **nursery**, so it needs enabling. | high | performance.md:103 and ownership.md:521 cite a lint the template does not enable. Tested: it fires on collect-then-`len`/`into_iter().sum()` and does **not** fire on the borrow-ending `keys().collect()` then `remove` loop (the classic false positive). |
| 4 | Bounds Checks #bounds-checks, bullet 3 "Add assertions on the ranges of index variables" | Hoist bounds with `assert!` so LLVM drops per-element checks. | conflict H-03 (see Conflicts), then refine H-04/PERF-03 | To remove bounds checks in a hot loop, MUST slice once up front with a fallible form (`get(..n)`, `split_at_checked`, `first_chunk::<N>()`, slice patterns, `as_chunks::<N>()`), then iterate or `zip`. Never hoist with `assert!`. Never enable `clippy::missing_asserts_for_indexing`. | Review, plus escape-hatches entry. `missing_asserts_for_indexing` is **restriction** and must stay off: it automates the `assert!` approach. | high | asm check: raw index loop has 2 panic calls; `assert!`-hoisted has 1; `get(..n)`+`zip`, `first_chunk` plus constant array indexes, slice pattern, `as_chunks` and `split_at_checked` have **0**. All are clean under `indexing_slicing`. |
| 5 | Build Configuration #disable-debug-info-generation; Compile Times | Dev builds with `debug = false` or `"line-tables-only"` compile 20-40% faster. | tooling (template) | Template SHOULD set `[profile.dev] debug = "line-tables-only"` and `[profile.dev.package."*"] debug = false`. | Template `Cargo.toml` (block below). Humans who want a debugger set `CARGO_PROFILE_DEV_DEBUG=true` locally. | med-high | Measured on example-workspace: `target/` 474 MB to 280 MB (-41%); clean `--all-targets` build 44-69 s to 37-39 s (noisy); incremental rebuild about 1.6 s either way. Panics still print `src/main.rs:8:61` (verified). Agents rebuild constantly and never attach a debugger. |
| 6 | Build Configuration #alternative-allocators | Swap the global allocator (jemalloc, mimalloc) in `src/main.rs`. | new ARC (or fold into ARC-14) | Only a binary crate (the composition root) MAY declare `#[global_allocator]`. Library crates MUST NOT. Choosing one is an H-16 dependency decision with a benchmark, `cfg`-gated to supported targets (jemalloc is not on MSVC). | Review. A scanner rule could hard-flag `#[global_allocator]` in `prod` (library) context: `/#\s*\[\s*global_allocator\s*\]/`. | med | Verified: a library that sets one makes every binary that also sets one fail with "the `#[global_allocator]` in this crate conflicts with global allocator in: a". It works under `unsafe_code = "forbid"`, so the existing dhat pattern is fine. |
| 7 | Heap Allocations #clone | `a.clone_from(&b)` reuses `a`'s allocation. | refine PERF-02 | When overwriting an existing owned value with a copy, SHOULD write `dst.clone_from(&src)`, or `src.clone_into(&mut dst)` for `&[T]`/`&str` sources, not `dst = src.clone()` or `dst = src.to_owned()`. | `clippy::assigning_clones` is **pedantic**, so it is already on (gate-denied). Document the fix in PERF-02. | med | Tested: it fires on both `self.buf = other.buf.clone()` and `self.name = name.to_owned()`. Agents hit this lint and need the named fix. `to_vec()` is not flagged, so review covers it. |
| 8 | Linting #disallowed-types; Hashing #alternative-hashers; Std types #mutex-rwlock-condvar-and-once | After adopting FxHash or parking_lot, ban the std types with `disallowed-types` so they are not used by accident. | refine PERF-06 (opt-in pattern, not a default) | A workspace that adopts a fast hasher (H-16) SHOULD also add `disallowed-types` entries for `std::collections::HashMap`/`HashSet` with a reason. Use the hasher crate's own alias (`foldhash::HashMap`, `foldhash::HashSet`, plus `HashMapExt` for `new`) so no local line names the std type. A local alias instead needs one `#[expect(clippy::disallowed_types, reason)]` at its definition (H-05). The same applies to `std::sync::Mutex` after a parking_lot decision. | clippy.toml (protected, H-05) at adoption time. `disallowed_types` is **style**, already enabled with no entries. | med | Verified: a local alias definition and any `use std::collections::HashMap` (even renamed) are flagged. Uses of the alias from another crate are **not** flagged. foldhash 0.1.5/0.2.0 ship `pub type HashMap<K,V> = std::collections::HashMap<K,V,fast::RandomState>`. `implicit_hasher` (pedantic) fires on `pub fn` taking the alias, not on `pub(crate)`. |
| 9 | Heap Allocations #reading-lines-from-a-file; I/O #reading-input-as-raw-bytes | `lines()` allocates per line; loop `read_line` into one reused `String`. Use `read_until` for bytes. | refine PERF-07 (plus PERF-10) | On hot or large inputs, SHOULD read lines with one reused buffer: `buf.clear(); (&mut r).take(MAX_LINE).read_line(&mut buf)?`. MUST cap line length for untrusted input. Byte-oriented input SHOULD use `read_until(b'\n', &mut Vec<u8>)` to skip UTF-8 validation. | Review. `clippy::unbuffered_bytes` (**perf**, on) catches `.bytes()` on an unbuffered reader. | med | performance.md's main PERF-07 example uses `lines()` with no per-line cap. A single newline-free input grows one `String` without bound, which violates PERF-10. Pattern B covers both fixes (tested). |
| 10 | Wrapper Types #wrapper-types | Values accessed together belong in one wrapper: `Arc<Mutex<(u32, u32)>>`, not two `Arc<Mutex<u32>>`. | refine PERF-12 | Lock granularity MUST follow invariants. Fields that share an invariant or are always read or written together MUST sit under one lock (one acquisition, a consistent snapshot). Split into per-field or per-shard locks only for fields that are independent, and only under measured contention. | Review | med | PERF-12 currently says only "split one lock into per-field locks". Splitting fields with a joint invariant is a race and adds lock-order deadlock risk. The book's merge advice is the missing half. |
| 11 | Logging and Debugging #logging-and-debugging | Don't do data collection for logging when logging is disabled. | refine OUT-08 / PERF-04 | Log-only computation (summaries, joins, sorting for a message) MUST be gated with `if tracing::enabled!(Level::DEBUG) { .. }` (or `event_enabled!`), or passed as a lazy field (`%x`, `?x`). Never build it unconditionally. | Review | med | performance.md covers lazy formatting only. The book's examples are about the collection code feeding the log. Verified: `enabled!`, `event_enabled!` and `span_enabled!` are macros in tracing 0.1.44. |
| 12 | Logging and Debugging (debug_assert); Build Configuration #release-builds | `debug_assert!` runs only in dev builds. Hot, non-safety assertions become `debug_assert!`. | refine H-03 (escape-hatches.md:158) | `debug_assert!` MAY guard **internal** invariants only. Caller-triggerable conditions return errors. Its condition MUST be side-effect free. A profile that ships MUST keep `debug-assertions = false` (the release default), because otherwise `debug_assert!` becomes a production panic (H-03). | Enable `clippy::debug_assert_with_mut_call` = "warn" (**nursery**, needs enabling; tested, it fires on `debug_assert!(v.pop().is_some())`). Review profile diffs. | med | Compatible with H-03 as the standard already sanctions it, but the guards are unstated. The scanner and clippy never flag `debug_assert!`, so the profile guard is the only thing keeping it out of production. |
| 13 | Std Library Types #option-and-result | `ok_or(expensive())` evaluates eagerly; use `ok_or_else`. The same holds for `map_or`, `unwrap_or` and `or`. | tooling (refines ERR-07) | Enable `clippy::or_fun_call`. | `or_fun_call = "warn"`. It is **nursery**; errors.md:456 says it is not enabled and asks for review by eye. | med | Tested: it fires on `ok_or(E::Missing(format!(..)))` and `map_or(String::from("none"), ..)`, and it does **not** fire on cheap tuple- or struct-variant constructors (`ok_or(E::Missing(k))`, `ok_or(E::Named { id })`), which are rust-craft's common case. The book gives exactly this example. |
| 14 | Build Configuration (all sections) | Per-setting trade-offs: codegen-units, LTO, allocators, target-cpu, PGO, opt-level, `panic = "abort"`, strip, linkers. | refine PERF-09 | PERF-09 MUST separate template defaults from per-application (H-16) choices. Defaults: `lto = "thin"`, `codegen-units = 1`, a `profiling` profile, and the dev debug settings (#5). Per-app, each with a benchmark and an AGENTS.md note: `lto = "fat"`, `opt-level = "s"/"z"`, `panic = "abort"`, `strip = "symbols"`, a global allocator, PGO/BOLT (`cargo-pgo`), and `-C target-cpu` (never `native` in repo config; name a level such as `x86-64-v3` for a known fleet). Member-crate `[profile.*]` MUST NOT exist. | Template, plus the PERF-09 table, plus `.claude/rules/cargo.md`. | med | Facts verified on 1.98.1: `panic = "abort"` in release leaves `cargo test --release` unwinding (`should_panic` passes). Release already drops debug sections but keeps symbols (0 `.debug_*` sections, "not stripped"). x86_64 Linux links with LLD by default (`.comment`: "Linker: LLD 22.1.8"), so no linker config belongs in the template, and mold/wild are per-developer `~/.cargo/config.toml` choices. See the verdict below. |
| 15 | Build Configuration intro ("Cargo only looks at the profile settings in the root Cargo.toml") | Profiles in dependencies or members are ignored. | tooling (hook/scanner, cargo rules) | Profiles MUST live only in the workspace root manifest. A `[profile` table in a member `Cargo.toml` is a finding. | Add a line to templates/.claude/rules/cargo.md. Optionally a manifest check that flags `^\[profile` in `crates/*/Cargo.toml`. | low-med | Verified: Cargo prints only "profiles for the non root package will be ignored", and `CARGO_BUILD_WARNINGS=deny` still exits 0, so the CI gate never catches it. An agent "tuning" a crate's profile silently does nothing. |
| 16 | Iterators #collect-and-extend (last paragraph) | When writing an iterator, implement `size_hint` (or `ExactSizeIterator::len`) so `collect` and `extend` pre-size. | new PERF | A hand-written `Iterator` SHOULD override `size_hint` whenever it can bound its remaining items, and implement `ExactSizeIterator`/`FusedIterator` when those contracts truly hold. | Review | med-low | Agents write custom iterators (see `Tokens` in ownership.md:169) and leave the default `(0, None)`, so every `collect` reallocates repeatedly. The existing rules cover consuming iterators, not producing them. |
| 17 | Std Library Types #rcarc-1 | `Rc::make_mut`/`Arc::make_mut` give clone-on-write: clone only when shared. | pattern (links OWN-11, PERF-11, ARC-13) | To update shared immutable state (an event-sourced read model or a config snapshot), SHOULD use `Arc::make_mut(&mut snap)`, not `Arc::new((*snap).clone())`. To take ownership, use `Arc::unwrap_or_clone` (1.76). | Review | med | It fits the owner's event-sourced read models: readers keep old snapshots and the projector mutates in place when it holds the only reference. Tested: no clone when unique, one clone when a reader holds it. Works for `Arc<[T]>`. |
| 18 | Type Sizes #smaller-integers | Store indices as `u32`/`u16` and coerce to `usize` at use. | pattern (refines PERF-05 layout facts plus H-11) | Compact index handles MUST convert without `as`: `usize::try_from(id.0).ok().and_then(\|i\| v.get(i))` on lookup, and `u32::try_from(len).map_err(ArenaFull)?` on insert. | `as_conversions` is already denied; review. | med-low | `usize: From<u32>` does not exist (verified E0277), so agents reach for `as` (H-11) or `unwrap` (H-02). The compliant shape is non-obvious. It extends ownership.md's arena (`NodeId(usize)`). |
| 19 | Inlining #harder-cases, #outlining | Split hot and cold variants with `#[inline(always)]`/`#[inline(never)]`; mark rare paths `#[cold]`. | refine PERF-08 | `#[inline(always)]` is an H-05 exception request with a benchmark, because it is already gate-denied. For a rare branch SHOULD use `std::hint::cold_path()` (stable 1.95, safe) or a `#[cold]` fn, and for the hot/cold split use `#[inline]` on the hot variant plus `#[inline(never)]` on the cold wrapper. | `clippy::inline_always` is **pedantic**, so it is already on (tested: it fires). | med-low | PERF-08 says "review `#[inline(always)]`" but it is already lint-enforced, so an agent following the book gets a red gate. `cold_path` is within MSRV 1.96 and not mentioned anywhere in rust-craft. |
| 20 | Heap Allocations #reusing-collections | Pass `&mut Vec` to fill instead of returning a fresh `Vec`; keep a "workhorse" collection. | refine PERF-02 (exception to OWN-01's "own in returns") | On a measured hot path, MAY add a `*_into(&self, .., out: &mut Vec<T>)` variant beside the returning one. It MUST document whether it appends (the std convention: `read_line`, `read_to_end`, `Extend`) or clears first. Keep a reusable scratch buffer as a private struct field, not a global. | Review | med-low | OWN-01 and the design defaults push "own in returns", so agents never produce the reuse form even when a benchmark calls for it. |
| 21 | Type Sizes intro plus #avoiding-regressions-1 | Types over 128 bytes are moved with `memcpy`. Size assertions are cfg-gated to x86_64 because sizes vary by platform. | refine PERF-05 | Hot types (moved per message, stored in large collections, sent over channels) SHOULD stay at or below 128 bytes on 64-bit targets. Size assertions SHOULD use `<=` bounds that hold on every target, or gate exact `==` checks with `#[cfg(target_pointer_width = "64")]`, and carry a message: `assert!(size_of::<T>() <= 16, "T grew (PERF-05)")`. | Review. CONSIDER clippy.toml `pass-by-value-size-limit = 128` (default 256) and `enum-variant-size-threshold = 128` (default 200); owner's call, noise untested. | med-low | It gives PERF-05 a number. Verified the defaults: a 300 B by-value parameter is flagged and 200 B is not; a 250 B variant gap is flagged and 150 B is not. |
| 22 | Std Library Types #vec-1 | `swap_remove` is O(1) and does not preserve order; `remove` is O(n). | refine escape-hatches "Panicking APIs" plus PERF-06 | Add `v.swap_remove(i)` to the panicking-APIs row with `remove` and `insert` (it panics when `i >= len`, verified in std source). When order does not matter, SHOULD use `swap_remove` over `remove` inside loops. | Review | low-med | The table at escape-hatches.md:214 lists `remove`, `insert`, `swap` and `drain`, but not `swap_remove`. |
| 23 | I/O #buffering | Forgetting to buffer is common when writing. | tooling (doc accuracy) | PERF-07 Detect SHOULD cite `clippy::unbuffered_bytes` (**perf**, on) and drop `verbose_file_reads`. | Edit performance.md:317. | low-med | `verbose_file_reads` is **restriction** and not enabled. It suggests `fs::read_to_string`, which works against PERF-10 streaming. Tested: `unbuffered_bytes` fires on `File::open(p)?.bytes()`. |
| 24 | General Tips (last paragraph) | Optimized code is non-obvious, so comment it with the measurement ("99% of the time this vector has 0 or 1 elements"). | refine PERF-01 (with CRF-04) | Code shaped for performance (special-cased sizes, `_into` variants, `cold_path`, fast hashers, boxed variants) MUST carry a why-comment naming the benchmark or profile that justified it. | Review | low-med | The plan records the numbers, but the code outlives the plan. It also stops the next agent "simplifying" the optimization away. |
| 25 | General Tips ("specially handling collections with 0, 1, or 2 elements") | Special-case the common small sizes first. | pattern | When a profile shows small inputs dominate, SHOULD branch with slice patterns (`[]`, `[x]`, `[a, b]`, `_`) before the general path. | Review | low | This shape is also H-04 compliant. Without it, agents write `if v.len() == 1 { v[0] }`, which the lint rejects. |
| 26 | Benchmarking #benchmarking | Wall time is noisy; instruction counts have lower variance. Use hyperfine for whole programs and Bencher/Gungraun for CI. | refine PERF-01 tool table | For CI regression gates, SHOULD prefer instruction-count benchmarks (Gungraun, formerly iai-callgrind; dev-dependency, H-16). For CLI end-to-end comparisons, use `hyperfine`. | performance.md tool table | low | Divan and criterion wall-time results on shared CI runners flap, and agents then "fix" noise. |
| 27 | Profiling #frame-pointers, #debug-info | Use frame pointers for better stack traces; stable profiles cannot set rustflags. | refine PERF-01 tool table | For perf or samply runs, SHOULD build with `RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling`. | performance.md tool table | low | A one-line tooling fact. It is not in rust-craft. |
| 28 | Heap Allocations #avoiding-regressions | dhat-rs heap-usage tests assert allocation counts. | pattern (TST-16 complement) | CONSIDER an allocation-count test (dhat testing mode, dev-dependency, H-16) for hot paths whose allocation rate was deliberately reduced. | Review | low | This is a deterministic alternative to a benchmark for allocation regressions. It fits TST-16. |

### Build configuration verdict (template)

The existing release profile is confirmed. I propose adding a `profiling` profile and a dev profile. Everything else is a per-application H-16 decision.

```toml
# Release profile (PERF-09): document deviations in the root AGENTS.md.
# Per-application decisions (H-16, benchmark required), never template defaults:
# lto = "fat", opt-level = "s"/"z", panic = "abort", strip = "symbols",
# a #[global_allocator] (binaries only), PGO/BOLT, -C target-cpu.
[profile.release]
lto = "thin"
codegen-units = 1

# `cargo build --profile profiling`: release code with file:line for profilers.
[profile.profiling]
inherits = "release"
debug = "line-tables-only"

# Dev and test builds keep file:line in panics and backtraces, but no debugger locals.
# To use a debugger locally: CARGO_PROFILE_DEV_DEBUG=true.
[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false                 # dependencies: no debug info (-41% target dir, measured)
```

| Book suggestion | Template now | Verdict |
|---|---|---|
| `codegen-units = 1` | set | Keep. It only costs release build time, and the CI gate never builds release. |
| `lto` thin or fat | `"thin"` | Keep thin. `"fat"` is per-app, with a benchmark (the book notes it is "not always" better). |
| `panic = "abort"` | unset | Per-app (H-16). Tests ignore it (verified). Because H-03 already bans panics, the choice decides only what residual std or overflow panics do: no `Drop`, no `BufWriter` flush, and `JoinHandle` cannot observe the panic. Keep unwind for servers; abort is fine for CLIs. |
| `opt-level` s or z | unset (3) | Per-app, for size-constrained targets only. |
| `strip` | unset | Keep the default. Release already has no debug sections but keeps symbols for backtraces (verified). `"symbols"` is per-app. |
| debug info (release) | unset | Keep off. Add the `profiling` profile instead. |
| debug info (dev) | unset | Add it (#5). |
| `-C target-cpu=native` | unset | Never in repo config (CI-built binaries hit SIGILL on older CPUs, and it moves the cache key). Per-deployment, with a named level. |
| PGO / cargo-pgo | none | Per-app build-pipeline decision (H-16). |
| jemalloc / mimalloc | none | Per-binary dependency (H-16), in the composition root only (#6). |
| lld / mold / wild | none | lld is already the default on x86_64 Linux on 1.98.1 (verified). mold/wild are per-developer in `~/.cargo/config.toml`, never in the repo, because CI and other developers may lack them. |
| Cranelift, `-Zthreads` | none | Skip: nightly-only, and the toolchain is pinned to stable. |
| (implied) `debug-assertions`, `overflow-checks` | defaults | Shipping profiles keep the defaults (`false`). `debug-assertions = true` turns `debug_assert!` into production panics (#12). |

### Lint verification (all `ok` in lintcheck.sh on 1.98.1)

| Lint | Group | Template status | Proposal |
|---|---|---|---|
| `needless_collect` | nursery | **not enabled** (cited as Detect in PERF-02, OWN-13) | enable `warn` (#3) |
| `or_fun_call` | nursery | not enabled | enable `warn` (#13) |
| `debug_assert_with_mut_call` | nursery | not enabled | enable `warn` (#12) |
| `redundant_clone` | nursery | enabled explicitly | covered (H-08) |
| `assigning_clones` | pedantic | on via pedantic | document in PERF-02 (#7) |
| `inline_always` | pedantic | on via pedantic | document in PERF-08 (#19) |
| `large_types_passed_by_value` | pedantic | on via pedantic (256 B default) | covered (OWN-16); threshold CONSIDER (#21) |
| `trivially_copy_pass_by_ref` | pedantic | on via pedantic | covered (OWN-16) |
| `inefficient_to_string` | pedantic | on via pedantic | covered (PERF-04) |
| `implicit_hasher` | pedantic | on via pedantic | note in the hasher opt-in (#8) |
| `format_push_string` | pedantic | on via pedantic **and** listed explicitly | the explicit line is redundant but harmless |
| `large_enum_variant` | perf | on (deny via `all`; 200 B gap default) | covered (PERF-05) |
| `slow_vector_initialization` | perf | on | covers the book's `vec![0; n]` advice |
| `unbuffered_bytes` | perf | on | cite in PERF-07 (#23) |
| `expect_fun_call`, `manual_memcpy`, `extend_with_drain`, `unnecessary_to_owned`, `vec_init_then_push` | perf | on | covered (PERF-02/04) |
| `chunks_exact_to_as_chunks` | style | on (deny) | fix the PERF-03 text (#1) |
| `disallowed_types` | style | on, no entries | opt-in entries (#8) |
| `assertions_on_constants` | style | on | does not fire on `const _: () = assert!(..)` (verified) |
| `verbose_file_reads` | restriction | not enabled | remove from PERF-07 Detect; do not enable (#23) |
| `missing_asserts_for_indexing` | restriction | not enabled | **never enable** (#4, conflicts H-03) |
| `large_stack_frames` | nursery | not enabled | no proposal |

## Conflicts

- **Bounds Checks, bullet 3, "Add assertions on the ranges of index variables."** This conflicts with H-03: `assert!` is a runtime panic path, and the scanner soft-flags it. It also conflicts with H-04, since the indexing it enables is denied by `indexing_slicing`.
  - Compatible nugget: hoist the check into a **fallible** slice-once instead: `let (Some(a), Some(b)) = (a.get(..n), b.get(..n)) else { return Err(..) };`, then `zip`, or `first_chunk`, `split_at_checked`, slice patterns or `as_chunks`.
  - asm-verified to remove every bounds check (0 panic calls vs 1 for the `assert!` version).
  - Record in escape-hatches.md that `clippy::missing_asserts_for_indexing` (restriction) automates the conflicting approach and must stay off.
- **Bounds Checks, "As a last resort ... `get_unchecked`."** This conflicts with H-01 (no `unsafe`). No nugget: if a profile still shows checks after slice-once, it is an H-05 exception request per escape-hatches.md H-04.
- **Logging and Debugging, "make it a `debug_assert!`."** Not a conflict. escape-hatches.md:158 already sanctions `debug_assert!` for internal invariants. It needs the guards in #12: internal-only, side-effect free, and never `debug-assertions = true` in a shipping profile.
- **Type Sizes, `static_assertions::assert_eq_size!`.** Not a conflict in substance, but it is a dependency (H-16). The std `const _: () = assert!(..)` already in PERF-05 is equivalent. It is a compile-time E0080, not a runtime panic, so it is H-03-compatible (the scanner fix is #2).
- **Inlining, the hot/cold split with `#[inline(always)]`.** This is not against a settled position, but it collides with the template's pedantic `inline_always`, which makes it an H-05 exception request. The compatible form is #19.

## Patterns / examples worth copying

All compile clean under the template lints plus the three proposed nursery lints. Full versions with tests are in `scratchpad/perf/sketches.rs`.

**A. Bounds hoisting without `assert!` (H-03/H-04-compliant; asm shows 0 panic calls).**
```rust
/// Dot product of the first `n` elements; `None` if either slice is shorter.
#[must_use]
pub fn dot(a: &[u32], b: &[u32], n: usize) -> Option<u32> {
    let (Some(a), Some(b)) = (a.get(..n), b.get(..n)) else { return None };
    Some(a.iter().zip(b).fold(0_u32, |s, (x, y)| s.wrapping_add(x.wrapping_mul(*y))))
}
/// Fixed header: constant indexes into an array are proven in bounds.
#[must_use]
pub fn header_sum(buf: &[u8]) -> Option<u32> {
    let h: &[u8; 4] = buf.first_chunk::<4>()?;
    Some(u32::from(h[0]) + u32::from(h[1]) + u32::from(h[2]) + u32::from(h[3]))
}
```

**B. Allocation-free, length-capped line loop (PERF-07 plus PERF-10).**
```rust
let mut line = String::new();
loop {
    line.clear(); // one buffer for the whole input
    let n = (&mut reader).take(MAX_LINE).read_line(&mut line).map_err(LineError::Io)?;
    if n == 0 { return Ok(()); }
    if !line.ends_with('\n') && u64::try_from(n).map_or(true, |n| n >= MAX_LINE) {
        return Err(LineError::TooLong { limit: MAX_LINE });
    }
    f(line.trim_end_matches(['\n', '\r']));
}
```
Byte-oriented variant: `while reader.read_until(b'\n', &mut buf)? != 0 { ..; buf.clear(); }`.

**C. Copy-on-write read model (`Arc::make_mut`; event-sourced projections).**
```rust
/// Applies an event; clones the snapshot only if a reader still holds it.
pub fn apply(snapshot: &mut Arc<Totals>, cents: u64) {
    let totals = Arc::make_mut(snapshot);
    totals.orders = totals.orders.saturating_add(1);
    totals.cents = totals.cents.saturating_add(cents);
}
```

**D. Compact `u32` handles without `as` (H-11) or `unwrap` (H-02).**
```rust
pub fn add(&mut self, value: T) -> Result<NodeId, ArenaFull> {
    let id = u32::try_from(self.nodes.len()).map_err(ArenaFull)?; // ArenaFull(TryFromIntError)
    self.nodes.push(value);
    Ok(NodeId(id))
}
pub fn get(&self, id: NodeId) -> Option<&T> {
    usize::try_from(id.0).ok().and_then(|i| self.nodes.get(i))
}
```

**E. One lock for a joint invariant (Wrapper Types).**
```rust
/// `hits <= total` holds under one lock; two locks would let readers see it broken.
pub struct Stats { inner: Mutex<StatsInner> }
pub fn record(&self, hit: bool) {
    let mut s = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
    s.total = s.total.saturating_add(1);
    if hit { s.hits = s.hits.saturating_add(1); }
}
```

**F. Fast-hasher opt-in with `disallowed-types` (adopted by an H-16 decision; clippy.toml edit is H-05).**
```toml
# clippy.toml, added in the same change that adopts foldhash
disallowed-types = [
  { path = "std::collections::HashMap", reason = "PERF-06: use foldhash::HashMap (keys are process-generated)" },
  { path = "std::collections::HashSet", reason = "PERF-06: use foldhash::HashSet" },
]
```
```rust
use foldhash::{HashMap, HashMapExt as _}; // external alias: no local line names the std type
let mut counts: HashMap<u32, u32> = HashMap::with_capacity(ids.len());
```

**G. Reuse instead of reallocate (`clone_into`, `_into` variants, `as_chunks`).**
```rust
src.clone_into(dst);                       // not `*dst = src.to_owned()` (assigning_clones)
pub fn squares_into(values: &[u32], out: &mut Vec<u64>) {   // appends; caller clears
    out.extend(values.iter().map(|v| u64::from(*v) * u64::from(*v)));
}
let (words, _tail) = bytes.as_chunks::<4>(); // not chunks_exact(4): chunks_exact_to_as_chunks
```

**H. Rare branch hint (1.95, safe) instead of `#[inline(always)]` splitting.**
```rust
match code {
    0 => Ok(value),
    _ => { std::hint::cold_path(); Err(DecodeError::Code { code }) }
}
```

**I. Gate log-only work.**
```rust
if tracing::enabled!(tracing::Level::DEBUG) {
    let summary = expensive_summary(&batch);
    tracing::debug!(%summary, "batch applied");
}
```

## Notable skips

- **Nightly-only:** Cranelift backend, `-Zthreads`, `-Zprint-type-sizes`, `-Zmacro-stats`, `build-std`, the `#[bench]` harness. The toolchain is pinned to stable 1.98.1.
- **`core::arch` intrinsics** (Machine Code): an `unsafe` or `target_feature` domain (H-01).
- **Byte-wise hashing** via `zerocopy`/`bytemuck` derives, and **nohash-hasher**: dependencies with niche, measurement-heavy benefit.
- **`ThinVec`, `smartstring`, `smallstr`**: niche dependencies. The existing PERF-06 "dependency plus benchmark" row covers the class.
- **jemalloc THP via `MALLOC_CONF`**: an ops or deployment setting, not code guidance.
- **Replacing `Option::map`/`map_err` with `match` to cut LLVM IR** (Compile Times #llvm-ir): works against CRF-10 readability for a small effect. Record at most as a measured last resort.
- **`chain` and `filter_map` micro-tips, and `iter().copied()` for LLVM** (Iterators): measure-only advice with no testable rule. `cloned_instead_of_copied` (pedantic, on) already covers the idiom part.
- **v0 symbol mangling, `cargo-wizard`, Compiler Explorer and `cargo-show-asm`**: tool pointers. At most one line in the PERF-01 tool table.
