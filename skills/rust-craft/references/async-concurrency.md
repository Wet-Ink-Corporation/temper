# Async and concurrency

How to choose between async and threads, keep the runtime at the edge, write async traits, own every task and thread, shut down cleanly, stay cancel-safe, bound every queue, and share state without locks across `.await`. It is the rust-craft authority on memory ordering (ASY-13) and lock discipline (ASY-14 to ASY-17), and covers H-13 (bounded resources, owned tasks) and H-14 (nothing blocking held across `.await`) in depth. Read it before writing `async`, `spawn`, `select!`, a channel, a lock, an atomic or a thread, and when reviewing any of them. Examples use tokio 1.x, tokio-util 0.7 (`rt` feature for `TaskTracker`), futures 0.3 and rayon 1; each is a dependency decision (ARC-12, H-16).

Rules covered: ASY-01, ASY-02, ASY-03, ASY-04, ASY-05, ASY-06, ASY-07, ASY-08, ASY-09, ASY-10, ASY-11, ASY-12, ASY-13, ASY-14, ASY-15, ASY-16, ASY-17, H-13, H-14

## Async or threads?

### ASY-01 · Async for I/O-bound concurrency only
**Why.** An async executor multiplexes many tasks over a few threads by switching at `.await`. Code that computes for milliseconds without awaiting holds a worker thread hostage, stalling every other task on it. Async buys cheap waiting, not faster computing.

| Workload | Use |
|---|---|
| Many concurrent sockets, HTTP calls, DB queries, timers | async (tokio) |
| CPU-heavy work or a blocking library (SQLite, sync SDKs) inside an async service | `tokio::task::spawn_blocking` or a rayon pool, awaited from async |
| Data-parallel CPU work over a collection, sync caller | `rayon` `par_iter` |
| A few parallel jobs that borrow local data | `std::thread::scope` (ASY-10) |
| One sequential CLI that reads a file and exits | plain sync code; no runtime |

Anything that runs more than about 100 µs without reaching an `.await` is blocking; measure borderline work (PERF-01). Short per-item work over many items may stay on the task if it yields cooperatively: call `tokio::task::coop::consume_budget().await` once per item (it yields only when the task's budget is spent), or `tokio::task::yield_now().await` every few hundred items. APIs that process items offer a batch form.

**Detect.** `async fn` with no `.await` (`clippy::unused_async`); loops over large data in async bodies with no yield point; a multi-thread runtime in a sequential CLI with no concurrent I/O.

## Runtime at the edge

### ASY-02 · Libraries stay runtime-agnostic; the binary picks the runtime
**Why.** A library that starts its own runtime panics when called inside another ("cannot start a runtime from within a runtime") and forces one executor on every user. The composition root (ARC-14) is the only place that knows the deployment.

**Do.** `#[tokio::main]` only in `main.rs` of a binary crate. Libraries expose `async fn` and traits; where they must spawn, they take a `tokio::runtime::Handle` or document that they need a tokio context, and they never call `Runtime::new()` or `block_on`.
```rust,fragment
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
**Why.** Since 1.75, `async fn` works in traits without `#[async_trait]` (which boxes every call). The catch: an `async fn` in a trait says nothing about whether its future is `Send`, so generic callers cannot `tokio::spawn` it. Writing the method as `fn .. -> impl Future<Output = T> + Send` puts the bound in the contract. Traits using `async fn` or `impl Future` are not dyn-compatible, so `dyn` needs a boxed-future twin.

**Do.** A public port with `Send` futures; an in-memory adapter:
```rust
# use std::collections::HashMap;
# #[derive(Debug, thiserror::Error)] #[error("store unavailable")] pub struct StoreError;
# pub struct MemoryStore { entries: HashMap<String, String> }
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
```rust,fragment
/// Generates `Clock` (Send futures) from `LocalClock` (no Send bound).
#[trait_variant::make(Clock: Send)]
pub trait LocalClock {
    /// Current time in milliseconds since the epoch.
    async fn now_millis(&self) -> u64;
}
```
For `dyn` (heterogeneous plug-ins, API-09), add a dyn-compatible twin with a named boxed-future alias and a blanket impl, so adapters implement only the static trait:
```rust
# use std::pin::Pin;
# #[derive(Debug, thiserror::Error)] #[error("store unavailable")] pub struct StoreError;
# pub trait Store: Send + Sync {
#     fn get(&self, k: &str) -> impl Future<Output = Result<Option<String>, StoreError>> + Send;
# }
/// A boxed, `Send` future borrowing for `'a`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Dyn-compatible twin of [`Store`] for heterogeneous collections and plug-ins.
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
**Why.** A detached `tokio::spawn` outlives its caller: its panic is lost, its errors are unobserved, shutdown cannot wait for it, and it leaks if it loops. An owner (a `JoinSet`, a stored `JoinHandle`, a `TaskTracker`) joins or aborts it, so task lifetimes nest like scopes. The same holds for std threads: keep each `JoinHandle` and join it on the shutdown path (ASY-15), or use `thread::scope`. Dropping a `JoinHandle` detaches the thread, and returning from `main` kills it mid-write.

| Owner | Use when | On drop |
|---|---|---|
| `JoinSet<T>` | fan-out within one function; collect results | aborts all remaining tasks |
| `JoinHandle<T>` stored in a struct or returned | one long-lived task (actor, listener) | detaches: keep it and `.await` or `.abort()` it |
| `tokio_util::task::TaskTracker` | many long-lived tasks spawned over time, waited on at shutdown | nothing; call `close()` then `wait().await` |
| `std::thread::JoinHandle<T>` | a dedicated OS thread (ASY-15) | detaches: keep it and `join()` it |

**Do.**
```rust
# use std::sync::Arc;
# use tokio::task::{JoinError, JoinSet};
# #[derive(Debug, thiserror::Error)] #[error("store unavailable")] pub struct StoreError;
# #[derive(Debug, thiserror::Error)]
# pub enum GetManyError { #[error(transparent)] Store(#[from] StoreError), #[error(transparent)] Task(#[from] JoinError) }
# pub trait Store: Send + Sync {
#     fn get(&self, k: &str) -> impl Future<Output = Result<Option<String>, StoreError>> + Send;
# }
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

A task that panicked is a bug, not a retryable failure. Log it with the task's identity and start an orderly shutdown so the supervisor restarts the process; never respawn it and carry on:
```rust
# use tokio::task::JoinSet;
# use tokio_util::sync::CancellationToken;
# #[derive(Debug, thiserror::Error)] pub enum ServiceError { #[error("a service task panicked")] TaskPanicked }
/// Waits for every service task; the first failure cancels the rest.
///
/// # Errors
///
/// The first task error, or [`ServiceError::TaskPanicked`].
pub async fn supervise(
    mut tasks: JoinSet<Result<(), ServiceError>>,
    shutdown: CancellationToken,
) -> Result<(), ServiceError> {
    let mut outcome = Ok(());
    while let Some(joined) = tasks.join_next().await {
        let failure = match joined {
            Ok(Ok(())) => continue,
            Ok(Err(err)) => err,
            Err(join_err) if join_err.is_panic() => {
                tracing::error!(task = %join_err.id(), "task panicked; shutting down");
                ServiceError::TaskPanicked
            }
            Err(join_err) => {
                tracing::debug!(task = %join_err.id(), "task cancelled during shutdown");
                continue;
            }
        };
        shutdown.cancel();
        if outcome.is_ok() {
            outcome = Err(failure);
        }
    }
    outcome
}
```
**Don't.**
```rust,ignore
tokio::spawn(async move { sync_forever(db).await });  // detached: nobody joins, errors vanish
```
**Detect.** A `tokio::spawn(` or `thread::Builder` spawn whose `JoinHandle` is not bound, stored or returned; `let _ = ...spawn`; a `JoinError` handled without checking `is_panic`.

## Graceful shutdown and cancellation

### ASY-05 · Cooperative cancellation with `CancellationToken`
**Why.** Dropping a future cancels it at whatever `.await` it is parked on, possibly mid-write. A token lets the task stop at a point it chooses, finish the current unit of work, and release resources. Pair it with a tracker so shutdown can wait.

**Do.** `select!` on `token.cancelled()` and a cancel-safe operation. `biased;` polls branches in order, so shutdown wins over a busy queue. The arm *body* runs to completion: a job already started is never cut off.
```rust
# use std::sync::Arc;
# use std::sync::atomic::{AtomicU64, Ordering};
# use tokio::sync::mpsc;
# use tokio_util::sync::CancellationToken;
# use tokio_util::task::TaskTracker;
# async fn handle(_job: u64) {
#     tokio::task::yield_now().await;
# }
pub struct Workers {
    tracker: TaskTracker,
    token: CancellationToken,
    processed: Arc<AtomicU64>,
}

impl Workers {
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
                            // Relaxed: a statistic; no other memory depends on it.
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
        // Relaxed: every worker has finished, and `wait` orders their increments before this load.
        self.processed.load(Ordering::Relaxed)
    }
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

### ASY-06 · No blocking calls in async code (H-14)
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

### H-14 · Nothing that blocks is held across `.await`; `Drop` never blocks
H-14 forbids blocking in async code (ASY-06) and holding a lock guard, `RefCell` borrow, entered tracing span or `watch::Ref` across `.await`.

**Why.** A `std::sync::MutexGuard` held across `.await` blocks every other task that wants the lock for as long as the awaited I/O takes, can deadlock the executor, and makes the future `!Send`. The others fail the same way: a second `RefCell` borrow panics; an entered span stays entered on the worker thread, so it attaches to whatever that thread polls next and corrupts the trace; a `watch::Ref` holds the channel's read lock, so the sender blocks.

**Do.** Take the lock in a statement that ends before any `.await`. The justification for shared mutable state is written on the type (OWN-11).
```rust
# use std::collections::HashMap;
# use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
# #[derive(Debug, thiserror::Error)] #[error("store unavailable")] pub struct StoreError;
# pub trait Store: Send + Sync {
#     fn get(&self, k: &str) -> impl Future<Output = Result<Option<String>, StoreError>> + Send;
# }
/// Values fetched from a [`Store`], shared by every request handler.
pub struct Cache {
    entries: Mutex<HashMap<String, Arc<str>>>,
}

impl Cache {
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
}
```
Copy out of a `watch::Ref` in its own statement, and attach a span to the future instead of entering it:
```rust
# use tokio::sync::watch;
# use tracing::Instrument as _;
# async fn send_batch(_limit: u64) {
#     tokio::task::yield_now().await;
# }
/// Sends one batch under the current rate limit, traced as `batch`.
pub async fn flush(limits: &watch::Receiver<u64>) {
    // The read lock ends at the `;`.
    let limit = *limits.borrow();
    send_batch(limit).instrument(tracing::info_span!("batch", limit)).await;
}
```
**Don't.**
```rust,ignore,expect=clippy::await_holding_lock+clippy::await_holding_invalid_type
# use std::collections::HashMap;
# use std::sync::{Mutex, PoisonError};
# use tokio::sync::watch;
# async fn fetch(_key: &str) -> String { String::new() }
# async fn send_batch(_limit: u64) {}
pub async fn fill(entries: &Mutex<HashMap<String, String>>, key: &str) {
    let mut map = entries.lock().unwrap_or_else(PoisonError::into_inner); // std guard...
    let value = fetch(key).await;                                         // ...held across .await
    map.insert(key.to_owned(), value);
}

pub async fn flush(limits: &watch::Receiver<u64>) {
    let span = tracing::info_span!("batch");
    let _entered = span.enter();  // entered across .await
    let limit = limits.borrow();  // read lock across .await
    send_batch(*limit).await;
}
```
**`Drop` never blocks.** `Drop` runs wherever the value dies, including on an executor thread, so it only signals: cancel a token, abort a task, close a channel. Waiting (`JoinHandle::join`, `block_on`, a blocking `recv`, a contended lock) belongs in an explicit `shutdown(self)` or `async fn close(self)` that callers invoke (ERR-13):
```rust
# use tokio_util::sync::CancellationToken;
# pub struct Workers { token: CancellationToken }
impl Drop for Workers {
    // Signal only: callers wait in `shutdown(self).await`.
    fn drop(&mut self) {
        self.token.cancel();
    }
}
```
**Detect.** `clippy::await_holding_lock`, `await_holding_refcell_ref`, and `await_holding_invalid_type`, which the template's `clippy.toml` drives through `await-holding-invalid-types` (`tracing::span::Entered`, `tracing::span::EnteredSpan`, `tokio::sync::watch::Ref`); `future_not_send` catches most of them too, since each makes the future `!Send`; `significant_drop_tightening`. A blocking `Drop` passes every lint: review `impl Drop` for `join`, `block_on`, `recv` and `lock`.

## Channels

### ASY-07 · H-13 · Bounded channels, chosen by pattern
**Why.** An unbounded channel converts a slow consumer into unbounded memory growth and hides overload until the process is killed. A bounded channel pushes back: `send().await` waits, `try_send` reports `Full`, and the producer decides to wait, shed or fail.

| Pattern | Channel | Notes |
|---|---|---|
| Work queue, many producers to one consumer | `tokio::sync::mpsc::channel(n)` | never `unbounded_channel` (H-13) |
| Request/response to an actor | `mpsc` for commands + `oneshot` per reply | reply error means requester gone |
| Fan-out events to every subscriber | `broadcast::channel(n)` | slow receivers get `RecvError::Lagged(k)`; handle it |
| Latest value (config, health, progress) | `watch::channel(v)` | receivers see only the newest value; copy out of `borrow()` (H-14) |
| Sync threads | `std::sync::mpsc::sync_channel(n)` | not `std::sync::mpsc::channel()` (unbounded) |

**Do.** Choose capacity deliberately (a named constant, CRF-09). At the edge, shed load instead of queueing forever:
```rust
# use tokio::sync::mpsc::{self, error::TrySendError};
# pub enum SubmitError { Busy, Stopped }
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
# use std::num::NonZeroU32;
# use std::time::Duration;
# use tokio::time;
# pub trait Transient { fn is_transient(&self) -> bool; }
# pub enum RetryError<E> { Failed { attempts: u32, source: E }, TimedOut { attempts: u32 } }
# #[derive(Clone, Copy)]
# pub struct RetryPolicy { max_attempts: NonZeroU32, attempt_timeout: Duration }
# impl RetryPolicy { fn backoff(self, attempt: u32) -> Duration { self.attempt_timeout.saturating_mul(attempt) } }
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

## Actors and shared state

### ASY-09 · Message passing first; one task owns the state
**Why.** When one task owns the state and others send it commands, there is no lock to hold across `.await`, no poisoning, and invariants live in one sequential loop. The handle is a cloneable `mpsc::Sender`; the actor stops when the last handle drops.

**Do.**
```rust
# use std::num::NonZeroUsize;
# use tokio::sync::{mpsc, oneshot};
# use tokio::task::JoinHandle;
# #[derive(Debug)] pub enum LedgerError { Overflow, Stopped }
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

When state must be shared instead, `std::sync::Mutex` is right for short critical sections that never span `.await` (H-14, ASY-14). `tokio::sync::Mutex` is right only when the guard must span `.await`, typically to serialize use of one I/O resource so writes do not interleave (a `FrameWriter` holding `tokio::sync::Mutex<W>` across `write_all(len)`, `write_all(frame)`, `flush()`). It is slower than `std::sync::Mutex`, and an actor owning the resource is usually cleaner.

## Threads without async

### ASY-10 · `thread::scope` to borrow, `rayon` for data parallelism
**Why.** A plain spawned thread needs `'static` data, which pushes people to clone or `Arc` everything, or leak (H-07). Scoped threads borrow from the caller and are joined before `scope` returns. But a scoped handle that is never joined explicitly is joined by `scope` itself, which panics if that thread panicked: a `?`, `try_fold` or `find` over the handles turns two worker panics into a panic of the caller. Join every handle before inspecting any result. rayon splits work across a pool with work stealing and no manual chunking.

**Do.**
```rust
# use std::num::NonZeroUsize;
# use std::{io, thread};
# pub enum ParallelError { Spawn(io::Error), WorkerPanicked }
/// Sums `values` across `threads` scoped threads, saturating at `u64::MAX`.
///
/// # Errors
///
/// [`ParallelError::Spawn`] if the OS refuses a thread,
/// [`ParallelError::WorkerPanicked`] if any worker panicked.
pub fn parallel_sum(values: &[u64], threads: NonZeroUsize) -> Result<u64, ParallelError> {
    let chunk_len = values.len().div_ceil(threads.get()).max(1);
    thread::scope(|scope| {
        // Spawn every worker before joining any, or they would run one at a time.
        let mut workers = Vec::new();
        for (index, chunk) in values.chunks(chunk_len).enumerate() {
            workers.push(
                thread::Builder::new()
                    .name(format!("sum-{index}"))
                    .spawn_scoped(scope, move || {
                        chunk.iter().fold(0_u64, |sum, value| sum.saturating_add(*value))
                    }),
            );
        }
        // Join every worker before returning, even after a failure: `scope`
        // panics on any un-joined handle whose thread panicked.
        let mut total = Ok(0_u64);
        for worker in workers {
            let part = match worker {
                // The panic payload has no diagnostic value here (ERR-04).
                Ok(handle) => handle.join().map_err(|_payload| ParallelError::WorkerPanicked),
                Err(err) => Err(ParallelError::Spawn(err)),
            };
            if let Ok(sum) = total {
                total = part.map(|part| sum.saturating_add(part));
            }
        }
        total
    })
}
```
For data parallelism, rayon needs no chunking or joining: `docs.par_iter().map(|doc| fnv1a(doc)).collect()`. Never call rayon from an async task directly (it blocks the worker); wrap it in `spawn_blocking` or hand it off via a `oneshot`. Atomics used by the workers follow ASY-13.

**Detect.** `try_fold`, `?` or `find` applied to `ScopedJoinHandle::join` results; `scope.spawn(` (ASY-15).

### ASY-15 · Spawn threads with `thread::Builder`, named, and join them
**Why.** `thread::spawn` and `Scope::spawn` panic when the OS refuses a thread (process limits, containers), which is an H-03 panic by another route. `Builder::spawn` and `Builder::spawn_scoped` return `io::Result`. A name shows up in panic messages, debuggers and profilers. `Builder` panics if the name contains a NUL byte, so names are literals or formatted from numbers.

**Do.**
```rust
# use std::sync::mpsc;
# use std::{io, thread};
# pub struct AuditPanicked;
# fn write_line(_line: &str) {}
/// A background thread that writes audit lines until its channel closes.
pub struct Auditor {
    lines: mpsc::SyncSender<String>,
    thread: thread::JoinHandle<usize>,
}

impl Auditor {
    /// Starts the audit thread with a bounded queue (ASY-07).
    ///
    /// # Errors
    ///
    /// The OS error if the thread cannot be created.
    pub fn start(queue: usize) -> io::Result<Self> {
        let (lines, inbox) = mpsc::sync_channel::<String>(queue);
        let thread = thread::Builder::new()
            .name("auditor".to_owned())
            .spawn(move || inbox.iter().inspect(|line| write_line(line)).count())?;
        Ok(Self { lines, thread })
    }

    /// Closes the queue and waits for the thread (ASY-04); returns lines written.
    ///
    /// # Errors
    ///
    /// [`AuditPanicked`] if the thread panicked.
    pub fn shutdown(self) -> Result<usize, AuditPanicked> {
        drop(self.lines);
        self.thread.join().map_err(|_payload| AuditPanicked)
    }
}
```
**Don't.**
```rust,ignore,expect=clippy::disallowed_methods+scan:thread-spawn
# use std::thread;
# fn audit_forever() {}
pub fn start() {
    thread::spawn(audit_forever); // panics if the OS refuses; the dropped handle detaches it
}
```
**Detect.** `clippy::disallowed_methods`: the template's `clippy.toml` lists `std::thread::spawn` and `std::thread::Scope::spawn`, and fires in tests too, so tests use `Builder` as well. Scanner `thread-spawn` (hard in production), and `detached-spawn` for an unbound handle.

## Memory ordering

### ASY-13 · Justify every atomic ordering
Every ordering carries a comment: `Relaxed` says why no other memory depends on the value, `Acquire`/`Release`/`AcqRel` name the operation they pair with, and `SeqCst` names the cross-variable total order it needs. Orderings are literal constants. Use `update`/`try_update` over hand-written CAS loops, `_weak` inside retry loops, and rule out ABA. Unsure means a `Mutex`, not `SeqCst`.

**Why.** An ordering is a claim about which *other* memory an operation synchronizes. `Relaxed` still gives each atomic a single modification order, so counters and id generators need nothing more. `Release` tells the reader to look for an `Acquire` on the same variable. `SeqCst` claims a dependency on every other `SeqCst` operation in the program, which is almost never what the code needs and is the hardest to review: it is a warning sign, not a safe default. An atomic holds one independent value; two values that must agree belong in a `Mutex` (or an actor), and one-time initialization is `OnceLock`/`LazyLock` (MOD-03).

| Situation | Ordering | Required comment |
|---|---|---|
| Counter, statistic, id, gauge; nothing else is read or written because of its value | `Relaxed` | why no other memory depends on it |
| Flag or counter that tells another thread "the data I wrote before this is ready" | writer `store(.., Release)`, reader `load(Acquire)` | each side names the other |
| RMW that claims (takes a slot, a job, a buffer) | `Acquire` | names the `Release` that frees it |
| RMW that frees or publishes (returns a slot, "done" decrement) | `Release`, plus `fence(Acquire)` on the zero path if the zero-observer then reads | names the partner |
| RMW that both consumes and publishes (hand-off `swap`) | `AcqRel` | names both partners |
| CAS failure ordering | `Relaxed`, or `Acquire` if the failed value is used to read published data | why |
| One global order across two or more atomics (store own flag, then load the other's) | `SeqCst` | the cross-variable property |
| Unsure | not an ordering: a `Mutex`, `OnceLock`, channel or `Barrier` | none |

- A counter is a statistic only if no decision about other memory is made from its value. A "last one out cleans up" or "all workers done" counter synchronizes: `Release` on the decrement, `Acquire` where zero is observed.
- Use the dedicated RMW first (`fetch_add`, `fetch_max`, `fetch_or`, `swap`). For a computed update use `update`, for a conditional one `try_update` (both 1.95), not `fetch_update` (deprecated in 1.99 as a rename of `try_update`, MOD-07) and not a hand-written CAS loop. If a CAS loop is unavoidable, use `compare_exchange_weak` inside it and `compare_exchange` for a single attempt.
- `fetch_add` and `fetch_sub` wrap silently in every build. Ids, balances and capacities use `try_update(.., |n| n.checked_add(1))` (ERR-09).
- A CAS or `try_update` on a value that can return to an earlier state (a slot index, a free-list head, a recycled id) documents why ABA cannot happen, packs a generation counter into the value, or uses a `Mutex`.
- Pass orderings as literal `Ordering::X` constants. rustc's `invalid_atomic_ordering` (deny by default) rejects invalid constants such as a `Release` load; an `Ordering` passed at run time skips that check and panics instead (H-03).
- Put the ordering on the operation. Use `fence` only when one fence covers several atomics or applies on one branch, with a comment naming its partner. `compiler_fence` never appears in application code.
- x86-64 hides most ordering mistakes. Crates with a non-`Relaxed` ordering run Miri and an aarch64 job in CI (TST-17).

**Do.**
```rust
# use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
# pub struct IdsExhausted;
/// Hands out unique ids; never wraps around.
pub struct Ids {
    next: AtomicU32,
}

impl Ids {
    /// The next unused id.
    ///
    /// # Errors
    ///
    /// [`IdsExhausted`] once the id space is used up.
    pub fn allocate(&self) -> Result<u32, IdsExhausted> {
        // Relaxed: only uniqueness matters, which the modification order of
        // `next` gives; no other memory is published through it.
        self.next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_exhausted_at| IdsExhausted)
    }
}

/// A value written once by one thread and read by any other.
/// (`OnceLock<u64>` is simpler; this shows the pairing comments.)
pub struct Published {
    value: AtomicU64,
    ready: AtomicBool,
}

impl Published {
    /// Publishes `value`.
    pub fn publish(&self, value: u64) {
        // Relaxed: made visible by the Release store below.
        self.value.store(value, Ordering::Relaxed);
        // Release: pairs with the Acquire in `get`; a thread that sees
        // `ready == true` also sees every write above.
        self.ready.store(true, Ordering::Release);
    }

    /// The value, once published.
    pub fn get(&self) -> Option<u64> {
        // Acquire: pairs with the Release in `publish`.
        if !self.ready.load(Ordering::Acquire) {
            return None;
        }
        // Relaxed: ordered after `publish`'s store by the Acquire above.
        Some(self.value.load(Ordering::Relaxed))
    }
}
```
**Don't.**
```rust,ignore,expect=scan:seqcst
pub fn next_id(next: &AtomicU32) -> u32 {
    next.fetch_add(1, Ordering::SeqCst) // "to be safe": pairs with nothing, and wraps to 0
}
pub fn is_ready(flag: &AtomicBool, order: Ordering) -> bool {
    flag.load(order) // a run-time `Release` here panics
}
```
**Detect.** rustc `invalid_atomic_ordering` (constants only); scanner `seqcst` (advisory, silenced by a comment naming the total order), `fetch-update` (advisory) and `compiler-fence` (hard in production); review for a missing or vague ordering comment, `Ordering` parameters, and a `Release` without its `Acquire`.

## Lock discipline

### ASY-14 · Scope guards, lock in one order, never re-enter
Never leave a guard as a temporary in a `match`/`if let`/`while let` scrutinee whose body does more than read. Never lock what the thread already holds. Acquire several locks in one documented order. Keep state that shares an invariant under one lock. Never call callbacks or `dyn` code, or return a guard from a `pub fn`, while holding one.

**Why.** Rust rules out data races, not deadlocks or torn invariants. A guard that lives longer than it looks, a second `lock()` of a `Mutex` the thread holds (std documents that it "might panic or deadlock"), two locks taken in opposite orders, and a callback that locks again all deadlock. Splitting one lock into several for contention (PERF-12) creates the last two risks, so fields that share an invariant stay under one lock.

**Where a scrutinee guard lives.** Checked on 1.98.1, edition 2024, with the template lints. A temporary in a scrutinee lives to the end of the whole construct; edition 2024 only releases it before `else`:

| Construct | Guard held in the body? | `significant_drop_in_scrutinee` |
|---|---|---|
| `if let P = m.lock()?.pop() { .. } else { .. }` | then-block: yes. else-block: no (edition 2021 held it there too) | fires |
| `match m.lock()?.pop() { .. }`, also `match m.lock()?.len()` | yes, in every arm | fires |
| `while let P = m.lock()?.pop() { .. }` | yes, in each iteration | fires |
| `for x in m.lock()?.clone() { .. }` | yes, for the whole loop | fires |
| let chain `if let P = m.lock()?.pop() && ok { .. }`, in any position | yes | **silent** |
| if-let guard `Some(_) if let P = m.lock()?.pop() => ..` | yes, in that arm | **silent** |
| `if m.lock()?.is_empty() { .. }`, `while` condition, plain match guard | no | n/a |
| `let x = m.lock()?.pop();` and `let P = m.lock()?.pop() else { .. };` | no: dropped at the `;` (and before `else`) | n/a |

`significant_drop_tightening` catches none of these; it only shortens named guards. In async code `await_holding_lock` and `future_not_send` do catch every shape, let chains included, once the body awaits.

**Do.** Take the value in its own `let` statement. Lock once at the public boundary and give private helpers `&mut Inner`:
```rust
# use std::collections::{HashMap, VecDeque};
# use std::sync::{Mutex, MutexGuard, PoisonError};
# pub enum TransferError { SameAccount(u32), Insufficient, Overflow }
# pub struct Job;
# fn run(_job: Job) {}
/// Jobs waiting to run.
pub struct Queue {
    jobs: Mutex<VecDeque<Job>>,
}

impl Queue {
    /// Runs the next job, if any, with the lock released.
    pub fn run_next(&self) {
        // Poison: each critical section is one `VecDeque` call, so it cannot tear.
        let next = self.jobs.lock().unwrap_or_else(PoisonError::into_inner).pop_front();
        if let Some(job) = next {
            run(job); // the guard was dropped at the `;` above
        }
    }
}

/// Balances that change together, so they share one lock.
pub struct Accounts {
    inner: Mutex<Inner>,
}

struct Inner {
    balances: HashMap<u32, u64>,
}

impl Accounts {
    /// Moves `amount` from one account to another.
    ///
    /// # Errors
    ///
    /// [`TransferError`] when the accounts match or a balance would leave `u64`.
    pub fn transfer(&self, from: u32, to: u32, amount: u64) -> Result<(), TransferError> {
        // Lock once here; `Inner` methods never lock.
        self.lock().transfer(from, to, amount)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Poison: `Inner::transfer` checks everything before it writes, so it cannot tear.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Inner {
    fn transfer(&mut self, from: u32, to: u32, amount: u64) -> Result<(), TransferError> {
        if from == to {
            return Err(TransferError::SameAccount(from));
        }
        let debited = self.balance(from).checked_sub(amount).ok_or(TransferError::Insufficient)?;
        let credited = self.balance(to).checked_add(amount).ok_or(TransferError::Overflow)?;
        self.balances.insert(from, debited);
        self.balances.insert(to, credited);
        Ok(())
    }

    fn balance(&self, account: u32) -> u64 {
        self.balances.get(&account).copied().unwrap_or(0)
    }
}
```
When two locks are unavoidable (per-account locks under measured contention), take them in one order documented on the type, such as ascending `id` whichever way the money moves, and reject the same object before locking, since locking one `Mutex` twice deadlocks: `let (first, second) = if a.id < b.id { (a, b) } else { (b, a) };`. While a guard is live, call no closure, callback or `dyn` method (it may lock again or block), and never return a guard from a `pub fn`: return a copy or an `Arc` snapshot (see Snapshot swap).

**Don't.**
```rust,ignore,expect=clippy::significant_drop_in_scrutinee
# use std::collections::VecDeque;
# use std::sync::{Mutex, PoisonError};
# pub struct Job;
# fn run(_job: Job) {}
pub fn run_next(jobs: &Mutex<VecDeque<Job>>, enabled: bool) {
    if let Some(job) = jobs.lock().unwrap_or_else(PoisonError::into_inner).pop_front() {
        run(job); // runs with the lock held, even in edition 2024
    }
    if enabled && let Some(job) = jobs.lock().unwrap_or_else(PoisonError::into_inner).pop_front() {
        run(job); // the same bug in a let chain, and no lint sees it
    }
}
```
**Detect.** `clippy::significant_drop_in_scrutinee` (enabled; misses let chains and if-let guards), `significant_drop_tightening`, `await_holding_lock`; Miri reports a same-thread double lock as a deadlock (TST-17). Lock order, callbacks under a lock and shared invariants split across locks: review.

### ASY-16 · Wait, don't spin
**Why.** A thread that loops on an atomic, `try_lock` or `yield_now` burns a core and can starve the very thread it waits for. `Condvar::wait` may wake spuriously and may panic when one `Condvar` is used with more than one `Mutex`; `wait_while` and `wait_timeout_while` re-check the predicate for you. On Linux, std's `Mutex` already spins briefly before it sleeps; do not add another spin in front of it.

**Do.** One `Mutex` per `Condvar`, in the same struct. Change the state under the lock, notify after. `notify_one` when any single waiter can make progress, `notify_all` only when every waiter must re-check (shutdown). Prefer the timeout form so the wait is bounded (H-13):
```rust
# use std::sync::{Condvar, Mutex, PoisonError};
# use std::time::Duration;
/// A work list that consumers wait on.
pub struct WorkList {
    items: Mutex<Vec<u64>>,
    /// Paired with `items` and no other mutex.
    not_empty: Condvar,
}

impl WorkList {
    /// Adds an item and wakes one waiting consumer.
    pub fn push(&self, item: u64) {
        // Poison: every critical section is one `Vec` call, so it cannot tear.
        self.items.lock().unwrap_or_else(PoisonError::into_inner).push(item);
        self.not_empty.notify_one();
    }

    /// Takes an item, waiting up to `timeout` for one to arrive.
    pub fn pop_timeout(&self, timeout: Duration) -> Option<u64> {
        // The lock goes straight into the wait: binding it to a `let` first
        // trips `significant_drop_tightening`, whose suggested rewrite is broken.
        let (mut items, _timed_out) = self
            .not_empty
            .wait_timeout_while(
                self.items.lock().unwrap_or_else(PoisonError::into_inner),
                timeout,
                |items| items.is_empty(), // re-checked after every wakeup, spurious or not
            )
            .unwrap_or_else(PoisonError::into_inner);
        let next = items.pop();
        drop(items);
        next
    }
}
```
Prefer a channel or a `Condvar` to `thread::park`. When parking, loop on an explicit condition, since `park` may return spuriously and several `unpark`s collapse into one; use `park_timeout` rather than `sleep` for periodic work that must wake early.

**Don't.**
```rust,ignore,expect=clippy::missing_spin_loop
# use std::sync::atomic::{AtomicBool, Ordering};
# use std::sync::{Condvar, Mutex, PoisonError};
static READY: AtomicBool = AtomicBool::new(false);

pub fn wait_until_ready() {
    // Acquire: pairs with the Release store in `start`.
    while !READY.load(Ordering::Acquire) {} // burns a core; the lint's `spin_loop()` hint does not fix that
}

pub fn wait_for_item(items: &Mutex<Vec<u64>>, not_empty: &Condvar) -> Option<u64> {
    let mut guard = items.lock().unwrap_or_else(PoisonError::into_inner);
    while guard.is_empty() {
        guard = not_empty.wait(guard).unwrap_or_else(PoisonError::into_inner); // hand-rolled `wait_while`, unbounded
    }
    guard.pop()
}
```
**Detect.** `clippy::missing_spin_loop` fires on an empty loop over a load of a local or static atomic, but not through a `&AtomicBool` parameter or on a CAS loop, and its `spin_loop()` hint does not remove the spin. Review for `Condvar::wait(`, and `try_lock` or `yield_now` inside loops.

### ASY-17 · Use std's synchronization primitives; never build your own
**Why.** A lock, once-cell, semaphore or channel built from atomic flags compiles in safe code and passes tests on x86-64, then loses wakeups, misses an `Acquire` or spins under contention on other hardware. std's primitives are tuned per platform (futexes, SRW locks, queue locks).

| Need | Use |
|---|---|
| Mutual exclusion | `Mutex` (`RwLock` only with a benchmark, PERF-12) |
| One-time initialization | `OnceLock`, `LazyLock` (MOD-03) |
| Wait for a condition | `Condvar` (ASY-16) |
| Wait for N threads | `Barrier`, or join the handles |
| Hand work to a thread | `mpsc::sync_channel(n)` (ASY-07) |
| Limit concurrency in async code | `tokio::sync::Semaphore` (PERF-10) |
| Read-mostly shared data | an `Arc` snapshot swap (below) |

`parking_lot`, `crossbeam`, `arc-swap` and similar crates are H-16 decisions with a benchmark (PERF-01).

**Don't.**
```rust,ignore
/// A "lock": a flag and a spin, no fairness, no parking, and every guarded
/// value needs its own ordering argument.
pub struct SpinLock { locked: AtomicBool }
impl SpinLock {
    pub fn lock(&self) { while self.locked.swap(true, Ordering::Acquire) { std::hint::spin_loop(); } }
    pub fn unlock(&self) { self.locked.store(false, Ordering::Release); }
}
```
**loom.** Model checking with loom explores every interleaving, but adopting it is a dependency decision (H-16), reserved for crates whose correctness rests on orderings. It needs `loom` under `[target.'cfg(loom)'.dependencies]`, `std::sync` imports swapped behind `#[cfg(loom)]`, and the custom cfg declared, a lint-config change (H-05). Without the declaration, the first `cfg(loom)` trips `unexpected_cfgs` and fails the zero-warnings gate:
```toml
[workspace.lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ["cfg(loom)"] }
```
Application code relies on std primitives plus Miri (TST-17).

**Detect.** Review: an `AtomicBool` named like a lock (`locked`, `busy`, `in_use`), `swap(true, ..)` or `compare_exchange` in a loop, a hand-written once-flag beside a value.

## Send and Sync

### ASY-11 · Keep public types and futures `Send`, and prove it
**Why.** Adding an `Rc` field (`!Send`) or a `Cell`/`RefCell` field (`!Sync`) silently changes a public type's auto traits, breaking every downstream `tokio::spawn`, far from the change. A const assertion fails in the crate that made the change. Futures cannot be named in an assertion, so `clippy::future_not_send` (enabled) checks that every `async fn` returns a `Send` future. It ignores bare type parameters but not what they imply: holding a `&T` across `.await` needs `T: Sync` (see `try_each` in ASY-12). A deliberately `!Send` future needs an approved `#[expect]` (H-05).

**Do.** Reason from the fields, then assert:

| Type | `Send` | `Sync` | Consequence |
|---|---|---|---|
| `Rc<T>`, raw pointers | no | no | single thread only |
| `Cell<T>`, `RefCell<T>`, `OnceCell<T>` | if `T: Send` | no | may move to a thread, never be shared: an `Arc<RefCell<T>>` cannot be sent, and `clippy::arc_with_non_send_sync` flags building one |
| `MutexGuard<'_, T>` | no | if `T: Sync` | a future holding one across `.await` is `!Send` (H-14) |
| `Mutex<T>` | if `T: Send` | if `T: Send` | makes a `Send` value shareable |
| `RwLock<T>` | if `T: Send` | if `T: Send + Sync` | swapping `Mutex<Cell<_>>` for `RwLock` fails to compile |
| `Arc<T>` | if `T: Send + Sync` | if `T: Send + Sync` | |
```rust
# use std::cell::Cell;
# use std::marker::PhantomData;
# pub struct Workers;
# pub struct SubmitError;
// Compile-time proof that these types can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Workers>();
    assert_send_sync::<SubmitError>();
};

/// Bound to the thread that created it: `!Send` and `!Sync`, without `unsafe`.
pub struct ThreadBound {
    _not_send: PhantomData<*const ()>,
}

/// May move to another thread but never be shared: `Send` and `!Sync`.
pub struct Unshared {
    _not_sync: PhantomData<Cell<()>>,
}
```
A deliberately `!Send` or `!Sync` type says so in its docs, and a `compile_fail` doctest proves it (TST-08). A type that would need `unsafe impl Send`/`Sync` is redesigned (H-01).

**Detect.** `clippy::future_not_send`, `arc_with_non_send_sync`; review for public types without an assertion.

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
pub async fn try_each<T: Sync, E>(
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
`T: Sync` keeps the future `Send` (ASY-11), since it holds a `&T` across `.await`. Callers write `try_each(&items, async |n: &i32| { seen.push(*n); Ok::<(), Infallible>(()) })`, mutating a local `Vec` with no `Arc`. Limit (checked on 1.98): `AsyncFn*` bounds cannot require the call's future to be `Send`. Awaiting in place is fine, but `tokio::spawn` of a future that calls an `AsyncFnMut` closure mutating its captures fails with "higher-ranked lifetime error". For work that must be spawned, keep `F: Fn() -> Fut, Fut: Future + Send`.
**Don't.**
```rust,ignore
pub async fn try_each<T, F, Fut>(items: &[T], f: F) where F: Fn(&T) -> Fut, Fut: Future<Output = ()> { /* ... */ }
try_each(&items, |n| { let seen = Arc::clone(&seen); async move { seen.lock()?.push(*n) } }) // clone + lock for a local Vec
```

## Streams

Concurrency without spawning: a stream of futures polled inside the current task, bounded by `buffer_unordered(n)` (completion order) or `buffered(n)` (input order). Dropping the stream cancels in-flight work; nothing leaks (ASY-04).
```rust,fragment
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

## Sharing patterns

### Snapshot swap (read-copy-update)
Read-mostly data (configuration, routing tables) sits behind `RwLock<Arc<T>>`. Readers clone the `Arc` in one statement and work on the snapshot with no lock held; writers build the new value outside the lock and swap the `Arc`. A reader never holds the lock across a request, and poison cannot tear a single assignment. `arc-swap` is the lock-free upgrade (H-16); the full pattern is patterns.md #30.
```rust
# use std::sync::{Arc, PoisonError, RwLock};
# pub struct Config;
/// The live configuration.
pub struct Live {
    current: RwLock<Arc<Config>>,
}

impl Live {
    /// The current configuration; the read lock ends at the `;`.
    pub fn snapshot(&self) -> Arc<Config> {
        // Poison: writers only replace the `Arc` in one assignment.
        Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Replaces the configuration; readers keep the snapshot they hold.
    pub fn replace(&self, config: Config) {
        let next = Arc::new(config);
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = next;
    }
}
```

### Split a borrow into role handles
To enforce "one producer, one consumer" without `Arc`, take `&mut self` once and hand out two role-restricted views: `fn split(&mut self) -> (Producer<'_>, Consumer<'_>)`, where both hold `&Pipe` (the queue behind its own `Mutex`), `Producer` has only `push` and `Consumer` only `pop`. The borrow checker forbids a second split while the handles live, and both work inside `thread::scope` (OWN-11).

## Testing async and concurrent code

- `#[tokio::test]` runs a current-thread runtime per test; use `flavor = "multi_thread"` only to test real parallelism.
- `#[tokio::test(start_paused = true)]` (tokio `test-util` feature, dev-dependency) freezes time and auto-advances when idle: timeouts, backoff and intervals run instantly and elapsed times are exact. Use `tokio::time::Instant`, not `std::time::Instant`.
- Never `sleep` to wait for a task; await its `JoinHandle`, a channel, or `TaskTracker::wait` (TST-10).
- Inject the jitter or RNG so waits are exact: with `retry(POLICY, no_jitter, async || ..)` failing twice, a paused-time test asserts `start.elapsed() == Duration::from_millis(100 + 200)`. Full worked example: `testing.md` §Determinism.
- Test shutdown: spawn, cancel the token, and assert `shutdown().await` returns; a hang here is a leaked task.
- Code with a non-`Relaxed` ordering, a `Condvar` or hand-written thread coordination runs under Miri and on an aarch64 runner in CI; passing on x86-64 is not evidence (TST-17).

## Review checklist
- [ ] Is async used only for I/O-bound work, with CPU work (over ~100 µs between awaits) on `spawn_blocking` or rayon? (ASY-01)
- [ ] Does only the binary start a runtime, with libraries free of `#[tokio::main]`/`block_on`? (ASY-02)
- [ ] Do async traits use native syntax, `impl Future + Send` for public ports, and a boxed alias for `dyn`? (ASY-03)
- [ ] Is every spawned task and thread owned and joined or aborted on shutdown, and is a panicked task treated as fatal? (ASY-04, H-13)
- [ ] Is cancellation cooperative, and is every future inside `select!` cancel safe or documented? (ASY-05)
- [ ] Is there no blocking call, and no lock guard, `RefCell` borrow, entered span or `watch::Ref` across `.await`? Does every `Drop` only signal? (ASY-06, H-14)
- [ ] Is every channel bounded, and chosen by pattern (mpsc, broadcast, watch, oneshot)? (ASY-07, H-13)
- [ ] Does every external call have a timeout, and are retries bounded with backoff and jitter? (ASY-08)
- [ ] Is shared state owned by an actor where possible, with `std::sync::Mutex` for short sections and `tokio::sync::Mutex` only when holding across `.await` is required? (ASY-09)
- [ ] Do sync parallel sections use `thread::scope` or rayon, joining every scoped handle before inspecting results? (ASY-10)
- [ ] Are public types `Send + Sync`, asserted at compile time, and do `async fn`s return `Send` futures? (ASY-11)
- [ ] Do closure-taking async APIs use `AsyncFn*` bounds unless the future must be spawned? (ASY-12)
- [ ] Is an atomic the right tool: one independent value, not two values that must agree? (ASY-13; otherwise a `Mutex`)
- [ ] Does every `Relaxed` have a one-line reason, and does every `Release`/`Acquire`/`AcqRel` name its partner? Does every `Release` have an `Acquire`, and vice versa? (ASY-13)
- [ ] Does every `SeqCst` name its total-order property, or has it been downgraded? (ASY-13)
- [ ] Are orderings literal constants, never `Ordering` parameters? (ASY-13, H-03)
- [ ] Are dedicated RMWs used, with `update`/`try_update` instead of CAS loops, `_weak` in any remaining loop, and no `fetch_update`? (MOD-07, ASY-13)
- [ ] Is overflow decided for every `fetch_add`/`fetch_sub`, and ABA ruled out for every CAS on a recyclable value? (ERR-09, ASY-13)
- [ ] Is no guard a temporary in a `match`/`if let`/`while let`/let-chain scrutinee whose body does more than read? (ASY-14)
- [ ] Is nothing locked twice on one thread, are multiple locks taken in one documented order, and does state sharing an invariant sit under one lock? (ASY-14)
- [ ] Is no callback or `dyn` method called, and no guard returned from a `pub fn`, while a lock is held? (ASY-14)
- [ ] Are threads spawned with `thread::Builder`, named, with the `io::Error` propagated and the handle joined? (ASY-15)
- [ ] Does every `Condvar` wait go through `wait_while`/`wait_timeout_while` with one `Mutex`, and is there no busy-wait loop? (ASY-16)
- [ ] Are there no hand-built locks, once-cells, semaphores or channels? (ASY-17)
- [ ] Is the crate covered by the Miri and aarch64 CI jobs if it uses orderings or coordination? (TST-17)
