# Async and concurrency

How to choose between async and threads, keep the runtime at the edge, write async traits, own every task, shut down cleanly, stay cancel-safe, bound every queue, and share state without locks across `.await`. It covers H-13 (bounded resources, owned tasks) and H-14 (no blocking in async, no guard across `.await`) in depth. Read it before writing `async`, `spawn`, `select!`, a channel, a lock, an atomic or a thread, and when reviewing any of them. Examples use tokio 1.x, tokio-util 0.7 (`rt` feature for `TaskTracker`), futures 0.3 and rayon 1; each is a dependency decision (ARC-12, H-16).

Rules covered: ASY-01, ASY-02, ASY-03, ASY-04, ASY-05, ASY-06, ASY-07, ASY-08, ASY-09, ASY-10, ASY-11, ASY-12, H-13, H-14

## Async or threads?

### ASY-01 · Async for I/O-bound concurrency only
**Why.** An async executor multiplexes many tasks over a few threads by switching at `.await`. Code that computes for milliseconds without awaiting holds a worker thread hostage, stalling every other task on it. Async buys cheap waiting, not faster computing.

| Workload | Use |
|---|---|
| Many concurrent sockets, HTTP calls, DB queries, timers | async (tokio) |
| CPU-heavy work or a blocking library (SQLite, sync SDKs) inside an async service | `tokio::task::spawn_blocking` or a rayon pool, awaited from async |
| Data-parallel CPU work over a collection, sync caller | `rayon` `par_iter` |
| A few parallel jobs that borrow local data | `std::thread::scope` |
| One sequential CLI that reads a file and exits | plain sync code; no runtime |

**Detect.** `async fn` with no `.await` (`clippy::unused_async`); loops over large data in async bodies; a multi-thread runtime in a sequential CLI with no concurrent I/O.

## Runtime at the edge

### ASY-02 · Libraries stay runtime-agnostic; the binary picks the runtime
**Why.** A library that starts its own runtime panics when called inside another ("cannot start a runtime from within a runtime") and forces one executor on every user. The composition root (ARC-14) is the only place that knows the deployment.

**Do.** `#[tokio::main]` only in `main.rs` of a binary crate. Libraries expose `async fn` and traits; where they must spawn, they take a `tokio::runtime::Handle` or document that they need a tokio context, and they never call `Runtime::new()` or `block_on`.
```rust
//! Composition root: the only place that chooses and starts a runtime.

use std::num::NonZeroUsize;

use anyhow::Context as _;
use asy::ledger::LedgerHandle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let queue = NonZeroUsize::new(64).context("queue capacity must be non-zero")?;
    let (ledger, actor) = LedgerHandle::spawn(queue);
    ledger.deposit(100).await.context("initial deposit")?;
    drop(ledger);
    let balance = actor.await.context("ledger actor failed")?;
    tracing::info!(balance, "ledger closed");
    Ok(())
}
```
**Detect.** `#[tokio::main]`, `Runtime::new`, `block_on` in a library crate; any tokio dependency in a domain crate, or runtime features (`rt`, `rt-multi-thread`) in an application crate (ARC-02).

## Async fn in traits

### ASY-03 · Native async traits; `Send` futures spelled out; boxed alias for `dyn`
**Why.** Since 1.75, `async fn` works in traits without `#[async_trait]` (which boxes every call). The catch: an `async fn` in a trait says nothing about whether its future is `Send`, so generic callers cannot `tokio::spawn` it. Writing the method as `fn .. -> impl Future<Output = T> + Send` puts the bound in the contract. Traits using `async fn` or `impl Future` are not object safe, so `dyn` needs a boxed-future twin.

**Do.** A public port with `Send` futures; an in-memory adapter:
```rust
/// Port: key-value lookups. The returned future is `Send`, so callers may spawn it.
pub trait Store: Send + Sync {
    /// Looks up `key`.
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<String>, StoreError>> + Send;
}

impl Store for MemoryStore {
    // Nothing to await, so return a ready future. An impl that awaits may
    // write `async fn get(..)`; the compiler still checks the `Send` bound.
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<String>, StoreError>> + Send {
        std::future::ready(Ok(self.entries.get(key).cloned()))
    }
}
```
`trait_variant` (a dependency, H-16) generates the `Send` variant from an `async fn` trait, useful when you want both a local and a `Send` flavor:
```rust
/// Generates `Clock` (Send futures) from `LocalClock` (no Send bound).
#[trait_variant::make(Clock: Send)]
pub trait LocalClock {
    /// Current time in milliseconds since the epoch.
    async fn now_millis(&self) -> u64;
}
```
For `dyn` (heterogeneous plug-ins, API-09), add an object-safe twin with a named boxed-future alias and a blanket impl, so adapters implement only the static trait:
```rust
/// A boxed, `Send` future borrowing for `'a`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Object-safe twin of [`Store`] for heterogeneous collections and plug-ins.
pub trait DynStore: Send + Sync {
    /// Looks up `key`. Named apart from [`Store::get`] so calls never need disambiguating.
    fn get_boxed<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<String>, StoreError>>;
}

impl<T: Store> DynStore for T {
    fn get_boxed<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<String>, StoreError>> {
        Box::pin(self.get(key))
    }
}
```
**Don't.**
```rust,ignore
#[async_trait::async_trait]                                 // boxes every call; MOD-04
pub trait Store { async fn get(&self, key: &str) -> ...; }  // public, no Send bound, so:
fn spawn_lookup<S: Store + 'static>(s: S) { tokio::spawn(async move { s.get("k").await }); } // error: future is not Send
```
**Detect.** `async_trait` in `Cargo.toml` (scan, advisory); rustc's `async_fn_in_trait` warning on public traits.

## Structured concurrency

### ASY-04 · H-13 · Every task has an owner
**Why.** A detached `tokio::spawn` outlives its caller: its panic is lost, its errors are unobserved, shutdown cannot wait for it, and it leaks if it loops. An owner (a `JoinSet`, a stored `JoinHandle`, a `TaskTracker`) joins or aborts it, so task lifetimes nest like scopes.

| Owner | Use when | On drop |
|---|---|---|
| `JoinSet<T>` | fan-out within one function; collect results | aborts all remaining tasks |
| `JoinHandle<T>` stored in a struct or returned | one long-lived task (actor, listener) | detaches: keep it and `.await` or `.abort()` it |
| `tokio_util::task::TaskTracker` | many long-lived tasks spawned over time, waited on at shutdown | nothing; call `close()` then `wait().await` |

**Do.**
```rust
/// Looks up every key concurrently, one task per key, sorted by key.
///
/// All tasks are owned by the `JoinSet`: returning early (including via `?`)
/// drops the set, which aborts the rest.
///
/// # Errors
///
/// The first failed lookup or task.
pub async fn get_many<S: Store + 'static>(
    store: Arc<S>,
    keys: Vec<String>,
) -> Result<Vec<(String, Option<String>)>, GetManyError> {
    let mut tasks = JoinSet::new();
    for key in keys {
        let store = Arc::clone(&store);
        tasks.spawn(async move {
            let value = store.get(&key).await?;
            Ok::<_, StoreError>((key, value))
        });
    }
    let mut found = Vec::with_capacity(tasks.len());
    while let Some(joined) = tasks.join_next().await {
        found.push(joined??);
    }
    found.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}
```
One task per key is unbounded fan-out for unbounded input: cap it with a `Semaphore`, or use a bounded stream (see Streams), when `keys` comes from outside (PERF-10).
**Don't.**
```rust,ignore
tokio::spawn(async move { sync_forever(db).await });  // detached: nobody joins, errors vanish
```
**Detect.** `tokio::spawn(` whose `JoinHandle` is not bound, stored or returned; `let _ = ...spawn`.

## Graceful shutdown and cancellation

### ASY-05 · Cooperative cancellation with `CancellationToken`
**Why.** Dropping a future cancels it at whatever `.await` it is parked on, possibly mid-write. A token lets the task stop at a point it chooses, finish the current unit of work, and release resources. Pair it with a tracker so shutdown can wait.

**Do.** Excerpt of `Workers` (fields `tracker: TaskTracker`, `token: CancellationToken`, `processed: Arc<AtomicU64>`). `select!` on `token.cancelled()` and a cancel-safe operation. `biased;` polls branches in order, so shutdown wins over a busy queue. The arm *body* runs to completion: a job already started is never cut off.
```rust
/// Spawns a worker that drains `jobs` until shutdown or the channel closes.
/// Must run inside a Tokio runtime (`TaskTracker::spawn` uses `tokio::spawn`).
pub fn spawn(&self, mut jobs: mpsc::Receiver<u64>) {
    let token = self.token.child_token();
    let processed = Arc::clone(&self.processed);
    self.tracker.spawn(async move {
        loop {
            tokio::select! {
                // Check shutdown first so a busy queue cannot starve it.
                biased;
                () = token.cancelled() => break,
                // `recv` is cancel safe: no job is lost if the other branch wins.
                job = jobs.recv() => match job {
                    Some(job) => {
                        handle(job).await;
                        // A statistic: no other memory depends on it.
                        processed.fetch_add(1, Ordering::Relaxed);
                    }
                    None => break,
                },
            }
        }
    });
}

/// Signals every worker to stop and waits for all of them.
pub async fn shutdown(self) -> u64 {
    self.token.cancel();
    self.tracker.close();
    self.tracker.wait().await;
    self.processed.load(Ordering::Relaxed)
}
```
In the binary, cancel the root token on `tokio::signal::ctrl_c()` and bound the wait with `tokio::time::timeout` so a stuck task cannot hang shutdown.

### Cancel safety
A `select!` branch that loses is dropped. The operation is cancel safe if dropping it loses nothing. From the tokio docs (`select!`, `mpsc::Sender`, `AsyncBufReadExt`):

| Operation | Cancel safe? | If cancelled |
|---|---|---|
| `mpsc::Receiver::recv`, `broadcast::Receiver::recv`, `watch::Receiver::changed` | yes | nothing lost |
| `TcpListener::accept`, `AsyncReadExt::read`, `read_buf`, `AsyncWriteExt::write`, `write_buf` | yes | nothing lost |
| `futures::StreamExt::next`, `tokio_stream::StreamExt::next`, `Lines::next_line` | yes | nothing lost |
| `mpsc::Sender::send` | not sent | the message is dropped with the future; use `reserve()` then `send` on the permit |
| `AsyncReadExt::read_exact`, `read_to_end`, `read_to_string` | no | partially read bytes lost |
| `AsyncBufReadExt::read_line` | no | partially read data is lost; use `Lines::next_line` or a codec |
| `AsyncBufReadExt::read_until` | resumable | partial bytes are appended to `buf`; safe in a loop only if the same `buf` is reused |
| `AsyncWriteExt::write_all` | no | unknown amount written |
| `Mutex::lock`, `RwLock::read`/`write`, `Semaphore::acquire`, `Notify::notified`, `Sender::reserve` | queue position | you lose your place in the fair queue |

Rule: every future inside `select!` is on the "yes" rows, or is created once outside the loop and polled by `&mut` (`tokio::pin!`), or the branch has a comment explaining why the loss is acceptable (ASY-05).

## Never block the executor

### ASY-06 · H-14 · No blocking calls in async code
**Why.** A worker thread blocked in `std::thread::sleep`, `std::fs`, a blocking HTTP client, or a long CPU loop cannot poll other tasks; with a few workers, a handful of blocked tasks stalls the service, including its timers.

**Do.** Use async equivalents (`tokio::time::sleep`, `tokio::fs`, async clients). Move CPU work and unavoidable blocking calls to the blocking pool and await the handle, which returns `Result<T, JoinError>`: `task::spawn_blocking(move || fnv1a(&bytes)).await`.
`spawn_blocking` tasks cannot be aborted once running, and the pool is large (hundreds of threads) but finite; for sustained CPU-heavy work use a rayon pool and a `oneshot` for the result. `tokio::task::block_in_place` panics on a `current_thread` runtime (including default `#[tokio::test]`) and suspends everything else running in the same task (siblings in `join!`/`select!`); prefer `spawn_blocking`.
**Don't.**
```rust,ignore
async fn handler() { std::thread::sleep(Duration::from_secs(1)); }  // blocks a worker thread
async fn load(p: &Path) -> io::Result<String> { std::fs::read_to_string(p) } // blocking I/O
```
**Detect.** `std::thread::sleep`, `std::fs::`, `std::net::`, `reqwest::blocking`, `.join()` on a `std::thread` handle, or `rayon` calls directly inside `async` bodies.

## Locks and `.await`

### ASY-09 · H-14 · Scope guards; message passing first
**Why.** A `std::sync::MutexGuard` held across `.await` blocks every other task that wants the lock for as long as the awaited I/O takes, can deadlock the executor, and makes the future `!Send`. Message passing (an actor, below) removes the lock entirely.

**Do.** Excerpt of `Cache { entries: Mutex<HashMap<String, Arc<str>>> }`. Take the lock in a statement that ends before any `.await`. `std::sync::Mutex` is right for short critical sections; the justification for shared mutable state is written on the type (OWN-11).
```rust
/// Returns the cached value for `key`, fetching and caching it on a miss.
///
/// # Errors
///
/// Propagates [`StoreError`] from the fetch.
pub async fn get_or_fetch(
    &self,
    store: &impl Store,
    key: &str,
) -> Result<Option<Arc<str>>, StoreError> {
    // The guard is a temporary: dropped at the end of this statement.
    let cached = self.entries().get(key).map(Arc::clone);
    if let Some(hit) = cached {
        return Ok(Some(hit));
    }
    let Some(fetched) = store.get(key).await? else {
        return Ok(None);
    };
    let value: Arc<str> = Arc::from(fetched);
    self.entries().insert(key.to_owned(), Arc::clone(&value));
    Ok(Some(value))
}

fn entries(&self) -> MutexGuard<'_, HashMap<String, Arc<str>>> {
    // Each critical section is a single map call, so a panic elsewhere
    // cannot leave the map half-updated; recovering the guard is sound.
    self.entries.lock().unwrap_or_else(PoisonError::into_inner)
}
```
`tokio::sync::Mutex` is right only when the guard must span `.await`, typically to serialize use of one I/O resource so writes do not interleave (a `FrameWriter` holding `tokio::sync::Mutex<W>` across `write_all(len)`, `write_all(frame)`, `flush()`). It is slower than `std::sync::Mutex`, and an actor owning the resource is usually cleaner.
**Don't.**
```rust,ignore
let mut map = self.entries.lock()?;         // std guard...
let v = store.get(key).await?;              // ...held across .await: clippy::await_holding_lock
map.insert(key.to_owned(), v);
```
**Detect.** `clippy::await_holding_lock`, `await_holding_refcell_ref`, `significant_drop_tightening` (all enabled by the template).

## Channels

### ASY-07 · H-13 · Bounded channels, chosen by pattern
**Why.** An unbounded channel converts a slow consumer into unbounded memory growth and hides overload until the process is killed. A bounded channel pushes back: `send().await` waits, `try_send` reports `Full`, and the producer decides to wait, shed or fail.

| Pattern | Channel | Notes |
|---|---|---|
| Work queue, many producers to one consumer | `tokio::sync::mpsc::channel(n)` | never `unbounded_channel` (H-13) |
| Request/response to an actor | `mpsc` for commands + `oneshot` per reply | reply error means requester gone |
| Fan-out events to every subscriber | `broadcast::channel(n)` | slow receivers get `RecvError::Lagged(k)`; handle it |
| Latest value (config, health, progress) | `watch::channel(v)` | receivers see only the newest value |
| Sync threads | `std::sync::mpsc::sync_channel(n)` | not `std::sync::mpsc::channel()` (unbounded) |

**Do.** Choose capacity deliberately (a named constant, CRF-09). At the edge, shed load instead of queueing forever:
```rust
/// Queues `job` without waiting; a full queue is reported, not buffered.
///
/// # Errors
///
/// [`SubmitError::Busy`] when the bounded queue is full,
/// [`SubmitError::Stopped`] when no worker is receiving.
pub fn submit(queue: &mpsc::Sender<u64>, job: u64) -> Result<(), SubmitError> {
    queue.try_send(job).map_err(|err| match err {
        TrySendError::Full(_) => SubmitError::Busy,
        TrySendError::Closed(_) => SubmitError::Stopped,
    })
}
```
**Detect.** `unbounded_channel`, `std::sync::mpsc::channel()`, `Vec`/`VecDeque` used as a queue with no cap.

## Timeouts and retries

### ASY-08 · Timeout every external call; retry with a bound, backoff and jitter
**Why.** A peer that never answers holds a task, a connection and its buffers forever. Unbounded retries turn an outage into a retry storm; synchronized retries without jitter hit the recovering service in waves. Retrying a permanent error (400, validation) only adds load.

**Do.** Per-attempt `tokio::time::timeout`, a capped attempt count (`NonZeroU32`), exponential backoff with a ceiling, and an injected jitter function so tests are exact:
```rust
/// Runs `op` under `policy`, sleeping `jitter(backoff)` between attempts.
///
/// # Errors
///
/// [`RetryError`] once a failure is permanent or attempts run out.
pub async fn retry<T, E: Transient>(
    policy: RetryPolicy,
    mut jitter: impl FnMut(Duration) -> Duration,
    mut op: impl AsyncFnMut() -> Result<T, E>,
) -> Result<T, RetryError<E>> {
    let mut attempt = 1;
    loop {
        let failure = match time::timeout(policy.attempt_timeout, op()).await {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(source)) if !source.is_transient() => {
                return Err(RetryError::Failed {
                    attempts: attempt,
                    source,
                });
            }
            Ok(Err(source)) => RetryError::Failed {
                attempts: attempt,
                source,
            },
            Err(_elapsed) => RetryError::TimedOut { attempts: attempt },
        };
        if attempt >= policy.max_attempts.get() {
            return Err(failure);
        }
        time::sleep(jitter(policy.backoff(attempt))).await;
        attempt += 1;
    }
}
```
**Don't.**
```rust,ignore
loop { match client.get(url).await { Ok(r) => break r, Err(_) => continue } } // no timeout, no bound, no backoff, retries 400s
```
`RetryPolicy` holds `max_attempts: NonZeroU32`, `attempt_timeout`, `base_delay`, `max_delay`; `backoff(n)` is `base_delay.saturating_mul(2^(n-1)).min(max_delay)` using `checked_shl`. `Transient` is a one-method trait the caller's error implements. Production passes `full_jitter` (a random delay in `0..=delay` from `RandomState::new().hash_one(..)`, no RNG dependency); tests pass the identity function. Only retry idempotent operations, or send an idempotency key.

## Actors

### ASY-09 · One task owns the state
**Why.** When one task owns the state and others send it commands, there is no lock to hold across `.await`, no poisoning, and invariants live in one sequential loop. The handle is a cloneable `mpsc::Sender`; the actor stops when the last handle drops.

**Do.**
```rust
enum Command {
    Deposit {
        amount: u64,
        reply: oneshot::Sender<Result<u64, LedgerError>>,
    },
}

/// Cheap, cloneable handle to the ledger actor.
#[derive(Debug, Clone)]
pub struct LedgerHandle {
    commands: mpsc::Sender<Command>,
}

impl LedgerHandle {
    /// Starts the actor. The caller owns the returned `JoinHandle` (H-13): the
    /// actor exits once every handle is dropped, yielding the final balance.
    /// Must run inside a Tokio runtime: `tokio::spawn` panics outside one. A
    /// library that must not assume a runtime takes a `Handle` instead (ASY-02).
    #[must_use]
    pub fn spawn(queue: NonZeroUsize) -> (Self, JoinHandle<u64>) {
        let (commands, inbox) = mpsc::channel(queue.get());
        let task = tokio::spawn(run(inbox));
        (Self { commands }, task)
    }

    /// Adds `amount` and returns the new balance.
    ///
    /// # Errors
    ///
    /// [`LedgerError::Overflow`] or [`LedgerError::Stopped`].
    pub async fn deposit(&self, amount: u64) -> Result<u64, LedgerError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Deposit { amount, reply })
            .await
            // the SendError only carries the unsent value (ERR-04)
            .map_err(|_closed| LedgerError::Stopped)?;
        // the actor dropped the reply sender; RecvError carries no cause (ERR-04)
        response.await.map_err(|_dropped| LedgerError::Stopped)?
    }
}

async fn run(mut inbox: mpsc::Receiver<Command>) -> u64 {
    let mut balance: u64 = 0;
    while let Some(command) = inbox.recv().await {
        match command {
            Command::Deposit { amount, reply } => {
                let outcome = balance.checked_add(amount).ok_or(LedgerError::Overflow);
                if let Ok(new_balance) = outcome {
                    balance = new_balance;
                }
                if reply.send(outcome).is_err() {
                    tracing::debug!("deposit requester went away before the reply");
                }
            }
        }
    }
    balance
}
```
`let _ = reply.send(..)` is denied (`let_underscore_must_use`, H-10); a dropped requester is observable, so log it. Keep the actor loop free of slow awaits: a slow command delays every queued one.

## Threads without async

### ASY-10 · `thread::scope` to borrow, `rayon` for data parallelism
**Why.** `std::thread::spawn` needs `'static` data, which pushes people to clone or `Arc` everything, or leak (H-07). Scoped threads borrow from the caller and are joined before `scope` returns. rayon splits work across a pool with work stealing and no manual chunking.

**Do.**
```rust
/// Sums `values` across `threads` scoped threads, saturating at `u64::MAX`.
///
/// # Errors
///
/// [`WorkerPanicked`] if any thread panicked.
pub fn parallel_sum(values: &[u64], threads: NonZeroUsize) -> Result<u64, WorkerPanicked> {
    let chunk_len = values.len().div_ceil(threads.get()).max(1);
    thread::scope(|scope| {
        // Spawn every thread before joining any, or they would run one at a time.
        let workers: Vec<_> = values
            .chunks(chunk_len)
            .map(|chunk| scope.spawn(move || saturating_sum(chunk)))
            .collect();
        workers.into_iter().try_fold(0_u64, |total, worker| {
            // the panic payload has no diagnostic value here (ERR-04)
            let part = worker.join().map_err(|_panic| WorkerPanicked)?;
            Ok(total.saturating_add(part))
        })
    })
}

/// Digests every document in parallel on the rayon pool (synchronous caller).
#[must_use]
pub fn digest_all(docs: &[Vec<u8>]) -> Vec<u64> {
    docs.par_iter().map(|doc| fnv1a(doc)).collect()
}
```
Never call rayon from an async task directly (it blocks the worker); wrap it in `spawn_blocking` or hand it off via a `oneshot`.

### Atomics and memory ordering (ASY-10)
Use atomics for a single independent value: a counter, a flag, an id generator. Anything involving two values that must stay consistent is a `Mutex` (or an actor).

| Need | Ordering |
|---|---|
| Statistics counter, nothing else reads memory based on it | `Relaxed` (as in `processed.fetch_add(1, Ordering::Relaxed)` above) |
| Flag that publishes data written before it ("ready", "initialized") | `store(.., Release)` by the writer, paired with `load(Acquire)` by the reader |
| Read-modify-write that both consumes and publishes | `AcqRel` on the RMW |
| Unsure | `SeqCst`, or better, a `Mutex`; one-time initialization is `OnceLock`/`LazyLock` (MOD-03), not a hand-rolled flag |

Every non-`SeqCst` ordering gets a comment saying what it pairs with. A `Release` without a matching `Acquire` is a bug.

## Send and Sync

### ASY-11 · Assert thread-safety at compile time
**Why.** Adding an `Rc` field (`!Send`) or a `Cell`/`RefCell` field (`!Sync`) silently changes a public type's auto traits, breaking every downstream `tokio::spawn`, far from the change. A const assertion fails in the crate that made the change.
```rust
// Compile-time proof that these types can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Workers>();
    assert_send_sync::<SubmitError>();
    assert_send_sync::<crate::ledger::LedgerHandle>();
};
```
Types that are deliberately `!Send` say so in their docs, and a compile-fail doctest proves it (TST-08).

## Async closures

### ASY-12 · `AsyncFn*` bounds over `Fn() -> Fut`
**Why.** Since 1.85, `async |x| ..` closures and `AsyncFn`/`AsyncFnMut`/`AsyncFnOnce` bounds let the returned future borrow the closure's captures. `F: FnMut(&T) -> Fut` cannot express that, which forced `move` plus clones or `Arc<Mutex<_>>` for simple accumulation.
```rust
/// Awaits `f` on each item in order, stopping at the first error.
///
/// `AsyncFnMut` lets the closure's future borrow the closure's own captures,
/// which `F: FnMut(&T) -> Fut` cannot express.
///
/// # Errors
///
/// The first error returned by `f`.
pub async fn try_each<T, E>(
    items: &[T],
    mut f: impl AsyncFnMut(&T) -> Result<(), E>,
) -> Result<usize, E> {
    let mut done = 0;
    for item in items {
        f(item).await?;
        done += 1;
    }
    Ok(done)
}
```
Callers write `try_each(&items, async |n: &i32| { seen.push(*n); Ok::<(), Infallible>(()) })`, mutating a local `Vec` with no `Arc`. Limit (checked on 1.98): `AsyncFn*` bounds cannot require the call's future to be `Send`. Awaiting in place is fine, but `tokio::spawn` of a future that calls an `AsyncFnMut` closure mutating its captures fails with "higher-ranked lifetime error". For work that must be spawned, keep `F: Fn() -> Fut, Fut: Future + Send`.
**Don't.**
```rust,ignore
pub async fn try_each<T, F, Fut>(items: &[T], f: F) where F: Fn(&T) -> Fut, Fut: Future<Output = ()> { /* ... */ }
try_each(&items, |n| { let seen = Arc::clone(&seen); async move { seen.lock()?.push(*n) } }) // clone + lock for a local Vec
```

## Streams

Concurrency without spawning: a stream of futures polled inside the current task, bounded by `buffer_unordered(n)` (completion order) or `buffered(n)` (input order). Dropping the stream cancels in-flight work; nothing leaks (ASY-04).
```rust
/// Looks up `keys` with at most `max_in_flight` concurrent requests, sorted by key.
///
/// # Errors
///
/// The first [`StoreError`]; remaining in-flight lookups are dropped.
pub async fn get_bounded(
    store: &impl Store,
    keys: &[String],
    max_in_flight: NonZeroUsize,
) -> Result<Vec<(String, Option<String>)>, StoreError> {
    let mut found: Vec<_> = stream::iter(keys)
        .map(|key| async move {
            let value = store.get(key).await?;
            // The result owns its key: the caller keeps `keys`.
            Ok::<_, StoreError>((key.clone(), value))
        })
        .buffer_unordered(max_in_flight.get())
        .try_collect()
        .await?;
    found.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}
```
`TryStreamExt::try_collect` stops at the first error. Never `join_all` over input-sized collections: it is unbounded concurrency (PERF-10).

## Testing async code

- `#[tokio::test]` runs a current-thread runtime per test; use `flavor = "multi_thread"` only to test real parallelism.
- `#[tokio::test(start_paused = true)]` (tokio `test-util` feature, dev-dependency) freezes time and auto-advances when idle: timeouts, backoff and intervals run instantly and elapsed times are exact. Use `tokio::time::Instant`, not `std::time::Instant`.
- Never `sleep` to wait for a task; await its `JoinHandle`, a channel, or `TaskTracker::wait` (TST-10).
- Inject the jitter or RNG so waits are exact: with `retry(POLICY, no_jitter, async || ..)` failing twice, a paused-time test asserts `start.elapsed() == Duration::from_millis(100 + 200)`. Full worked example: `testing.md` §Determinism.
- Test shutdown: spawn, cancel the token, and assert `shutdown().await` returns; a hang here is a leaked task.

## Review checklist
- [ ] Is async used only for I/O-bound work, with CPU work on `spawn_blocking` or rayon? (ASY-01)
- [ ] Does only the binary start a runtime, with libraries free of `#[tokio::main]`/`block_on`? (ASY-02)
- [ ] Do async traits use native syntax, `impl Future + Send` for public ports, and a boxed alias for `dyn`? (ASY-03)
- [ ] Is every spawned task owned by a `JoinSet`, stored `JoinHandle` or `TaskTracker`, and joined or aborted on shutdown? (ASY-04, H-13)
- [ ] Is cancellation cooperative, and is every future inside `select!` cancel safe or documented? (ASY-05)
- [ ] Is there no blocking call and no lock guard across `.await`? (ASY-06, H-14)
- [ ] Is every channel bounded, and chosen by pattern (mpsc, broadcast, watch, oneshot)? (ASY-07, H-13)
- [ ] Does every external call have a timeout, and are retries bounded with backoff and jitter? (ASY-08)
- [ ] Is shared state owned by an actor where possible, with `std::sync::Mutex` for short sections and `tokio::sync::Mutex` only when holding across `.await` is required? (ASY-09)
- [ ] Do sync parallel sections use `thread::scope` or rayon, and does every atomic ordering have a justification? (ASY-10)
- [ ] Are public types `Send + Sync`, asserted at compile time? (ASY-11)
- [ ] Do closure-taking async APIs use `AsyncFn*` bounds unless the future must be spawned? (ASY-12)
