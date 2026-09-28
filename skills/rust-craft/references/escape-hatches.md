# Escape hatches

An escape hatch is code that tells the compiler to stop checking something: that a value is present, an index is in bounds, a cast fits, a lint doesn't apply, or a lifetime is long enough. Each one moves a guarantee from compile time to hope. This file catalogues them, gives the correct shape for each, and defines the one legitimate route for a real exception.

Read it when a lint or a temper hook refuses code, or when you are tempted to write any construct in the catalogue.

Rules covered: H-01..H-16 (detail), with pointers to ownership.md, errors.md, api-design.md and async-concurrency.md for the full treatments.

## How enforcement works (so refusals make sense)

| Layer | When | What it does |
|---|---|---|
| Workspace lints (`Cargo.toml`, `clippy.toml`) | `cargo clippy`, CI | Denies the hatches outright: `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `as_conversions`, `allow_attributes` and the rest (see templates). |
| temper pre-edit hook | Before a `Write`/`Edit` lands | Scans the edit. New hard hits are refused with the rule ID and fix. An `#[expect(..., reason)]` turns a hit into a question for the human. Removed tests, lint config edits and new dependencies also go to the human. |
| temper Bash hook | Before a shell command | Refuses writing `.rs` files through the shell, `--no-verify`, and lowering lint levels on the command line. |
| temper gate | `Stop`, the gate command, the reviewer | Runs fmt, clippy `-D warnings`, tests and rustdoc, and scans every changed file against the base revision. |
| Reviewer | Before done | Judges what tools cannot: whether a clone is justified, whether an `Arc<Mutex<_>>` is necessary, whether an exception's reason holds. |

A refusal is information about the design, not an obstacle to route around. **Routing around** is itself an H-12 violation. That covers writing the file through the shell, `Option::unwrap` as a function path, `.unwrap_or_else(|_| panic!())`, `match x { Some(v) => v, None => unreachable!() }`, `std::process::abort()`, and splitting code to hide it from the scanner. Each of these is caught, and each wastes a review round.

## Catalogue

| Hatch | Rule | Lint / check | Why it's tempting | Correct shape |
|---|---|---|---|---|
| `unsafe` | H-01 | `unsafe_code = "forbid"` | Speed, FFI, "I know it's fine" | Safe std APIs; restructure; FFI only in an approved `-sys` crate |
| `unwrap()`, `expect()` | H-02 | `unwrap_used`, `expect_used`, scan | Short; "can't fail" | `?`, `ok_or_else`, `let ... else`, or a type that makes the case impossible |
| `panic!`, `todo!`, `unimplemented!`, `unreachable!`, `abort()` | H-03 | `panic`, `todo`, `unimplemented`, `unreachable`, disallowed `process::abort` | Placeholders; "impossible" branches | Return an error; exhaustive enums; `Status: WIP` for unfinished work |
| `v[i]`, `&s[a..b]` | H-04 | `indexing_slicing`, `string_slice` | Terse | `get`, iterators, slice patterns, `split_first`, `chunks_exact`, `char_indices` |
| `#[allow(...)]` | H-05 | `allow_attributes`, scan | Silence the noise | Fix the code, or make an exception request (below) |
| `dbg!`, `println!` | H-06 | `dbg_macro`, `print_stdout`, `print_stderr` | Quick visibility | `tracing` events; `writeln!` on a locked handle in binaries |
| `Box::leak`, `mem::forget`, `transmute`, `static mut` | H-07 | `disallowed_methods`, `mem_forget`, `unsafe_code` | Get a `'static`; skip `Drop` | Move ownership; `thread::scope`; `Arc`; RAII |
| `'static` bound added to compile | H-07, OWN-07 | review | Makes E0597 or E0521 go away | Fix who owns the data (ownership.md, "Fighting the borrow checker") |
| `.clone()` to satisfy borrowck | H-08 | `redundant_clone`, scan advisory, review | Makes E0502 or E0505 go away | Split borrows, shorten scopes, `mem::take`, restructure |
| `Result<T, String>`, `Box<dyn Error>` in a lib API | H-09 | scan, review | No need to design an error type | `thiserror` enum with context fields |
| `let _ = f();`, `.ok();` | H-10 | `let_underscore_must_use`, `unused_result_ok` | Ignore an annoying `Result` | Handle, propagate, or document why it is irrelevant |
| `x as u32` | H-11 | `as_conversions`, `cast_*` | Short | `From`, `TryFrom`, `checked_*`, `saturating_*` |
| `#[ignore]`, deleted assert, loosened expectation | H-12 | test-removal hook, review | Get green | Fix the code, or ask whether the test is wrong |
| `unbounded_channel()`, detached `spawn` | H-13 | scan advisory, review | Simpler | Bounded channel; `JoinSet` or `TaskTracker` |
| `std::thread::sleep` or a held `MutexGuard` in async code | H-14 | `await_holding_lock`, review | It works in tests | `tokio::time::sleep`; scope the guard before `.await` |

## H-02: unwrap and expect

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

```rust,ignore
let name = users.get(&id).unwrap();           // crashes with no context
let port: u16 = env_port.parse().expect("port"); // configuration errors become panics
let first = items.first().unwrap();           // "items is never empty" is a hope, not a type
```

**Detect.** `clippy::unwrap_used`, `clippy::expect_used`, `clippy::unwrap_in_result`, and the scanner, which also catches `Option::unwrap` used as a function path.

"It can't fail" means the invariant belongs in a type. A non-empty list is a `(T, Vec<T>)` pair or a `NonEmpty<T>` newtype, a validated port is a `Port` newtype built by `TryFrom`, and a `Regex` compiled from a literal is built once in a `LazyLock` whose failure is a test failure. errors.md, "Eliminating unwrap/expect", has a table of the common cases.

## H-03: panics

**Why.** A panic in a library takes the decision away from the caller. In a server it kills a task, or with `panic = "abort"` the whole process. `todo!()` ships unfinished code that compiles.

**Do.** Make the impossible case unrepresentable, so there is no branch left to panic in:

```rust
use std::num::NonZeroU32;

/// Splits `total` into `parts` near-equal shares. Division by zero is impossible by type.
#[must_use]
pub fn share(total: u32, parts: NonZeroU32) -> u32 {
    total / parts.get()
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

```rust,ignore
fn next(phase: u8) -> u8 {
    match phase { 0 => 2, 2 => 1, 1 => 0, _ => unreachable!() } // the type allows 253 other values
}
fn parse(input: &str) -> Ast { todo!() }                      // compiles, ships, crashes
```

**Detect.** `clippy::panic`, `todo`, `unimplemented`, `unreachable`, and the disallowed `process::abort`. Also review `assert!` in production code. Use `debug_assert!` for internal sanity checks and return an error for anything a caller can trigger.

Unfinished work is reported, not stubbed. End the turn with `Status: WIP` and list what remains.

## H-04: indexing and slicing

**Why.** `v[i]` and `&s[a..b]` panic on out-of-bounds input. With strings they also panic when the index falls inside a UTF-8 character. The compiler can prove bounds for you when you use the right shapes.

**Do.**

```rust
/// Sum of adjacent distances, with no indexing and no overflow.
#[must_use]
pub fn total_variation(samples: &[i64]) -> u64 {
    samples
        .array_windows::<2>()
        .map(|[a, b]| a.abs_diff(*b))
        .fold(0, u64::saturating_add)
}

/// Splits a `key=value` pair without byte arithmetic.
#[must_use]
pub fn split_pair(line: &str) -> Option<(&str, &str)> {
    line.split_once('=').map(|(k, v)| (k.trim(), v.trim()))
}

/// Head and tail through a slice pattern.
#[must_use]
pub fn head_and_rest(items: &[u8]) -> Option<(u8, &[u8])> {
    match items {
        [first, rest @ ..] => Some((*first, rest)),
        [] => None,
    }
}
```

**Don't.**

```rust,ignore
for i in 0..v.len() - 1 { total += (v[i + 1] - v[i]).abs(); } // empty input underflows; i64 subtraction can overflow
let key = &line[..line.find('=').unwrap()];                   // two hatches in one line
```

**Detect.** `clippy::indexing_slicing`, `clippy::string_slice`, and `clippy::needless_range_loop`.

**Exceptions.** Fixed-size arrays indexed by constants within bounds are not flagged by the lint. For a measured hot loop where bounds checks show up in a profile, first try iterators and `chunks_exact`, which usually let LLVM drop the checks. If a profile still demands indexing, that is an exception request (below), with the benchmark as the reason.

### Panicking APIs (H-03 beyond the macros)

The lints catch panic *macros*. They do not catch std or crate functions that panic on bad input, so a green gate is not proof of panic-freedom. Use the fallible forms in production code:

| Panics | Use instead |
|---|---|
| `a + b`, `a * b`, `a - b` on integers (overflow; panics in debug, wraps in release) | `checked_*`, `saturating_*`, `abs_diff`, or `wrapping_*` when wrapping is the intent (ERR-09) |
| `strict_*` integer ops (1.91) | `checked_*`: "strict" means "panic on overflow" |
| `Instant + Duration`, `Instant - Instant`, `Duration - Duration` | `checked_add`, `checked_duration_since`, `saturating_duration_since`, `Duration::checked_sub` |
| `slice.split_at(i)`, `v.remove(i)`, `v.insert(i, x)`, `v.swap(i, j)`, `v.drain(a..b)` | `split_at_checked`, and a `get` or length check first |
| `chunks(0)`, `windows(0)`, `as_chunks::<0>()`, `step_by(0)` | non-zero constants, or `NonZeroUsize` parameters |
| `RefCell::borrow_mut` while borrowed | `try_borrow_mut`, or better, a design without `RefCell` (OWN-12) |
| `HashMap::get_disjoint_mut` with overlapping keys | the slice form (returns `Result`), or keys distinct by construction |
| `tokio::spawn` or `Handle::current()` outside a runtime | take a `Handle` and call `handle.spawn`, or `Handle::try_current()` (ASY-02) |
| `tracing_subscriber::...::init()` when a subscriber is already set | `try_init()` and report the error |
| `mutex.lock().unwrap()` (panics if the lock is poisoned) | map the `PoisonError` to your error type (ERR-04 comment rule applies), or recover with `PoisonError::into_inner` when a panic mid-update cannot leave the data inconsistent, and say why |
| `String::from_utf8(..).unwrap()`, `str::from_utf8(..).unwrap()` | `?` with a typed error, or `from_utf8_lossy` when lossiness is the documented meaning |

Reviewers treat a call to one of these, on input the code does not control, as a finding under H-03.

## H-05: lint suppression and the exception protocol

**Why.** A lint is a reviewer that never gets tired. `#[allow]` removes that reviewer permanently, silently, and for everything else in its scope.

`#[allow]` is denied (`clippy::allow_attributes`). The only suppression form is `#[expect(lint, reason = "...")]`. It is denied without a reason (`allow_attributes_without_reason`), and it errors when it stops being needed, so it can't rot.

### The exception protocol (the only legitimate hatch)

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
/// Converts a percentage (0.0 to 100.0) to whole basis points.
/// Returns `None` if `pct` is outside that range or is NaN.
#[must_use]
pub fn basis_points(pct: f64) -> Option<u16> {
    if !(0.0..=100.0).contains(&pct) {
        return None;
    }
    let bp = (pct * 100.0).round();
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "bp is in 0.0..=10000.0 after the range check above, so the cast is exact"
    )]
    let bp = bp as u16;
    Some(bp)
}
```

The reviewer checks that the reason is true, that it is minimal, and that a human approved it. An `#[expect]` whose reason is "clippy is wrong" or "needed to compile" is a blocker.

### Never
- Edit `[workspace.lints]`, `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, CI workflows or `.claude/` to make a check pass. Those files are protected, and editing them asks the human.
- Pass `-A`, `--cap-lints`, or `RUSTFLAGS` overrides to cargo.
- Use `#![expect]` or `#![allow]` at the crate or module level.

## H-06: debug output

**Why.** `println!` from a library is output the caller didn't ask for, can't redirect, and can't turn off. It also panics when stdout is a closed pipe (`my-cli | head`). `dbg!` is a debugging tool that leaks into production.

**Do.** Libraries emit `tracing` events with structured fields. Binaries write their output through a locked handle and propagate I/O errors:

```rust
use std::io::{self, Write};

/// Writes one report line per item to `out`.
///
/// # Errors
/// Returns any I/O error from `out`, including a closed pipe.
pub fn write_report<W: Write>(out: &mut W, items: &[(String, u64)]) -> io::Result<()> {
    for (name, count) in items {
        writeln!(out, "{name}\t{count}")?;
    }
    out.flush()
}
```

A binary calls it with `write_report(&mut io::stdout().lock(), &items)?`.

**Detect.** `clippy::dbg_macro`, `print_stdout` and `print_stderr` (tests are allowed), plus the scanner advisories for commented-out code and TODOs without a tracking reference.

## H-07 and H-08: ownership escape hatches

`Box::leak`, `mem::forget`, `transmute`, `static mut`, `'static` bounds added to make an error go away, and clones that exist only to satisfy the borrow checker all have the same root cause. The ownership model is wrong. ownership.md, "Fighting the borrow checker", maps each error code to the shape that fixes it. The short version:

| Symptom | Hatch you're tempted to use | Real fix |
|---|---|---|
| "borrowed value does not live long enough" in a spawned thread | `Box::leak`, `'static` | `std::thread::scope`, or move owned data into the thread |
| "cannot borrow as mutable because it is also borrowed" | `.clone()` | Split the borrow, shorten the first borrow, or compute then mutate |
| "cannot move out of `self.x` which is behind a mutable reference" | `.clone()` | `std::mem::take`, `Option::take`, `mem::replace` |
| Two owners need the same data | `Rc<RefCell<_>>` | One owner plus borrows, or an arena with index handles, or message passing |
| Global configuration | `static mut`, `lazy_static` | Parse once, pass `&Config` down (ARC-11) |

## H-09 and H-10: errors that disappear

Type-erased and stringly errors (H-09) make callers parse strings to decide what to do. Discarded results (H-10) make failures vanish. Both are covered in errors.md. The quick test: after your change, can a caller tell *which* failure happened and *what* it was about, without string matching? Is every `Result` either handled, propagated, or documented as irrelevant?

## H-11: numeric casts

**Why.** `as` silently truncates (`300_u32 as u8 == 44`), wraps signs (`-1_i32 as u32 == 4294967295`), saturates floats and loses precision, with no signal.

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
```

**Detect.** `clippy::as_conversions` and the `cast_*` family. For float to integer conversions, use the exception protocol (the H-05 example).

## H-12: weakening verification

Never delete a test, add `#[ignore]`, loosen an assertion, change an expected value, accept snapshot changes blindly, catch a panic to make a test pass, `cfg` a test away, lower a lint, or skip a hook. The hooks route test removal and snapshot acceptance to the human, and the reviewer compares the test diff against the base.

When a test fails, decide explicitly and write the decision down:
- **The code is wrong.** Fix the code.
- **The test is wrong** (a wrong expectation, a changed requirement). That is an H-16 decision. State the old expectation, the new one, and why, and get approval before changing it.

## H-13 and H-14: resources and async

Unbounded channels, uncapped buffers built from external input, I/O without timeouts, and detached tasks (H-13) turn load into memory exhaustion and shutdown into data loss. Blocking calls and held locks in async code (H-14) stall every task on the executor thread. async-concurrency.md and performance.md have the full treatment. The scanner flags `unbounded_channel`, bare `spawn(` statements and `thread::sleep` as advisories, and the reviewer decides.

## Review checklist
- [ ] Does the gate's scan show no new hard hits, and does every advisory have a judgment? (H-01..H-11)
- [ ] Does every `#[expect]` sit on a single item, carry a checkable reason, and have human approval? (H-05)
- [ ] Is every `.clone()`, `Arc`, `Rc`, `RefCell` and `Mutex` in the diff justified by ownership, not by a borrow error? (H-07, H-08, OWN-11)
- [ ] Does every public library function return a typed error? (H-09)
- [ ] Is every `Result` handled, propagated, or documented as irrelevant? (H-10)
- [ ] Is every test from the base revision still present and at least as strict? (H-12)
- [ ] Are channels bounded, tasks owned and I/O timed out? (H-13)
- [ ] Is there no blocking call and no guard held across `.await`? (H-14)
- [ ] Did anything route around a refusal (shell writes, function-path unwraps, `unwrap_or_else(|_| panic!())`)? (H-12)
