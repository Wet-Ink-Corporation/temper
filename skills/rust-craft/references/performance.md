# Performance and resources

Measuring, allocation, strings, data layout, collections, hashing, I/O, dispatch, the release profile, bounded resources, sharing and locks. Read it before touching a hot path, before claiming anything is faster or smaller, when choosing a collection or hasher, when writing bulk output, and when reviewing a diff that adds buffers, caches, locks or `Arc`s. Async-specific resource rules (channels, tasks, timeouts) are in `async-concurrency.md`.

Rules covered: PERF-01, PERF-02, PERF-03, PERF-04, PERF-05, PERF-06, PERF-07, PERF-08, PERF-09, PERF-10, PERF-11, PERF-12, PERF-13 (and TST-16)

Clear code comes first. A change that trades clarity for speed users will notice is a HITL decision (H-16); record the benchmark numbers in the plan.

## Measure first

### PERF-01 · Measure before optimizing (TST-16)
**Why.** Intuition about hot spots is wrong more often than right, and the optimizer already removes much of what looks expensive. An optimization without a before/after number is a readability cost with no proven benefit. TST-16 makes the benchmark part of the change. The plan records the numbers, but the code outlives the plan: code shaped for speed (a boxed variant, an `_into` form, a special-cased size, `cold_path`, a fast hasher) carries a why-comment naming the benchmark or profile, so the next change does not simplify it away.
**Do.** Put benchmarks in `benches/`, with `harness = false` in the crate's `Cargo.toml`. `divan` is the lighter choice; `criterion` gives statistical comparison against a saved baseline. Both are pre-approved (`.claude/temper.json`), so adding either as a dev-dependency needs no H-16 decision.
```toml
[dev-dependencies]
divan = { workspace = true }

[[bench]]
name = "tokenize"
harness = false
```
```rust,fragment
//! Benchmarks for `textkit::tokenize` (PERF-01, TST-16).

use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

#[divan::bench(args = [64, 4096])]
fn tokenize(bencher: Bencher<'_, '_>, words: usize) {
    let text = "Lorem ipsum, dolor sit amet! ".repeat(words / 5);
    bencher.bench(|| textkit::tokenize(black_box(&text)));
}
```
Build inputs outside the timed closure, wrap inputs in `black_box`, return the result so it is not optimized away, and bench at least two sizes to see the scaling.

| Question | Tool | Command |
|---|---|---|
| Is it faster? | divan / criterion | `cargo bench --bench tokenize`; criterion: `-- --save-baseline before`, then `-- --baseline before` |
| Did CI catch a regression? | instruction counts: Gungraun (formerly iai-callgrind) under Valgrind; dev-dependency, H-16 | `cargo bench --bench <name>`; wall-time results flap on shared runners, instruction counts do not |
| Is the whole program faster? | `hyperfine` | `hyperfine --warmup 3 'old/app input' 'target/release/app input'` |
| Where does time go? | `samply`, `perf`, `cargo flamegraph` | `RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling`, then `samply record target/profiling/app <args>` |
| Where do allocations come from? | `dhat` heap profiler | `cargo run --release --features dhat-heap`, open `dhat-heap.json` in the DHAT viewer (`dh_view.html`) |
| Did an allocation cut hold? | dhat testing mode (CONSIDER; dev-dependency, H-16) | one integration test file with `dhat::Alloc` as its allocator, `dhat::Profiler::builder().testing().build()`, assertions on `dhat::HeapStats::get()` |
| Why is the build slow? | Cargo timings | `cargo build --release --timings`, open `target/cargo-timings/cargo-timing.html` |
| Which generics bloat the binary? | `cargo llvm-lines`, `cargo bloat` | `cargo llvm-lines --release --bin app \| head -30` |

`dhat` behind a feature, so production builds keep the system allocator. Only a binary sets a `#[global_allocator]` (ARC-14):
```rust,fragment
//! Heap profiling entry point; `--features dhat-heap` enables the profiler.

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    // Dropping the profiler at the end of `main` writes dhat-heap.json.
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    let words = textkit::tokenize("one two three");
    drop(words);
}
```
**Detect.** Review: a diff described as "faster", "cheaper" or "optimized", or one that adds `unsafe`-free tricks (manual unrolling, `#[inline]`, custom hashers, `SmallVec`), without a benchmark or profile in the change or plan; optimized code without a why-comment.

## Allocation patterns

### PERF-02 · Allocate deliberately
**Why.** Each growth of a `Vec` or `String` reallocates and copies; a `collect()` that is only iterated again allocates for nothing; a fresh buffer per loop iteration turns one allocation into N; `dst = src.clone()` drops a buffer that could have been reused.
**Do.**
```rust
/// Appends the square of every value to `out`, so a caller can reuse one buffer (OWN-17).
pub fn squares_into(values: &[u32], out: &mut Vec<u64>) {
    out.extend(values.iter().map(|v| u64::from(*v) * u64::from(*v)));
}

/// Allocating form: pre-sized, a thin wrapper over [`squares_into`].
#[must_use]
pub fn squares(values: &[u32]) -> Vec<u64> {
    let mut out = Vec::with_capacity(values.len());
    squares_into(values, &mut out);
    out
}

/// Sums the byte lengths of normalized lines, reusing one scratch buffer.
#[must_use]
pub fn normalized_len<'a>(lines: impl IntoIterator<Item = &'a str>) -> usize {
    let mut scratch = String::new();
    let mut total = 0_usize;
    for line in lines {
        scratch.clear(); // keeps capacity
        scratch.extend(line.chars().filter(|c| !c.is_whitespace()));
        total = total.saturating_add(scratch.len());
    }
    total
}

/// Drops empty entries in place instead of building a filtered copy.
pub fn drop_empty(names: &mut Vec<String>) {
    names.retain(|n| !n.is_empty());
}

/// The last request seen, kept for error reports.
#[derive(Debug, Default)]
pub struct LastRequest {
    path: String,
    headers: Vec<(String, String)>,
}

impl LastRequest {
    /// Overwrites in place: `clone_into` reuses the existing buffers.
    pub fn record(&mut self, path: &str, headers: &[(String, String)]) {
        path.clone_into(&mut self.path);
        headers.clone_into(&mut self.headers);
    }
}
```
For an owned source the same reuse is `dst.clone_from(&src)`.
**Don't.**
```rust,ignore,expect=clippy::needless_collect+clippy::assigning_clones
# pub struct Order { id: u64 }
# pub struct Input<'a> { orders: &'a [Order], lines: &'a [&'a str], path: &'a str }
# pub struct Report { path: String, total: u64, lens: usize, kept: Vec<String> }
# impl Report {
#     pub fn update(&mut self, input: &Input<'_>, names: Vec<String>) {
#         let Input { orders, lines, path } = *input;
let ids: Vec<_> = orders.iter().map(|o| o.id).collect(); self.total = ids.into_iter().sum(); // needless_collect
for line in lines { let mut buf = String::new(); buf.push_str(line); self.lens += buf.len(); } // alloc per iteration
let kept: Vec<_> = names.into_iter().filter(|n| !n.is_empty()).collect();    // copy instead of retain
self.path = path.to_owned();                                                  // assigning_clones: drops the old buffer
#         self.kept = kept;
#     }
# }
```
**Detect.** `clippy::needless_collect` (nursery, enabled in the template), `assigning_clones` (pedantic), `manual_memcpy`, `extend_with_drain`; dhat's "total blocks" per call site; review allocations inside loops.
Also: `Vec::with_capacity` only when the size is known or bounded (never from untrusted input without a cap, PERF-10); `shrink_to_fit` only after a large, lasting shrink; `extend` over repeated `push`; `drain(..)`/`mem::take` to reuse storage. A hot-path API that produces a buffer offers the caller-owned `*_into` form, and the allocating form wraps it (OWN-17).

### PERF-03 · Iterators and adapters over index loops
**Why.** Iterators carry their own bounds, so the compiler drops the per-index checks, and they express the intent (`windows`, `as_chunks`, `zip`) that index arithmetic hides. Index loops also collide with H-04.
**Do.** `for (a, b) in xs.iter().zip(ys)`, `xs.windows(2)`, `slice::array_windows::<2>()`, `iter().rev()`, `retain`, `dedup_by_key`, `extend`. For a constant chunk size use `as_chunks::<N>()` (1.88): it yields arrays plus the remainder, so `from_le_bytes(*w)` needs no conversion, and `chunks_exact(4)` with a literal fails the gate (`clippy::chunks_exact_to_as_chunks`, style). Keep `chunks_exact(n)` plus `.remainder()` for a runtime `n`.

To remove bounds checks from a loop, slice once up front with a fallible form (`get(..n)`, `split_at_checked`, `first_chunk::<N>()`, a slice pattern) and iterate the result. Never hoist the check with `assert!` (H-03), and leave `clippy::missing_asserts_for_indexing` off: it asks for exactly those asserts. When a profile shows small inputs dominate, branch on slice patterns (`[]`, `[x]`, `[a, b]`, `_`) before the general path; unlike `if v.len() == 1 { v[0] }`, they need no index.
```rust
use std::borrow::Cow;

/// Sums the little-endian `u32` words of `bytes`; a trailing partial word is ignored.
#[must_use]
pub fn sum_words(bytes: &[u8]) -> u64 {
    let (words, _tail) = bytes.as_chunks::<4>();
    words.iter().fold(0_u64, |acc, w| {
        acc.saturating_add(u64::from(u32::from_le_bytes(*w)))
    })
}

/// Dot product of the first `n` elements; `None` if either slice is shorter
/// or the sum overflows.
#[must_use]
pub fn dot(a: &[u32], b: &[u32], n: usize) -> Option<u64> {
    // Slice once, fallibly: the loop below has no per-element bounds checks.
    let (Some(a), Some(b)) = (a.get(..n), b.get(..n)) else {
        return None;
    };
    a.iter().zip(b).try_fold(0_u64, |acc, (x, y)| {
        acc.checked_add(u64::from(*x) * u64::from(*y))
    })
}

/// Joins names for a log line.
#[must_use]
pub fn join_names<'a>(names: &[&'a str]) -> Cow<'a, str> {
    // The request profile shows most calls pass zero or one name, so those
    // cases borrow instead of allocating (PERF-01).
    match names {
        [] => Cow::Borrowed(""),
        [one] => Cow::Borrowed(*one),
        _ => Cow::Owned(names.join(", ")),
    }
}
```
**Don't.**
```rust,ignore,expect=clippy::needless_range_loop+clippy::indexing_slicing+clippy::chunks_exact_to_as_chunks+scan:assert-in-prod
# pub fn dot(xs: &[u32], ys: &[u32]) -> u32 { let mut total = 0;
for i in 0..xs.len() { total += xs[i] * ys[i]; } // indexing_slicing (H-04): a bounds check per step; zip instead
# total }
# pub fn prefix_sum(a: &[u32], n: usize) -> u32 { let mut s = 0;
assert!(a.len() >= n, "a is too short"); for i in 0..n { s += a[i]; } // H-03 panic path to hoist the check; needless_range_loop, H-04
# s }
# pub fn words(bytes: &[u8]) -> usize { let mut words = 0;
for w in bytes.chunks_exact(4) { words += w.len() / 4; } // chunks_exact_to_as_chunks: use as_chunks::<4>()
# words }
```
**Detect.** `clippy::needless_range_loop`, `indexing_slicing`, `explicit_counter_loop`, `chunks_exact_to_as_chunks`.

## Strings and formatting

### PERF-04 · No needless string building
**Why.** `format!` and `to_string()` allocate a fresh `String` every call; `push_str(&format!(..))` allocates twice. `Display` impls that build a `String` and then write it pay for the same bytes twice. Write into the sink you already have.
**Do.**
```rust
use std::fmt::{self, Write as _};

/// One output row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    id: u32,
    name: String,
}

impl fmt::Display for Row {
    // Writes straight into the formatter's sink: no intermediate String.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\t{}", self.id, self.name)
    }
}

/// Renders rows into a sink; pass `&mut buf` to reuse a buffer (OWN-01, OWN-17).
///
/// # Errors
/// Propagates any error from the sink.
pub fn render_into(mut out: impl fmt::Write, rows: &[Row]) -> fmt::Result {
    for row in rows {
        writeln!(out, "{row}")?;
    }
    Ok(())
}

/// Builds a key with one `write!` into a pre-sized buffer.
///
/// # Errors
/// Never for `String` in practice; propagated rather than discarded (H-10).
pub fn cache_key(tenant: &str, id: u32) -> Result<String, fmt::Error> {
    let mut key = String::with_capacity(tenant.len().saturating_add(11));
    write!(key, "{tenant}:{id}")?;
    Ok(key)
}
```
**Don't.**
```rust,ignore,expect=clippy::format_push_string+clippy::recursive_format_impl
# use std::fmt;
# pub struct Row { id: u32 }
# fn log_line(_line: &str) {}
# pub fn key(out: &mut String, tenant: &str, id: u32) {
out.push_str(&format!("{tenant}:{id}"));                 // format_push_string: two allocations
log_line(&format!("{id}")); // in a hot loop: pass `id` and format at the sink
# }
# impl fmt::Display for Row {
fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.to_string()) } // recursion
# }
```
**Detect.** `clippy::format_push_string`, `format_collect`, `to_string_in_format_args`, `inefficient_to_string`, `unnecessary_to_owned`, `or_fun_call` (nursery, enabled in the template: `ok_or(Error::Missing(format!(..)))` builds the string even on success).
Also: for one-off `Display` adapters, `std::fmt::from_fn` (1.93) avoids a wrapper type; it needs `rust-version` 1.93 or later (the template sets 1.96). Return `&str`/`Cow<'_, str>` instead of `String` when input usually passes through (OWN-10). `tracing` fields format lazily, so do not pre-format log messages.

Logging follows the same rule. Gate log-only work (a summary, a sort, a join) with `tracing::enabled!`, and record one event per batch, never one per item in a hot loop:
```rust
use tracing::Level;

/// Applies a batch: one event per batch, and the log-only scan runs only when DEBUG is on.
pub fn apply_batch(batch: &[u64]) -> u64 {
    let total = batch.iter().fold(0_u64, |acc, v| acc.saturating_add(*v));
    if tracing::enabled!(Level::DEBUG) {
        let largest = batch.iter().max();
        tracing::debug!(count = batch.len(), total, ?largest, "batch applied");
    }
    total
}
```
```rust,ignore
for v in batch { tracing::debug!(value = %format!("{v:x}"), "applied"); } // per-item event and an eager format!
```

## Data layout

### PERF-05 · Keep hot types small
**Why.** An enum is as large as its largest variant, so one rare 256-byte variant makes every `Vec<Message>` element, every move and every channel slot 256 bytes. Values over 128 bytes are moved with `memcpy`, so hot types (moved per message, stored in large collections, sent over channels) stay at or below 128 bytes on 64-bit targets. `Vec<Box<T>>` adds an allocation and a pointer chase per element for no gain.
**Do.** Box the rare, large variant and pin the size with a compile-time assertion so regressions fail the build. Use a `<=` bound that holds on every target (pointers are 4 bytes on 32-bit), or gate an exact `==` with `#[cfg(target_pointer_width = "64")]`, and give it a message:
```rust
/// A large, rarely used payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    state: [u8; 256],
}

/// A hot message type; the rare large variant is boxed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Keep-alive.
    Ping,
    /// Small data.
    Value(u64),
    /// Full state, sent rarely.
    // Boxed so every `Message` stays two words.
    Snapshot(Box<Snapshot>),
}

// Fails the build if someone adds a fat variant.
const _: () = assert!(
    size_of::<Message>() <= 16,
    "Message grew: box the new variant (PERF-05)"
);
```
A `const` assertion cannot name an `async fn`'s future. Pin hot futures (thousands in flight) with a test, and drop or scope values that are not needed after an `.await` so they are not stored in the future:
```rust
/// Handles one request; thousands run at once.
pub async fn handle(id: u64) -> u64 {
    tokio::task::yield_now().await;
    id.saturating_add(1)
}

#[cfg(test)]
mod tests {
    #[test]
    fn handle_future_stays_small() {
        let size = size_of_val(&super::handle(7));
        assert!(size <= 64, "handle's future is {size} bytes (PERF-05)");
    }
}
```
**Don't.**
```rust,ignore,expect=clippy::large_enum_variant+clippy::vec_box
# use std::sync::Arc;
# pub struct Item(u64);
# pub struct Config;
pub enum Message { Ping, Snapshot([u8; 256]) } // large_enum_variant: every Ping is 257 bytes
pub struct Batch { items: Vec<Box<Item>> }     // vec_box: extra alloc + indirection per element
pub struct Client { inner: Arc<Inner> }        // with `Inner` below: two pointer hops per read;
pub struct Inner { config: Arc<Config> }       // lift hot fields up
```
**Detect.** `clippy::large_enum_variant`, `vec_box`, `box_collection`, `large_futures`, `large_stack_arrays`; `size_of` assertions and future-size tests; review: an `Arc` inside a type that is itself shared through `Arc`, unless both levels are shared independently; `-Zprint-type-sizes` on nightly only when investigating.
Layout facts: `repr(Rust)` reorders fields to minimize padding, so do not hand-order for size. Niches are free: `Option<Box<T>>`, `Option<&T>`, `Option<NonZero<u32>>` are the size of the inner type. Use `u32` indices instead of `usize` in large index-heavy structures only with measurement, converting without `as` (ownership.md §Graphs). Struct-of-arrays beats array-of-structs when a hot loop reads one field of many elements.

## Collections choice

### PERF-06 · Choose by access pattern
**Why.** The collection fixes the complexity of every operation and the memory layout. The default is `Vec`: contiguous, cache-friendly, and faster than "better" structures up to surprisingly large sizes.

| Access pattern | Use | Notes |
|---|---|---|
| Sequence, append, iterate, index | `Vec<T>` | default; pre-size when known |
| Small set/map (< ~32), rarely changed | `Vec<(K, V)>` or sorted `Vec` + `binary_search` | often beats hashing; measure |
| Queue, push/pop at both ends | `VecDeque<T>` | ring buffer; `pop_front_if` (1.93) |
| Key lookup, order irrelevant | `HashMap<K, V>` / `HashSet<T>` | default SipHash; see §Hashing |
| Key lookup, order observable (output, iteration, ranges) | `BTreeMap<K, V>` / `BTreeSet<T>` | deterministic output (OUT-04), `range(..)` |
| Priority / top-k | `BinaryHeap<T>` (`Reverse` for min-heap) | |
| Frozen after build | `Box<[T]>`, `Arc<[T]>` | drops capacity word (OWN-15) |
| Stable ids with removal | arena `Vec` + generational index | new crate is H-16 |
| Inline small vectors | `smallvec`/`arrayvec` | dependency (H-16) plus a benchmark |
| Linked list | not used | `LinkedList` loses to `Vec`/`VecDeque` in nearly every workload |

**Detect.** Review: `HashMap` iterated into user-visible output (non-deterministic; OUT-04); `Vec::contains` or `remove(0)` inside a loop over a large collection; `LinkedList` anywhere.

## Hashing

### Faster hashers only for keys an attacker cannot control (PERF-06)
**Why.** std `HashMap` uses randomly keyed SipHash-1-3, which resists HashDoS: an attacker who controls keys (HTTP headers, JSON object keys, user names) cannot force every insert into one bucket. Faster non-cryptographic hashers (foldhash, FxHash) are much faster on small keys but weaken or drop that guarantee (FxHash is unseeded). Use them only for keys the program generates (interned ids, internal indices), only with a benchmark, and adding the crate is H-16.
**Do.** Once a workspace adopts a fast hasher, ban the std types in `clippy.toml` (a protected file, H-05) in the same change, so nothing uses them by accident. Use the hasher crate's own aliases: a local `type FastMap<K, V> = HashMap<K, V, ..>` names the banned type and is itself flagged.
```toml
# clippy.toml, in the change that adopts foldhash (with its `std` feature)
disallowed-types = [
  { path = "std::collections::HashMap", reason = "PERF-06: keys are process-generated; use foldhash::HashMap" },
  { path = "std::collections::HashSet", reason = "PERF-06: use foldhash::HashSet" },
]
```
```rust,fragment
use foldhash::{HashMap, HashMapExt as _};

/// Counts occurrences of each interned id.
#[must_use]
pub fn count_ids(ids: &[u32]) -> HashMap<u32, u32> {
    // Interned ids are process-generated, never attacker-controlled, and
    // benches/ids.rs shows hashing dominating with SipHash (PERF-01, PERF-06).
    let mut counts = HashMap::with_capacity(ids.len());
    for id in ids {
        let n = counts.entry(*id).or_insert(0_u32);
        *n = n.saturating_add(1);
    }
    counts
}
```
**Don't.**
```rust,ignore
let headers: FxHashMap<String, String> = ..; // attacker-chosen keys, unseeded fast hash: HashDoS
```
**Detect.** Review: any non-default `BuildHasher` needs the benchmark and a comment on key provenance; once adopted, `clippy::disallowed_types` (style, denied) enforces the ban. Cheaper than a new hasher: avoid hashing at all (index handles, sorted `Vec`), hash once with the entry API (OWN-09), and look up `String` keys by `&str`.

## I/O

### PERF-07 · Buffer, lock once, stream
**Why.** Unbuffered `File` reads and writes are one syscall each. `stdout` is line-buffered and locks per `println!`, so bulk output pays a lock and often a syscall per line. `read_to_string` on a large or untrusted file holds the whole file in memory, and so does `lines()` on a single newline-free line. And `println!` panics when stdout is closed (`app | head`), which violates H-03; `writeln!` returns the error to `?`.
**Do.** Read lines into one reused buffer through a per-line cap (PERF-10); `lines()` allocates a `String` per line and has no cap.
```rust
use std::fs::File;
use std::io::{self, BufRead, BufWriter, Read, Write};
use std::path::Path;

/// Longest accepted line, in bytes (PERF-10).
pub const MAX_LINE: u64 = 64 * 1024;

/// Reading lines failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LineError {
    /// A line reached the cap without a newline.
    #[error("line exceeds {limit} bytes")]
    TooLong {
        /// The cap in bytes.
        limit: u64,
    },
    /// Underlying I/O failed.
    #[error("failed to read input")]
    Io(#[from] io::Error),
}

/// Counts non-empty lines. Memory stays constant in input size and line length.
///
/// # Errors
/// [`LineError::TooLong`] past the cap, [`LineError::Io`] on read failure.
pub fn count_lines(mut reader: impl BufRead) -> Result<usize, LineError> {
    let mut line = String::new();
    let mut count = 0_usize;
    loop {
        line.clear(); // one buffer for the whole input
        let n = (&mut reader).take(MAX_LINE).read_line(&mut line)?;
        if n == 0 {
            return Ok(count);
        }
        if !line.ends_with('\n') && u64::try_from(n).map_or(true, |n| n >= MAX_LINE) {
            return Err(LineError::TooLong { limit: MAX_LINE });
        }
        if !line.trim().is_empty() {
            count = count.saturating_add(1);
        }
    }
}

/// Writes one record per line through a buffer: one syscall per ~8 KiB.
///
/// # Errors
/// Returns the I/O error from creating, writing or flushing.
pub fn write_records(path: &Path, records: &[(u32, &str)]) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    for (id, name) in records {
        writeln!(out, "{id}\t{name}")?;
    }
    // Flush explicitly: `Drop` would swallow the error.
    out.flush()
}
```
Bulk stdout in a binary: lock once, buffer, `writeln!` with `?`, and treat a closed pipe as a normal end of output.
```rust
//! Prints a report to stdout; exits cleanly when piped into `head`.

use std::io::{self, BufWriter, Write};

fn write_report(out: impl Write) -> io::Result<()> {
    let mut out = BufWriter::new(out);
    for n in 0..100_000_u32 {
        writeln!(out, "line {n}")?;
    }
    out.flush()
}

fn main() -> io::Result<()> {
    // `println!` panics on a closed pipe; `writeln!` returns the error to `?`.
    match write_report(io::stdout().lock()) {
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}
```
**Don't.**
```rust,ignore,expect=clippy::print_stdout+clippy::unbuffered_bytes+scan:print
# use std::fs::{self, File};
# use std::io::{self, BufRead, Read, Write};
# use std::path::Path;
# pub fn all(records: &[u32], untrusted_path: &Path, reader: impl BufRead, p: &Path) -> io::Result<usize> {
for r in records { println!("{r}"); }             // lock + line flush per row; panics on EPIPE
let text = fs::read_to_string(untrusted_path)?;   // unbounded memory (PERF-10)
let mut n = 0; for line in reader.lines() { n += line?.len(); } // a String per line, no length cap
let mut f = File::create(p)?; for r in records { writeln!(f, "{r}")?; } // syscall per line
for b in File::open(p)?.bytes() { n += usize::from(b? == b'\n'); } // unbuffered_bytes: a syscall per byte
#     Ok(n + text.len())
# }
```
**Detect.** `clippy::print_stdout` (denied), `unbuffered_bytes` (perf, denied: `.bytes()` on an unbuffered reader); review: `File` used without `BufReader`/`BufWriter` in a loop; `BufWriter` dropped without `flush()`; whole-file reads or `lines()` on inputs with no size bound.
Also: byte-oriented input skips UTF-8 validation with `read_until(b'\n', &mut Vec<u8>)` on the same capped reader. Library functions take `impl Read`/`impl Write` by value (OWN-01), so the caller decides buffering and locking and passes `&mut w` to keep the writer. Async I/O follows the same rules with `tokio::io::BufWriter`.

## Dispatch, generics and inlining

### PERF-08 · Static dispatch for hot code; `#[inline]` only with evidence
**Why.** Generics monomorphize: each concrete type gets its own copy, which lets the optimizer inline and specialize, at the cost of compile time and binary size. `dyn Trait` is one copy with an indirect call that blocks inlining. Choose per call site: hot, homogeneous paths go generic; heterogeneous collections, plug-ins and cold paths go `dyn` (API-09).
**Do.** Keep generic wrappers thin: convert at the edge and hand off to a non-generic inner function, so the real body is compiled once instead of once per caller type.
```rust
use std::fs;
use std::io;
use std::path::Path;

/// Reads a config file; generic at the edge, one monomorphic body inside.
///
/// # Errors
/// Returns the I/O error from reading.
pub fn read_config(path: impl AsRef<Path>) -> io::Result<String> {
    fn inner(path: &Path) -> io::Result<String> {
        let text = fs::read_to_string(path)?;
        Ok(text.trim().to_owned())
    }
    inner(path.as_ref())
}

/// A pricing rule.
pub trait Rule {
    /// Adjusts a price in cents.
    fn apply(&self, cents: u64) -> u64;
}

/// Hot loop: generic, so the call is inlined per rule type.
#[must_use]
pub fn apply_all<R: Rule>(rule: &R, prices: &[u64]) -> Vec<u64> {
    prices.iter().map(|p| rule.apply(*p)).collect()
}

/// Heterogeneous rules configured at startup: `dyn` is the right tool.
#[must_use]
pub fn apply_chain(rules: &[Box<dyn Rule>], cents: u64) -> u64 {
    rules.iter().fold(cents, |acc, rule| rule.apply(acc))
}
```
Mark a rare branch with `std::hint::cold_path()` (1.95) or move it into a `#[cold]` function. For a hot/cold split, put `#[inline]` on the small hot function and `#[inline(never)]` on the cold one. `#[inline(always)]` is denied by `clippy::inline_always` (pedantic); keeping one is an H-05 request with a benchmark.
```rust
/// A status byte other than zero.
#[derive(Debug, thiserror::Error)]
#[error("bad status code {code}")]
pub struct BadCode {
    code: u8,
}

/// Decodes a status byte. Errors are rare, so the branch is marked cold.
///
/// # Errors
/// [`BadCode`] for any non-zero status.
pub fn decode(code: u8, value: u32) -> Result<u32, BadCode> {
    if code == 0 {
        Ok(value)
    } else {
        std::hint::cold_path();
        Err(BadCode { code })
    }
}
```
**Don't.**
```rust,ignore,expect=clippy::inline_always
# use std::io::{self, Read};
# pub trait Rule { fn apply(&self, cents: u64) -> u64; }
#[inline(always)] pub fn parse_everything<R: Read>(mut r: R) -> io::Result<u64> { /* 200 lines */ io::copy(&mut r, &mut io::sink()) } // inline_always: bloat, no benchmark
fn apply_all(rules: &dyn Rule, prices: &[u64]) -> Vec<u64> { prices.iter().map(|p| rules.apply(*p)).collect() } // dyn in the innermost hot loop without measurement
```
**Detect.** `clippy::inline_always` (pedantic, denied by the gate); `cargo llvm-lines` for the functions with the most monomorphized copies; review `#[inline]` on large or crate-private functions (useless: generic and crate-local functions are already inlinable). `#[inline]` helps small non-generic `pub` functions called across crates when LTO is off; cite the benchmark. `cold_path` and `#[cold]` are measured too.

## Release profile

### PERF-09 · Keep the template profiles; tune per application with evidence
**Why.** Profile settings trade build time, binary size, debuggability and panic behavior for speed. The template's profiles suit almost every workspace. Anything beyond them is a per-application H-16 decision with a benchmark, recorded in the root `AGENTS.md` decisions log with the reason and the measurement.

The template's profiles:
```toml
[profile.release]
lto = "thin"
codegen-units = 1

# `cargo build --profile profiling`: release code with file:line for profilers.
[profile.profiling]
inherits = "release"
debug = "line-tables-only"

# `cargo bench`: source lines in profiler output.
[profile.bench]
debug = "line-tables-only"

# Faster dev and test builds; panics still report file:line.
# To use a debugger locally: CARGO_PROFILE_DEV_DEBUG=true.
[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false
```

| Profile | Setting | Why |
|---|---|---|
| `release` | `lto = "thin"`, `codegen-units = 1` | cross-crate inlining and better optimization; costs only release build time |
| `profiling` | `inherits = "release"`, `debug = "line-tables-only"` | release code that profilers can map to source; `release` itself stays lean |
| `bench` | `debug = "line-tables-only"` | source lines in profiles of `cargo bench` |
| `dev` | `debug = "line-tables-only"`, dependencies `debug = false` | smaller `target/` and faster builds; agents never attach a debugger |

Per-application decisions (H-16, benchmark, decisions-log entry):

| Setting | Effect | Cost / caveat |
|---|---|---|
| `lto = "fat"` | more cross-crate optimization | much slower link; not always faster |
| `opt-level = "s"` / `"z"` | smaller binary | slower code; size-constrained targets only |
| `panic = "abort"` | smaller binary, no landing pads | `Drop` does not run on panic (no `BufWriter` flush), `JoinHandle::join` never sees one; tests still unwind. Keep unwind for servers |
| `strip = "symbols"` | smaller binary | no symbols in backtraces; release already has no debug sections |
| `#[global_allocator]` (mimalloc, jemalloc) | faster allocation-heavy workloads | binaries only, in the composition root (ARC-14): a library that sets one breaks every binary that sets another; `cfg`-gate to supported targets |
| PGO / BOLT (`cargo-pgo`) | faster branchy code | a build-pipeline change |
| `-C target-cpu=<level>` | newer instructions | a named level (`x86-64-v3`) for a known server fleet, in `.cargo/config.toml` for that target; never `native` in repo config (SIGILL on older CPUs); never for distributed CLIs or libraries |
| `overflow-checks = true` in `release` | overflow panics instead of wrapping silently | CONSIDER for money, balance and counter-heavy domains as a backstop; checked arithmetic (ERR-09) stays mandatory |

Profiles live only in the root manifest: Cargo ignores a member's `[profile.*]` with a warning that `build.warnings = "deny"` does not catch, so the hook denies it. Shipped profiles keep `debug-assertions = false` (the release default): turning it on makes every `debug_assert!` a production panic (H-03).
**Detect.** Hook (member profiles); review: profile changes without a decisions-log entry; `panic = "abort"` in a workspace whose code relies on `join()` errors or `Drop` for cleanup; `debug-assertions = true` in a shipped profile; `target-cpu=native` in committed config; `#[global_allocator]` in a library. Editing profiles in a shared workspace is an H-16 decision.

## Bounded resources

### PERF-10 · Bound everything that grows with input (H-13)
**Why.** Every buffer, collection, recursion or queue sized by input is a memory or stack exhaustion bug waiting for a large or hostile input. Bounds turn "OOM kill" into a typed error the caller can handle.
**Do.**
```rust
use std::io::{self, Read};

/// Reading a bounded input failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// Input exceeded the cap.
    #[error("input exceeds {limit} bytes")]
    TooLarge {
        /// The cap in bytes.
        limit: u64,
    },
    /// Underlying I/O failed.
    #[error("failed to read input")]
    Io(#[from] io::Error),
}

/// Reads at most `limit` bytes; rejects larger input instead of growing forever.
///
/// # Errors
/// [`ReadError::TooLarge`] past the cap, [`ReadError::Io`] on read failure.
pub fn read_capped(reader: impl Read, limit: u64) -> Result<Vec<u8>, ReadError> {
    let mut buf = Vec::new();
    // One extra byte distinguishes "exactly limit" from "more than limit".
    reader.take(limit.saturating_add(1)).read_to_end(&mut buf)?;
    if u64::try_from(buf.len()).map_or(true, |n| n > limit) {
        return Err(ReadError::TooLarge { limit });
    }
    Ok(buf)
}

/// Maximum nesting accepted by [`depth`].
pub const MAX_DEPTH: usize = 128;

/// Bracket nesting that [`depth`] rejects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DepthError {
    /// A `]` without a matching `[`, or a `[` never closed.
    #[error("unbalanced brackets")]
    Unbalanced,
    /// Nesting deeper than [`MAX_DEPTH`].
    #[error("brackets nest deeper than {}", MAX_DEPTH)]
    TooDeep,
}

/// Nesting depth of brackets.
///
/// # Errors
/// [`DepthError::Unbalanced`] or [`DepthError::TooDeep`].
pub fn depth(input: &str) -> Result<usize, DepthError> {
    let mut current = 0_usize;
    let mut max = 0_usize;
    for c in input.chars() {
        match c {
            '[' => {
                current = current
                    .checked_add(1)
                    .filter(|d| *d <= MAX_DEPTH)
                    .ok_or(DepthError::TooDeep)?;
                max = max.max(current);
            }
            ']' => current = current.checked_sub(1).ok_or(DepthError::Unbalanced)?,
            _ => {}
        }
    }
    if current == 0 {
        Ok(max)
    } else {
        Err(DepthError::Unbalanced)
    }
}
```

| Grows with input | Bound |
|---|---|
| Bytes read | `Read::take(limit)`, body-size limits in HTTP frameworks, `serde_json` from a capped buffer |
| Line length | `take(MAX_LINE).read_line(&mut buf)` into one reused buffer (PERF-07) |
| Pre-allocation from a length prefix | `Vec::with_capacity(declared.min(MAX))`; never trust the declared length |
| Collections, caches | capacity limit plus eviction (LRU, TTL); reject when full |
| Recursion | explicit depth counter, or an explicit stack (`Vec`) instead of recursion |
| Concurrency | `tokio::sync::Semaphore`, `buffer_unordered(n)`, fixed worker pools (`async-concurrency.md`) |
| Queues | bounded channels (ASY-07); backpressure or reject when full |
| External calls | timeouts (ASY-08) |

**Detect.** Review: `read_to_end`/`read_to_string`/`collect` on untrusted input without a cap; `with_capacity(n)` where `n` comes from input; recursive functions over user data without a depth limit; `unbounded_channel`; caches without eviction. Limits are named constants or config (CRF-09).

## Sharing large immutable data

### PERF-11 · `Arc::clone(&x)` and frozen shared buffers
**Why.** Deep-cloning large read-only data per worker or per request multiplies memory and time. `Arc<[T]>`/`Arc<str>` share one allocation, and a clone is an atomic increment. Writing `Arc::clone(&x)` instead of `x.clone()` makes the cheap clone visible, so reviewers can tell it from a deep copy (H-08).
**Do.**
```rust
use std::io;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Immutable lookup data loaded once and shared by every worker.
#[derive(Debug, Clone)]
pub struct Catalog {
    names: Arc<[Arc<str>]>,
}

impl Catalog {
    /// Freezes the names; after this nothing reallocates.
    #[must_use]
    pub fn new(names: Vec<String>) -> Self {
        Self {
            names: names.into_iter().map(Arc::from).collect(),
        }
    }
}

/// The worker gets a refcount bump, not a deep copy. The caller keeps and joins the handle (ASY-15).
///
/// # Errors
/// Returns the OS error if the thread cannot be started.
pub fn spawn_worker(catalog: &Catalog, id: usize) -> io::Result<JoinHandle<usize>> {
    thread::Builder::new().name(format!("worker-{id}")).spawn({
        let names = Arc::clone(&catalog.names);
        move || names.len()
    })
}
```
**Don't.**
```rust,ignore,expect=clippy::clone_on_ref_ptr+clippy::rc_buffer
# use std::sync::Arc;
# #[derive(Clone)] pub struct Config;
# pub struct Catalog { names: Arc<[Arc<str>]> }
# pub fn per_request(catalog: &Catalog, config: &Config) -> (Arc<[Arc<str>]>, Arc<Config>) {
let names = catalog.names.clone();   // clone_on_ref_ptr: is this a refcount or a deep copy?
let cfg = Arc::new(config.clone());   // per request: build the Arc once at startup
#     (names, cfg)
# }
pub struct Shared { data: Arc<Vec<u8>> } // rc_buffer: double indirection, use Arc<[u8]>
```
**Detect.** `clippy::clone_on_ref_ptr`, `rc_buffer`, `redundant_clone`. For byte buffers sliced and passed between network stages, `bytes::Bytes` gives cheap sub-slicing (dependency, H-16).

## Locks and contention

### PERF-12 · Short critical sections; nothing blocks under a lock
**Why.** Everything done while holding a lock serializes every other thread that wants it, and I/O under a lock turns one slow disk or peer into a global stall. Take the lock, do the minimum (a push, a swap, a copy), release, then do the slow work. Drop every guard before anything that blocks: I/O, `thread::sleep`, `JoinHandle::join`, blocking channel `send`/`recv`, waiting on another lock, and calls into unknown code (callbacks, `dyn Trait`). A join or `recv` under a lock deadlocks when the other side needs that lock, and an `.await` under a lock is a deadlock risk (H-14).
**Do.**
```rust
use std::io::{self, Write};
use std::mem;
use std::sync::{Mutex, PoisonError};

/// Log lines buffered by many threads and flushed by one.
#[derive(Debug, Default)]
pub struct LogBuffer {
    lines: Mutex<Vec<String>>,
}

impl LogBuffer {
    /// Appends a line; the critical section is one `push`.
    pub fn push(&self, line: String) {
        // A Vec of lines stays valid after a panic elsewhere.
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }

    /// Swaps the buffer out under the lock, then writes with the lock released.
    ///
    /// # Errors
    /// Returns the sink's I/O error. On a write error the failed line and the
    /// rest of the batch go back to the front of the buffer, so a retry may
    /// repeat a partly written line. Lines the sink accepted are its own, even
    /// when the final flush fails.
    pub fn flush_to(&self, mut sink: impl Write) -> io::Result<()> {
        let batch = mem::take(&mut *self.lines.lock().unwrap_or_else(PoisonError::into_inner));
        let written = batch
            .iter()
            .enumerate()
            .try_for_each(|(i, line)| writeln!(sink, "{line}").map_err(|err| (i, err)));
        if let Err((failed, err)) = written {
            self.requeue(batch.into_iter().skip(failed));
            return Err(err);
        }
        sink.flush()
    }

    // Puts unsent lines back ahead of any pushed since the swap.
    fn requeue(&self, unsent: impl Iterator<Item = String>) {
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        let newer = mem::take(&mut *lines);
        lines.extend(unsent);
        lines.extend(newer);
    }
}

#[derive(Debug, Default)]
struct Counts {
    hits: u64,
    total: u64,
}

/// Cache statistics; every snapshot satisfies `hits <= total`.
#[derive(Debug, Default)]
pub struct CacheStats {
    // Both fields under one lock: the invariant spans them (ASY-14).
    counts: Mutex<Counts>,
}

impl CacheStats {
    /// Records one lookup.
    pub fn record(&self, hit: bool) {
        // Nothing in the section can panic, so poison never leaves the pair torn.
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        counts.total = counts.total.saturating_add(1);
        if hit {
            counts.hits = counts.hits.saturating_add(1);
        }
    }

    /// A consistent `(hits, total)` snapshot.
    pub fn snapshot(&self) -> (u64, u64) {
        let counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        (counts.hits, counts.total)
    }
}
```
**Don't.**
```rust,ignore
let lines = self.lines.lock()?;
for l in lines.iter() { writeln!(file, "{l}")?; }   // I/O while every producer waits
let guard = state.lock()?; worker.join()?;         // join under a lock: the worker may need it
let r = cache.read()?; if r.get(&k).is_none() { cache.write()?.insert(k, v); } // upgrade while `r` lives: deadlock
hits: Mutex<u64>, total: Mutex<u64>,               // one invariant, two locks: readers see it broken
```
```rust,ignore,expect=clippy::await_holding_lock+clippy::future_not_send
# use std::future::{Ready, ready};
# use std::sync::{Mutex, PoisonError};
# pub struct State(Vec<u8>);
# fn send(_state: &State) -> Ready<()> { ready(()) }
# pub async fn publish(state: &Mutex<State>) {
let guard = state.lock().unwrap_or_else(PoisonError::into_inner);
send(&guard).await; // H-14: guard held across .await, and the future is no longer `Send`
# }
```
**Detect.** `clippy::significant_drop_tightening`, `significant_drop_in_scrutinee` (nursery, enabled in the template), `await_holding_lock`; review: any blocking call, allocation-heavy work, or call into unknown code inside a guard's scope; a read guard alive where `write()` is taken; fields that must agree split across locks.

Under contention, in order:
1. Shrink the critical section.
2. Split the lock, but only between fields with no shared invariant: fields that must agree stay under one lock (`CacheStats` above, ASY-14).
3. Shard: a fixed `Box<[Mutex<HashMap<..>>]>`, indexed by `hash % shard_count` with a `NonZero<usize>` count, then `get`.
4. Give each worker its own counter and sum on read. One atomic written by every core is itself a contention point; pad the per-worker counters (PERF-13).
5. Move the state into an owner task and message-pass (ASY-09).

Default to `Mutex`. Use `RwLock` only when reads dominate, read sections are non-trivial, and a benchmark shows the gain. Its priority policy is OS-dependent and not guaranteed (writer-preferring on Linux, Windows and macOS today), so a second `read()` while holding one can deadlock behind a waiting writer. Never upgrade in place: drop the read guard, take `write()`, and re-check the condition. `RwLockWriteGuard::downgrade` (1.92) goes the other way. Replacing std `Mutex` with `parking_lot` is a dependency decision (H-16) that needs a benchmark.

### PERF-13 · Pad contended independent values to their own cache line
When a benchmark shows contention on independent atomics or small locks stored side by side, pad each to its own cache line (`#[repr(align(128))]`) and assert the alignment at compile time. Never pad without a benchmark.
**Why.** Cores share memory in cache-line units. Two counters written by different threads on the same line bounce that line between cores (false sharing), which can cost several times the uncontended speed even though no value is shared. 128 bytes covers x86-64, whose prefetcher pulls adjacent line pairs, and the big cores of aarch64; 64 is enough on most other targets. Padding costs memory, and splitting values that are always read together makes things slower, so measure first.
**Do.**
```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// One value per cache line.
#[derive(Debug, Default)]
#[repr(align(128))]
pub struct CachePadded<T>(T);

const _: () = assert!(
    align_of::<CachePadded<AtomicU64>>() == 128,
    "PERF-13: one counter per cache line"
);

/// Per-worker hit counters.
#[derive(Debug)]
pub struct HitCounters {
    // Padded: benches/counters.rs shows false sharing between workers
    // without it (PERF-01).
    shards: Box<[CachePadded<AtomicU64>]>,
}

impl HitCounters {
    /// One counter per worker.
    #[must_use]
    pub fn new(workers: usize) -> Self {
        Self {
            shards: (0..workers).map(|_| CachePadded::default()).collect(),
        }
    }

    /// Counts a hit for `worker`; unknown workers are ignored.
    pub fn hit(&self, worker: usize) {
        if let Some(shard) = self.shards.get(worker) {
            // Relaxed: a statistic; no other memory depends on it.
            shard.0.update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                n.saturating_add(1)
            });
        }
    }

    /// Sum of all workers' hits.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.shards.iter().fold(0_u64, |acc, shard| {
            // Relaxed: a statistic; readers need no ordering with other memory.
            acc.saturating_add(shard.0.load(Ordering::Relaxed))
        })
    }
}
```
**Don't.**
```rust,ignore
pub struct HitCounters { shards: Vec<AtomicU64> } // eight writers share one cache line
#[repr(align(128))] pub struct Config { .. }       // padding without a benchmark: memory for nothing
```
**Detect.** Review: per-thread or per-shard atomics and small locks in a `Vec`, array or adjacent fields on a path a benchmark shows contended; `repr(align)` without a benchmark or an alignment assertion.

## Review checklist
- [ ] Does every performance claim cite a benchmark or profile, shipped in `benches/`, and does optimized code carry a why-comment naming it? (PERF-01, TST-16)
- [ ] Are known sizes pre-allocated, buffers reused in loops (`clone_from`/`clone_into`, `*_into` forms), and no `collect()` only to iterate? (PERF-02)
- [ ] Iterators and adapters instead of index loops; `as_chunks` for constant chunk sizes; bounds hoisted by slicing once, never by `assert!`? (PERF-03)
- [ ] No `format!`/`to_string()` on hot paths; `Display` writes directly; no `push_str(&format!(..))`; log-only work gated, one event per batch? (PERF-04)
- [ ] Hot types at or below 128 bytes, large variants boxed, sizes asserted on every target, hot futures pinned by a test; no `Vec<Box<T>>` or nested `Arc`? (PERF-05)
- [ ] Collection matches the access pattern; `BTreeMap` for observable order; non-default hashers justified by key provenance and a benchmark, with the std types banned? (PERF-06)
- [ ] File I/O buffered, `BufWriter` flushed explicitly, stdout locked once, large inputs streamed through a capped reused line buffer, broken pipe handled? (PERF-07)
- [ ] Hot generic code statically dispatched, generic wrappers thin, `#[inline]` justified, rare branches `cold_path`? (PERF-08)
- [ ] Template profiles kept; deviations documented with a benchmark; no member profiles, `debug-assertions` in shipped profiles, or library allocators? (PERF-09)
- [ ] Every input-sized buffer, collection, recursion, queue and concurrency level bounded? (PERF-10, H-13)
- [ ] Refcount clones written `Arc::clone(&x)`; large read-only data shared as `Arc<[T]>`/`Arc<str>`? (PERF-11)
- [ ] Critical sections minimal; no I/O, join, blocking `recv` or `.await` under a lock; invariants under one lock; no read-to-write upgrade? (PERF-12, H-14)
- [ ] Contended independent atomics padded only with a benchmark and an alignment assertion? (PERF-13)
