# Rust Atomics and Locks (Mara Bos): findings

Source: https://mara.nl/atomics/. Read: preface and chapters 1-3 and 7-8 in full; chapters 4, 5, 6 and 9 in full, mined for design lessons that apply to safe code; chapter 10 skimmed.

Verification: every API name, stabilization version, lint name and behavioural claim below was checked on the pinned toolchain (rustc/clippy 1.98.1, plus MSRV 1.96 where it matters) with scratch crates under `scratchpad/atomics/`, or against the 1.98.1 `rust-src` doc text. Miri claims were checked with nightly Miri (2026-09-27). Code sketches come from `scratchpad/atomics/ws/crates/pats/src/lib.rs`, which passes `cargo clippy --all-targets` under the template `[workspace.lints]` and `clippy.toml` (with `significant_drop_in_scrutinee` also on), passes 7 tests, and builds on 1.96.

## Already covered (summary)

- Scoped threads for borrowing (ch1 §Scoped Threads): ASY-10, OWN-07, OWN-11 ladder, ownership.md E0373 row.
- `move` closures that clone only the `Arc` handle (ch1 §Naming Clones): OWN-14, PERF-11.
- `Arc` vs `Rc`, and `Rc` is `!Send` (ch1 §Reference Counting): OWN-11.
- `Cell`/`RefCell` vs `Mutex`/atomics as a design decision (ch1 §Interior Mutability): OWN-12.
- Short critical sections, dropping the guard before slow work (ch1 §Rust's Mutex): PERF-12.
- A `Mutex` instead of several atomics when values must be consistent (ch2 §Statistics): the async-concurrency.md atomics section.
- `OnceLock`/`LazyLock` instead of hand-rolled lazy init (ch2 §Lazy Initialization): MOD-03.
- The Mutex+Condvar channel "might grow without bounds" (ch5): H-13, ASY-07.
- Consuming `self` so an operation happens once (ch5 §Safety Through Types): API-13, OWN-04.
- Compile-time `Send + Sync` assertions: ASY-11.
- `Deref` only for guard or smart-pointer types (ch4): API-19.
- `black_box` in benchmarks (ch7 §Impact on Performance, ch9 §Benchmarking): TST-16, performance.md.
- Benchmark before choosing spin versus sleep, or a lock implementation (ch9 "it depends"): PERF-01; `parking_lot` is an H-16 decision (PERF-12).
- Mapping the `join()` panic payload (ch1): the errors.md unwrap table.
- `Box::leak`, `static mut` and `mem::forget` as sharing tricks (ch1 §Leaking): H-07.
- `lock()` on `&mut Mutex` versus `get_mut` (ch5 `Drop` uses `get_mut`): `clippy::mut_mutex_lock` (style, on via `all`; verified firing).
- `let _ = m.lock()`: rustc `let_underscore_lock` (deny-by-default; verified).
- Invalid constant orderings (`load(Release)`, `store(Acquire)`, `fence(Relaxed)`, CAS failure `Release`): rustc `invalid_atomic_ordering` (deny-by-default; verified). See #3 for the runtime gap.

## Candidates

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement (lint + level / clippy.toml / scanner regex / review / template / CI) | Value (high/med/low) | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| 1 | ch3 [#common-misconceptions](https://mara.nl/atomics/memory-ordering.html#common-misconceptions) "SeqCst ... a warning sign"; [#release-and-acquire-ordering](https://mara.nl/atomics/memory-ordering.html#release-and-acquire-ordering); [#relaxed](https://mara.nl/atomics/memory-ordering.html#relaxed) | `SeqCst` claims a dependency on every `SeqCst` op in the program, which makes it harder to review. `Release` tells the reader "relates to an acquire on the same variable". `Relaxed` still gives a per-variable total modification order. | refine ASY-10 (new ASY-13) | ASY-13 (MUST): Every atomic ordering carries a justification comment. `Relaxed` says why no other memory depends on the value (counter, statistic, id, flag with no payload). `Release`/`Acquire`/`AcqRel` name the operation(s) they pair with. `SeqCst` names the cross-variable single-total-order property it needs (store-own-flag-then-load-other-flag). "Unsure" is not a justification; use a `Mutex`, `OnceLock` or channel instead. | Review. Scanner soft advisory `/\bSeqCst\b/g` (prod, bin: soft; test: null), message "justify the total-order property or downgrade". | high | The current async-concurrency.md table has this inverted: it says "Unsure → `SeqCst`" and exempts `SeqCst` from the pairing comment ("Every non-`SeqCst` ordering gets a comment"). The owner wants this file to be the ordering authority, so the defaults matter. |
| 2 | ch3 [#example-locking](https://mara.nl/atomics/memory-ordering.html#example-locking), [#example-lazy-initialization-with-indirection](https://mara.nl/atomics/memory-ordering.html#example-lazy-initialization-with-indirection); ch6 [#basic-reference-counting](https://mara.nl/atomics/building-arc.html#basic-reference-counting) | `Acquire` applies only to the load half of an RMW and `Release` only to the store half. A lock takes with `Acquire` and frees with `Release`. The last-owner decrement is `Release` plus `fence(Acquire)` on the zero path. The CAS failure ordering is a load ordering. | refine ASY-10 table (ASY-13) | ASY-13 (MUST): Choose RMW orderings by role: claim or lock → `Acquire`; release or publish → `Release`; a hand-off that both consumes and publishes → `AcqRel`. A counter is a "statistic" (`Relaxed`) only if no decision about other memory is made from its value. A "last one out cleans up" or "all workers done" counter synchronizes: `Release` on decrement, `Acquire` (or `fence(Acquire)`) where zero is observed. `compare_exchange`'s failure ordering is `Relaxed` unless the failed value is used to read data another thread published (then `Acquire`). | Review; rustc `invalid_atomic_ordering` (deny-by-default, on) rejects `Release`/`AcqRel` failure orderings. | high | The existing table has only three rows and no role for claim-only or release-only RMWs. The counter rule is the mistake agents actually make: a "done" counter used to read results. |
| 3 | ch3 [#release-and-acquire-ordering](https://mara.nl/atomics/memory-ordering.html#release-and-acquire-ordering): "Release only applies to store operations, and acquire only to load operations". Verified on 1.98.1: `a.load(o)` with a runtime `o = Release` panics ("there is no such thing as a release load"); CAS with runtime failure `AcqRel` panics. | Invalid ordering combinations exist. The compiler catches constants; runtime values panic. | new ASY (ASY-13 clause) + escape-hatches.md panicking-API row | ASY-13 (MUST): Pass atomic orderings as literal `Ordering::X` constants at the call site. Never take `Ordering` as a function parameter or compute it: invalid combinations then panic at runtime (H-03), and `invalid_atomic_ordering` only checks constants. | rustc `invalid_atomic_ordering` (deny-by-default, already on) for constants. Scanner soft `/\b\w+\s*:\s*(?:std::sync::atomic::)?Ordering\b/g` in `fn` params (prod: soft; advisory only, because it also matches `std::cmp::Ordering` parameters in comparators). | med | This closes a panic path the gate cannot see. It is cheap to state. |
| 4 | ch3 [#fences](https://mara.nl/atomics/memory-ordering.html#fences), [#compiler-fences](https://mara.nl/atomics/memory-ordering.html#compiler-fences) | A fence moves the ordering off the operation. It pays off only when one fence covers several atomics or applies conditionally. `compiler_fence` is for signal and interrupt handlers. | new (ASY-13 clause) | ASY-13 (SHOULD): Put the ordering on the operation. Use `fence(Acquire)`/`fence(Release)` only when one fence covers several atomics or applies on one branch, and name the paired fence or operation in a comment. `compiler_fence` MUST NOT appear in application code. | Scanner soft `/\bcompiler_fence\s*\(/g` (prod: hard). Review. | low | Rare in app code, but when it appears it needs a rule. |
| 5 | ch2 [#fetch-and-modify-operations](https://mara.nl/atomics/atomics.html#fetch-and-modify-operations) ("fetch_add and fetch_sub implement wrapping behavior"), [#example-id-allocation](https://mara.nl/atomics/atomics.html#example-id-allocation), [#example-handle-overflow](https://mara.nl/atomics/atomics.html#example-handle-overflow) | Atomic add and sub wrap silently. An id counter wraps back to 0. Check before the store, not after. | refine ERR-09 | ERR-09 (MUST, addition): `fetch_add`/`fetch_sub` wrap silently in every build. Where wrap breaks uniqueness or accounting (ids, balances, capacities), use `try_update(.., \|n\| n.checked_add(k))` (1.95) and map `Err` to a typed error. Otherwise comment that wrap is intended. Never check the returned value after the fact: the counter has already moved. | Review. | high | Agents write `NEXT_ID.fetch_add(1, Relaxed)` for ids by default. The lints cannot see it, because it is a method call and not an operator. |
| 6 | ch2 [#fetch-update](https://mara.nl/atomics/atomics.html#fetch-update); ch7 [#compare-exchange-optimization](https://mara.nl/atomics/hardware.html#compare-exchange-optimization) ("use the dedicated fetch-and-modify methods rather than a compare-and-exchange loop"). Verified in 1.98.1 `core/src/sync/atomic.rs`: `try_update` and `update` are `#[stable(since = "1.95.0")]`; `fetch_update` is `#[deprecated(since = "1.99.0", note = "renamed to try_update")]` (silent on 1.98.1; `-W deprecated_in_future` shows it). | Use the dedicated RMW (`fetch_add`, `fetch_max`, `fetch_or`, `swap`). For a computed or conditional update use the library helper, not a hand-written CAS loop. | refine MOD-07 | MOD-07 (SHOULD): Use the dedicated atomic RMW methods first. For a computed update use `Atomic*::update` (1.95), and for a conditional one use `try_update` (1.95), which returns `Result<old, old>`. Do not hand-write CAS loops, and do not use `fetch_update` (deprecated in 1.99 as a rename of `try_update`). | Scanner soft `/\.\s*fetch_update\s*\(/g` (prod: soft, test: soft). Optional rustc `deprecated_in_future = "warn"` (an H-05 lint-config decision). | high | The standard lists modern APIs by name, and this one changed recently. Both are within MSRV 1.96, and the example compiled on 1.96 and 1.98.1. |
| 7 | ch2 [#cas](https://mara.nl/atomics/atomics.html#cas) (weak "should be preferred where ... spurious failure is insignificant"); [#example-racy-init](https://mara.nl/atomics/atomics.html#example-racy-init) (strong when not looping); ch7 [#compare-and-exchange-on-arm](https://mara.nl/atomics/hardware.html#compare-and-exchange-on-arm) | `compare_exchange_weak` may fail spuriously; it maps onto LL/SC on ARM. `compare_exchange` suits a single attempt. | new (ASY-13 clause) | ASY-13 (SHOULD): If a CAS loop is unavoidable (see #6), use `compare_exchange_weak` inside the retry loop and `compare_exchange` for a single attempt whose failure means "someone else won". | Review. | med | This is the classic weak/strong question. The rule is one line and testable by reading. |
| 8 | ch2 [#cas](https://mara.nl/atomics/atomics.html#cas) (ABA problem); 1.98.1 `try_update` docs ("be careful of the ABA problem if this atomic integer is an index") | CAS succeeds if a value went A→B→A in between. Algorithms keyed on reused values (indexes, pointers) break. | new (ASY-13 clause) | ASY-13 (MUST): A CAS or `try_update` on a value that can return to an earlier state (slot index, free-list head, reused id) documents why ABA cannot occur, or packs a generation counter into the value, or uses a `Mutex`. | Review. | med | Safe code can still build free-lists over `Vec` indexes with `AtomicUsize`. ABA is a logic bug, not UB, so no tool catches it. |
| 9 | ch1 [#lifetime-of-mutexguard](https://mara.nl/atomics/basics.html#lifetime-of-mutexguard): the guard in `if let Some(item) = list.lock().unwrap().pop()` lives through the body; a plain `if` condition does not. Verified on 1.98.1, edition 2024: the guard is still held in the `if let` then-block, all `match` arms, `while let` bodies and let-chain bodies; it is released in the `if let` else-block (the edition 2024 change; held in 2021), in the plain `if` body, and after `let x = m.lock()?.pop();`. | Temporaries in a `match`/`if let`/`while let` scrutinee live to the end of the construct. Move the pop into its own `let` statement. | new (ASY-14) + refine PERF-12 | ASY-14 (MUST): Never leave a lock guard as a temporary in a `match`, `if let`, `while let` or let-chain scrutinee when the body does more than read the guarded value. Take the value in its own `let` statement (the guard drops at the `;`), or bind the guard and `drop` it explicitly. Edition 2024 only drops scrutinee temporaries before `else`: the then-block, match arms and `while let` body still hold the lock. | `clippy::significant_drop_in_scrutinee` (nursery; NOT enabled; needs enabling, see #10). Verified to flag all three shapes. The template's `significant_drop_tightening` caught none of them. | high | This is the most common lock bug agents write. modern-rust.md's table row ("drop before `else`") is correct but invites the wrong conclusion that edition 2024 fixed the pitfall. |
| 10 | Tooling for #9 (verified) | Enabling the lint surfaces #9 at the gate. | tooling | Add `significant_drop_in_scrutinee = "warn"` to `[workspace.lints.clippy]` (# ASY-14). Rewrite modern-rust.md's `cached_or_fill` to bind first (`let cached = *cache.lock()?; if let Some(v) = cached { return Ok(v); } ...`). The current form trips the lint (verified), because the lint still says "lives until the end of the `if let`", although edition 2024 releases before `else`. The rewrite is lint-clean. | Template `Cargo.toml` (H-05/H-16 human decision). | high | A nursery lint, but it had no other false positives in the probes. The one hit on an existing example has an obvious, clearer rewrite. |
| 11 | ch1 [#rusts-mutex](https://mara.nl/atomics/basics.html#rusts-mutex) (drop the guard before `thread::sleep`: 10 s becomes 1 s) | Holding a lock across a wait serializes every thread. | refine PERF-12 | PERF-12 (MUST, extend "no I/O under a lock"): Drop every guard before a blocking call: I/O, `thread::sleep`, `JoinHandle::join`, blocking channel `send`/`recv`, `Condvar::wait` on another lock, `Barrier::wait`, and calls into unknown code (callbacks, `dyn Trait`). | Review. `significant_drop_tightening` (on) catches only a named guard kept past its last use (verified). | med | PERF-12 names I/O and `.await` only. Join, recv and callbacks under a lock are the deadlock-shaped variants. |
| 12 | ch8 [#posix](https://mara.nl/atomics/os-primitives.html#posix) (recursive locking: UB, error, deadlock or success depending on configuration), [#windows-srw](https://mara.nl/atomics/os-primitives.html#windows-srw) (a second shared lock on the same thread can deadlock behind a queued writer). std 1.98.1 docs: `Mutex::lock` "will not return on the second call (it might panic or deadlock)"; `RwLock::read`/`write` "might panic ... if the lock is already held by the current thread"; `OnceLock::get_or_init` re-entrant init "deadlocks". Verified: Miri reports "the evaluated program deadlocked" for a double `lock()`. | Never lock what the current thread already holds. | new (ASY-14 clause) | ASY-14 (MUST): No re-entrant locking. A method that holds a guard never calls code that locks the same `Mutex`/`RwLock` (including a second `read()`) or initializes the same `OnceLock`/`LazyLock`. Lock once at the public boundary and pass `&mut Inner`/`&Inner` to private helpers. | Review (grep for `self.lock()`/`.read()` inside methods called while a guard is live). Miri (#30) catches the deterministic cases. | high | Agents add a `fn inner(&self) -> MutexGuard` helper and call it from two methods on one path. That deadlocks, or panics, which is an H-03 violation by another route. |
| 13 | Not in the book (brief topic). Verified pattern: `transfer()` in `pats` locks two `Mutex<u64>` in ascending `id` order, and 2×500 opposite transfers complete. | Deadlock avoidance when holding two locks. | new (ASY-14 clause) | ASY-14 (MUST): A code path that holds more than one lock acquires them in one documented global order (field order within a type, a stable key such as `id` across instances), and rejects "same object" before locking (`SameAccount` error), since locking one `Mutex` twice is #12. Prefer one lock over two when the data changes together. | Review. Miri did NOT find a two-thread inversion in 64 seeds (verified), so this stays a review rule. | high | This is the canonical multi-lock deadlock, and no tool in the template finds it. |
| 14 | ch1 [#condvar](https://mara.nl/atomics/basics.html#condvar); ch9 [#avoiding-spurious-wake-ups](https://mara.nl/atomics/building-locks.html#avoiding-spurious-wake-ups), [#thundering-herd](https://mara.nl/atomics/building-locks.html#thundering-herd). std 1.98.1 docs: `Condvar::wait` "may panic if it is used with more than one mutex over time"; `wait_while`/`wait_timeout_while` are stable since 1.42. | Condvars wake spuriously. A condvar is used with a single mutex. `notify_all` wakes a thundering herd. | new (ASY-16) | ASY-16 (MUST): Wait on a `Condvar` only through `wait_while`/`wait_timeout_while` with the predicate, never a bare `wait` (spurious wakeups). Keep each `Condvar` in the same struct as the one `Mutex` it pairs with. Change the state under the lock, then notify. Use `notify_one` when any single waiter can make progress, and `notify_all` only when every waiter must re-check. (SHOULD) Prefer `wait_timeout_while` so the wait is bounded (H-13). | Review; scanner soft `/\.\s*wait\s*\(\s*\w+\s*\)/g` on Condvar receivers (advisory). | med | rust-craft has no Condvar guidance at all. Agents write `while cond { g = cv.wait(g).unwrap(); }`, which is H-02 plus a hand loop. |
| 15 | Tooling friction found while verifying #14 | `clippy::significant_drop_tightening` (enabled in the template) warns on `let g = m.lock()..; let (g, _) = cv.wait_timeout_while(g, ..)`. Its suggested fix is nonsense (a double `unwrap_or_else`). Passing the lock expression inline as the first argument is clean. | tooling / pattern | Document the lint-clean shape in async-concurrency.md: `cv.wait_timeout_while(self.m.lock().unwrap_or_else(PoisonError::into_inner), timeout, \|q\| q.is_empty())`, then `drop` the returned guard after the last use. | None. This documents a nursery false positive the zero-warning gate would otherwise turn into a mystery. | low | It saves an agent a PROC-09 loop on a lint it cannot suppress (H-05). |
| 16 | ch1 [#parking](https://mara.nl/atomics/basics.html#parking): `park` can wake spuriously; an `unpark` before `park` is kept, but unparks "don't stack up". ch2 [#synchronization](https://mara.nl/atomics/atomics.html#synchronization): `park_timeout` instead of `sleep` for interruptible periodic waits. | Parking is for the simplest single-consumer cases. | new (ASY-16 clause) | ASY-16 (SHOULD): Prefer channels or `Condvar` to `thread::park`. When parking, loop on an explicit condition, since `park` may return spuriously and repeated `unpark`s collapse into one. Use `park_timeout` instead of `sleep` for periodic work that must wake early. | Review. | low | Rare in app code, but the spurious-return trap is real. |
| 17 | ch4 [#a-minimal-implementation](https://mara.nl/atomics/building-spinlock.html#a-minimal-implementation) (`spin_loop` hint); ch7 [#failing-compare-exchange](https://mara.nl/atomics/hardware.html#failing-compare-exchange) (a failing CAS still claims the cache line); ch9 [#optimizing-further](https://mara.nl/atomics/building-locks.html#optimizing-further) and #benchmarking ("it depends" on the platform) | Spinning wastes CPU, and whether it pays off depends on the hardware. | new (ASY-16 clause) | ASY-16 (MUST): No busy-wait loops in application code (loops over an atomic load, `try_lock`, a CAS or `yield_now` waiting for another thread). Block on `Condvar`, a channel, `Barrier`, `JoinHandle::join` or `park`. A spin kept on benchmark evidence (PERF-01) is bounded, calls `std::hint::spin_loop()`, checks with a `load` before any CAS, and falls back to blocking. | `clippy::missing_spin_loop` (perf, on via `all`) is weak: verified to fire on `while local.load(..) {}` but NOT through a `&AtomicBool` parameter or on a CAS loop. So: review. | med | Agents "wait for ready" with `while !flag.load(..) {}` or `yield_now` loops, and the lint misses the parameter form. |
| 18 | ch4-6 and ch9 as a whole; ch2 [#example-lazy-init](https://mara.nl/atomics/atomics.html#example-lazy-init) ("the standard library provides exactly this functionality through `Once` and `OnceLock`"); ch9 #benchmarking (results differ by OS and CPU) | Building primitives is subtle, and the std versions are tuned per platform. | new (ASY-17) or refine ASY-10 | ASY-17 (MUST): Do not build synchronization primitives. Use std: `Mutex`, `RwLock`, `Condvar`, `OnceLock`, `LazyLock`, `Barrier`, `mpsc::sync_channel`, `thread::park`, `thread::scope`, `Arc`/`Weak`. Reject hand-rolled locks, once-cells, semaphores and channels built from atomic flags. Crate alternatives (`parking_lot`, `crossbeam`, `arc-swap`) are H-16 decisions with a benchmark. | Review; scanner soft `/\bAtomicBool\b[^;\n]*\b(?:lock\|busy\|locked\|in_use)\b/gi` (advisory). | high | This is the owner's requested headline. Safe code can still build a broken "lock" from `AtomicBool` plus a spin guarding other atomics. |
| 19 | ch1 [#thread-builder](https://mara.nl/atomics/basics.html#thread-builder): "`std::thread::spawn` ... simply panics if it is unable to spawn a new thread"; `Builder::spawn` returns `io::Result`; names show in panic messages and tools. Verified in 1.98.1 source: `Scope::spawn` also panics on OS failure (use `Builder::spawn_scoped`, stable 1.63), and `Builder::spawn`/`spawn_scoped` panic "if a thread name was set and it contained null bytes" (confirmed at runtime). | Use `Builder` for fallible, named threads. | new (ASY-15) + escape-hatches.md panicking-API rows | ASY-15 (MUST): Production code spawns threads with `thread::Builder::new().name(..).spawn(..)` or `.spawn_scoped(scope, ..)` and propagates the `io::Error`. `thread::spawn` and `Scope::spawn` panic when the OS refuses a thread (H-03). Thread names are literals or validated to contain no `\0`, since `Builder::spawn` panics on a NUL. | Option A (verified): `clippy.toml` `disallowed-methods` for `std::thread::spawn` and `std::thread::Scope::spawn`. This works, but also fires inside `#[cfg(test)]`, so tests must use `Builder` too. Option B: scanner rule `thread-spawn`, `/\bthread\s*::\s*spawn\s*\(/g` prod: hard, test: null (production-only). | high | This matches the owner's "panicking APIs" stance exactly. Today it is absent from escape-hatches.md, and existing examples use the panicking forms: ownership.md `spawn_sum`/`spawn_counter`/`parallel_sum`, async-concurrency.md ASY-10. |
| 20 | ch1 [#scoped-threads](https://mara.nl/atomics/basics.html#scoped-threads) ("all threads that haven't been joined yet are automatically joined"); 1.98.1 `thread::scope` docs: "If any of the automatically joined threads panicked, this function will panic." Verified: the async-concurrency.md ASY-10 `parallel_sum` shape (`workers.into_iter().try_fold(.., \|w\| w.join().map_err(..)?)`) panics with "a scoped thread panicked" when two workers panic; joining all handles first returns `Err(WorkerPanicked)`. | Un-joined scoped threads are joined at the end of the scope, and their panics resurface. | refine ASY-10 (existing example is wrong) | ASY-10 (MUST, addition): Inside `thread::scope`, join every `ScopedJoinHandle` before inspecting any result or returning early with `?`. A handle skipped by a short-circuit (`try_fold`, `?`, `find`) is auto-joined, and `scope` panics if that thread panicked. | Review. Fix the ASY-10 example (see Patterns: "Scoped workers, always joined"). | high | This is a latent panic in a rust-craft template example that agents copy. It fires exactly in the failure mode the example claims to handle. |
| 21 | ch1 [#threads](https://mara.nl/atomics/basics.html#threads) ("Returning from main will exit the entire program, even if other threads are still running"; truncated output) | Detached std threads die mid-work. | refine H-13 / ASY-04 | H-13/ASY-04 (MUST, clarify): The owned-work rule covers std threads, not only tokio tasks. Keep every `JoinHandle` and `join` it on the shutdown path, or use `thread::scope`. Dropping a `JoinHandle` detaches the thread, and `main` returning kills it mid-write. | Scanner soft: extend `detached-spawn` to `/^[ \t]*(?:std\s*::\s*)?thread\s*::\s*(?:spawn\|Builder\b[^;]*\.spawn)\s*\(/gm` (unbound statement). | med | The existing text and scanner name tokio only. |
| 22 | ch1 [#thread-safety](https://mara.nl/atomics/basics.html#thread-safety) (Send/Sync definitions; `PhantomData<Cell<()>>` opt-out; raw pointers are neither); ch5 [#blocking](https://mara.nl/atomics/building-channels.html#blocking) (`PhantomData<*const ()>` makes a type `!Send`); ch1 [#reader-writer-lock](https://mara.nl/atomics/basics.html#reader-writer-lock) (`Mutex<T>` needs `T: Send`; `RwLock<T>` also needs `Sync`). Verified on 1.98.1: `Cell`/`OnceCell` are Send and !Sync; `MutexGuard` is !Send and Sync; `Arc<Cell<_>>` is !Send; `Mutex<Cell<u32>>` is Sync while `RwLock<Cell<u32>>` is not; both markers compile under `forbid(unsafe_code)`. | Send and Sync follow the fields. You can opt out safely; opting in needs `unsafe impl`. | refine ASY-11 / OWN-12 | ASY-11 (SHOULD, addition): Reason about `Send`/`Sync` from the fields. `Cell`, `RefCell` and `OnceCell` are `Send` but not `Sync`: they may move into a thread but not be shared. `MutexGuard` is not `Send`. `Mutex<T>` is `Sync` when `T: Send`, while `RwLock<T>` also needs `T: Sync`. `Arc<T>` crosses threads only if `T: Send + Sync`. To make a type deliberately `!Send` or `!Sync`, add a `PhantomData<*const ()>` or `PhantomData<Cell<()>>` field and prove it with a `compile_fail` doctest (TST-08). A type that would need `unsafe impl Send/Sync` is redesigned (H-01). | Review; `clippy::arc_with_non_send_sync` (suspicious, on; verified flags `Arc::new(RefCell::new(..))`). | med | "!Sync" is often misread as "single-threaded". The Mutex/RwLock asymmetry explains why swapping a `Mutex` for an `RwLock` fails to compile. |
| 23 | ch6 [#mutation](https://mara.nl/atomics/building-arc.html#mutation) (`get_mut` only when count is 1; associated-function style `Arc::get_mut(&mut a)` "advisable for types that implement Deref"), [#weak-pointers](https://mara.nl/atomics/building-arc.html#weak-pointers). Verified stable: `Arc::into_inner` 1.70, `unwrap_or_clone` 1.76, `make_mut`/`get_mut`/`try_unwrap`/`downgrade` 1.4. | Exclusive access through `Arc` is conditional. `Weak` breaks cycles. Deref wrappers use associated functions. | refine OWN-11 / H-08 / API-19 | OWN-11 (SHOULD, addition): To take a value back out of an `Arc` after sharing ends, use `Arc::into_inner` (1.70), not `try_unwrap(..).unwrap()` or a clone. Use `Arc::make_mut` for copy-on-write. Use `Weak` for back-references only when an arena does not fit. API-19 (SHOULD, addition): Methods of a `Deref` wrapper are associated functions (`Wrapper::op(&w)`), so they never shadow the target's methods. | Review. | med | Agents clone out of `Arc` at the end of a thread fan-out (H-08). `into_inner` is the sanctioned exit. |
| 24 | ch1 [#reader-writer-lock](https://mara.nl/atomics/basics.html#reader-writer-lock) (writer starvation; implementation is OS-dependent); ch8 #windows-srw; ch9 [#avoiding-writer-starvation](https://mara.nl/atomics/building-locks.html#avoiding-writer-starvation). 1.98.1 docs: "The priority policy of the lock is dependent on the underlying operating system's implementation", with a recursive-read deadlock example. 1.98.1 source: futex `RwLock` (Linux, Windows 8+, BSDs) refuses new readers while writers wait; `queue.rs` (macOS and other Unix) is "writer-preferring ... unfair". | Writer starvation versus reader blocking is a policy choice, and std's choice is not guaranteed. | refine OWN-12 / PERF-12 | PERF-12 (SHOULD, addition): Default to `Mutex`. Use `RwLock` only when reads dominate, read sections are non-trivial, and a benchmark shows a gain. Do not depend on std's priority policy (OS-dependent and unguaranteed; writer-preferring on Linux, Windows and macOS in 1.98.1), which means recursive reads can deadlock (#12). Never upgrade read→write in place: drop the read guard, take `write()`, and re-check the condition. `RwLockWriteGuard::downgrade` (1.92) goes the other way. | Review; `clippy::readonly_write_lock` (perf, on) fired on `.write().unwrap()` but NOT on the rust-craft `.write().unwrap_or_else(PoisonError::into_inner)` idiom (verified), so review. | med | OWN-12's "writer starvation; measure first" is too thin, and the upgrade deadlock is common. |
| 25 | ch10 [#rcu](https://mara.nl/atomics/inspiration.html#rcu) (replace the whole value through an indirection; readers keep old copies until done) | Read-copy-update for read-mostly data. | pattern | Pattern "Snapshot swap": `RwLock<Arc<T>>` (or `Mutex<Arc<T>>`). Readers take `Arc::clone(&*lock.read()..)` in one statement and work lock-free on the snapshot. Writers build the new `T` outside the lock and replace the `Arc`. Poison cannot tear an `Arc` replacement. | Review; patterns.md. | med | Agents hold `config.read()` across a whole request. This safe std-only RCU removes that, with `arc-swap` as the H-16 upgrade. |
| 26 | ch1 [#lock-poisoning](https://mara.nl/atomics/basics.html#lock-poisoning) ("recovering ... is not often done ... most code either disregards poison or uses unwrap()"). Verified 1.98.1: `Mutex::clear_poison` stable 1.77; `RwLock` "may only be poisoned if a panic occurs while it is locked exclusively"; `LazyLock` poisoning is "_unrecoverable_" (every later access panics); `OnceLock` leaves the cell uninitialized when `f` panics; `Condvar::wait*` and `Mutex`/`RwLock` `into_inner`/`get_mut` return `LockResult`; `std::sync::nonpoison` is unstable (`sync_nonpoison`); `clippy::map_err_ignore` (on) fires on `.map_err(\|_\| Poisoned)` but not `\|_poisoned\|`. | Poisoning flags possibly torn data, and the `Err` still carries the guard. | refine H-02 (escape-hatches.md, errors.md) | escape-hatches.md/errors.md (MUST): Handle `PoisonError` by one of three documented choices. (a) Every critical section is a single all-or-nothing call: `unwrap_or_else(PoisonError::into_inner)` plus a comment. (b) The invariant can be re-checked: take `into_inner()`, validate, repair if needed, then `clear_poison()` (1.77); return an error if it cannot be repaired. (c) Otherwise, map to a unit variant through `impl<T> From<PoisonError<T>> for Poisoned`: the error cannot hold the guard, because it borrows the lock and is `!Send`, which fails ERR-01. Apply the same to `Condvar::wait*`, `Mutex::into_inner` and `get_mut`. A `LazyLock` initializer must not be able to panic, since its poison is permanent; use `LazyLock<Result<T, E>>`. | Review; `map_err_ignore` (on) forces named placeholders. | high | The current guidance is correct but lacks the repair path (`clear_poison`) and the `LazyLock` exception, and never says where the variant comes from. The brief asked specifically. |
| 27 | Same verified facts as #3, #12, #14, #19, #20, #26 | Collected panicking std concurrency APIs. | pattern (escape-hatches.md "Panicking APIs" rows) | Add rows. `thread::spawn` → `Builder::spawn`. `Scope::spawn` → `Builder::spawn_scoped`. `Builder::spawn*` with a NUL in the name → literal or validated names. `thread::scope` with an un-joined panicked worker → join all first. `RefCell::borrow` (not just `borrow_mut`) → `try_borrow`. `LocalKey::with`/`with_borrow`/`with_borrow_mut` (panic during TLS destruction or when already borrowed) → `try_with`, or no re-entry. `Condvar::wait*` with two mutexes → one Condvar per Mutex. Re-locking a held `Mutex`/`RwLock`, or re-entrant `OnceLock` init → #12. `LazyLock` after a panicking init → a `Result` payload. Atomic ops with a runtime `Ordering` → constants. | Review (the table is the reviewer's checklist for H-03 beyond the macros). | high | The table's own premise: "a green gate is not proof of panic-freedom". Each row is verified against the 1.98.1 docs or at runtime. |
| 28 | ch7 [#performance-experiment](https://mara.nl/atomics/hardware.html#performance-experiment) (false sharing: 3 adjacent `AtomicU64`s about 10x slower, fixed by `#[repr(align(64))]`; "you might need to use 128-byte alignment"; "a dense array of small mutexes might not always perform as well"). crossbeam-utils 0.8.23 `CachePadded` uses 128 on x86_64/aarch64 (Intel adjacent-line prefetch; big.LITTLE big cores) and 64 on most others. Verified: a `#[repr(align(128))]` newtype compiles clean under `forbid(unsafe_code)` and the template lints, with `const _: () = assert!(align_of::<..>() == 128)`. | Pad independently written hot atomics to separate cache lines. Keep data accessed together on one line. | new PERF (PERF-13) | PERF-13 (SHOULD): When a benchmark shows contention (PERF-01) on independent per-thread or per-shard atomics or small locks stored contiguously (`Vec`/array/adjacent fields), wrap each in a local `#[repr(align(128))]` newtype (64 is enough on targets without adjacent-line prefetch) and assert its alignment at compile time. Do not pad without a benchmark, and do not split fields that are always accessed together. | Review; TST-16 benchmark. | med | This is safe code with a real 2-10x effect on sharded counters. PERF-12's sharding advice (`Vec<Mutex<HashMap>>`) currently invites false sharing. |
| 29 | ch3 #common-misconceptions ("Relaxed operations are free" is a myth under sharing); ch7 #performance-experiment (a store-heavy thread slows other cores' loads) | Relaxed is only cheap when uncontended, because one atomic hammered by all cores ping-pongs its cache line. | refine PERF-12 | PERF-12 (SHOULD, amend "use atomics for single counters"): A single atomic written by every worker is itself a contention point. For hot counters, keep per-thread or per-shard counts (padded per #28) and sum on read, with a benchmark. | Review. | low | It corrects the implication that atomics are the contention fix. |
| 30 | ch7 [#reordering-experiment](https://mara.nl/atomics/hardware.html#reordering-experiment) (all-`Relaxed` spin lock passes on x86-64, fails on Apple M1 about 0.4% of the time); ch6 [#miri](https://mara.nl/atomics/building-arc.html#miri); ch9 condvar test ("ideally run on a computer with a weakly ordered processor architecture"). Verified with nightly Miri: a safe-code `Relaxed` data-plus-flag publish test FAILS under Miri at the default seed (reader saw flag=true, data=0), the `Release`/`Acquire` version passes, a double `lock()` is reported as "the evaluated program deadlocked", and a two-thread lock-order inversion was NOT found in 64 seeds (`-Zmiri-many-seeds=0..64`). | x86-64 hides ordering bugs. Test under an interpreter with weak-memory emulation, or on ARM. | new TST (TST-17) + CI | TST-17 (MUST for crates with a non-`Relaxed` ordering, a `Condvar` or hand-written thread coordination): CI runs `cargo +nightly miri test` for those crates (optionally `MIRIFLAGS=-Zmiri-many-seeds=0..16`), and runs the test suite on an aarch64 runner (GitHub `ubuntu-24.04-arm`). Passing on x86-64 alone is not evidence that the orderings are correct. | CI template: add a `miri` job (nightly toolchain plus the `miri` component, scoped with `-p`) and an `arm` job. A CI change needs human approval (H-05/H-16). | high | Relaxed-versus-Acquire mistakes are invisible in the current gate, and Miri caught one in safe code. The rule also records Miri's limits. |
| 31 | ch6 #miri; loom is not in the book (brief topic). Verified: `#[cfg(loom)]` triggers `unexpected_cfgs` warnings on 1.98.1, which fail the zero-warnings gate, and `[lints.rust] unexpected_cfgs = { level = "warn", check-cfg = ["cfg(loom)"] }` silences them. loom 0.7.2 README: `[target.'cfg(loom)'.dependencies]`, `RUSTFLAGS="--cfg loom"`. loom is not in `preapprovedDependencies`. | Model checking of every interleaving. | tooling | loom (MUST ask): adopting loom is an H-16 dependency decision and needs three things: `[target.'cfg(loom)'.dev-dependencies] loom`, `std::sync` imports swapped behind `cfg(loom)`, and `check-cfg = ["cfg(loom)"]` in `[workspace.lints.rust]` (a lint-config edit, H-05). Recommend it only for crates whose correctness rests on orderings (custom lock-free structures). Application code relies on std primitives (#18) plus Miri (#30). | H-16 plus template `[workspace.lints.rust]` when adopted. | med | Without the check-cfg note, the first `cfg(loom)` breaks the gate, and an agent would be tempted into a forbidden `#[allow]`. |
| 32 | ch9 [#condition-variable](https://mara.nl/atomics/building-locks.html#condition-variable) test (`assert!(wakeups < 10)` proves the wait blocked rather than spun) | Test that waiting code actually waits. | refine TST-10 | TST-10 (SHOULD, addition): Tests for blocking or coordination code sequence threads with `Barrier`, channels or `JoinHandle::join` (never sleeps), and assert that the wait did not degrade into a spin (count wakeups or predicate checks). | Review. | low | A useful test shape, but rare. |
| 33 | Not in the book (brief topic: thread-locals). Verified: `thread_local!` with `const { .. }` init and `LocalKey::with_borrow_mut`/`set`/`get` (stable 1.73) are lint-clean. A non-const init is denied by `clippy::missing_const_for_thread_local` (perf, on). | Thread-local state. | covered (lint) + refine ARC-09 | ARC-09 (note): A `thread_local!` holding domain state is global mutable state, per thread. Allow it only for caches and scratch buffers, with `const` init and access through `with_borrow_mut` in a single call that never re-enters the same key (it panics when already borrowed; #27). | `missing_const_for_thread_local` (on). | low | For completeness. The existing lints cover most of it. |
| 34 | ch1 [#mutexes-in-other-languages](https://mara.nl/atomics/basics.html#mutexes-in-other-languages) ("Mutex<()> ... you are probably better off defining a (possibly zero-sized) type to interface with that hardware and wrapping that in a Mutex") | The mutex should own what it protects. | refine API-02 / OWN-12 | OWN-12 (SHOULD): Never use `Mutex<()>` as a free-standing lock beside the data it "protects". Put the protected resource (a handle, possibly zero-sized) inside the `Mutex`, so access requires the guard. | Scanner soft `/\bMutex\s*<\s*\(\s*\)\s*>/g` (prod: soft). | low | This is C-style locking, which agents port from other languages. |
| 35 | ch5 [#borrowing-to-avoid-allocation](https://mara.nl/atomics/building-channels.html#borrowing-to-avoid-allocation) (`split(&mut self) -> (Sender<'_, T>, Receiver<'_, T>)` turns one exclusive borrow into two role handles) | Split an exclusive borrow into typed role handles to enforce "one sender, one receiver" without `Arc`. | pattern | Pattern "Split borrow into role handles": `fn split(&mut self) -> (Producer<'_>, Consumer<'_>)` hands out role-restricted views, and the borrow checker forbids a second split while they live. Use it with `thread::scope` (OWN-11 rung "scope borrow"). | patterns.md. | low | A nice API-13-adjacent shape. Low frequency. |
| 36 | ch1 #thread-safety; verified `clippy::arc_with_non_send_sync` (suspicious, on) | ownership.md says "`Arc<RefCell<T>>` does not compile across threads". It does compile until sent, and clippy flags construction. | refine OWN-11 (wording) | OWN-11: Replace the sentence with "`Arc<RefCell<T>>` is flagged by `clippy::arc_with_non_send_sync` and cannot be sent to another thread; use `Rc` for single-threaded sharing or `Mutex` for cross-thread sharing." | Lint already on. | low | An accuracy fix, and it names the enforcement. |
| 37 | Consolidated scanner additions for #1, #3, #4, #6, #19, #21 | Cheap regex advisories that route atomics code to a reviewer. | tooling | Add to `scan.mjs` RULES: `seqcst` (ASY-13, `/\bSeqCst\b/g`, prod/bin soft); `ordering-param` (ASY-13, the regex in #3, prod soft); `compiler-fence` (ASY-13, `/\bcompiler_fence\s*\(/g`, prod hard); `fetch-update` (MOD-07, `/\.\s*fetch_update\s*\(/g`, all soft); `thread-spawn` (ASY-15, `/\bthread\s*::\s*spawn\s*\(/g`, prod/bin hard, test null); extend `detached-spawn` to std threads (#21). | Scanner (`scripts/lib/scan.mjs`). | med | The scanner already has `test`/`prod` contexts, which avoids the clippy.toml problem of also firing in tests (#19). |

Defects in current rust-craft text, found and verified during this pass:
1. async-concurrency.md ASY-10 `parallel_sum`: `try_fold` over `join()` short-circuits, so `thread::scope` panics when two or more workers panic (#20). It also uses `Scope::spawn`, which panics on OS failure (#19).
2. async-concurrency.md atomics table: "Unsure → `SeqCst`" and "Every non-`SeqCst` ordering gets a comment" invert the book's guidance (#1).
3. ownership.md `spawn_sum`, `spawn_counter` and `parallel_sum` use `thread::spawn`/`Scope::spawn`, the panicking forms (#19).
4. modern-rust.md `cached_or_fill` trips `significant_drop_in_scrutinee` once that lint is enabled (#10).
5. ownership.md OWN-11: "`Arc<RefCell<T>>` does not compile across threads" is imprecise (#36).
6. MOD-07's modern-API list lacks `try_update`/`update` (1.95), and nothing warns that `fetch_update` is deprecated from 1.99 (#6).

## Conflicts

- ch5 [#safety-through-runtime-checks](https://mara.nl/atomics/building-channels.html#safety-through-runtime-checks), summary: "Panicking is an important tool for creating a safe interface" (`panic!("can't send more than one message!")`). Conflicts with H-03: rust-craft forbids panics in production code, while the book uses them as the safe alternative to UB. Compatible nugget: the book's own next step (§Safety Through Types) moves the precondition into the type (consuming `self`), and where a runtime check stays, it returns `Result` (for example `TryRecvError`-style variants).
- ch2 [#example-id-allocation](https://mara.nl/atomics/atomics.html#example-id-allocation) and ch6 `Arc::clone`: `assert!(id < 1000)` and `std::process::abort()` on counter overflow. Conflicts with H-03 (`process::abort` is a disallowed method in the template). Compatible nugget: #5, check before modifying via `try_update(.., checked_add)` and return a typed error.
- ch1 throughout: `lock().unwrap()`, `join().unwrap()`, and "most code either disregards poison or uses `unwrap()`". Conflicts with H-02. Compatible nugget: the three-way poison decision (#26).
- ch1 [#leaking](https://mara.nl/atomics/basics.html#leaking) (`Box::leak` to share with threads) and ch3 examples with `static mut`. Conflicts with H-07/H-01. Compatible nugget: none new, since scoped threads and `Arc` are already OWN-07/OWN-11.
- ch3-6 and ch9 build primitives with `UnsafeCell`, `MaybeUninit`, `NonNull` and `unsafe impl Send/Sync`. Conflicts with H-01. Compatible nuggets: guard types tie release to `Drop` (#25 area and API-19); consuming `self` enforces "once"; `PhantomData` markers opt out of auto traits (#22); `split(&mut self)` yields role handles (#35).

## Patterns / examples worth copying

All sketches are excerpts from `scratchpad/atomics/ws/crates/pats/src/lib.rs` (clippy-clean under the template lints plus `significant_drop_in_scrutinee`, 7 tests passing, and builds on 1.96).

**Memory-ordering cheat sheet** (proposed replacement for the async-concurrency.md "Atomics and memory ordering" table; ASY-13):

| Situation | Ordering | Required comment |
|---|---|---|
| Counter, statistic, id, gauge; nothing else is read or written because of its value | `Relaxed` | "Relaxed: statistic; no other memory depends on it" |
| Flag or counter that tells another thread "the data I wrote before this is ready" | writer `store(.., Release)`, reader `load(Acquire)` | each side names the other (`// pairs with Acquire in Summary::read`) |
| RMW that claims (take a slot or lock) | `Acquire` | names the `Release` that frees it |
| RMW that frees or publishes (unlock, "done" decrement) | `Release` (plus `fence(Acquire)` on the zero path if the zero-observer then reads) | names the partner |
| RMW that both consumes and publishes (hand-off swap) | `AcqRel` | names both partners |
| CAS failure ordering | `Relaxed`, or `Acquire` if the failed value is used to read published data | why |
| Needs one global order across two or more atomics (store own flag, then load the other's) | `SeqCst` | the cross-variable property. Otherwise use a `Mutex`. |
| Unsure | not an ordering: use `Mutex`, `OnceLock`, a channel or `Barrier` | none |

**Review checklist for atomics** (append to async-concurrency.md):
- [ ] Is an atomic the right tool: one independent value, not two values that must agree? (ASY-13; otherwise a `Mutex`)
- [ ] Does every `Relaxed` have a one-line reason, and does every `Release`/`Acquire`/`AcqRel` name its pairing operation? Does every `Release` have an `Acquire` partner, and vice versa? (ASY-13)
- [ ] Does every `SeqCst` name its total-order property, or has it been downgraded? (ASY-13)
- [ ] Are orderings literal constants, never `Ordering` parameters? (ASY-13, H-03)
- [ ] Are dedicated RMWs used, with `update`/`try_update` instead of CAS loops, `_weak` in any remaining loop, and no `fetch_update`? (MOD-07, ASY-13)
- [ ] Is overflow decided for every `fetch_add`/`fetch_sub`? (ERR-09)
- [ ] Is ABA ruled out for every CAS on a recyclable value? (ASY-13)
- [ ] Are there no spin or busy-wait loops, and no hand-built locks or once-cells? (ASY-16, ASY-17)
- [ ] Is the crate covered by the Miri and aarch64 CI jobs? (TST-17)

**ID allocation without wrap-around** (#5, #6):
```rust
pub fn allocate(&self) -> Result<u32, IdsExhausted> {
    // Relaxed: only uniqueness matters, and the total modification order of
    // `next` gives that; no other memory is published through it.
    self.next
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_exhausted_at| IdsExhausted)
}
```

**Release/Acquire publication with a pairing comment** (#1, #2):
```rust
pub fn finish(&self, total: u64, max: u64) {
    self.total.store(total, Ordering::Relaxed);
    self.max.store(max, Ordering::Relaxed);
    // Release pairs with the Acquire in `read`: both stores above become
    // visible to any thread that observes `done == true`.
    self.done.store(true, Ordering::Release);
}
pub fn read(&self) -> Option<(u64, u64)> {
    if !self.done.load(Ordering::Acquire) { return None; } // pairs with Release in `finish`
    Some((self.total.load(Ordering::Relaxed), self.max.load(Ordering::Relaxed)))
}
```
(Prefer `OnceLock<Summary>` when the payload is not already atomics.)

**Guard out of the scrutinee** (#9):
```rust
// Don't: the guard lives through the body, even in edition 2024.
if let Some(job) = self.queue.lock().unwrap_or_else(PoisonError::into_inner).pop_front() {
    run(job); // runs with the lock held
}
// Do: the guard drops at the `;`.
let next = self.queue.lock().unwrap_or_else(PoisonError::into_inner).pop_front();
if let Some(job) = next { run(job); }
```

**Lock once, helpers take `&mut Inner`** (#12):
```rust
fn lock(&self) -> MutexGuard<'_, Inner> {
    // Every mutation is a single `entry` update, so poison cannot tear the map.
    self.inner.lock().unwrap_or_else(PoisonError::into_inner)
}
pub fn credit_pair(&self, a: u32, b: u32, amount: u64) {
    let mut inner = self.lock();
    inner.credit(a, amount); // `Inner::credit(&mut self)`: no second lock
    inner.credit(b, amount);
}
```

**Lock ordering by a stable key** (#13):
```rust
if from.id == to.id { return Err(TransferError::SameAccount(from.id)); } // same Mutex twice = deadlock
let (first, second) = if from.id < to.id { (from, to) } else { (to, from) };
let mut first_bal = first.balance.lock().unwrap_or_else(PoisonError::into_inner);
let mut second_bal = second.balance.lock().unwrap_or_else(PoisonError::into_inner);
```

**Condvar with a predicate and a bound** (#14, #15; lint-clean shape):
```rust
let (mut items, _timed_out) = self
    .not_empty
    .wait_timeout_while(
        self.items.lock().unwrap_or_else(PoisonError::into_inner),
        timeout,
        |items| items.is_empty(), // re-checked after every (spurious) wakeup
    )
    .unwrap_or_else(PoisonError::into_inner);
let next = items.pop();
drop(items);
next
```

**Poison: re-validate, repair, clear** (#26):
```rust
let mut pair = match self.pair.lock() {
    Ok(guard) => guard,
    Err(poisoned) => {
        let guard = poisoned.into_inner();
        if guard.left.checked_add(guard.right) != Some(guard.total) {
            return Err(Torn); // cannot repair: report, do not trust
        }
        self.pair.clear_poison(); // invariant verified (1.77)
        guard
    }
};
```

**Scoped workers: named, fallible, always joined** (#19, #20; replacement for the ASY-10 example):
```rust
thread::scope(|scope| {
    let mut workers = Vec::with_capacity(threads.get());
    for (i, chunk) in values.chunks(chunk_len).enumerate() {
        let worker = thread::Builder::new()
            .name(format!("sum-{i}"))
            .spawn_scoped(scope, move || chunk.iter().fold(0_u64, |a, v| a.saturating_add(*v)))
            .map_err(ParallelError::Spawn)?;
        workers.push(worker);
    }
    // Join every worker before inspecting any result: an un-joined handle makes
    // `thread::scope` panic if that worker panicked.
    let parts: Vec<_> = workers.into_iter().map(thread::ScopedJoinHandle::join).collect();
    parts.into_iter().try_fold(0_u64, |total, part| {
        Ok(total.saturating_add(part.map_err(|_payload| ParallelError::WorkerPanicked)?))
    })
})
```
(The early `?` on a spawn error is safe: the handles already spawned have not panicked, since they run infallible folds. If they could panic, collect spawn results first as well.)

**Snapshot swap, a safe RCU** (#25):
```rust
pub fn snapshot(&self) -> Arc<Config> {
    Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner)) // lock ends at `;`
}
pub fn replace(&self, config: Config) {
    let next = Arc::new(config); // build outside the lock
    *self.current.write().unwrap_or_else(PoisonError::into_inner) = next;
}
```

**Cache-line padding in safe code** (#28):
```rust
/// One cache line per value; 128 covers x86-64 adjacent-line prefetch and aarch64 big cores.
#[derive(Debug, Default)]
#[repr(align(128))]
pub struct CachePadded<T>(pub T);
const _: () = assert!(align_of::<CachePadded<AtomicU64>>() == 128);
// shards: Box<[CachePadded<AtomicU64>]>, one per worker; sum with Relaxed loads after join.
```

**Deliberately `!Send` without unsafe** (#22):
```rust
pub struct ThreadBound { _not_send: PhantomData<*const ()> }  // !Send + !Sync
pub struct NotShared { _not_sync: PhantomData<Cell<()>> }     // Send, !Sync
```

**Miri job sketch** (#30; a CI change needs human approval):
```yaml
miri:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@nightly
      with: { components: miri }
    - run: cargo miri test -p <crates-with-atomics>
      env: { MIRIFLAGS: "-Zmiri-many-seeds=0..16" }
```

## Notable skips

- ch3 §Consume Ordering: Rust does not expose `Consume`; nothing to adopt.
- ch3 §Out-of-Thin-Air Values: a theory-only anomaly, "not something you need to take into account in practice".
- ch3 §Compiler Fences, `membarrier`: signal handlers and process-wide barriers need unsafe or FFI (H-01). Only the "never in app code" nugget is kept (#4).
- ch4-6 and ch9 implementation mechanics (`UnsafeCell`, `MaybeUninit`, `NonNull`, `ManuallyDrop`, `unsafe impl Sync`, 3-state futex mutex, requeue): unsafe-only (H-01). Their design lessons are captured in #12-#22 and #35.
- ch7 assembly, LL/SC and MESI details: explanatory. Only the actionable consequences are kept (#6, #17, #28, #30).
- ch8 syscalls, futex, `libc`, `windows-sys`, pthread movability: FFI and unsafe (H-01, H-16).
- ch9 `atomic-wait` and `lock_api` crates: only relevant for building primitives (#18 says don't); dependencies (H-16).
- ch10 lock-free linked list, queue-based locks, parking-lot locks, sequence locks: unsafe or dependency territory. Semaphores exist as `tokio::sync::Semaphore` (PERF-10). Only RCU becomes a safe pattern (#25).
- Unstable in 1.98.1, so not proposed: `OnceLock::get_or_try_init`/`try_insert`, `Mutex::get_cloned`/`set`/`replace` (`lock_value_accessors`), `std::sync::nonpoison`, `ReentrantLock`, `std::sync::mpmc`, `std::sync::oneshot`, `MappedMutexGuard`, `thread::sleep_until`.

Sources for CI runner availability: [arm64 hosted runners GA for public repos (2025-08-07)](https://github.blog/changelog/2025-08-07-arm64-hosted-runners-for-public-repositories-are-now-generally-available/), [arm64 standard runners in private repos (2026-01-29)](https://github.blog/changelog/2026-01-29-arm64-standard-runners-are-now-available-in-private-repositories/).
