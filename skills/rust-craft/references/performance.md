# Performance and resources

Measuring, allocation, strings, data layout, collections, hashing, I/O, dispatch, the release profile, bounded resources, sharing and locks. Read it before touching a hot path, before claiming anything is faster or smaller, when choosing a collection or hasher, when writing bulk output, and when reviewing a diff that adds buffers, caches, locks or `Arc`s. Async-specific resource rules (channels, tasks, timeouts) are in `async-concurrency.md`.

Rules covered: PERF-01, PERF-02, PERF-03, PERF-04, PERF-05, PERF-06, PERF-07, PERF-08, PERF-09, PERF-10, PERF-11, PERF-12 (and TST-16)

Clear code comes first. A change that trades clarity for speed users will notice is a HITL decision (H-16); record the benchmark numbers in the plan.

## Measure first

### PERF-01 · Measure before optimizing (TST-16)
**Why.** Intuition about hot spots is wrong more often than right, and the optimizer already removes much of what looks expensive. An optimization without a before/after number is a readability cost with no proven benefit. TST-16 makes the benchmark part of the change.
**Do.** Put benchmarks in `benches/`, with `harness = false` in the crate's `Cargo.toml`. `divan` is the lighter choice; `criterion` gives statistical comparison against a saved baseline. Either is a dev-dependency, which is still an H-16 decision the first time.
```toml
[dev-dependencies]
divan = { workspace = true }

[[bench]]
name = "tokenize"
harness = false
```
```rust
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
| Where does time go? | `cargo flamegraph` (perf/dtrace) | `CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --bin app -- <args>` |
| Where do allocations come from? | `dhat` heap profiler | `cargo run --release --features dhat-heap`, open `dhat-heap.json` in the DHAT viewer (`dh_view.html`) |
| Why is the build slow? | Cargo timings | `cargo build --release --timings`, open `target/cargo-timings/cargo-timing.html` |
| Which generics bloat the binary? | `cargo llvm-lines`, `cargo bloat` | `cargo llvm-lines --release --bin app \| head -30` |

`dhat` behind a feature, so production builds keep the system allocator:
```rust
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
**Detect.** Review: a diff described as "faster", "cheaper" or "optimized", or one that adds `unsafe`-free tricks (manual unrolling, `#[inline(always)]`, custom hashers, `SmallVec`), without a benchmark or profile in the change or plan.

## Allocation patterns

### PERF-02 · Allocate deliberately
**Why.** Each growth of a `Vec` or `String` reallocates and copies; a `collect()` that is only iterated again allocates for nothing; a fresh buffer per loop iteration turns one allocation into N.
**Do.**
```rust
/// Squares every value; the output size is known, so pre-size.
#[must_use]
pub fn squares(values: &[u32]) -> Vec<u64> {
    let mut out = Vec::with_capacity(values.len());
    out.extend(values.iter().map(|v| u64::from(*v) * u64::from(*v)));
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
```
**Don't.**
```rust,ignore
let ids: Vec<_> = orders.iter().map(|o| o.id).collect(); for id in ids { .. } // needless_collect
for line in lines { let mut buf = String::new(); .. }                         // alloc per iteration
let kept: Vec<_> = names.into_iter().filter(|n| !n.is_empty()).collect();    // copy instead of retain
```
**Detect.** `clippy::needless_collect`, `manual_memcpy`, `extend_with_drain`; dhat's "total blocks" per call site; review allocations inside loops.
Also: `Vec::with_capacity` only when the size is known or bounded (never from untrusted input without a cap, PERF-10); `shrink_to_fit` only after a large, lasting shrink; `extend` over repeated `push`; `drain(..)`/`mem::take` to reuse storage.

### PERF-03 · Iterators and adapters over index loops
**Why.** Iterators carry their own bounds, so the compiler drops the per-index checks, and they express the intent (`windows`, `chunks_exact`, `zip`) that index arithmetic hides. Index loops also collide with H-04.
**Do.** `for (a, b) in xs.iter().zip(ys)`, `xs.windows(2)`, `xs.chunks_exact(4)` plus `.remainder()`, `slice::array_windows::<2>()` (1.94; raise `rust-version` deliberately), `iter().rev()`, `retain`, `dedup_by_key`, `extend`.
**Don't.**
```rust,ignore
for i in 0..xs.len() { total += xs[i] * ys[i]; } // needless_range_loop, H-04, bounds check per step
```
**Detect.** `clippy::needless_range_loop`, `indexing_slicing`, `explicit_counter_loop`.

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

/// Renders rows into a caller-owned buffer, which it can reuse.
///
/// # Errors
/// Propagates any error from the sink.
pub fn render_into(out: &mut impl fmt::Write, rows: &[Row]) -> fmt::Result {
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
```rust,ignore
out.push_str(&format!("{}:{}", tenant, id));                 // format_push_string: two allocations
fn fmt(&self, f: ..) -> fmt::Result { f.write_str(&self.to_string()) } // recursion or double work
log_line(&format!("{id}")) // in a hot loop: pass `id` and format at the sink
```
**Detect.** `clippy::format_push_string`, `format_collect`, `to_string_in_format_args`, `inefficient_to_string`, `unnecessary_to_owned`.
Also: for one-off `Display` adapters, `std::fmt::from_fn` (1.93) avoids a wrapper type; it needs `rust-version` 1.93 or later (the template sets 1.96). Return `&str`/`Cow<'_, str>` instead of `String` when input usually passes through (OWN-10). `tracing` fields format lazily, so do not pre-format log messages.

## Data layout

### PERF-05 · Keep hot types small
**Why.** An enum is as large as its largest variant, so one rare 256-byte variant makes every `Vec<Message>` element, every move and every channel slot 256 bytes. `Vec<Box<T>>` adds an allocation and a pointer chase per element for no gain.
**Do.** Box the rare, large variant and pin the size with a compile-time assertion so regressions fail the build:
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
    /// Rare and large: boxed so every `Message` stays two words.
    Snapshot(Box<Snapshot>),
}

// Fails the build if someone adds a fat variant.
const _: () = assert!(size_of::<Message>() <= 16);
```
**Don't.**
```rust,ignore
pub enum Message { Ping, Snapshot([u8; 256]) } // large_enum_variant: every Ping is 257 bytes
let items: Vec<Box<Item>> = ..;               // vec_box: extra alloc + indirection per element
```
**Detect.** `clippy::large_enum_variant`, `vec_box`, `box_collection`, `large_futures`, `large_stack_arrays`; `size_of` assertions; `-Zprint-type-sizes` on nightly only when investigating.
Layout facts: `repr(Rust)` reorders fields to minimize padding, so do not hand-order for size. Niches are free: `Option<Box<T>>`, `Option<&T>`, `Option<NonZero<u32>>` are the size of the inner type. Use `u32` indices instead of `usize` in large index-heavy structures only with measurement. Struct-of-arrays beats array-of-structs when a hot loop reads one field of many elements.

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

### PERF-06 (continued) · Faster hashers only for keys an attacker cannot control
**Why.** std `HashMap` uses randomly keyed SipHash-1-3, which resists HashDoS: an attacker who controls keys (HTTP headers, JSON object keys, user names) cannot force every insert into one bucket. Faster non-cryptographic hashers (foldhash, FxHash) are much faster on small keys but weaken or drop that guarantee (FxHash is unseeded). Use them only for keys the program generates (interned ids, internal indices), only with a benchmark, and adding the crate is H-16.
**Do.**
```rust
use std::collections::HashMap;

/// Map for keys this process generates itself (interned ids), never for
/// attacker-controlled keys. Adopted after a benchmark (PERF-01, PERF-06).
pub type FastMap<K, V> = HashMap<K, V, foldhash::fast::RandomState>;

/// Counts ids with the fast hasher, pre-sized.
#[must_use]
pub fn count_ids(ids: &[u32]) -> FastMap<u32, u32> {
    let mut counts =
        FastMap::with_capacity_and_hasher(ids.len(), foldhash::fast::RandomState::default());
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
**Detect.** Review: any non-default `BuildHasher` needs the benchmark and a comment on key provenance. Cheaper than a new hasher: avoid hashing at all (index handles, sorted `Vec`), hash once with the entry API (OWN-09), and look up `String` keys by `&str`.

## I/O

### PERF-07 · Buffer, lock once, stream
**Why.** Unbuffered `File` reads and writes are one syscall each. `stdout` is line-buffered and locks per `println!`, so bulk output pays a lock and often a syscall per line. `read_to_string` on a large or untrusted file holds the whole file in memory. And `println!` panics when stdout is closed (`app | head`), which violates H-03; `writeln!` returns the error to `?`.
**Do.**
```rust
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;

/// Counts non-empty lines, streaming: memory stays constant in file size.
///
/// # Errors
/// Returns the I/O error from opening or reading.
pub fn count_lines(path: &Path) -> io::Result<usize> {
    let reader = BufReader::new(File::open(path)?);
    let mut count = 0_usize;
    for line in reader.lines() {
        if !line?.trim().is_empty() {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
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
```rust,ignore
for r in &records { println!("{r}"); }            // lock + line flush per row; panics on EPIPE
let text = fs::read_to_string(untrusted_path)?;   // unbounded memory (PERF-10)
let mut f = File::create(p)?; for r in rs { writeln!(f, "{r}")?; } // syscall per line
```
**Detect.** `clippy::print_stdout` (denied), `verbose_file_reads`; review: `File` used without `BufReader`/`BufWriter` in a loop; `BufWriter` dropped without `flush()`; whole-file reads of inputs with no size bound.
Also: `BufRead::read_line` into one reused `String` avoids the per-line allocation of `lines()` on hot paths. Library functions take `impl Read`/`impl Write` (or `&mut impl Write`) so the caller decides buffering and locking. Async I/O follows the same rules with `tokio::io::BufWriter`.

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
**Don't.**
```rust,ignore
#[inline(always)] pub fn parse_everything<R: Read>(r: R) -> .. { /* 200 lines */ } // bloat, no benchmark
fn apply_all(rules: &dyn Rule, prices: &[u64]) // dyn in the innermost hot loop without measurement
```
**Detect.** `cargo llvm-lines` for the functions with the most monomorphized copies; review `#[inline(always)]` (almost never justified) and `#[inline]` on large or crate-private functions (useless: generic and crate-local functions are already inlinable). `#[inline]` helps small non-generic `pub` functions called across crates when LTO is off; cite the benchmark. `#[cold]` on rarely taken error constructors can help the hot path; also measured.

## Release profile

### PERF-09 · Tune the release profile consciously
**Why.** Profile settings trade build time, binary size, debuggability and panic behavior for speed. The template ships `lto = "thin"` and `codegen-units = 1`; any change is documented in the root `AGENTS.md` with the reason and the measurement.

| Setting | Values | Effect | Cost / caveat |
|---|---|---|---|
| `opt-level` | `3` (default), `2`, `"s"`, `"z"` | speed vs size | `"z"` for size-constrained targets only |
| `lto` | `false`, `"thin"` (template), `"fat"`/`true` | cross-crate inlining, dead code removal | `"fat"`: much slower link; measure the gain |
| `codegen-units` | `16` (default), `1` (template) | better optimization | slower, less parallel builds |
| `panic` | `"unwind"` (default), `"abort"` | smaller binary, no landing pads | no unwinding: `JoinHandle::join` never sees a panic, `Drop` does not run on panic |
| `debug` | `false`, `"line-tables-only"`, `true` | usable profiles and backtraces | larger artifacts; use for profiling builds |
| `strip` | `"none"`, `"debuginfo"`, `"symbols"` | smaller binary | no symbols for crash reports |
| `incremental` | `false` (release default) | | leave off for release |

Use a custom profile for profiling instead of editing `release`:
```toml
[profile.profiling]
inherits = "release"
debug = "line-tables-only"
```
**Detect.** Review: profile changes without an `AGENTS.md` note; `panic = "abort"` in a workspace whose code relies on `join()` errors or `Drop` for cleanup. Editing profiles in a shared workspace is an H-16 decision.

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

/// Nesting depth of brackets; `None` if unbalanced or deeper than [`MAX_DEPTH`].
#[must_use]
pub fn depth(input: &str) -> Option<usize> {
    let mut current = 0_usize;
    let mut max = 0_usize;
    for c in input.chars() {
        match c {
            '[' => {
                current = current.checked_add(1).filter(|d| *d <= MAX_DEPTH)?;
                max = max.max(current);
            }
            ']' => current = current.checked_sub(1)?,
            _ => {}
        }
    }
    Some(max)
}
```

| Grows with input | Bound |
|---|---|
| Bytes read | `Read::take(limit)`, body-size limits in HTTP frameworks, `serde_json` from a capped buffer |
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
use std::sync::Arc;
use std::thread;

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

/// Each worker gets a refcount bump, not a deep copy.
#[must_use]
pub fn spawn_workers(catalog: &Catalog, workers: usize) -> Vec<thread::JoinHandle<usize>> {
    (0..workers)
        .map(|_| {
            let names = Arc::clone(&catalog.names);
            thread::spawn(move || names.len())
        })
        .collect()
}
```
**Don't.**
```rust,ignore
let names = catalog.names.clone();   // clone_on_ref_ptr: is this a refcount or a deep copy?
let cfg = Arc::new(config.clone());   // per request: build the Arc once at startup
struct Shared { data: Arc<Vec<u8>> }  // rc_buffer: double indirection, use Arc<[u8]>
```
**Detect.** `clippy::clone_on_ref_ptr`, `rc_buffer`, `redundant_clone`. For byte buffers sliced and passed between network stages, `bytes::Bytes` gives cheap sub-slicing (dependency, H-16).

## Locks and contention

### PERF-12 · Short critical sections, no I/O under a lock
**Why.** Everything done while holding a lock serializes every other thread that wants it; I/O under a lock turns one slow disk or peer into a global stall, and an `.await` under a lock is a deadlock risk (H-14). Take the lock, do the minimum (a push, a swap, a copy), release, then do the slow work.
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
    /// Returns the I/O error from the sink.
    pub fn flush_to(&self, sink: &mut impl Write) -> io::Result<()> {
        let batch = mem::take(&mut *self.lines.lock().unwrap_or_else(PoisonError::into_inner));
        for line in &batch {
            writeln!(sink, "{line}")?;
        }
        Ok(())
    }
}
```
**Don't.**
```rust,ignore
let lines = self.lines.lock()?;
for l in lines.iter() { writeln!(file, "{l}")?; } // I/O while every producer waits
let guard = state.lock()?; client.send(&guard).await; // H-14: guard across .await
```
**Detect.** `clippy::significant_drop_tightening`, `await_holding_lock`; review: any I/O, allocation-heavy work, or call into unknown code (callbacks, trait objects) inside a guard's scope.

Under contention, in order: shrink the critical section; split one lock into per-field locks or per-shard locks (`Vec<Mutex<HashMap<..>>>` indexed by `hash % shards` with `get`); use atomics for single counters; move the state into an owner task and message-pass (ASY-09); `RwLock` only when reads dominate and a benchmark shows the gain. Replacing std `Mutex` with `parking_lot` is a dependency decision (H-16) that needs a benchmark.

## Review checklist
- [ ] Does every performance claim cite a benchmark or profile, shipped in `benches/`? (PERF-01, TST-16)
- [ ] Are known sizes pre-allocated, buffers reused in loops, and no `collect()` only to iterate? (PERF-02)
- [ ] Iterators and adapters instead of index loops? (PERF-03)
- [ ] No `format!`/`to_string()` on hot paths; `Display` writes directly; no `push_str(&format!(..))`? (PERF-04)
- [ ] Hot enums small, large variants boxed, size asserted where it matters; no `Vec<Box<T>>`? (PERF-05)
- [ ] Collection matches the access pattern; `BTreeMap` for observable order; non-default hashers justified by key provenance and a benchmark? (PERF-06)
- [ ] File I/O buffered, `BufWriter` flushed explicitly, stdout locked once, large inputs streamed, broken pipe handled? (PERF-07)
- [ ] Hot generic code statically dispatched, generic wrappers thin, `#[inline]` justified? (PERF-08)
- [ ] Release profile changes documented with a reason? (PERF-09)
- [ ] Every input-sized buffer, collection, recursion, queue and concurrency level bounded? (PERF-10, H-13)
- [ ] Refcount clones written `Arc::clone(&x)`; large read-only data shared as `Arc<[T]>`/`Arc<str>`? (PERF-11)
- [ ] Critical sections minimal, no I/O or `.await` under a lock? (PERF-12, H-14)
