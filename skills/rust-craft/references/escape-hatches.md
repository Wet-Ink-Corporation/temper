# Escape hatches

An escape hatch is code that tells the compiler to stop checking something: that a value is present, an index is in bounds, a cast fits, a lint doesn't apply, or a lifetime is long enough. Each one moves a guarantee from compile time to hope. This file catalogues them, gives the correct shape for each, and defines the one legitimate route for a real exception.

Read it when a lint or a temper hook refuses code, or when you are tempted to write any construct in the catalogue.

Rules covered: H-01, H-03, H-04, H-05, H-06 and H-11 in detail; H-02 with errors.md; H-07 to H-10 and H-12 to H-14 in short, with pointers to ownership.md, errors.md, testing.md and async-concurrency.md for the full treatments. H-15 and H-16 are in process.md.

## How enforcement works (so refusals make sense)

| Layer | When | What it does |
|---|---|---|
| Workspace lints (`Cargo.toml`, `clippy.toml`) | `cargo clippy`, CI | Denies the hatches outright: `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `as_conversions`, `allow_attributes`, `disallowed_methods` for a few panicking std calls, and the rest (see templates). |
| temper pre-edit hook | Before a `Write`/`Edit` lands | Scans the edit. New hard hits are refused with the rule ID and fix. An `#[expect(..., reason)]` turns a hit into a question for the human. Removed tests, lint config edits and new dependencies also go to the human. |
| temper Bash hook | Before a shell command | Refuses writing `.rs` files through the shell, `--no-verify`, and lowering lint levels on the command line. |
| temper gate | `Stop`, the gate command, the reviewer | Runs fmt, clippy `-D warnings`, tests and rustdoc, and scans every changed file against the base revision. |
| Reviewer | Before done | Judges what tools cannot: whether a clone is justified, whether an `Arc<Mutex<_>>` is necessary, whether an exception's reason holds, whether a std call can panic on this input. |

A refusal is information about the design, not an obstacle to route around. **Routing around** is itself an H-12 violation. That covers writing the file through the shell, `Option::unwrap` as a function path, `.unwrap_or_else(|_| panic!())`, `match x { Some(v) => v, None => unreachable!() }`, `std::process::abort()`, and splitting code to hide it from the scanner. Each of these is caught, and each wastes a review round.

## Catalogue

| Hatch | Rule | Lint / check | Why it's tempting | Correct shape |
|---|---|---|---|---|
| `unsafe` | H-01 | `unsafe_code = "forbid"` | Speed, FFI, "I know it's fine" | Safe std APIs; restructure; FFI only through the H-01 exception checklist |
| `unwrap()`, `expect()` | H-02 | `unwrap_used`, `expect_used`, scan | Short; "can't fail" | `?`, `ok_or_else`, `let ... else`, or a type that makes the case impossible |
| `panic!`, `todo!`, `unimplemented!`, `unreachable!`, `abort()` | H-03 | `panic`, `todo`, `unimplemented`, `unreachable`, disallowed `process::abort` | Placeholders; "impossible" branches | Return an error; exhaustive enums; `Status: WIP` for unfinished work |
| std calls that panic on bad input (`a / b`, `.sum()`, `thread::spawn`, `env::args`, `Duration::from_secs_f64`) | H-03 | `disallowed_methods` for some, review for the rest | "It's std, so it's safe" | The fallible form from the table under H-03 |
| `v[i]`, `&s[a..b]`, `map[&k]` | H-04 | `indexing_slicing`, `string_slice`; scan advisory for maps | Terse | `get`, iterators, slice patterns, `split_first`, `as_chunks`, `char_indices`, `entry` |
| `#[allow(...)]` | H-05 | `allow_attributes`, scan | Silence the noise | Fix the code, or make an exception request (below) |
| `dbg!`, `println!` | H-06 | `dbg_macro`, `print_stdout`, `print_stderr` | Quick visibility | `tracing` events; `writeln!` on a locked handle in binaries |
| `Box::leak`, `mem::forget`, `transmute`, `static mut` | H-07 | `disallowed_methods`, `mem_forget`, `unsafe_code` | Get a `'static`; skip `Drop` | Move ownership; `thread::scope`; `Arc`; RAII |
| `'static` bound added to compile | H-07, OWN-07 | review | Makes E0597 or E0521 go away | Fix who owns the data (ownership.md, "Fighting the borrow checker") |
| `.clone()` to satisfy borrowck | H-08 | `redundant_clone`, scan advisory, review | Makes E0502 or E0505 go away | Split borrows, shorten scopes, `mem::take`, restructure |
| `Result<T, String>`, `Box<dyn Error>` in a lib API | H-09 | scan, review | No need to design an error type | `thiserror` enum with context fields |
| `let _ = f();`, `.ok();`, `Err(_) => {}` | H-10 | `let_underscore_must_use`, `unused_result_ok`, scan | Ignore an annoying `Result` | Handle, propagate, or document why it is irrelevant |
| `x as u32`, `Kind::A as u8` | H-11 | `as_conversions`, `cast_*` | Short | `From`, `TryFrom`, `checked_*`, `saturating_*`, a `match` in `From` for enums |
| `#[ignore]`, deleted assert, loosened expectation | H-12 | test-removal hook, review | Get green | Fix the code, or ask whether the test is wrong |
| `unbounded_channel()`, detached `spawn` | H-13 | scan advisory, review | Simpler | Bounded channel; `JoinSet` or `TaskTracker` |
| `std::thread::sleep`, or a lock guard, entered span or `watch::Ref` held across `.await` | H-14 | `await_holding_lock`, `await_holding_invalid_type`, review | It works in tests | `tokio::time::sleep`; scope the guard before `.await`; `.instrument(span)` |

## Rules

### H-01 · No `unsafe` code anywhere

**Why.** `unsafe` switches off the checks that make Rust worth using. One wrong block is undefined behaviour anywhere in the program, and nothing reports it until production does. Safe std covers the usual reasons to reach for it (bounds-check removal, byte reinterpretation, unchecked UTF-8), and the speed it promises is rarely measured.

**Do.** Use the safe API that tells the compiler the same fact:

```rust
/// Decodes little-endian `u32` words and returns any trailing partial word.
#[must_use]
pub fn words(bytes: &[u8]) -> (Vec<u32>, &[u8]) {
    let (whole, tail) = bytes.as_chunks::<4>();
    (whole.iter().map(|word| u32::from_le_bytes(*word)).collect(), tail)
}

/// Validates UTF-8 once instead of trusting it.
///
/// # Errors
/// Returns where the first invalid byte sequence starts.
pub fn text(bytes: &[u8]) -> Result<&str, std::str::Utf8Error> {
    std::str::from_utf8(bytes)
}
```

**Don't.**

```rust,ignore,expect=unsafe_code+scan:unsafe
fn word(bytes: [u8; 4]) -> u32 {
    unsafe { std::mem::transmute::<[u8; 4], u32>(bytes) } // byte order depends on the target
}
fn nth(values: &[u64], i: usize) -> u64 {
    unsafe { *values.get_unchecked(i) } // out of bounds is undefined behaviour, not a panic
}
```

**Detect.** `unsafe_code = "forbid"`, which no `#[expect]` or `#[allow]` inside the crate can lower, and the scanner.

**Exceptions.** None an agent can grant. FFI, or a measured need that safe code cannot meet, is an H-16 decision, and relaxing `forbid` for a crate is a lint-configuration change (H-05). A human approval covers only this shape:
- Raw bindings live in a `<lib>-sys` crate, generated by `bindgen` and regenerated and diffed in CI. One wrapper crate exposes a safe API over them. Only these two crates contain `unsafe`; every other crate keeps `forbid`.
- The human lists both crates in `.claude/temper.json` under `"manifests": { "ownLints": ["crates/<name>"] }`, the human-approved list of members that carry their own full `[lints]` table, which the gate and hooks honour. Each table copies the workspace lints without `unsafe_code = "forbid"` and adds `clippy::undocumented_unsafe_blocks`, because Cargo cannot combine `workspace = true` with overrides.
- Each `unsafe` block is minimal and carries a `// SAFETY:` comment naming the invariant that makes it sound. `unsafe_op_in_unsafe_fn` already warns in edition 2024, and `clippy::missing_safety_doc` requires a `# Safety` section on every `unsafe fn`.
- Memory is freed on the side that allocated it, through a `Drop` wrapper.
- The wrapper's tests exercise every `unsafe` block and run under Miri in CI with `-Zmiri-many-seeds`. Miri cannot execute FFI calls, so those tests carry `#[cfg_attr(miri, ignore = "calls C through FFI")]` and get a separate review. Miri finds undefined behaviour only on the paths the tests take, so a clean run does not prove soundness.
- The crate charter's decisions log records the approval.

### Unwrap and expect (H-02)

errors.md, "Eliminating `unwrap` and `expect`", holds the H-02 section. This is the short form.

**Why.** `unwrap` turns a value the type system says may be absent into a crash with no context, and it makes absence invisible at the call site. Every production `unwrap` is an unhandled case.

**Do.** Propagate, convert, or make absence impossible:

```rust
/// A registered user.
#[derive(Debug)]
pub struct User {
    /// Unique id.
    pub id: u64,
    /// Name shown in the UI.
    pub name: String,
}

/// Errors from the lookups below.
#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    /// No user has this id.
    #[error("user {id} not found")]
    NotFound {
        /// The id that was looked up.
        id: u64,
    },
    /// A `key=value` line had no `=`.
    #[error("missing '=' in line {line:?}")]
    MissingEquals {
        /// The offending line.
        line: String,
    },
}

/// Returns the display name of the user with `id`.
///
/// # Errors
/// Returns [`LookupError::NotFound`] when no user has this id.
pub fn display_name(users: &[User], id: u64) -> Result<&str, LookupError> {
    users
        .iter()
        .find(|u| u.id == id)
        .map(|u| u.name.as_str())
        .ok_or(LookupError::NotFound { id })
}

/// Splits a `key=value` line.
///
/// # Errors
/// Returns [`LookupError::MissingEquals`] when the line has no `=`.
pub fn parse_pair(line: &str) -> Result<(&str, &str), LookupError> {
    let Some((key, value)) = line.split_once('=') else {
        return Err(LookupError::MissingEquals { line: line.to_owned() });
    };
    Ok((key.trim(), value.trim()))
}
```

**Don't.**

```rust,ignore,expect=clippy::unwrap_used+clippy::expect_used+scan:unwrap
use std::collections::HashMap;

fn name(users: &HashMap<u64, String>, id: u64) -> &str {
    users.get(&id).unwrap() // crashes with no context
}
fn port(env_port: &str) -> u16 {
    env_port.parse().expect("port") // configuration errors become panics
}
fn first(items: &[u8]) -> u8 {
    *items.first().unwrap() // "items is never empty" is a hope, not a type
}
```

**Detect.** `clippy::unwrap_used`, `clippy::expect_used`, `clippy::unwrap_in_result`, and the scanner, which also catches `Option::unwrap` used as a function path.

"It can't fail" means the invariant belongs in a type. A non-empty list is a `(T, Vec<T>)` pair or a `NonEmpty<T>` newtype, a validated port is a `Port` newtype built by `TryFrom`, and a `Regex` compiled from a literal is built once in a `LazyLock` whose failure is a test failure. The errors.md section has a table of the common cases.

Some clippy suggestions end in `.unwrap()`: `unchecked_time_subtraction` proposes `a.checked_sub(b).unwrap()`. Keep the checked call and finish it yourself with `.ok_or(Error::..)?`, or with a documented `saturating_*`.

**Lock poisoning.** `lock()` returns `Err(PoisonError)` when another thread panicked while holding the guard, so the data may be half-updated. Choose one of three documented handlings, and apply it to `RwLock`, `Condvar::wait*`, `Mutex::into_inner` and `get_mut` too:
1. **Recover.** Every critical section is one call that completes or does not happen (`push`, `insert`): `.unwrap_or_else(PoisonError::into_inner)`, with a comment saying why the data cannot be torn (errors.md has the example).
2. **Repair.** The invariant can be re-checked: take the data with `PoisonError::into_inner`, validate or rebuild it, then call `Mutex::clear_poison` (1.77). Return an error when it cannot be repaired.
3. **Report.** Otherwise, map it to a unit error variant through `From`. The error cannot keep the `PoisonError`: its guard borrows the lock and is not `Send`, which breaks ERR-01.

```rust
use std::sync::{Mutex, PoisonError};

/// A writer panicked while holding the lock; the data may be half-updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("session table lock poisoned")]
pub struct Poisoned;

impl<T> From<PoisonError<T>> for Poisoned {
    // The guard inside borrows the lock and is not `Send`: the error keeps neither.
    fn from(_guard: PoisonError<T>) -> Self {
        Self
    }
}

/// Open session ids, shared by request threads.
#[derive(Debug, Default)]
pub struct Sessions {
    open: Mutex<Vec<u64>>,
}

impl Sessions {
    /// Returns how many sessions are open.
    ///
    /// # Errors
    /// Returns [`Poisoned`] if a writer panicked mid-update.
    pub fn count(&self) -> Result<usize, Poisoned> {
        Ok(self.open.lock()?.len())
    }
}
```

`LazyLock` poison is permanent: once its initializer panics, every later access panics. An initializer that can fail returns a `Result` (`LazyLock<Result<T, E>>`, errors.md H-02).

### H-03 · No panics in production code

**Why.** A panic in a library takes the decision away from the caller. In a server it kills a task, or with `panic = "abort"` the whole process. `todo!()` ships unfinished code that compiles.

**Do.** Make the impossible case unrepresentable, so there is no branch left to panic in:

```rust
use std::num::NonZeroU32;

/// Splits `total` into `parts` near-equal shares. Division by zero is impossible by type.
#[must_use]
pub fn share(total: u32, parts: NonZeroU32) -> u32 {
    total / parts
}

/// Traffic light phases. An exhaustive match needs no `unreachable!()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    /// Stop.
    Red,
    /// Prepare to stop.
    Amber,
    /// Go.
    Green,
}

impl Light {
    /// The phase that follows this one.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Red => Self::Green,
            Self::Green => Self::Amber,
            Self::Amber => Self::Red,
        }
    }
}
```

**Don't.**

```rust,ignore,expect=clippy::unreachable+clippy::todo+scan:panic
fn next(phase: u8) -> u8 {
    match phase { 0 => 2, 2 => 1, 1 => 0, _ => unreachable!() } // the type allows 253 other values
}
fn parse(input: &str) -> Vec<String> { todo!() }               // compiles, ships, crashes
```

**Detect.** `clippy::panic`, `todo`, `unimplemented`, `unreachable`, the disallowed `process::abort`, and `disallowed_methods` for the std calls marked *clippy.toml* in the table below. The scanner flags runtime `assert!` in production code for review.

Unfinished work is reported, not stubbed. End the turn with `Status: WIP` and list what remains.

#### Assertions

A runtime `assert!` is a panic with a condition. Two forms remain in production code:
- **Compile-time assertions**, `const _: () = assert!(..);` and `const { assert!(..) }`. They fail the build (E0080), never the program.
- **`debug_assert!`** for invariants the code itself establishes. Anything a caller or input can trigger returns an error instead. The condition has no side effects (`clippy::debug_assert_with_mut_call`), and every shipped profile keeps `debug-assertions = false` (PERF-09), or the check becomes a production panic.

Both carry a message naming the broken condition, with the values for runtime checks (`clippy::missing_assert_message`; tests are exempt).

```rust
/// Bytes per record on disk.
pub const RECORD_LEN: usize = 16;

// Fails the build, never the program.
const _: () = assert!(RECORD_LEN.is_multiple_of(4), "RECORD_LEN must be whole 32-bit words");

/// Splits `buf` into whole records and the unparsed tail.
#[must_use]
pub fn records(buf: &[u8]) -> (&[[u8; RECORD_LEN]], &[u8]) {
    let (whole, tail) = buf.as_chunks::<RECORD_LEN>();
    // Internal invariant of `as_chunks`, not caller input.
    debug_assert!(tail.len() < RECORD_LEN, "tail of {} bytes holds a whole record", tail.len());
    (whole, tail)
}
```

#### Panicking APIs (H-03 beyond the macros)

The lints catch panic *macros*. They do not catch std or crate functions that panic on bad input, so a green gate is not proof of panic-freedom. Before calling an API on data the code does not control, read its `# Panics` section (PROC-01); for `Vec` and `String` methods that is often the `[T]` or `str` page. Rows marked *clippy.toml* are denied by `disallowed_methods`; review catches the rest.

| Call | Panics when | Use instead |
|---|---|---|
| **Arithmetic** | | |
| `a + b`, `a - b`, `a * b`, `pow`, unary `-`, `abs`, `<<`, `>>` on integers | overflow, or a shift by the bit width or more; panics in debug, wraps in release | `checked_*`, `saturating_*`, `abs_diff`, or `wrapping_*` when wrapping is the intent (ERR-09) |
| `strict_*` | overflow, in every build: "strict" means "panic" | `checked_*` |
| `a / b`, `a % b` | `b == 0`, or signed `MIN / -1`, in release too | `checked_div`, `checked_rem`, or a `NonZeroU*` divisor (unsigned only; ERR-09) |
| `div_euclid`, `rem_euclid`, `div_ceil`, `next_multiple_of` | a zero divisor, or signed `MIN` by `-1` for the euclid pair, in release too | `checked_div_euclid`, `checked_rem_euclid`, `checked_next_multiple_of`; check the divisor before `div_ceil` |
| `ilog`, `ilog2`, `ilog10`; signed `isqrt` | an argument of zero or less, or an `ilog` base below 2; a negative `isqrt` input. In release too | `checked_ilog*`, `checked_isqrt` |
| `Iterator::sum`, `product` on integers | overflow; panics in debug, wraps in release | `copied().try_fold(0, u64::checked_add)`, or `fold(0, u64::saturating_add)` when a cap is the documented meaning (ERR-09) |
| `Ord::clamp`, `f64::clamp` | `min > max`, or a NaN bound | bounds validated once, into a type that guarantees `min <= max` |
| **Collections and slices** | | |
| `map[&k]` on `HashMap`/`BTreeMap`, `deque[i]` on `VecDeque` | the key or index is missing; no lint sees it | `get`, `get_mut`, `entry` (H-04) |
| `split_at`; `Vec::remove`, `insert`, `swap_remove`, `swap`, `drain`, `split_off`, `splice`, `extend_from_within` | an index or range out of bounds (`insert(len, x)` is valid) | `split_at_checked`; a `get` or length check first |
| `chunks(0)`, `chunks_exact(0)`, `rchunks(0)`, `windows(0)`, `as_chunks::<0>()`, `array_windows::<0>()`, `step_by(0)` | a zero size, at run time even for a literal | a non-zero constant, or a `NonZeroUsize` parameter; for a const generic `N`, `const { assert!(N > 0, "..") }` |
| `copy_from_slice`, `clone_from_slice` | the lengths differ | compare lengths first, or `iter_mut().zip(src)` |
| `rotate_left`, `rotate_right`, `select_nth_unstable` | `k > len`, `index >= len` | check the bound first |
| `sort`, `sort_by`, `sort_unstable_by` and the `_by_key` forms | the comparison is not a total order (documented, input-dependent) | `f64::total_cmp`; never `partial_cmp(..).unwrap_or(..)` |
| `BTreeMap::range`, `BTreeSet::range` | `start > end`, or equal bounds both `Excluded`, only when the map is non-empty, so empty test fixtures pass | check `from < to` first |
| `Vec::with_capacity`, `reserve` | a capacity over `isize::MAX` bytes | cap the length (PERF-10), then `try_reserve` |
| `HashMap::get_disjoint_mut` | two keys are equal | keys distinct by construction; the slice method of the same name returns a `Result` |
| **Strings** | | |
| `String::insert`, `insert_str`, `remove`, `split_off`, `drain`, `replace_range`; `str::split_at` | an index not on a `char` boundary, or past the end | `is_char_boundary`, `floor_char_boundary`, `ceil_char_boundary`, `split_at_checked`, `char_indices` |
| `String::truncate` | a new length inside a `char` (past the end is a no-op) | `floor_char_boundary` first |
| `char::to_digit`, `is_digit`, `uN::from_str_radix`; `char::from_digit` | a radix outside `2..=36`; above 36 for `from_digit`. Clippy misses even a literal | a named radix constant in `2..=36` |
| `String::from_utf8(..).unwrap()`, `str::from_utf8(..).unwrap()` | invalid UTF-8 (an H-02 shape) | `?` with a typed error, or `from_utf8_lossy` when lossiness is the documented meaning |
| **Time** | | |
| `Instant + Duration`, `Instant - Duration`, `SystemTime + Duration`, `SystemTime - Duration` | overflow, in release too | `checked_add`, `checked_sub` |
| `Instant - Instant`, `duration_since`, `elapsed` | never today: they saturate to zero, and std may bring the panic back | `saturating_duration_since` or `checked_duration_since`, to state the intent |
| `Duration` `+`, `-`, `*`, `/` | overflow, underflow or a zero divisor, in release too | `checked_*`, `saturating_*` |
| `Iterator::sum::<Duration>()` | overflow, in release too | `try_fold(Duration::ZERO, Duration::checked_add)` |
| `Duration::from_secs_f64`, `from_secs_f32`, `mul_f64`, `mul_f32`, `div_f64`, `div_f32` (*clippy.toml*) | a negative, NaN or overflowing result | `Duration::try_from_secs_f64`/`f32`; scale with `try_from_secs_f64(d.as_secs_f64() * k)` |
| `Duration::new`, `from_mins`, `from_hours` | overflow (`new` carries nanoseconds into seconds) | `Duration::from_secs(s).checked_add(Duration::from_nanos(n))`; `minutes.checked_mul(60).map(Duration::from_secs)` |
| **Threads and sync** | | |
| `thread::spawn`, `thread::Scope::spawn` (*clippy.toml*) | the OS cannot create a thread | `thread::Builder::new().name(..).spawn(..)` or `.spawn_scoped(scope, ..)`, which return `io::Result` (ASY-15) |
| `Builder::spawn`, `spawn_scoped` | the thread name contains a NUL byte | literal names, or names validated first |
| `thread::scope` | a thread you did not join panicked: the scope joins it and panics | join every handle before inspecting any result (ASY-10) |
| `RefCell::borrow`, `borrow_mut` | a conflicting borrow is live | `try_borrow`, `try_borrow_mut`; better, a design without `RefCell` (OWN-12) |
| `LocalKey::with`, `with_borrow`, `with_borrow_mut` | the key is being destroyed, or (cell forms) an outer call already borrows it | `try_with`; never re-enter a key from inside its own closure |
| `Mutex::lock`, `RwLock::read`, `write` on a lock this thread holds; re-entrant `OnceLock::get_or_init` | panics or deadlocks | lock once at the public boundary and pass `&mut Inner` down (ASY-14) |
| `lock().unwrap()`, `Condvar::wait(..).unwrap()` | the lock is poisoned | one of the three poison handlings (H-02, above) |
| `Condvar::wait` and its `_while` and `_timeout` forms | one `Condvar` is used with more than one `Mutex` | one `Condvar` per `Mutex`, in the same struct (ASY-16) |
| `LazyLock` access after its initializer panicked | always: `LazyLock` poison is permanent | an initializer that cannot panic; `LazyLock<Result<T, E>>` when it can fail |
| atomic `load`, `store`, `compare_exchange` with a runtime `Ordering` | an invalid combination, such as a `Release` load or an `AcqRel` failure ordering | literal `Ordering::X` constants at the call site (ASY-13) |
| **Env and process** | | |
| `env::args`, `env::vars` (*clippy.toml*) | an argument or variable is not valid Unicode (during iteration) | `args_os`, `vars_os`, then `into_string()` and report the error |
| `tracing_subscriber` `init()` | a global subscriber is already set | `try_init()`, and report the error |
| **Formatting** | | |
| `format!`, `.to_string()` | a `Display` impl returns an `Err` the formatter did not produce | `Display` returns `fmt::Error` only by propagating it from `write!`; validate in the constructor (API-03) |
| **tokio** | | |
| `tokio::spawn`, `Handle::current()`, `tokio::time::sleep` outside a runtime | no runtime on this thread | take a `Handle` and call `handle.spawn`, or `Handle::try_current()` (ASY-02) |
| `Runtime::block_on`, `Handle::block_on` inside a runtime | always | `.await` it; async code never blocks on async work (ASY-06) |
| `mpsc::channel(0)`, `broadcast::channel(0)`, `time::interval(Duration::ZERO)` | a zero capacity or period | capacities and periods validated at config parse (`NonZeroUsize`, a checked interval type) |

Reviewers treat a call to one of these, on input the code does not control, as a finding under H-03.

### H-04 · No panicking index or slice

**Why.** `v[i]` and `&s[a..b]` panic on out-of-bounds input. With strings they also panic when the index falls inside a UTF-8 character. `map[&k]` on a `HashMap` or `BTreeMap` and `deque[i]` on a `VecDeque` panic the same way. The compiler can prove bounds for you when you use the right shapes.

**Do.** `parse_pair` under H-02 splits a line with `split_once`, with no byte arithmetic. More shapes:

```rust
/// Sum of adjacent distances, with no indexing and no overflow.
#[must_use]
pub fn total_variation(samples: &[i64]) -> u64 {
    samples
        .array_windows::<2>()
        .map(|[a, b]| a.abs_diff(*b))
        .fold(0, u64::saturating_add)
}

/// Head and tail through a slice pattern.
#[must_use]
pub fn head_and_rest(items: &[u8]) -> Option<(u8, &[u8])> {
    match items {
        [first, rest @ ..] => Some((*first, rest)),
        [] => None,
    }
}

/// Dot product of the first `n` elements, saturating at `u64::MAX`.
/// Returns `None` if either slice is shorter than `n`.
#[must_use]
pub fn dot(a: &[u32], b: &[u32], n: usize) -> Option<u64> {
    // Slicing once up front lets the compiler drop the per-element bounds checks.
    let (Some(a), Some(b)) = (a.get(..n), b.get(..n)) else {
        return None;
    };
    // A `u32` times a `u32` always fits in a `u64`; only the running sum can overflow.
    let sum = a.iter().zip(b).fold(0_u64, |sum, (x, y)| {
        sum.saturating_add(u64::from(*x) * u64::from(*y))
    });
    Some(sum)
}
```

**Don't.**

```rust,ignore,expect=clippy::indexing_slicing+clippy::string_slice+scan:map-index
use std::collections::HashMap;

fn total_variation(v: &[i64]) -> i64 {
    let mut total = 0;
    for i in 0..v.len() - 1 { total += (v[i + 1] - v[i]).abs(); } // empty input underflows; i64 subtraction can overflow
    total
}
fn key(line: &str) -> &str {
    &line[..line.find('=').unwrap()] // two hatches in one line
}
fn balance(balances: &HashMap<u64, u64>, account: u64) -> u64 {
    balances[&account] // panics on a missing key, and no lint sees it
}
```

**Detect.** `clippy::indexing_slicing`, `clippy::string_slice` and `clippy::needless_range_loop`. They do not see `map[&k]` or `deque[i]`: the scanner's `map-index` advisory and review catch those.

**Exceptions.** Fixed-size arrays indexed by constants within bounds are not flagged by the lint. For a measured hot loop where bounds checks show up in a profile, slice once up front with a fallible form, then iterate or `zip` (`dot` above): `get(..n)`, `split_at_checked`, `first_chunk::<N>()`, a slice pattern, or `as_chunks::<N>()` for a constant chunk size (`chunks_exact` only when the size is a runtime value). Never hoist bounds with `assert!` (H-03), and never enable `clippy::missing_asserts_for_indexing`, which automates exactly that. If a profile still demands indexing, that is an exception request (below), with the benchmark as the reason.

### H-05 · No `#[allow]`; `#[expect]` only with human approval

**Why.** A lint is a reviewer that never gets tired. `#[allow]` removes that reviewer permanently, silently, and for everything else in its scope.

`#[allow]` is denied (`clippy::allow_attributes`). The only suppression form is `#[expect(lint, reason = "...")]`. It is denied without a reason (`allow_attributes_without_reason`), and it errors when it stops being needed, so it can't rot.

#### The exception protocol (the only legitimate hatch)

Use it when a rule's purpose is fully met by other means, so the lint is a false positive in this exact place, and no safe restructuring exists. Common legitimate cases:

- a float to integer conversion after an explicit range check, since std has no `TryFrom<f64>` for integers;
- a `too_many_lines` function that is one flat `match` over a protocol's message types;
- measured hot-path indexing (see H-04).

Steps:
1. **Prove it.** Write down why the lint's concern cannot occur here, and what you tried first.
2. **Scope it minimally.** Put the attribute on the single item or statement, never on a module or crate.
3. **Write the reason** so a reviewer can check it: say what invariant holds and where it is established.
4. **Ask.** The temper hook turns the edit into a permission prompt for the human. When you are a subagent, report the request in your Status block as a decision needed.
5. **Record** the decision in the crate charter's decisions table.

```rust
/// A percentage outside 0 to 100, or NaN.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("percentage {0} is outside 0 to 100")]
pub struct OutOfRange(pub f64);

/// Converts a percentage (0.0 to 100.0) to whole basis points.
///
/// # Errors
/// Returns [`OutOfRange`] if `pct` is outside that range or is NaN.
pub fn basis_points(pct: f64) -> Result<u16, OutOfRange> {
    if !(0.0..=100.0).contains(&pct) {
        return Err(OutOfRange(pct));
    }
    let bp = (pct * 100.0).round();
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bp is in 0.0..=10000.0 after the range check above, so the cast is exact"
    )]
    let bp = bp as u16;
    Ok(bp)
}
```

The reviewer checks that the reason is true, that it is minimal, and that a human approved it. An `#[expect]` whose reason is "clippy is wrong" or "needed to compile" is a blocker.

#### Never
- Edit `[workspace.lints]`, `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, CI workflows or `.claude/` to make a check pass. Those files are protected, and editing them asks the human.
- Pass `-A`, `--cap-lints`, or `RUSTFLAGS` overrides to cargo.
- Use `#![expect]` or `#![allow]` at the crate or module level.

### H-06 · No debug output

**Why.** `println!` from a library is output the caller didn't ask for, can't redirect, and can't turn off. It also panics when stdout is a closed pipe (`my-cli | head`). `dbg!` is a debugging tool that leaks into production.

**Do.** Libraries emit `tracing` events with structured fields. Binaries write their output through a locked handle and propagate I/O errors:

```rust
use std::io::{self, Write};

/// Writes one report line per item to `out`.
///
/// # Errors
/// Returns any I/O error from `out`, including a closed pipe.
pub fn write_report<W: Write>(mut out: W, items: &[(String, u64)]) -> io::Result<()> {
    for (name, count) in items {
        writeln!(out, "{name}\t{count}")?;
    }
    out.flush()
}
```

A binary calls it with `write_report(io::stdout().lock(), &items)?`, and a caller that keeps the writer passes `&mut out` (OWN-01).

**Detect.** `clippy::dbg_macro`, `print_stdout` and `print_stderr` (tests are allowed), plus the scanner advisories for commented-out code and TODOs without a tracking reference.

### H-07 and H-08: ownership escape hatches

`Box::leak`, `mem::forget`, `transmute`, `static mut`, `'static` bounds added to make an error go away, and clones that exist only to satisfy the borrow checker all have the same root cause. The ownership model is wrong. ownership.md, "Fighting the borrow checker", maps each error code to the shape that fixes it. The short version:

| Symptom | Hatch you're tempted to use | Real fix |
|---|---|---|
| "borrowed value does not live long enough" in a spawned thread | `Box::leak`, `'static` | `std::thread::scope`, or move owned data into the thread |
| "cannot borrow as mutable because it is also borrowed" | `.clone()` | Split the borrow, shorten the first borrow, or compute then mutate |
| "cannot move out of `self.x` which is behind a mutable reference" | `.clone()` | `std::mem::take`, `Option::take`, `mem::replace` |
| A fallible call consumes a value you still need when it fails | `.clone()` before the call | The callee hands the value back in its error (ERR-14) |
| Two owners need the same data | `Rc<RefCell<_>>` | One owner plus borrows, or an arena with index handles, or message passing |
| Global configuration | `static mut`, `lazy_static` | Parse once, pass `&Config` down (ARC-11) |

### H-09 and H-10: errors that disappear

Type-erased and stringly errors (H-09) make callers parse strings to decide what to do. Discarded results (H-10), including empty `Err(_) => {}` arms and `filter_map(Result::ok)`, make failures vanish. Both are covered in errors.md. The quick test: after your change, can a caller tell *which* failure happened and *what* it was about, without string matching? Is every `Result` either handled, propagated, or documented as irrelevant?

### H-11 · No lossy `as` casts

**Why.** `as` silently truncates (`300_u32 as u8 == 44`), wraps signs (`-1_i32 as u32 == 4294967295`), saturates floats and loses precision, with no signal. On an enum it exposes the discriminant, which changes when someone reorders or inserts a variant.

**Do.**

```rust
/// Lossless widening uses `From`.
#[must_use]
pub fn widen(bytes: u32) -> u64 {
    u64::from(bytes)
}

/// Narrowing that can fail uses `TryFrom` and reports it.
///
/// # Errors
/// Returns an error if `len` does not fit in a `u16` frame length.
pub fn frame_len(len: usize) -> Result<u16, std::num::TryFromIntError> {
    u16::try_from(len)
}

/// Deliberate saturation is spelled out.
#[must_use]
pub fn clamp_to_u8(value: u32) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

/// Why the CLI failed. Each kind has a stable exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Bad command line.
    Usage,
    /// The input was read but refused.
    Rejected,
}

impl From<Failure> for u8 {
    fn from(failure: Failure) -> Self {
        // Exhaustive: a new variant does not compile until it has a code.
        match failure {
            Failure::Usage => 2,
            Failure::Rejected => 3,
        }
    }
}
```

`main` returns `ExitCode::from(u8::from(failure))`.

**Don't.**

```rust,ignore,expect=clippy::as_conversions+clippy::cast_possible_truncation+scan:as-cast
#[derive(Clone, Copy)]
enum Failure { Usage = 2, Rejected = 3 }

fn exit_code(failure: Failure) -> u8 {
    failure as u8 // a reordered or inserted variant silently changes the code
}
fn frame_len(len: usize) -> u16 {
    len as u16 // 70_000 becomes 4_464
}
```

**Detect.** `clippy::as_conversions` and the `cast_*` family. For float to integer conversions, use the exception protocol (the H-05 example).

### Weakening verification (H-12)

testing.md holds the H-12 section. Never delete a test, add `#[ignore]`, loosen an assertion, change an expected value, accept snapshot changes blindly, catch a panic to make a test pass, `cfg` a test away, switch off `test`, `doctest`, `harness` or `autotests` in a manifest, lower a lint, or skip a hook. The hooks route test removal, manifest test switches and snapshot acceptance to the human, and the reviewer compares the test diff against the base.

A test Miri cannot run (process spawning, network, FFI) carries `#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]`, so it still shows in the count as ignored, with its reason. Never hide tests from Miri with `#[cfg(not(miri))]` on a test or module; the scanner rejects `ignore` without a reason in either form.

When a test fails, decide explicitly and write the decision down:
- **The code is wrong.** Fix the code.
- **The test is wrong** (a wrong expectation, a changed requirement). That is an H-16 decision. State the old expectation, the new one, and why, and get approval before changing it.

### H-13 and H-14: resources and async

Unbounded channels, uncapped buffers built from external input, I/O without timeouts, and detached tasks or threads (H-13) turn load into memory exhaustion and shutdown into data loss. Blocking calls in async code, and lock guards, `RefCell` borrows, entered tracing spans or `watch::Ref`s held across `.await` (H-14), stall every task on the executor thread, corrupt the span tree, or block the `watch` sender. async-concurrency.md and performance.md have the full treatment.

`clippy.toml`'s `await-holding-invalid-types` makes `clippy::await_holding_invalid_type` reject `tracing::span::Entered`, `tracing::span::EnteredSpan` and `tokio::sync::watch::Ref` across `.await`. Instrument the future with `.instrument(span)` or `#[instrument]` instead, and copy what you need out of a `watch::Ref` before awaiting. `Drop` never blocks either (ERR-13). The scanner flags `unbounded_channel`, bare `spawn(` statements and `thread::sleep` as advisories, and denies `thread::spawn` (ASY-15); the reviewer decides the rest.

## Review checklist
- [ ] Does the gate's scan show no new hard hits, and does every advisory have a judgment? (H-01..H-11)
- [ ] Is there no `unsafe`, and does any human-approved exception follow the H-01 checklist? (H-01)
- [ ] Does every `PoisonError` get one of the three documented handlings? (H-02)
- [ ] Is every std call on input the code does not control free of the panicking APIs in the H-03 table (division, `sum`, `thread::spawn`, `env::args`, float `Duration`, char-boundary `String` methods)? (H-03)
- [ ] Is every remaining assertion compile-time, or a side-effect-free `debug_assert!` on an internal invariant, with a message? (H-03)
- [ ] Is every `map[&k]` and `deque[i]` replaced by `get` or `entry`? (H-04)
- [ ] Does every `#[expect]` sit on a single item, carry a checkable reason, and have human approval? (H-05)
- [ ] Is every `.clone()`, `Arc`, `Rc`, `RefCell` and `Mutex` in the diff justified by ownership, not by a borrow error? (H-07, H-08, OWN-11)
- [ ] Does every public library function return a typed error? (H-09)
- [ ] Is every `Result` handled, propagated, or documented as irrelevant? (H-10)
- [ ] Does every enum-to-code conversion use a `match` in `From`, not `as`? (H-11)
- [ ] Is every test from the base revision still present and at least as strict? (H-12)
- [ ] Are channels bounded, tasks and threads owned, and I/O timed out? (H-13)
- [ ] Is there no blocking call, and no guard, entered span or `watch::Ref` held across `.await`? (H-14)
- [ ] Did anything route around a refusal (shell writes, function-path unwraps, `unwrap_or_else(|_| panic!())`)? (H-12)
