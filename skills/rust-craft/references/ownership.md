# Ownership, borrowing and lifetimes

Signatures, lifetimes, borrow-checker errors, smart pointers, interior mutability and clones. Read it before designing a signature or a shared type, whenever rustc reports a borrow or lifetime error (E04xx, E05xx, E07xx), and before approving any `.clone()`, `Arc`, `Mutex`, `RefCell` or `'static` in a diff. After two failed attempts at the same error (PROC-09), stop and pick a shape from §Fighting the borrow checker.

Rules covered: OWN-01, OWN-02, OWN-03, OWN-04, OWN-05, OWN-06, OWN-07, OWN-08, OWN-09, OWN-10, OWN-11, OWN-12, OWN-13, OWN-14, OWN-15, OWN-16, H-07, H-08

## Signatures

### Signature decision table

| The function... | Parameter | Return |
|---|---|---|
| reads a string, sequence or path | `&str`, `&[T]`, `&Path` (`impl AsRef<Path>` at public edges) | |
| reads any source of items once | `impl IntoIterator<Item = T>` | |
| mutates the caller's value in place | `&mut T` | `()` or a summary |
| stores the value (field, map, channel) | `String`, `Vec<T>`, `T` | |
| constructor or builder that stores | `impl Into<String>`, `impl Into<PathBuf>` | `Self` |
| consumes or transitions the value | `self` / `T` | the new value |
| usually passes input through, sometimes allocates | `&str` | `Cow<'_, str>` |
| exposes contents the caller only iterates | | `&[T]` or `impl Iterator<Item = &T>` |
| creates a value | | owned `T`, `String`, `Vec<T>` |
| hands out a view of `self` | `&self` | `&str`, `&[T]`, `&Path`, `&T` |
| shares a small `Copy` value (<= 16 bytes) | `T` by value | `T` |

### OWN-01 · Borrow in parameters, own in returns
**Why.** A borrowed parameter accepts owned, borrowed and sliced inputs with no allocation; an owned one forces every caller that keeps its value to clone. Returns are the opposite: the callee created the value, so hand over ownership.
### OWN-02 · Take ownership when the callee stores
**Why.** If the callee stores a `&str` argument it must allocate internally, and a caller that already had a `String` pays twice. Taking `String` moves it for free and makes any copy the caller's visible decision.
### OWN-03 · Never `&String`, `&Vec<T>`, `&Box<T>`, `&PathBuf`, `&OsString`
**Why.** They add a pointer hop and reject `&str`, array and slice inputs for no benefit.

OWN-01 to OWN-04 share the example below.
### OWN-04 · Choose the receiver deliberately
**Why.** The receiver is the contract: `&self` queries, `&mut self` mutates, `self` converts or ends a typestate. `into_*` takes `self`, `as_*` and `to_*` take `&self` (API-01).
**Do.**
```rust
use std::borrow::Cow;

/// Reads only, so it borrows.
#[must_use]
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Allocates only when a tab is present (OWN-10).
#[must_use]
pub fn expand_tabs(line: &str) -> Cow<'_, str> {
    if line.contains('\t') {
        Cow::Owned(line.replace('\t', "    "))
    } else {
        Cow::Borrowed(line)
    }
}

/// A customer that owns its data.
#[derive(Debug, Clone)]
pub struct Customer {
    name: String,
}

impl Customer {
    /// Constructor that stores: `impl Into<String>` spares callers a `.to_owned()`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Query: `&self`, returns a borrowed view.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Mutation that stores its argument: take the owned `String`.
    pub fn rename(&mut self, name: String) {
        self.name = name;
    }

    /// Conversion: consumes `self`, hands back the field without copying.
    #[must_use]
    pub fn into_name(self) -> String {
        self.name
    }
}
```
**Don't.**
```rust,ignore
pub fn word_count(text: &String) -> usize { .. }       // OWN-03: rejects &str, extra hop
pub fn total(values: Vec<u64>) -> u64 { .. }            // OWN-01: forces a Vec, consumes it
pub fn rename(&mut self, name: &str) { self.name = name.to_string(); } // OWN-02: hidden alloc
pub fn name(&self) -> String { self.name.clone() }      // OWN-01: allocates on every read
```
**Detect.** `clippy::needless_pass_by_value`, `ptr_arg`, `borrowed_box`, `wrong_self_convention`. Review: any `.to_string()`/`.clone()` of a parameter inside the callee means the parameter should have been owned.
**Exceptions.** `impl Into<String>` in non-constructor methods is fine when the method is builder-like; say so in the doc comment.

### OWN-10 · Return `Cow` when you usually borrow
**Why.** `expand_tabs` above returns the input untouched in the common case. `Cow<'_, str>` makes that free and still owns when it must. Do not return `Cow` when you always allocate (return `String`) or never do (return `&str`).

## Lifetimes and elision

### OWN-05 · Rely on elision; name lifetimes only for real relationships
**Why.** Elision rules: each elided input gets its own lifetime; with exactly one input lifetime, or a `&self`/`&mut self`, the output takes it. Name a lifetime only when there are several inputs and the output is tied to a specific one, or in a type that holds borrows.
**Do.**
```rust
/// Elision ties the output to the only input.
#[must_use]
pub fn first_word(text: &str) -> &str {
    text.split_whitespace().next().unwrap_or_default()
}

/// Two reference inputs: elision cannot choose, so name the relationship.
#[must_use]
pub fn longer<'a>(left: &'a str, right: &'a str) -> &'a str {
    if right.len() > left.len() {
        right
    } else {
        left
    }
}

/// Queued items.
#[derive(Debug, Default)]
pub struct Queue {
    items: Vec<u32>,
}

impl Queue {
    /// Write `'_` in the path type: the borrow of `self` is visible.
    pub fn iter(&self) -> std::slice::Iter<'_, u32> {
        self.items.iter()
    }
}

impl<'a> IntoIterator for &'a Queue {
    type Item = &'a u32;
    type IntoIter = std::slice::Iter<'a, u32>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}
```
**Don't.**
```rust,ignore
pub fn first_word<'a>(text: &'a str) -> &'a str { .. }  // needless_lifetimes
pub fn iter(&self) -> std::slice::Iter<u32> { .. }     // hidden lifetime: mismatched_lifetime_syntaxes
fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result   // elided_lifetimes_in_paths (rust_2018_idioms)
```
**Detect.** `clippy::needless_lifetimes`, `elidable_lifetime_names`, `unused_lifetimes`, `redundant_lifetimes`. Since 1.89 rustc warns by default on `mismatched_lifetime_syntaxes`: an input written `&self` and an output path with the lifetime hidden (`Iter<u32>`). Write `'_` in every path type that carries a lifetime: `Iter<'_, T>`, `Formatter<'_>`, `Ref<'_, T>`, `Bencher<'_, '_>`. The template's `rust_2018_idioms` also denies hidden path lifetimes in parameters.

### OWN-06 · Borrowing structs are short-lived views
**Why.** A lifetime parameter on a struct infects every type and function that holds it. Parsers, tokenizers, iterators and formatting adapters borrow; domain entities, config, services and anything stored in a collection or sent across a task own. Name struct lifetimes after what they borrow (`'src`, `'buf`, `'de`).
**Do.**
```rust
/// A short-lived borrowing tokenizer over source text.
#[derive(Debug, Clone)]
pub struct Tokens<'src> {
    rest: &'src str,
}

impl<'src> Tokens<'src> {
    /// Starts tokenizing `src`.
    #[must_use]
    pub fn new(src: &'src str) -> Self {
        Self { rest: src }
    }
}

impl<'src> Iterator for Tokens<'src> {
    type Item = &'src str;

    fn next(&mut self) -> Option<&'src str> {
        let trimmed = self.rest.trim_start();
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let (token, rest) = trimmed.split_at_checked(end)?;
        self.rest = rest;
        (!token.is_empty()).then_some(token)
    }
}
```
**Don't.**
```rust,ignore
pub struct OrderService<'a> { repo: &'a PgRepo, cfg: &'a Config } // OWN-06: lifetime leaks into every caller
```
**Detect.** Review: a lifetime parameter on a type in the application or domain layer, or on anything stored in a `Vec`, map or `Arc`.

### OWN-07 · Never `'static` to silence a lifetime error (H-07)
**Why.** `'static` on a bound, `Box::leak`, or a global makes the error disappear by making the value immortal: memory leaks, tests share state, and the real ownership question stays unanswered. The honest fixes are: move the owned value in, scope the borrow (`thread::scope`), or share an `Arc`.
**Don't.**
```rust,ignore
fn start(cfg: &Config) { let cfg: &'static Config = Box::leak(Box::new(cfg.clone())); spawn(cfg) } // H-07
static mut CACHE: Vec<u8> = Vec::new();                                                           // H-07
fn run<F: Fn() + 'static>(f: F)  // 'static added only because a borrow did not live long enough
```
**Detect.** `disallowed_methods` (`Box::leak`), `clippy::mem_forget`, `static_mut_refs`; scan for `'static` added in a diff; review every new `'static` bound: it is legitimate only when the value really is sent to an unscoped thread or task (then the caller moves an owned value in).
**Exceptions.** `&'static str` literals and `const`/`static` immutable tables are fine; they are not escape hatches.

## Fighting the borrow checker

### OWN-08 · Restructure, do not clone
**Why.** A borrow error says two uses of one value overlap. Cloning hides the overlap and ships the allocation and the divergent copy to production. First write down who owns each value and where each borrow ends; the table then gives the shape.

| Symptom | Real cause | Correct shape |
|---|---|---|
| E0499 cannot borrow as mutable more than once | Two `&mut` paths to one value, often `&mut self` method called while iterating `&mut self.field` | Borrow disjoint fields directly; helper takes the field, not `&mut self`; `get_disjoint_mut`/`split_at_mut`; split the struct |
| E0502 cannot borrow as mutable because also borrowed as immutable | A shared borrow (iterator, `get`) is still live at the mutation | Compute then mutate; shorten the scope; copy the small `Copy` value out; collect keys/indices first |
| E0505 cannot move out because it is borrowed | Moving a value while a reference into it is live | End the borrow first (copy out what you need), or move later, or pass a borrow instead of moving |
| E0506 cannot assign because it is borrowed | Assigning while a reference to the old value is live | Compute the new value, drop the reference, then assign |
| E0382 use of moved value | Value consumed earlier (by-value call, `for x in v`, `move` closure) | Borrow instead (`&v`, `.iter()`); move only at the last use; `Arc` if there really are two owners |
| E0597 does not live long enough | Owner is dropped while a borrow is still needed | Move the owner to an outer scope, or return/store owned data |
| E0716 temporary value dropped while borrowed | Borrowing from a temporary (`String::from(..).as_str()`, `lock().unwrap().get()`) | Bind the temporary with `let` first; or narrow RPIT captures with `use<..>` |
| E0373 closure may outlive the current function | Thread or task closure borrows a local | `move` an owned value in; or `std::thread::scope` to borrow |
| E0521 borrowed data escapes outside of method | `&self` data sent into `thread::spawn`/`tokio::spawn` | Clone only the needed `Arc` handle before `move`; move owned data; scope |
| E0507 cannot move out of `*x` / of index / out of enum behind `&mut` | Taking ownership through a reference | `mem::take`, `mem::replace`, `Option::take`/`replace`, `Vec::swap_remove`/`drain`, iterate by value, match on `&` |
| E0515 cannot return reference to local variable | The value is created in the function | Return the owned value (`String`, `Vec<T>`), or `Cow` |
| E0502/E0716 at a call site that holds a returned `impl Iterator` | Edition 2024 RPIT captures a borrow the iterator does not use | `+ use<>` / `+ use<'a>` (§Returning iterators) |

#### Shape: disjoint fields and helpers that take fields (E0499)
```rust,ignore
for item in &mut self.items { self.record(item); } // E0499: `record(&mut self)` re-borrows all of self
```
```rust
/// Inventory with an audit log.
#[derive(Debug, Default)]
pub struct Inventory {
    quantities: Vec<u32>,
    log: Vec<String>,
}

impl Inventory {
    /// Adds `amount` to every line and logs each change.
    pub fn restock_all(&mut self, amount: u32) {
        // Disjoint fields: `self.quantities` and `self.log` borrow independently.
        for qty in &mut self.quantities {
            *qty = qty.saturating_add(amount);
            Self::record(&mut self.log, *qty);
        }
    }

    // Takes only the field it needs, not `&mut self`.
    fn record(log: &mut Vec<String>, qty: u32) {
        log.push(format!("restocked to {qty}"));
    }
}
```

#### Shape: `get_disjoint_mut` for two elements of one slice (E0499)
```rust,ignore
let src = &mut balances[from]; let dst = &mut balances[to]; // E0499 and H-04 panicking index
```
```rust
use std::slice::GetDisjointMutError;

/// Failure to transfer between two accounts.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransferError {
    /// Indices out of range or equal.
    #[error("invalid account pair")]
    Accounts(#[from] GetDisjointMutError),
    /// Not enough funds.
    #[error("insufficient funds")]
    Insufficient,
}

/// Moves `amount` from `from` to `to`.
///
/// # Errors
/// [`TransferError`] for a bad index pair or insufficient funds.
pub fn transfer(
    balances: &mut [u64],
    from: usize,
    to: usize,
    amount: u64,
) -> Result<(), TransferError> {
    let [src, dst] = balances.get_disjoint_mut([from, to])?;
    *src = src.checked_sub(amount).ok_or(TransferError::Insufficient)?;
    *dst = dst.saturating_add(amount);
    Ok(())
}
```
Slice `get_disjoint_mut` (1.86) returns `Err` on overlap or out-of-range. `HashMap::get_disjoint_mut` panics on duplicate keys: check `from != to` first, or use the slice form.

#### Shape: compute, then mutate (E0502, E0506)
```rust,ignore
for s in scores.iter() { if *s > 100 { scores.push(*s / 2); } } // E0502: iterator still borrows
```
```rust
/// Appends half of every large score.
pub fn append_halves(scores: &mut Vec<u32>) {
    // Compute from a shared borrow first; it ends at the semicolon.
    let halves: Vec<u32> = scores.iter().filter(|s| **s > 100).map(|s| s / 2).collect();
    scores.extend(halves); // then mutate
}
```
The `collect` here is the fix, not waste: the new elements must exist before the vector can grow. When the computed thing is a single `Copy` value (`let max = v.iter().copied().max();`), no collection is needed.

#### Shape: move out of `&mut` with `mem::take` / `mem::replace` / `Option::take` (E0507)
```rust,ignore
self.conn = match self.conn { Conn::Idle => .., open => open }; // E0507: cannot move out of `self.conn`
let batch = self.batch.clone(); self.batch.clear();             // H-08: clone to dodge E0507
```
```rust
use std::mem;

/// Connection state.
#[derive(Debug, Default)]
pub enum Conn {
    /// Not connected.
    #[default]
    Idle,
    /// Connected, with an output buffer.
    Open(Vec<u8>),
}

/// Owns a connection, a batch and an optional pending request.
#[derive(Debug, Default)]
pub struct Session {
    conn: Conn,
    batch: Vec<u8>,
    pending: Option<String>,
}

impl Session {
    /// Moves the old state out by value; `mem::replace(&mut x, Conn::Idle)`
    /// is the form for types without `Default`.
    pub fn open(&mut self) {
        self.conn = match mem::take(&mut self.conn) {
            Conn::Idle => Conn::Open(Vec::new()),
            open @ Conn::Open(_) => open,
        };
    }

    /// Hands the batch to the caller and leaves an empty one.
    pub fn flush(&mut self) -> Vec<u8> {
        mem::take(&mut self.batch)
    }

    /// Consumes the pending request, if any.
    pub fn take_pending(&mut self) -> Option<String> {
        self.pending.take()
    }
}
```
For `Vec` elements: `swap_remove(i)`/`remove(i)` (bounds-checked by `if i < v.len()`), `drain(..)`, or `into_iter()` when you own the whole `Vec`.

#### Shape: restructure into smaller structs (E0499 across methods)
When a method needs `&mut` to part A and `&` to part B, and both are behind `&mut self` methods, the struct is doing two jobs. Split it so each part has its own methods and the parent borrows them independently.
```rust
/// Simulation state.
#[derive(Debug, Default)]
pub struct World {
    tick: u64,
}

/// Statistics derived from the world.
#[derive(Debug, Default)]
pub struct Stats {
    last_tick: u64,
}

impl Stats {
    fn record(&mut self, world: &World) {
        self.last_tick = world.tick;
    }
}

/// Split into parts so each borrow touches only one part.
#[derive(Debug, Default)]
pub struct Engine {
    world: World,
    stats: Stats,
}

impl Engine {
    /// Advances the world, then records stats from it.
    pub fn step(&mut self) {
        self.world.tick = self.world.tick.saturating_add(1);
        self.stats.record(&self.world); // &mut stats, & world: disjoint
    }
}
```

#### Shape: index handles instead of stored references
A struct that wants to hold `&` into a sibling collection (`parent: &Node`, `current: &Item`) should hold an index or id instead and look it up at use. See §Graphs and cyclic data.

#### Shape: move into the thread, or scope it (E0373, E0521)
```rust,ignore
let handle = thread::spawn(|| sum(&values));        // E0373: borrows `values`
thread::spawn(move || self.names.len());             // E0521: `&self` escapes
thread::spawn(move || ctx.clone().names.len());      // H-08/OWN-14: clones the whole context
```
```rust
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// A worker thread panicked.
#[derive(Debug, thiserror::Error)]
#[error("worker thread panicked")]
pub struct WorkerPanicked;

fn sum(values: &[u64]) -> u64 {
    values.iter().fold(0, |acc, v| acc.saturating_add(*v))
}

/// Moves ownership into the thread; the caller owns the handle.
#[must_use]
pub fn spawn_sum(values: Vec<u64>) -> JoinHandle<u64> {
    thread::spawn(move || sum(&values))
}

/// Borrows across threads without `'static`: the scope joins before returning.
///
/// # Errors
/// [`WorkerPanicked`] if the helper thread panicked.
pub fn parallel_sum(values: &[u64]) -> Result<u64, WorkerPanicked> {
    let (left, right) = values.split_at(values.len() / 2);
    thread::scope(|s| {
        let worker = s.spawn(|| sum(left));
        let right_sum = sum(right);
        // the panic payload has no diagnostic value here (ERR-04)
        let left_sum = worker.join().map_err(|_payload| WorkerPanicked)?;
        Ok(left_sum.saturating_add(right_sum))
    })
}

/// Application context.
#[derive(Debug)]
pub struct AppContext {
    names: Arc<[String]>,
}

impl AppContext {
    /// Clones only the handle the thread needs, then moves it (OWN-14).
    #[must_use]
    pub fn spawn_counter(&self) -> JoinHandle<usize> {
        let names = Arc::clone(&self.names);
        thread::spawn(move || names.len())
    }
}
```
For async tasks the same rules apply; task ownership (`JoinSet`, H-13) is in `async-concurrency.md`.

### OWN-09 · Entry API for insert-or-update
**Why.** `contains_key` then `insert`, or `get` then `insert`, hashes twice and often forces a clone of the key to satisfy the borrow checker. `entry` hashes once and hands back `&mut V`.
**Do.**
```rust
use std::collections::HashMap;

/// Counts occurrences with one hash lookup per word.
#[must_use]
pub fn word_frequencies(text: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        let count = counts.entry(word).or_insert(0_usize);
        *count = count.saturating_add(1);
    }
    counts
}
```
**Don't.**
```rust,ignore
if !counts.contains_key(word) { counts.insert(word, 0); } // double lookup
*counts.get_mut(word).unwrap() += 1;                     // third lookup, H-02
```
**Detect.** `clippy::map_entry`. Exception to the shape: for `String` keys looked up by `&str`, `entry` needs an owned key every call; use `get_mut` then `insert(key.to_owned(), ..)` on miss (see `HitCounter` below).

### OWN-14 · Closures borrow; `move` only across a boundary
**Why.** Non-`move` closures borrow exactly the fields they use (edition 2021+ disjoint capture). `move` is for closures that outlive the frame: threads, tasks, returned closures. Before a `move`, clone the one `Arc` handle it needs (`spawn_counter` above), never `self.clone()`.
**Detect.** Review: `let this = self.clone(); spawn(move || ..)`; `move` on closures passed to `iter().map(..)` that do not need it.

## Returning iterators and the 2024 capture rules

### OWN-13 · Return iterators or slices, not fresh collections
**Why.** Returning `Vec<&T>` allocates for callers that only iterate, count or take the first. Return `&[T]` for stored sequences, `impl Iterator` for filtered or mapped views; the caller can still `collect`.

In edition 2024, return-position `impl Trait` captures every generic and lifetime in scope (MOD-06). So `fn active(&self) -> impl Iterator<Item = &User>` already borrows `self`; the edition-2021 spelling `+ '_` is still accepted and documents the borrow. The new problem is the opposite: the returned iterator is assumed to borrow arguments it does not use, which triggers E0502/E0716 at call sites. Declare the exact captures with `use<..>` (1.82): `use<>` captures nothing, `use<'a, T>` captures listed ones. Every type parameter in scope must be listed.
**Do.**
```rust
/// A user directory.
#[derive(Debug, Default)]
pub struct Directory {
    names: Vec<String>,
    next_id: u32,
}

impl Directory {
    /// Borrowed view; `+ 'a` is implied in 2024 but documents the borrow.
    pub fn starting_with<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.names
            .iter()
            .map(String::as_str)
            .filter(move |n| n.starts_with(prefix))
    }

    /// `use<>`: the iterator borrows nothing, so callers may mutate `self`.
    pub fn upcoming_ids(&self, count: u32) -> impl Iterator<Item = u32> + use<> {
        let start = self.next_id;
        (0..count).map(move |offset| start.saturating_add(offset))
    }

    /// Adds a user.
    pub fn add(&mut self, name: String) {
        self.names.push(name);
        self.next_id = self.next_id.saturating_add(1);
    }
}

/// Compiles only because of `use<>`; without it, E0502 on `dir.add`.
pub fn reserve_ids(dir: &mut Directory) -> Vec<u32> {
    let ids = dir.upcoming_ids(2);
    dir.add(String::from("pending"));
    ids.collect()
}
```
**Don't.**
```rust,ignore
pub fn active(&self) -> Vec<&User> { self.users.iter().filter(|u| u.active).collect() } // OWN-13
pub fn active(&self) -> Box<dyn Iterator<Item = &User> + '_> { .. } // heap + dyn for no reason
```
**Detect.** `clippy::needless_collect` (inside functions); review: public getters returning `Vec` built from `self`.

## Sharing

### OWN-11 · Walk the sharing ladder
**Why.** Each rung adds runtime cost, failure modes (poisoning, deadlock, `BorrowMutError` panics, leaks through `Rc` cycles) and coupling. Stop at the first rung that works, and write the justification in the type's doc comment.

| Rung | Use when | Justification required |
|---|---|---|
| Single owner | One component owns the value for its whole life | none |
| Borrow (`&T`, `&mut T`) | Others need it for less than the owner's lifetime | none |
| Move into thread/task | The value is needed only by the spawned work | none |
| `thread::scope` borrow | Parallel work finishes before the owner continues | none |
| `Arc<T>` (immutable) | Several threads/tasks read the same data with independent lifetimes | "shared read-only, owners outlive each other unpredictably" |
| Actor / channel | Shared mutable state with request/response or ordering | preferred over a lock for async (ASY-09) |
| `Arc<Mutex<T>>` / `Arc<RwLock<T>>` | Several threads mutate one state; short critical sections | doc comment: who writes, who reads, why an actor was rejected |
| Atomics | A single counter or flag | the chosen `Ordering` and why |
| `Rc<T>` | Single-threaded shared ownership (UI trees, persistent structures) | why an owner plus borrows or an arena does not fit |
| `Rc<RefCell<T>>` | Single-threaded shared mutation | as `Rc`, plus why `&mut` cannot be threaded through; prefer an arena |
| Arena + index handles | Graphs, trees with parent links, cyclic data | none: this is the default for graphs |

`Rc<Mutex<T>>` is denied (`clippy::rc_mutex`). `Arc<RefCell<T>>` does not compile across threads; do not try to make it.
**Do.**
```rust
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// Per-path hit counts shared by all request workers.
///
/// Shared mutable state (OWN-11): every worker increments and the reporter
/// reads concurrently. An actor was rejected because callers need no reply.
#[derive(Debug, Default)]
pub struct HitCounter {
    counts: Mutex<HashMap<String, u64>>,
}

impl HitCounter {
    /// Records one hit on `path`.
    pub fn hit(&self, path: &str) {
        // Plain counters stay valid after a panic elsewhere: recover from poison.
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        match counts.get_mut(path) {
            Some(n) => *n = n.saturating_add(1),
            // The map stores the key: allocate only on first sight.
            None => {
                counts.insert(path.to_owned(), 1);
            }
        }
    }
}
```
The `Mutex` lives inside the type and the API takes `&self`; callers hold `Arc<HitCounter>`, never `Arc<Mutex<HashMap<..>>>`. Recovering from poison is only correct when the protected data cannot be left half-updated; otherwise map the `PoisonError` to a typed error.
**Don't.**
```rust,ignore
pub struct App { state: Arc<Mutex<AppState>> } // no justification; every field behind one lock
let shared = Rc::new(RefCell::new(node));    // tree with parent links: use an arena
```
**Detect.** scan (advisory) and review: every new `Arc<Mutex`, `Arc<RwLock`, `Rc<RefCell`, `Rc<` needs the doc-comment justification; `clippy::rc_mutex`.

### OWN-12 · Interior mutability is a documented design decision

| Need | Type | Thread-safe | Failure mode |
|---|---|---|---|
| Replace a `Copy` value through `&self` | `Cell<T>` | no | none |
| Compute once, read many (cache) | `OnceCell<T>` / `OnceLock<T>` | `OnceLock` | none |
| Global lazy init | `LazyLock<T>` (MOD-03) | yes | init panic poisons |
| Borrow-checked-at-runtime mutation | `RefCell<T>` | no | panics on overlapping `borrow_mut` |
| Counter or flag | `AtomicU64`, `AtomicBool` | yes | wrong `Ordering` |
| Shared mutable struct, short sections | `Mutex<T>` | yes | poisoning, deadlock |
| Read-mostly shared struct | `RwLock<T>` | yes | writer starvation; measure first |

Use it only when the type's contract says `&self` operations have an internal side effect (cache, counter, connection pool). Never add a `RefCell` because a method "should" have been `&mut self` but the caller only has `&`: change the receiver or the caller.
**Do.**
```rust
use std::cell::OnceCell;

/// A document with a lazily computed word count.
///
/// Interior mutability (OWN-12): the cache fills on first read through
/// `&self`; observable state never changes.
#[derive(Debug)]
pub struct Document {
    text: String,
    words: OnceCell<usize>,
}

impl Document {
    /// Word count, computed once.
    pub fn word_count(&self) -> usize {
        *self
            .words
            .get_or_init(|| self.text.split_whitespace().count())
    }
}
```
**Detect.** Review: `Cell`/`RefCell`/`Mutex` field without a doc line saying why; `RefCell` introduced in the same diff as a borrow error.

### OWN-15 · Frozen data: `Box<str>`, `Box<[T]>`, `Arc<str>`, `Arc<[T]>`
**Why.** `String`/`Vec` carry a capacity word and invite mutation. `Arc<String>` is a double indirection (`clippy::rc_buffer`). For data that never grows, the boxed slice forms are one word smaller and `Arc<str>` clones are a refcount bump. Build them with `String::into_boxed_str`, `Vec::into_boxed_slice`, `Arc::from(s)` or `.collect::<Arc<[T]>>()`; an interned `Symbol(Arc<str>)` newtype is the common shape.
**Detect.** `clippy::rc_buffer`; review: `String`/`Vec` fields that are never mutated after construction and are cloned or shared widely.

### OWN-16 · `Copy` only for small value types
**Why.** `Copy` makes every use a silent memcpy and is a semver promise: removing it later breaks callers. Derive it for ids, coordinates, units and fieldless enums that will stay a few words. Pass such types by value (`clippy::trivially_copy_pass_by_ref`); pass large ones by reference (`large_types_passed_by_value`).
```rust
/// 8 bytes, value-like, will stay small: `Copy`, passed by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    x: i32,
    y: i32,
}

/// Manhattan distance.
#[must_use]
pub fn manhattan(a: Point, b: Point) -> u32 {
    a.x.abs_diff(b.x).saturating_add(a.y.abs_diff(b.y))
}
```

## Clone audit (H-08)

Every `.clone()`, `.to_owned()`, `.to_string()`, `.to_vec()` and `String::from`/`Vec::from` on borrowed data in a diff needs one of these reasons, visible from the code or a comment.

| Legitimate | Illegitimate |
|---|---|
| The callee stores it and only has a borrow (the map key on first insert, a field) | Silencing E0382/E0502/E0505/E0507 |
| Cheap handle: `Arc::clone(&x)`, `Rc::clone(&x)`, `Copy` types | Cloning `self` or a whole context to move one field into a closure |
| Real duplication: the copy diverges (snapshot, undo, test fixture) | Cloning a parameter that should have been owned (OWN-02) |
| Crossing an owned boundary: sending to a thread/task/channel that needs its own copy, and the source is still used | Returning a clone from a getter instead of `&T` (OWN-01) |
| Converting a borrowed input into an owned output the function returns | `.to_vec()` then iterate; `.clone()` then immediately drop the original |

Reviewer procedure for each clone:
1. What type is cloned? Refcount handles are fine but must be written `Arc::clone(&x)` (`clippy::clone_on_ref_ptr`, PERF-11).
2. Is the original used after the clone? If not, move it (`clippy::redundant_clone` catches the local cases).
3. Does the clone end up stored, sent or returned? If not, it was for the borrow checker: ask which OWN-08 shape applies.
4. Was a nearby signature changed to take `&T` when the body needs `T`? Fix the signature.
5. Is it on a hot path (loop, per request)? Then PERF-02 applies too.

Record each accepted non-obvious clone with a short comment (`// the map stores the key`).

## Graphs and cyclic data

Owning references cannot express cycles or parent links: `Box` is a tree only, `Rc` cycles leak, `Rc<RefCell<_>>` moves borrow checking to runtime panics. Store nodes in one `Vec` (the arena) and link them by index handles. Mutation goes through `&mut Graph`, so ordinary borrow rules apply and no interior mutability is needed.
```rust
use std::collections::VecDeque;

/// Handle to a node; only [`Graph::add`] creates one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

/// The handle does not belong to this graph.
#[derive(Debug, thiserror::Error)]
#[error("unknown node {0:?}")]
pub struct UnknownNode(NodeId);

#[derive(Debug)]
struct Node<T> {
    value: T,
    edges: Vec<NodeId>,
}

/// Directed graph: nodes in one `Vec`, edges are indices, cycles are free.
#[derive(Debug)]
pub struct Graph<T> {
    nodes: Vec<Node<T>>,
}

impl<T> Graph<T> {
    /// Adds a node and returns its handle.
    pub fn add(&mut self, value: T) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            value,
            edges: Vec::new(),
        });
        id
    }

    /// Adds an edge `from -> to`.
    ///
    /// # Errors
    /// [`UnknownNode`] if either handle is foreign.
    pub fn connect(&mut self, from: NodeId, to: NodeId) -> Result<(), UnknownNode> {
        if to.0 >= self.nodes.len() {
            return Err(UnknownNode(to));
        }
        let node = self.nodes.get_mut(from.0).ok_or(UnknownNode(from))?;
        node.edges.push(to);
        Ok(())
    }

    /// Plain `&mut` access through the arena: no `RefCell` needed.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        self.nodes.get_mut(id.0).map(|n| &mut n.value)
    }

    /// Nodes reachable from `start`, breadth first; terminates on cycles.
    #[must_use]
    pub fn reachable(&self, start: NodeId) -> Vec<NodeId> {
        let mut seen = vec![false; self.nodes.len()];
        let mut order = Vec::new();
        let mut queue = VecDeque::from([start]);
        while let Some(id) = queue.pop_front() {
            match seen.get_mut(id.0) {
                Some(flag) if !*flag => *flag = true,
                _ => continue,
            }
            order.push(id);
            if let Some(node) = self.nodes.get(id.0) {
                queue.extend(node.edges.iter().copied());
            }
        }
        order
    }
}

impl<T> Default for Graph<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}
```
Design notes: `NodeId`'s field is private so handles come only from `add`. This arena never removes nodes; if removal is needed, a stale handle must not alias a new node. Use generational indices (a `slotmap`-style crate is a new dependency, H-16) or tombstones. Lookups return `Option`/`Result`, never index (H-04).

## Review checklist
- [ ] Do read-only parameters borrow (`&str`, `&[T]`, `&Path`, `impl IntoIterator`) and stored ones own? (OWN-01, OWN-02)
- [ ] Any `&String`, `&Vec<T>`, `&Box<T>`, `&PathBuf`? (OWN-03)
- [ ] Does each receiver match query / mutate / consume? (OWN-04)
- [ ] Are named lifetimes necessary and meaningfully named; `'_` written in path types? (OWN-05)
- [ ] Do lifetime parameters appear only on short-lived views? (OWN-06)
- [ ] Any new `'static` bound, `Box::leak`, `mem::forget`, `static mut`, global? (OWN-07, H-07)
- [ ] Was each borrow error fixed with a shape rather than a clone? (OWN-08, H-08)
- [ ] Insert-or-update uses the entry API or a single `get_mut`? (OWN-09)
- [ ] Usually-borrowing transforms return `Cow`? (OWN-10)
- [ ] Every `Arc<Mutex>`, `RwLock`, `Rc`, `RefCell` has a written justification and is the lowest workable rung? (OWN-11)
- [ ] Interior mutability documented on the type, not patching a receiver? (OWN-12)
- [ ] Getters return slices or iterators, with correct `use<..>` captures? (OWN-13, MOD-06)
- [ ] `move` closures clone only the needed handle? (OWN-14)
- [ ] Frozen data uses `Box<str>`/`Arc<[T]>` where it helps; no `Arc<String>`? (OWN-15)
- [ ] `Copy` only on small value types? (OWN-16)
- [ ] Every clone passes the clone audit? (H-08)
