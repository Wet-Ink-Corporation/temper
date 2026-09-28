# Patterns

A catalog of complete, compiling patterns that apply the rules in the other references. Each ` ```rust ` block compiles on Rust 1.98.1 (edition 2024) and passes `cargo clippy --all-targets -- -D warnings` under the workspace template lints, checked by the doc-example harness (`tests/doc_examples.py`); hidden `# ` lines hold imports and stubs. The two ` ```rust,fragment ` blocks need a crate the harness does not have (`futures`, `clap`). A few blocks continue an earlier one (a binary using the library above it, a DTO using pattern 1's `Email`), and they say so. Read this when the design defaults in SKILL.md §3 point at a shape (newtype, builder, typestate, port, actor) and you want the idiomatic version to copy, or when a reviewer asks "what should this look like instead?".

Start from the index; read only the pattern you need. Snippets mostly omit `#[cfg(test)]` modules. Patterns 11, 22 and 26 keep theirs, because the fake or the test is part of the pattern. `use` lines are included so each block compiles as a module.

| # | Pattern | Rules |
|---|---|---|
| 1 | Validated newtype | API-02, API-03, API-07, ERR-03, OWN-02 |
| 2 | Id newtypes | API-02, OWN-16, CRF-09 |
| 3 | Params struct with `Default` and struct update | API-10, API-06, CRF-09, ERR-09 |
| 4 | Non-consuming builder | API-10, API-13, OWN-02 |
| 5 | Typestate builder | API-13, TST-08, OWN-04 |
| 6 | Sealed trait | API-12, API-17 |
| 7 | Extension trait | API-18, API-12, H-04 |
| 8 | RAII guard | API-11, H-07, OWN-12 |
| 9 | Enum state machine with `mem::take` / `mem::replace` | API-02, CRF-03, OWN-08, H-08 |
| 10 | Decider (event sourcing) | ARC-13, ARC-04, TST-03, ERR-10 |
| 11 | Port, adapter, fake, composition root | ARC-02, ARC-03, ARC-09, TST-09, ERR-04, ERR-10 |
| 12 | Layered errors: `thiserror` library, `anyhow` binary | ERR-01, ERR-03, ERR-04, ERR-05, ERR-06, H-09, ARC-14 |
| 13 | Iterator-returning accessor | OWN-13, API-08, MOD-06 |
| 14 | `Cow` normalization | OWN-10, PERF-02 |
| 15 | Arena graph with index handles | OWN-11, H-04, H-07 |
| 16 | Actor with `mpsc` and `oneshot` | ASY-09, ASY-07, ASY-04, H-13, OWN-11 |
| 17 | Graceful shutdown with `CancellationToken` and `JoinSet` | ASY-04, ASY-05, ASY-08, H-13 |
| 18 | Bounded concurrency | PERF-10, H-13, ASY-01, ASY-04 |
| 19 | DTO to domain with serde and `TryFrom` | OUT-01, OUT-02, OUT-05, API-03, ERR-04 |
| 20 | Versioned event enum with upcast | OUT-03, OUT-02, ARC-13 |
| 21 | CLI JSON output to locked stdout with `ExitCode` | OUT-06, PERF-07, ARC-14, ERR-06, H-06 |
| 22 | Redacted secret newtype | OUT-08, OUT-10, API-06 |
| 23 | Explicit `finish` with a best-effort `Drop` | ERR-13, H-10, H-14 |
| 24 | Hand back the consumed argument | ERR-14, H-08, ERR-01, OUT-08 |
| 25 | On-stack `dyn` for a local runtime choice | API-09, PERF-08 |
| 26 | Closure-friendly role trait | API-18, ARC-03, TST-09 |
| 27 | Kind-parameterized API | API-13, API-12, API-21 |
| 28 | Tree walk: `Visitor` with `ControlFlow`, moving `Fold` | CRF-03, H-03, H-08, PERF-10 |
| 29 | Service handle and named `dyn` wrapper | API-23, H-08, API-09, ARC-03 |
| 30 | Snapshot swap (read-copy-update) | OWN-11, PERF-12, ASY-14, H-02 |
| 31 | Checked value-object arithmetic | API-22, ERR-09, API-02 |
| 32 | Typed aggregate id | API-02, ARC-13, OWN-16 |

## 1. Validated newtype

**Use when.** A string or number has rules (format, range, length) that more than one function would otherwise re-check.

**Rules.** API-02, API-03, API-07, ERR-03, OWN-02

```rust
use std::fmt;
use std::str::FromStr;

/// An email address with one `@`, non-empty local part and domain, at most 254 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

/// Why a string is not an [`Email`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EmailError {
    /// Longer than the limit.
    #[error("email is {len} bytes; the limit is {max}")]
    TooLong {
        /// Actual length.
        len: usize,
        /// Limit.
        max: usize,
    },
    /// Zero or several `@`.
    #[error("email must contain exactly one `@`")]
    AtSign,
    /// Nothing before or after the `@`.
    #[error("email has an empty local part or domain")]
    EmptyPart,
}

impl Email {
    const MAX_LEN: usize = 254;

    /// The address as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Email {
    type Error = EmailError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if raw.len() > Self::MAX_LEN {
            return Err(EmailError::TooLong {
                len: raw.len(),
                max: Self::MAX_LEN,
            });
        }
        let Some((local, domain)) = raw.split_once('@') else {
            return Err(EmailError::AtSign);
        };
        if domain.contains('@') {
            return Err(EmailError::AtSign);
        }
        if local.is_empty() || domain.is_empty() {
            return Err(EmailError::EmptyPart);
        }
        Ok(Self(raw))
    }
}

impl FromStr for Email {
    type Err = EmailError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_owned())
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
```

**Notes.**
- `TryFrom<String>` keeps the caller's allocation; `FromStr` exists so `parse()`, clap and config loaders work. Both share one validation path.
- No public field, no `new_unchecked`, no `DerefMut`. Expose `as_str()`/`AsRef<str>`; never `Deref` to emulate inheritance (API-19).
- Normalize inside the constructor (trim, lowercase the domain) if equality should ignore those differences; then `Eq`/`Hash` are correct for free.
- With serde, validate on the way in: `#[serde(try_from = "String")]` on the newtype, or go through a DTO (pattern 19).

## 2. Id newtypes

**Use when.** Several entities have integer or UUID ids that could be swapped at a call site.

**Rules.** API-02, OWN-16, CRF-09

```rust
use std::fmt;

/// Identifies a customer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomerId(u64);

/// Identifies an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderId(u64);

impl CustomerId {
    /// Wraps a raw id from storage.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw value, for storage.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl OrderId {
    /// Wraps a raw id from storage.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw value, for storage.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for CustomerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cus_{}", self.0)
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ord_{}", self.0)
    }
}

/// A cancellation request. Swapping the ids is a type error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancel {
    /// Order to cancel.
    pub order: OrderId,
    /// Customer asking.
    pub by: CustomerId,
}
```

**Notes.**
- `Copy`, `Ord` and `Hash` are right for ids: they are small values used as map keys.
- No `From<u64>` and no arithmetic: conversions stay explicit (`new`, `get`) so a raw number never silently becomes an id.
- A prefixed `Display` (`cus_42`) makes logs unambiguous. For wire formats add `#[serde(transparent)]`.
- Five or more id types: a small `macro_rules!` that stamps them out is a legitimate macro (ARC-15), because functions cannot declare types.

## 3. Params struct with `Default` and struct update

**Use when.** A function or constructor has optional settings, every combination is valid, and callers usually change one or two.

**Rules.** API-10, API-06, CRF-09, ERR-09

```rust
use std::num::NonZeroU32;
use std::time::Duration;

/// How to spread retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Jitter {
    /// Exact exponential delays.
    None,
    /// Uniform in `0..=delay`.
    #[default]
    Full,
}

/// Retry settings. Plain data: every combination of fields is valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts, including the first.
    pub max_attempts: NonZeroU32,
    /// Delay before the first retry.
    pub initial_backoff: Duration,
    /// Upper bound on any delay.
    pub max_backoff: Duration,
    /// Randomization strategy.
    pub jitter: Jitter,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: NonZeroU32::MIN.saturating_add(2),
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            jitter: Jitter::Full,
        }
    }
}

impl RetryPolicy {
    /// Un-jittered delay before retry number `retry` (0-based), capped at `max_backoff`.
    #[must_use]
    pub fn delay(&self, retry: u32) -> Duration {
        self.initial_backoff
            .saturating_mul(2_u32.saturating_pow(retry))
            .min(self.max_backoff)
    }
}
```

**Notes.**
- Call site: `RetryPolicy { max_backoff: Duration::from_millis(250), ..RetryPolicy::default() }`.
- Public fields are fine only because there is no cross-field invariant (API-04). If one appears (`initial <= max`), switch to a builder (pattern 4).
- `#[non_exhaustive]` forbids struct-literal construction outside the crate, even with `..Default::default()`. If the struct must grow without a breaking change, use a builder instead.
- Enums instead of bools (`Jitter`), `NonZeroU32` instead of a runtime check, `Duration` instead of `u64` millis.

## 4. Non-consuming builder

**Use when.** Construction has required and optional parts, callers configure conditionally, or one configuration is reused for many values.

**Rules.** API-10, API-13, OWN-02

```rust
use std::time::Duration;

/// A request description. Plain data once built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Target URL.
    pub url: String,
    /// Headers in insertion order.
    pub headers: Vec<(String, String)>,
    /// Whole-request timeout.
    pub timeout: Duration,
}

impl Request {
    /// Starts a builder. `url` is the one required input, so it is taken here.
    pub fn builder(url: impl Into<String>) -> RequestBuilder {
        RequestBuilder {
            url: url.into(),
            headers: Vec::new(),
            timeout: RequestBuilder::DEFAULT_TIMEOUT,
        }
    }
}

/// Why a [`RequestBuilder`] could not build.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// A zero timeout would fail every request.
    #[error("request timeout must be greater than zero")]
    ZeroTimeout,
}

/// Builds [`Request`]s; created by [`Request::builder`]. Setters take
/// `&mut self`, so a builder can be configured conditionally and reused.
#[derive(Debug, Clone)]
#[must_use = "a builder does nothing until `build` is called"]
pub struct RequestBuilder {
    url: String,
    headers: Vec<(String, String)>,
    timeout: Duration,
}

impl RequestBuilder {
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

    /// Appends a header.
    pub fn header(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Overrides the default 30 s timeout.
    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = timeout;
        self
    }

    /// Builds a request; the builder stays usable for the next one.
    ///
    /// # Errors
    /// [`BuildError::ZeroTimeout`] when the timeout is zero.
    pub fn build(&self) -> Result<Request, BuildError> {
        if self.timeout.is_zero() {
            return Err(BuildError::ZeroTimeout);
        }
        Ok(Request {
            url: self.url.clone(),
            headers: self.headers.clone(),
            timeout: self.timeout,
        })
    }
}
```

**Notes.**
- Required inputs go to `Request::builder(url)`, so forgetting one does not compile. There is no public `RequestBuilder::new` and no `Default` that would start without them (API-10).
- Setters are named after the field (`timeout`, never `set_timeout`) and cannot fail. Validation, including cross-field checks, happens once, in `build`.
- `&mut self -> &mut Self` setters let callers write `if traced { builder.header(..); }` without rebinding.
- `build(&self)` clones fields so the builder stays reusable. When fields are expensive or the builder is single-use, prefer a consuming builder (`fn header(self, ..) -> Self`, `build(self)`) and move instead.
- When the required steps have an order, or there are too many to pass at once, make them types instead (pattern 5).
- The entry point and setters take `impl Into<String>` because the builder stores the value (OWN-02).

## 5. Typestate builder

**Use when.** Forgetting a required step or calling steps out of order must be a compile error, not a runtime `Result`.

**Rules.** API-13, TST-08, OWN-04

```rust
use std::time::Duration;

/// Marker: no endpoint set yet.
#[derive(Debug)]
pub struct NoEndpoint;

/// Marker: endpoint set.
#[derive(Debug)]
pub struct Endpoint(String);

/// Builds a [`Client`]; created by [`Client::builder`]. `build` exists only
/// once an endpoint is set:
///
/// ```compile_fail
/// let client = my_crate::Client::builder().build();
/// ```
#[derive(Debug)]
#[must_use]
pub struct ClientBuilder<E> {
    endpoint: E,
    timeout: Duration,
}

/// A configured client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    endpoint: String,
    timeout: Duration,
}

impl Client {
    /// Starts a builder with a 10 s timeout and no endpoint.
    pub const fn builder() -> ClientBuilder<NoEndpoint> {
        ClientBuilder {
            endpoint: NoEndpoint,
            timeout: Duration::from_secs(10),
        }
    }

    /// Endpoint URL.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl ClientBuilder<NoEndpoint> {
    /// Sets the endpoint, unlocking `build`.
    pub fn endpoint(self, url: impl Into<String>) -> ClientBuilder<Endpoint> {
        ClientBuilder {
            endpoint: Endpoint(url.into()),
            timeout: self.timeout,
        }
    }
}

impl<E> ClientBuilder<E> {
    /// Sets the timeout; valid in every state.
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl ClientBuilder<Endpoint> {
    /// Builds the client. Infallible: the type proves the endpoint is set.
    #[must_use]
    pub fn build(self) -> Client {
        Client {
            endpoint: self.endpoint.0,
            timeout: self.timeout,
        }
    }
}
```

**Notes.**
- The `compile_fail` doctest is the proof (TST-08); `cargo test` fails if `build()` ever becomes callable without an endpoint.
- Marker types are `pub` so users can name `ClientBuilder<Endpoint>` in signatures.
- Each required field doubles the state space. With more than two unordered ones, take them in `Client::builder(..)` instead (pattern 4).
- State known only at runtime (a connection that can drop) is data, not a type: use an enum (pattern 9).

## 6. Sealed trait

**Use when.** A public trait is used in bounds or as `dyn`, but downstream crates must not implement it, so you can add methods later without a breaking change.

**Rules.** API-12, API-17

```rust
mod private {
    /// The seal: public inside a private module, so it can appear as a bound
    /// but downstream crates can never name it to implement it.
    pub trait Sealed {}
}

/// A text encoding for bytes. Downstream crates cannot implement it.
pub trait Encoding: private::Sealed {
    /// Short name, for logs.
    const NAME: &'static str;

    /// Appends the encoding of `bytes` to `out`.
    fn encode_into(bytes: &[u8], out: &mut String);
}

/// Lowercase hexadecimal.
#[derive(Debug, Clone, Copy)]
pub struct Hex;

impl private::Sealed for Hex {}

impl Encoding for Hex {
    const NAME: &'static str = "hex";

    fn encode_into(bytes: &[u8], out: &mut String) {
        let nibbles = bytes.iter().flat_map(|b| [b >> 4, b & 0x0f]);
        out.extend(nibbles.filter_map(|n| char::from_digit(u32::from(n), 16)));
    }
}

/// Encodes with any sealed [`Encoding`].
#[must_use]
pub fn encode<E: Encoding>(bytes: &[u8]) -> String {
    let mut out = String::new();
    E::encode_into(bytes, &mut out);
    out
}
```

**Notes.**
- Do not write `pub(crate) trait Sealed`. A crate-private supertrait on a public trait trips `private_bounds`.
- Associated consts and generic functions (`encode::<Hex>`) keep dispatch static.

## 7. Extension trait

**Use when.** You need methods on a foreign type (`str`, `Result`, a dependency's type) and a free function reads worse at call sites.

**Rules.** API-18, API-12, H-04

```rust
/// Extra methods on `str`.
pub trait StrExt {
    /// The longest prefix of at most `max` chars, never splitting a char.
    fn truncate_chars(&self, max: usize) -> &str;
}

impl StrExt for str {
    fn truncate_chars(&self, max: usize) -> &str {
        self.char_indices()
            .nth(max)
            .and_then(|(end, _)| self.split_at_checked(end))
            .map_or(self, |(head, _)| head)
    }
}
```

**Notes.**
- Name it `<Type>Ext`; callers must `use` it, which makes the extension visible in imports.
- Inherent methods win over trait methods silently. If std later adds a method with the same name and signature, callers switch to it without warning, so pick distinctive names.
- Seal it (pattern 6) unless downstream implementations are intended.
- `split_at_checked` and `char_indices` keep the slicing non-panicking (H-04); `&self[..end]` would trip `clippy::string_slice`.

## 8. RAII guard

**Use when.** Something must be undone on every exit path (counter, lock, temp resource, span), including `?` and early `return`.

**Rules.** API-11, H-07, OWN-12

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts requests in flight.
#[derive(Debug, Default)]
pub struct InFlight {
    count: AtomicUsize,
}

impl InFlight {
    /// Marks one request as started; it ends when the guard drops.
    pub fn enter(&self) -> InFlightGuard<'_> {
        // Relaxed: a statistic; no other memory is published through it.
        // Cannot wrap: every increment is owned by a live guard.
        self.count.fetch_add(1, Ordering::Relaxed);
        InFlightGuard { owner: self }
    }

    /// Requests currently in flight.
    #[must_use]
    pub fn current(&self) -> usize {
        // Relaxed: the value is only reported, never used to order other reads.
        self.count.load(Ordering::Relaxed)
    }
}

/// Decrements the counter on drop: early return, `?` and unwinding included.
#[derive(Debug)]
#[must_use = "dropping the guard immediately ends the request"]
pub struct InFlightGuard<'a> {
    owner: &'a InFlight,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        // Relaxed: pairs with nothing; see `enter`.
        self.owner.count.fetch_sub(1, Ordering::Relaxed);
    }
}
```

**Notes.**
- Bind it as `let _guard = stats.enter();`. `let _ = stats.enter();` drops it immediately; `clippy::let_underscore_must_use` (deny) and `let_underscore_drop` catch that.
- `Drop` cannot fail or `.await`. For fallible or async cleanup, add an explicit `fn finish(self) -> Result<..>` (or `async fn close`) and keep `Drop` as the best-effort fallback (pattern 23, ERR-13).
- Never `mem::forget` a guard (H-07). A guard that protects a lock must not live across `.await` (H-14).
- `Relaxed` is enough for a statistics counter, and each use says why in a comment. Anything that orders other memory names the operation it pairs with (ASY-13).

## 9. Enum state machine with `mem::take` / `mem::replace`

**Use when.** An object moves through states, each state owns different data, and transitions should move that data rather than clone it.

**Rules.** API-02, CRF-03, OWN-08, H-08

```rust
use std::mem;

/// An authenticated session token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session(String);

/// Connection lifecycle. Each state carries only the data valid in it.
#[derive(Debug, Default, PartialEq, Eq)]
pub enum Connection {
    /// Not connected.
    #[default]
    Idle,
    /// Dialing; `attempt` starts at 1.
    Connecting {
        /// Current attempt.
        attempt: u32,
    },
    /// Connected with a live session.
    Connected {
        /// The session.
        session: Session,
    },
}

/// Inputs to the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Start dialing.
    Dial,
    /// Dial failed; retry.
    Failed,
    /// Handshake succeeded.
    Established(Session),
}

/// An event that is invalid in the current state. The state is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("event {event:?} is invalid in state {state}")]
pub struct InvalidTransition {
    /// State name.
    pub state: &'static str,
    /// The rejected event.
    pub event: Event,
}

impl Connection {
    /// Applies `event`, moving owned data between states without cloning.
    ///
    /// # Errors
    /// [`InvalidTransition`] when `event` does not apply; `self` is left as it was.
    pub fn apply(&mut self, event: Event) -> Result<(), InvalidTransition> {
        *self = match (mem::take(self), event) {
            (Self::Idle, Event::Dial) => Self::Connecting { attempt: 1 },
            (Self::Connecting { attempt }, Event::Failed) => Self::Connecting {
                attempt: attempt.saturating_add(1),
            },
            (Self::Connecting { .. }, Event::Established(session)) => Self::Connected { session },
            (state, event) => {
                let name = state.name();
                *self = state;
                return Err(InvalidTransition { state: name, event });
            }
        };
        Ok(())
    }

    /// Disconnects, returning the session if there was one.
    pub fn hang_up(&mut self) -> Option<Session> {
        match mem::replace(self, Self::Idle) {
            Self::Connected { session } => Some(session),
            Self::Idle | Self::Connecting { .. } => None,
        }
    }

    const fn name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Connecting { .. } => "connecting",
            Self::Connected { .. } => "connected",
        }
    }
}
```

**Notes.**
- `mem::take(self)` needs a cheap `Default` variant (`Idle`); it moves the old state out so its fields (the `Session`) can be moved into the new state with no clone.
- On an invalid event the old state is put back before returning `Err`, so a rejected event never corrupts the machine.
- `mem::replace(self, Self::Idle)` fits when you also want to return data from the old state (`hang_up`).
- The final `(state, event)` arm is a catch-all for invalid pairs: new variants will not force a review there. Keep the valid transitions listed explicitly above it.
- Alternative shape: `fn apply(self, event) -> Result<Self, (Self, InvalidTransition)>` when callers own the machine by value.

## 10. Decider (event sourcing)

**Use when.** An aggregate's history is stored as events and business rules must be testable without storage.

**Rules.** ARC-13, ARC-04, TST-03, ERR-10

```rust
use std::num::NonZeroU64;

/// Requests to change an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Open the account.
    Open,
    /// Add funds.
    Deposit(NonZeroU64),
    /// Remove funds.
    Withdraw(NonZeroU64),
}

/// Facts that happened. Past tense, never rejected on replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The account was opened.
    Opened,
    /// Funds were added.
    Deposited(NonZeroU64),
    /// Funds were removed.
    Withdrawn(NonZeroU64),
}

/// Account state rebuilt from events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// No `Opened` event yet.
    #[default]
    Unopened,
    /// Open with a balance.
    Open {
        /// Current balance.
        balance: u64,
    },
}

/// Why a command was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// `Open` on an open account.
    #[error("account is already open")]
    AlreadyOpen,
    /// Money movement before `Open`.
    #[error("account is not open")]
    NotOpen,
    /// Withdrawal larger than the balance.
    #[error("balance {balance} is less than {requested}")]
    InsufficientFunds {
        /// Current balance.
        balance: u64,
        /// Requested amount.
        requested: u64,
    },
    /// Deposit would overflow the balance.
    #[error("deposit would overflow the balance")]
    Overflow,
}

/// Decides which events a command produces. Pure: no I/O, no clock.
///
/// # Errors
/// A [`Rejection`] when the command is invalid in `state`.
pub fn decide(command: &Command, state: &State) -> Result<Vec<Event>, Rejection> {
    match (command, *state) {
        (Command::Open, State::Unopened) => Ok(vec![Event::Opened]),
        (Command::Open, State::Open { .. }) => Err(Rejection::AlreadyOpen),
        (Command::Deposit(_) | Command::Withdraw(_), State::Unopened) => Err(Rejection::NotOpen),
        (Command::Deposit(amount), State::Open { balance }) => {
            balance
                .checked_add(amount.get())
                .ok_or(Rejection::Overflow)?;
            Ok(vec![Event::Deposited(*amount)])
        }
        (Command::Withdraw(amount), State::Open { balance }) => {
            if amount.get() > balance {
                return Err(Rejection::InsufficientFunds {
                    balance,
                    requested: amount.get(),
                });
            }
            Ok(vec![Event::Withdrawn(*amount)])
        }
    }
}

/// Applies one event. Total: events are facts, so nothing is rejected here.
#[must_use]
pub fn evolve(state: State, event: &Event) -> State {
    match (state, event) {
        (State::Unopened, Event::Opened) => State::Open { balance: 0 },
        (State::Open { balance }, Event::Deposited(amount)) => State::Open {
            balance: balance.saturating_add(amount.get()),
        },
        (State::Open { balance }, Event::Withdrawn(amount)) => State::Open {
            balance: balance.saturating_sub(amount.get()),
        },
        // `decide` never emits these; replaying them changes nothing.
        (state @ State::Open { .. }, Event::Opened)
        | (state @ State::Unopened, Event::Deposited(_) | Event::Withdrawn(_)) => state,
    }
}

/// Rebuilds state from history.
pub fn fold<'a>(events: impl IntoIterator<Item = &'a Event>) -> State {
    events.into_iter().fold(State::default(), evolve)
}
```

**Notes.**
- `decide` is where rules live and the only place a command is rejected. `evolve` is total: events are facts and replay never fails.
- Tests read as given (events), when (command), then (events or rejection), with no fakes at all.
- The imperative shell: load events, `fold`, `decide`, append the new events with an expected-version check (optimistic concurrency), then publish.
- Stored events are versioned and upcast on read (pattern 20). `decide` gets time and ids as data in the command, never from a clock (TST-10).

## 11. Port, adapter, fake, composition root

**Use when.** Application logic needs I/O (storage, network, clock) and must be tested without it.

**Rules.** ARC-02, ARC-03, ARC-09, TST-09, ERR-04, ERR-10

```rust
use std::path::PathBuf;
use std::{fs, io};

/// Identifies a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteId(pub u64);

// ---- application layer: owns the port ----

/// Storage failure, as the application sees it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// No note with this id.
    #[error("note {0:?} not found")]
    NotFound(NoteId),
    /// The backend failed.
    #[error("storage backend failed for note {id:?}")]
    Backend {
        /// Note being accessed.
        id: NoteId,
        /// Underlying cause.
        #[source]
        source: io::Error,
    },
}

/// Port: persists note bodies.
pub trait NoteStore {
    /// Loads a note body.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] or [`StoreError::Backend`].
    fn load(&self, id: NoteId) -> Result<String, StoreError>;

    /// Replaces a note body.
    ///
    /// # Errors
    /// [`StoreError::Backend`].
    fn save(&mut self, id: NoteId, body: &str) -> Result<(), StoreError>;
}

/// Why appending failed; says which step.
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    /// Reading the existing note failed.
    #[error("could not load note")]
    Load(#[source] StoreError),
    /// Writing the updated note failed.
    #[error("could not save note")]
    Save(#[source] StoreError),
}

/// Use case: append a line to a note, creating it if needed.
#[derive(Debug)]
pub struct Notes<S> {
    store: S,
}

impl<S: NoteStore> Notes<S> {
    /// Wraps a store.
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Appends `line` and returns the new body.
    ///
    /// # Errors
    /// [`AppendError`] naming the failed step.
    pub fn append(&mut self, id: NoteId, line: &str) -> Result<String, AppendError> {
        let mut body = match self.store.load(id) {
            Ok(body) => body,
            Err(StoreError::NotFound(_)) => String::new(),
            Err(other) => return Err(AppendError::Load(other)),
        };
        body.push_str(line);
        body.push('\n');
        self.store.save(id, &body).map_err(AppendError::Save)?;
        Ok(body)
    }
}

// ---- adapter layer: implements the port ----

/// Stores each note as `<dir>/<id>.txt`.
#[derive(Debug)]
pub struct FsNoteStore {
    dir: PathBuf,
}

impl FsNoteStore {
    /// Uses `dir`, which must exist.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, id: NoteId) -> PathBuf {
        self.dir.join(format!("{}.txt", id.0))
    }
}

impl NoteStore for FsNoteStore {
    fn load(&self, id: NoteId) -> Result<String, StoreError> {
        fs::read_to_string(self.path(id)).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                StoreError::NotFound(id)
            } else {
                StoreError::Backend { id, source }
            }
        })
    }

    fn save(&mut self, id: NoteId, body: &str) -> Result<(), StoreError> {
        fs::write(self.path(id), body).map_err(|source| StoreError::Backend { id, source })
    }
}

// ---- composition root (in the binary) ----

/// Wires the production graph.
pub fn compose(data_dir: impl Into<PathBuf>) -> Notes<FsNoteStore> {
    Notes::new(FsNoteStore::new(data_dir))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// Hand-written fake of the port (TST-09).
    #[derive(Default)]
    struct InMemoryNotes {
        notes: HashMap<NoteId, String>,
        fail_saves: bool,
    }

    impl NoteStore for InMemoryNotes {
        fn load(&self, id: NoteId) -> Result<String, StoreError> {
            self.notes.get(&id).cloned().ok_or(StoreError::NotFound(id))
        }

        fn save(&mut self, id: NoteId, body: &str) -> Result<(), StoreError> {
            if self.fail_saves {
                let source = io::Error::other("disk full");
                return Err(StoreError::Backend { id, source });
            }
            self.notes.insert(id, body.to_owned());
            Ok(())
        }
    }

    #[test]
    fn append_creates_a_missing_note() -> Result<(), AppendError> {
        let mut notes = Notes::new(InMemoryNotes::default());
        assert_eq!(notes.append(NoteId(1), "first")?, "first\n");
        Ok(())
    }

    #[test]
    fn a_failed_save_is_reported_as_the_save_step() {
        let store = InMemoryNotes { fail_saves: true, ..InMemoryNotes::default() };
        let result = Notes::new(store).append(NoteId(1), "line");
        assert!(matches!(result, Err(AppendError::Save(StoreError::Backend { .. }))));
    }
}
```

**Notes.**
- In a real workspace the three sections are three crates: the application crate owns `NoteStore` and its error, the adapter crate depends on it, the binary composes. Nothing points outward.
- Generic `Notes<S: NoteStore>` is static dispatch. Use `Box<dyn NoteStore>`, or a named wrapper (pattern 29), only when the adapter is chosen at runtime.
- `StoreError::NotFound` is a domain outcome the use case handles; `Backend` is infrastructure and propagates (ERR-10).
- `AppendError::Load` and `Save` wrap the same source type, so they use `map_err`, not `#[from]` (ERR-04): the caller learns which step failed.
- The fake is hand-written and can inject failures (`fail_saves`). No mocking crate, no test-only branches in production code.

## 12. Layered errors: `thiserror` library, `anyhow` binary

**Use when.** A library reports failures that a binary turns into a readable message and exit status.

**Rules.** ERR-01, ERR-03, ERR-04, ERR-05, ERR-06, H-09, ARC-14

`config.rs`:

```rust
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::Deserialize;

/// Validated server settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// TCP port.
    pub port: u16,
    /// Worker count.
    pub workers: NonZeroUsize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    port: u16,
    workers: usize,
}

/// Why the configuration could not be loaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The file could not be read.
    #[error("could not read {path}")]
    Read {
        /// File path.
        path: PathBuf,
        /// I/O cause.
        #[source]
        source: io::Error,
    },
    /// The file is not valid JSON for the schema.
    #[error("{path} is not a valid config")]
    Parse {
        /// File path.
        path: PathBuf,
        /// Parser cause.
        #[source]
        source: serde_json::Error,
    },
    /// `workers` was zero.
    #[error("`workers` must be at least 1")]
    ZeroWorkers,
}

/// Loads and validates the config file at `path`.
///
/// # Errors
/// [`ConfigError`] naming the failed step and the path.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_owned(),
        source,
    })?;
    let raw: RawConfig = serde_json::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })?;
    let workers = NonZeroUsize::new(raw.workers).ok_or(ConfigError::ZeroWorkers)?;
    Ok(Config {
        port: raw.port,
        workers,
    })
}
```

`main.rs`:

```rust
//! Service binary: load config, run workers.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Context as _;
use my_service::config::{self, Config}; // the library crate above (package `my-service`)
# mod my_service {
#     pub mod config {
#         use std::num::NonZeroUsize;
#         use std::{io, path::Path};
#         pub struct Config {
#             pub port: u16,
#             pub workers: NonZeroUsize,
#         }
#         pub fn load(_path: &Path) -> io::Result<Config> {
#             Err(io::ErrorKind::NotFound.into())
#         }
#     }
# }

fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("usage: serve <config.json>")?;
    let config = config::load(&path).context("loading configuration")?;
    announce(&mut io::stdout().lock(), &config).context("writing startup banner")?;
    Ok(())
}

fn announce(out: &mut impl Write, config: &Config) -> io::Result<()> {
    writeln!(
        out,
        "listening on :{} with {} workers",
        config.port, config.workers
    )
}
```

**Notes.**
- Output for a missing file: `Error: loading configuration`, then `Caused by: 0: could not read /nonexistent.json 1: No such file or directory (os error 2)`. Each layer says only its own step (ERR-05); the reporter prints the chain.
- The library error carries the path as a typed field (ERR-03) and the cause via `#[source]`; `anyhow` never appears in its signatures (H-09).
- Handle or propagate, never both: no logging before `?` (ERR-08).
- `main` returning `anyhow::Result<()>` exits with status 1 and prints the chain with `Debug`. For more exit codes return `ExitCode` (pattern 21). Backtraces print when `RUST_BACKTRACE=1`.

## 13. Iterator-returning accessor

**Use when.** A type owns a collection and callers need to read (a view of) it.

**Rules.** OWN-13, API-08, MOD-06

```rust
/// A team member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Display name.
    pub name: String,
    /// Still on the team.
    pub active: bool,
}

/// A team that owns its members.
#[derive(Debug, Default)]
pub struct Team {
    members: Vec<Member>,
}

impl Team {
    /// Adds a member.
    pub fn add(&mut self, member: Member) {
        self.members.push(member);
    }

    /// All members, as a slice: callers get `len`, indexing via `get`, and iteration.
    #[must_use]
    pub fn members(&self) -> &[Member] {
        &self.members
    }

    /// Active members, lazily; nothing is allocated.
    pub fn active(&self) -> impl Iterator<Item = &Member> + '_ {
        self.members.iter().filter(|m| m.active)
    }

    /// Names of active members, borrowed from `self`.
    pub fn active_names(&self) -> impl Iterator<Item = &str> {
        self.active().map(|m| m.name.as_str())
    }
}
```

**Notes.**
- Stored `Vec` and no filtering: return `&[T]`, which gives callers `len`, `get`, `iter`, `binary_search` and `chunks` for free.
- Filtering or mapping: return `impl Iterator<Item = &T>`. Edition 2024 captures `&self` automatically, so `+ '_` (on `active`) is optional; `active_names` shows the bare form.
- Never return `Vec<&T>` or a cloned `Vec<T>` that the caller only iterates (PERF-02).
- When the adapter chain preserves them, promise more: `impl DoubleEndedIterator<Item = &T> + ExactSizeIterator`.

## 14. `Cow` normalization

**Use when.** A function usually returns its input unchanged but sometimes has to build a new value.

**Rules.** OWN-10, PERF-02

```rust
use std::borrow::Cow;

/// Collapses runs of whitespace to one space and trims the ends.
/// Borrows when `input` is already normalized, which is the common case.
#[must_use]
pub fn normalize_whitespace(input: &str) -> Cow<'_, str> {
    if is_normalized(input) {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    for word in input.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    Cow::Owned(out)
}

fn is_normalized(s: &str) -> bool {
    s.is_empty()
        || s.split(' ')
            .all(|word| !word.is_empty() && !word.contains(char::is_whitespace))
}
```

**Notes.**
- A cheap scan decides; the common case costs zero allocations.
- Callers that need ownership call `.into_owned()`, which does not re-allocate an `Owned` value.
- Same shape for `Cow<'_, [T]>`, `Cow<'_, Path>` and serde fields with `#[serde(borrow)]` (OUT-09).
- If the function allocates on nearly every call, return `String` and skip the `Cow`.

## 15. Arena graph with index handles

**Use when.** Nodes reference each other (graphs, trees with parent links, ECS-style data), including cycles.

**Rules.** OWN-11, H-04, H-07

```rust
use std::collections::VecDeque;

/// Handle to a node. Only meaningful for the graph that issued it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(usize);

/// The id does not belong to this graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("unknown node {0:?}")]
pub struct UnknownNode(pub NodeId);

#[derive(Debug)]
struct Node<T> {
    value: T,
    edges: Vec<NodeId>,
}

/// A directed graph that owns all nodes in one `Vec`. Cycles need no
/// `Rc`/`RefCell`: edges are plain indices.
#[derive(Debug)]
pub struct Graph<T> {
    nodes: Vec<Node<T>>,
}

impl<T> Default for Graph<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}

impl<T> Graph<T> {
    /// Adds a node and returns its handle.
    pub fn add_node(&mut self, value: T) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            value,
            edges: Vec::new(),
        });
        id
    }

    /// Adds the edge `from -> to`.
    ///
    /// # Errors
    /// [`UnknownNode`] if either id is not from this graph.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) -> Result<(), UnknownNode> {
        if to.0 >= self.nodes.len() {
            return Err(UnknownNode(to));
        }
        let node = self.nodes.get_mut(from.0).ok_or(UnknownNode(from))?;
        node.edges.push(to);
        Ok(())
    }

    /// The value at `id`.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&T> {
        self.nodes.get(id.0).map(|n| &n.value)
    }

    /// Nodes reachable from `start` (including it), breadth-first.
    #[must_use]
    pub fn reachable(&self, start: NodeId) -> Vec<NodeId> {
        let mut seen = vec![false; self.nodes.len()];
        let mut order = Vec::new();
        let mut queue = VecDeque::from([start]);
        while let Some(id) = queue.pop_front() {
            let (Some(node), Some(flag)) = (self.nodes.get(id.0), seen.get_mut(id.0)) else {
                continue;
            };
            if *flag {
                continue;
            }
            *flag = true;
            order.push(id);
            queue.extend(node.edges.iter().copied());
        }
        order
    }
}
```

**Notes.**
- One owner (`Vec<Node<T>>`), `Copy` handles, no `Rc<RefCell<_>>`, no lifetimes in the node type.
- Every access goes through `get`/`get_mut` (H-04); an id from another graph yields `None` or `UnknownNode`, never a panic.
- Removal: keep tombstones, or use generational indices (`slotmap`, a new dependency, H-16) so stale ids are detected.
- If ids from different graphs must never mix, give each graph an id and store it in `NodeId`.

## 16. Actor with `mpsc` and `oneshot`

**Use when.** Mutable state is shared by many tasks and operations on it must not interleave.

**Rules.** ASY-09, ASY-07, ASY-04, H-13, OWN-11

```rust
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Why a request to the account actor failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AccountError {
    /// The actor has stopped.
    #[error("account actor is gone")]
    Closed,
    /// The deposit would overflow the balance.
    #[error("deposit would overflow the balance")]
    Overflow,
}

enum Request {
    Deposit {
        amount: u64,
        reply: oneshot::Sender<Result<u64, AccountError>>,
    },
    Balance {
        reply: oneshot::Sender<u64>,
    },
}

/// Cheap, cloneable handle to the account actor.
#[derive(Debug, Clone)]
pub struct Account {
    requests: mpsc::Sender<Request>,
}

impl Account {
    const QUEUE: usize = 64;

    /// Starts the actor. It stops when every handle is dropped; the caller
    /// owns the returned `JoinHandle` and awaits it on shutdown. Must run
    /// inside a Tokio runtime: `tokio::spawn` panics outside one.
    #[must_use]
    pub fn spawn() -> (Self, JoinHandle<()>) {
        let (requests, inbox) = mpsc::channel(Self::QUEUE);
        let task = tokio::spawn(run(inbox, 0));
        (Self { requests }, task)
    }

    /// Adds `amount` and returns the new balance.
    ///
    /// # Errors
    /// [`AccountError::Overflow`], or [`AccountError::Closed`] if the actor stopped.
    pub async fn deposit(&self, amount: u64) -> Result<u64, AccountError> {
        let (reply, answer) = oneshot::channel();
        self.send(Request::Deposit { amount, reply }).await?;
        // the actor dropped the reply sender; RecvError carries no cause (ERR-04)
        answer.await.map_err(|_closed| AccountError::Closed)?
    }

    /// Current balance.
    ///
    /// # Errors
    /// [`AccountError::Closed`] if the actor stopped.
    pub async fn balance(&self) -> Result<u64, AccountError> {
        let (reply, answer) = oneshot::channel();
        self.send(Request::Balance { reply }).await?;
        // the actor dropped the reply sender; RecvError carries no cause (ERR-04)
        answer.await.map_err(|_closed| AccountError::Closed)
    }

    async fn send(&self, request: Request) -> Result<(), AccountError> {
        self.requests
            .send(request)
            .await
            // the SendError only carries the unsent value (ERR-04)
            .map_err(|_closed| AccountError::Closed)
    }
}

async fn run(mut inbox: mpsc::Receiver<Request>, mut balance: u64) {
    while let Some(request) = inbox.recv().await {
        let delivered = match request {
            Request::Deposit { amount, reply } => {
                let result = balance.checked_add(amount).ok_or(AccountError::Overflow);
                if let Ok(new_balance) = result {
                    balance = new_balance;
                }
                reply.send(result).is_ok()
            }
            Request::Balance { reply } => reply.send(balance).is_ok(),
        };
        if !delivered {
            tracing::debug!("requester dropped before the reply");
        }
    }
}
```

**Notes.**
- The actor task owns `balance` exclusively: no `Arc<Mutex<_>>` and no lock across `.await`.
- The bounded queue (`QUEUE`) gives backpressure: senders wait when the actor falls behind (H-13).
- The actor stops when the last handle is dropped (`recv` returns `None`); the returned `JoinHandle` is the owner that awaits it (ASY-04). To stop while handles still exist, add a `CancellationToken` (pattern 17).
- A failed `reply.send` means the caller gave up; it is logged at `debug`, not discarded with `let _ =` (H-10).

## 17. Graceful shutdown with `CancellationToken` and `JoinSet`

**Use when.** A service runs several long-lived tasks that must stop cleanly on a signal, within a deadline.

**Rules.** ASY-04, ASY-05, ASY-08, H-13

```rust
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// What one worker did before it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerReport {
    /// Worker index.
    pub id: usize,
    /// Jobs completed.
    pub processed: u64,
}

/// Outcome of a shutdown.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Workers that stopped on their own.
    pub finished: Vec<WorkerReport>,
    /// Workers aborted after the grace period.
    pub aborted: usize,
    /// Workers that panicked.
    pub panicked: usize,
}

/// Processes jobs until cancelled or the queue closes.
pub async fn worker(
    id: usize,
    mut jobs: mpsc::Receiver<u64>,
    token: CancellationToken,
) -> WorkerReport {
    let mut processed = 0_u64;
    loop {
        tokio::select! {
            biased;
            () = token.cancelled() => break,
            // `recv` is cancel-safe: a message is never lost when the other branch wins.
            job = jobs.recv() => match job {
                Some(_job) => processed = processed.saturating_add(1),
                None => break,
            },
        }
    }
    WorkerReport { id, processed }
}

/// Owns every worker task; nothing is detached.
#[derive(Debug, Default)]
pub struct Supervisor {
    tasks: JoinSet<WorkerReport>,
    token: CancellationToken,
}

impl Supervisor {
    /// Spawns a worker and returns the sender for its bounded queue.
    pub fn spawn_worker(&mut self, id: usize, capacity: usize) -> mpsc::Sender<u64> {
        let (tx, rx) = mpsc::channel(capacity);
        self.tasks.spawn(worker(id, rx, self.token.child_token()));
        tx
    }

    /// Cancels all workers, waits up to `grace`, then aborts stragglers.
    pub async fn shutdown(mut self, grace: Duration) -> ShutdownReport {
        self.token.cancel();
        let mut report = ShutdownReport::default();
        let drained = tokio::time::timeout(grace, async {
            while let Some(joined) = self.tasks.join_next().await {
                record(&mut report, joined);
            }
        })
        .await;
        if drained.is_err() {
            self.tasks.abort_all();
            while let Some(joined) = self.tasks.join_next().await {
                record(&mut report, joined);
            }
        }
        report
    }
}

fn record(report: &mut ShutdownReport, joined: Result<WorkerReport, tokio::task::JoinError>) {
    match joined {
        Ok(done) => report.finished.push(done),
        Err(e) if e.is_cancelled() => report.aborted = report.aborted.saturating_add(1),
        Err(_panicked) => report.panicked = report.panicked.saturating_add(1),
    }
}
```

**Notes.**
- `biased;` checks cancellation first, so a busy queue cannot starve shutdown.
- Every branch in `select!` must be cancel-safe; `mpsc::Receiver::recv` is, and the comment says so (ASY-05).
- `child_token()` lets you stop one worker without stopping the others; cancelling the parent stops all.
- Two phases: cooperative cancel with a grace timeout, then `abort_all`. `JoinError::is_cancelled` separates aborted tasks from panics.
- In the binary, trigger `token.cancel()` from `tokio::signal::ctrl_c()`. Test timeouts with `#[tokio::test(start_paused = true)]` (tokio `test-util`), not sleeps (TST-10).

## 18. Bounded concurrency

**Use when.** Many independent I/O operations must run concurrently without overwhelming the process or the remote side.

**Rules.** PERF-10, H-13, ASY-01, ASY-04

On one task, with the `futures` crate (a dependency, H-16):

```rust,fragment
use std::num::NonZeroUsize;

use futures::stream::{self, StreamExt};

/// Runs `fetch` for every id with at most `limit` in flight on this task.
/// Results arrive in completion order.
pub async fn fetch_all<T, E>(
    ids: &[u64],
    limit: NonZeroUsize,
    fetch: impl AsyncFn(u64) -> Result<T, E>,
) -> Vec<Result<T, E>> {
    let fetch = &fetch;
    stream::iter(ids.iter().copied())
        .map(|id| fetch(id))
        .buffer_unordered(limit.get())
        .collect()
        .await
}
```

Across the runtime, with tokio alone:

```rust
use std::io;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{AcquireError, Semaphore};
use tokio::task::{JoinError, JoinSet};

/// Why a spawned batch failed.
#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    /// The limiter was closed.
    #[error("concurrency limiter closed")]
    Limiter(#[from] AcquireError),
    /// A job panicked or was aborted.
    #[error("job task failed")]
    Join(#[from] JoinError),
}

/// Stats every path in parallel across the runtime, at most `limit` tasks alive at once.
///
/// # Errors
/// [`BatchError`] if the limiter closed or a task panicked. Per-file I/O
/// errors are returned alongside each path.
pub async fn file_sizes(
    paths: Vec<PathBuf>,
    limit: NonZeroUsize,
) -> Result<Vec<(PathBuf, io::Result<u64>)>, BatchError> {
    let permits = Arc::new(Semaphore::new(limit.get()));
    let mut tasks = JoinSet::new();
    for path in paths {
        // Acquire before spawning, so at most `limit` tasks exist.
        let permit = Arc::clone(&permits).acquire_owned().await?;
        tasks.spawn(async move {
            let size = tokio::fs::metadata(&path).await.map(|m| m.len());
            drop(permit);
            (path, size)
        });
    }
    let mut sizes = Vec::with_capacity(tasks.len());
    while let Some(joined) = tasks.join_next().await {
        sizes.push(joined?);
    }
    Ok(sizes)
}
```

**Notes.**
- `buffer_unordered(n)` polls up to `n` futures concurrently on the current task: no `Send` or `'static` bounds and no parallelism. Use `buffered(n)` to keep input order.
- `Semaphore` plus `JoinSet` spreads work across runtime threads. Acquire the permit before `spawn`, so at most `limit` tasks exist instead of one task per input waiting on a permit.
- Per-item results are returned; one failure does not abort the batch. Wrap each remote call in `tokio::time::timeout` (ASY-08).
- CPU-bound work belongs in `rayon` or `spawn_blocking`, not in these futures (ASY-01).

## 19. DTO to domain with serde and `TryFrom`

**Use when.** Data arrives from outside (HTTP, files, queues) and must become a validated domain value.

**Rules.** OUT-01, OUT-02, OUT-05, API-03, ERR-04

```rust
use serde::Deserialize;

# mod email {
#     #[derive(Debug, Clone, PartialEq, Eq)]
#     pub struct Email(String);
#     #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#     #[error("invalid email")]
#     pub struct EmailError;
#     impl TryFrom<String> for Email {
#         type Error = EmailError;
#         fn try_from(raw: String) -> Result<Self, EmailError> {
#             if raw.contains('@') { Ok(Self(raw)) } else { Err(EmailError) }
#         }
#     }
# }
# use email::{Email, EmailError};
// `Email` and `EmailError` are pattern 1's (`use crate::email::{Email, EmailError};`).

/// Wire shape of a signup request. Unvalidated.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignupDto {
    /// Raw email.
    pub email: String,
    /// Raw display name.
    pub display_name: String,
    /// Claimed age in years.
    pub age: u8,
}

/// A validated signup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signup {
    email: Email,
    display_name: String,
    age: u8,
}

/// Why a [`SignupDto`] is not a valid [`Signup`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SignupError {
    /// Bad email.
    #[error("invalid email")]
    Email(#[from] EmailError),
    /// Display name empty or too long.
    #[error("display name must be 1..={max} chars, got {len}")]
    DisplayName {
        /// Actual length in chars.
        len: usize,
        /// Limit.
        max: usize,
    },
    /// Below the minimum age.
    #[error("age {age} is below {min}")]
    Underage {
        /// Claimed age.
        age: u8,
        /// Minimum.
        min: u8,
    },
}

impl Signup {
    const MAX_NAME: usize = 64;
    const MIN_AGE: u8 = 16;

    /// The validated email.
    #[must_use]
    pub const fn email(&self) -> &Email {
        &self.email
    }
}

impl TryFrom<SignupDto> for Signup {
    type Error = SignupError;

    fn try_from(dto: SignupDto) -> Result<Self, Self::Error> {
        let email = Email::try_from(dto.email)?;
        let display_name = dto.display_name.trim().to_owned();
        let len = display_name.chars().count();
        if !(1..=Self::MAX_NAME).contains(&len) {
            return Err(SignupError::DisplayName {
                len,
                max: Self::MAX_NAME,
            });
        }
        if dto.age < Self::MIN_AGE {
            return Err(SignupError::Underage {
                age: dto.age,
                min: Self::MIN_AGE,
            });
        }
        Ok(Self {
            email,
            display_name,
            age: dto.age,
        })
    }
}

/// Why a signup body was refused.
#[derive(Debug, thiserror::Error)]
pub enum ParseSignupError {
    /// Not valid JSON for the wire shape.
    #[error("malformed signup body")]
    Syntax(#[source] serde_json::Error),
    /// Well-formed but invalid.
    #[error("invalid signup")]
    Invalid(#[source] SignupError),
}

/// Parses and validates a JSON signup body.
///
/// # Errors
/// [`ParseSignupError::Syntax`] or [`ParseSignupError::Invalid`].
pub fn parse_signup(body: &str) -> Result<Signup, ParseSignupError> {
    let dto: SignupDto = serde_json::from_str(body).map_err(ParseSignupError::Syntax)?;
    Signup::try_from(dto).map_err(ParseSignupError::Invalid)
}
```

**Notes.**
- The DTO mirrors the wire (camelCase, all fields raw); the domain type has private, validated fields. Deserialized data is never trusted as valid (OUT-05).
- `deny_unknown_fields` turns a typo (`"admn": true`) into an error instead of a silently ignored field.
- `Syntax` and `Invalid` are separate variants so an HTTP layer can map them to different responses.
- `#[from] EmailError` is fine: only one step produces it (ERR-04).
- Shortcut: `#[serde(try_from = "SignupDto")]` on `Signup` makes serde call the `TryFrom`, at the cost of a serde dependency in the domain type.

## 20. Versioned event enum with upcast

**Use when.** Persisted or published records outlive the code that wrote them.

**Rules.** OUT-03, OUT-02, ARC-13

```rust
use serde::{Deserialize, Serialize};

/// Current in-memory event. Code only ever handles this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRegistered {
    /// Display name.
    pub display_name: String,
    /// Marketing consent; absent before v2, meaning "no".
    pub marketing_opt_in: bool,
}

/// Every schema version ever written. Never edit or remove a variant.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "schema")]
enum Stored {
    /// v1: the field was called `name`.
    #[serde(rename = "user_registered/1")]
    V1 { name: String },
    /// v2: renamed field, added consent.
    #[serde(rename = "user_registered/2")]
    V2 {
        display_name: String,
        marketing_opt_in: bool,
    },
}

impl From<Stored> for UserRegistered {
    fn from(stored: Stored) -> Self {
        match stored {
            Stored::V1 { name } => Self {
                display_name: name,
                marketing_opt_in: false,
            },
            Stored::V2 {
                display_name,
                marketing_opt_in,
            } => Self {
                display_name,
                marketing_opt_in,
            },
        }
    }
}

/// Reads any stored version, upcast to the current shape.
///
/// # Errors
/// The deserialization error for unknown or malformed records.
pub fn read(json: &str) -> Result<UserRegistered, serde_json::Error> {
    serde_json::from_str::<Stored>(json).map(UserRegistered::from)
}

/// Writes the latest version only.
///
/// # Errors
/// The serialization error (not expected for this shape).
pub fn write(event: &UserRegistered) -> Result<String, serde_json::Error> {
    serde_json::to_string(&Stored::V2 {
        display_name: event.display_name.clone(),
        marketing_opt_in: event.marketing_opt_in,
    })
}
```

**Notes.**
- Every version ever written stays as a variant; history is never edited. A change adds `V3` and extends the `From` impl.
- Only the latest version is written. Readers upcast old versions, so the rest of the code sees one shape.
- The tag names both the event and the version (`user_registered/2`), so one stream can hold many event types.
- Keep a fixture per stored version and snapshot the serialized latest version (TST-07); a failing fixture is a migration decision (H-16).

## 21. CLI JSON output to locked stdout with `ExitCode`

**Use when.** A command-line tool is used by scripts or agents as well as by people.

**Rules.** OUT-06, PERF-07, ARC-14, ERR-06, H-06

Argument parsing uses `clap` with its `derive` feature, so this block is not compiled by the doc-example harness.

```rust,fragment
//! CLI binary: JSON Lines output to a locked stdout, exit codes from main.

use std::fmt;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context as _;
use clap::{Parser, ValueEnum};
use serde::Serialize;

/// Some paths could not be read (0 = all ok, 1 = error, 2 = usage, from clap).
const EXIT_PARTIAL: u8 = 3;
/// Failed, and stderr was unavailable to say why.
const EXIT_UNREPORTED: u8 = 4;

/// Report file sizes.
#[derive(Debug, Parser)]
struct Args {
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// Files to inspect.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

/// One output record; the wire shape is part of the CLI contract.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Entry<'a> {
    Ok { path: &'a Path, bytes: u64 },
    Error { path: &'a Path, error: String },
}

impl fmt::Display for Entry<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok { path, bytes } => write!(f, "{bytes}\t{}", path.display()),
            Self::Error { path, error } => write!(f, "-\t{}\t({error})", path.display()),
        }
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(EXIT_PARTIAL),
        Err(err) => report(&err),
    }
}

fn run(args: &Args) -> anyhow::Result<bool> {
    let mut out = BufWriter::new(io::stdout().lock());
    let all_ok = write_entries(&mut out, args.format, &args.paths).context("writing output")?;
    out.flush().context("flushing output")?;
    Ok(all_ok)
}

fn write_entries(out: &mut impl Write, format: Format, paths: &[PathBuf]) -> io::Result<bool> {
    let mut all_ok = true;
    for path in paths {
        let entry = match std::fs::metadata(path) {
            Ok(meta) => Entry::Ok {
                path,
                bytes: meta.len(),
            },
            Err(e) => {
                all_ok = false;
                Entry::Error {
                    path,
                    error: e.to_string(),
                }
            }
        };
        match format {
            Format::Json => {
                serde_json::to_writer(&mut *out, &entry)?;
                writeln!(out)?;
            }
            Format::Text => writeln!(out, "{entry}")?,
        }
    }
    Ok(all_ok)
}

fn report(err: &anyhow::Error) -> ExitCode {
    match writeln!(io::stderr().lock(), "error: {err:#}") {
        Ok(()) => ExitCode::FAILURE,
        Err(_) => ExitCode::from(EXIT_UNREPORTED),
    }
}
```

**Notes.**
- Data goes to stdout, diagnostics to stderr. JSON Lines (`--format json`): one object per line, tagged with `status`, so consumers stream it.
- Lock stdout once and wrap it in `BufWriter` (PERF-07). Call `flush()` explicitly: `BufWriter`'s `Drop` swallows write errors.
- `print!`/`println!` are denied by the template (`clippy::print_stdout`); `writeln!` on the lock returns an `io::Result` that `?` propagates.
- Exit codes are part of the contract: 0 all ok, 1 error, 2 usage (clap), 3 partial failure. Document them in `--help`.
- Piping into `head` closes stdout early; if that should count as success, check `io::ErrorKind::BrokenPipe` where the write fails and stop quietly.

## 22. Redacted secret newtype

**Use when.** A value (password, token, API key) must never reach logs, `Debug` output or serialized responses.

**Rules.** OUT-08, OUT-10, API-06

```rust
use std::fmt;

use serde::Deserialize;

/// A credential that never appears in `Debug` output or logs.
///
/// No `Display`, no `Serialize`, no `Clone`: every read goes through
/// [`Secret::expose`], which is easy to grep for.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// Wraps a credential.
    #[must_use]
    pub const fn new(value: String) -> Self {
        Self(value)
    }

    /// The raw value. Call only where the secret is sent to its consumer.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

/// Database settings; safe to log with `{:?}`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DbConfig {
    /// Host name.
    pub host: String,
    /// Password.
    pub password: Secret,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_secret() {
        let raw = "hunter2-7f3a";
        let config = DbConfig { host: "db".to_owned(), password: Secret::new(raw.to_owned()) };
        let rendered = format!("{config:?}");
        assert!(!rendered.contains(raw), "Debug leaked the secret: {rendered}");
        assert!(rendered.contains("Secret([REDACTED])"), "unexpected Debug: {rendered}");
    }
}
```

**Notes.**
- `Debug` redacts, so `#[derive(Debug)]` on containing configs and `tracing` fields like `?config` stay safe. The test proves it through the container, which is where a leak would come from (OUT-08).
- No `Display`, `Serialize`, `PartialEq` or `Clone`: leaking needs a visible `expose()` call, which reviewers grep for, and there is exactly one copy of the secret to zeroize or drop.
- Compare secrets with a constant-time function (`subtle`), never `==`. Zeroing memory on drop needs `zeroize` or `secrecy`: a new dependency (H-16).

## 23. Explicit `finish` with a best-effort `Drop`

**Use when.** Teardown can fail (flushing a buffer, syncing a file, committing a transaction, sending a final frame) and the caller must learn about it.

**Rules.** ERR-13, H-10, H-14

```rust
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// An append-only record file. Call [`Journal::finish`] on the success path:
/// a dropped journal can lose its buffered tail.
#[derive(Debug)]
pub struct Journal {
    // `Option` because `finish(self)` cannot move a field out of a `Drop` type (E0509).
    out: Option<BufWriter<File>>,
}

impl Journal {
    /// Creates or truncates the journal at `path`.
    ///
    /// # Errors
    /// The I/O error from creating the file.
    pub fn create(path: &Path) -> io::Result<Self> {
        Ok(Self { out: Some(BufWriter::new(File::create(path)?)) })
    }

    /// Appends one record as a line.
    ///
    /// # Errors
    /// The I/O error from writing.
    pub fn append(&mut self, record: &str) -> io::Result<()> {
        // `None` only inside `finish` and `drop`, which both end the journal.
        let out = self.out.as_mut().ok_or(io::ErrorKind::NotConnected)?;
        writeln!(out, "{record}")
    }

    /// Flushes and syncs. The only place a failed write-back is visible.
    ///
    /// # Errors
    /// The I/O error from flushing or syncing.
    pub fn finish(mut self) -> io::Result<()> {
        let Some(out) = self.out.take() else {
            return Ok(());
        };
        out.into_inner().map_err(io::IntoInnerError::into_error)?.sync_all()
    }
}

impl Drop for Journal {
    // Reached with `Some` only when `finish` was skipped: an early return, `?` or unwinding.
    fn drop(&mut self) {
        if let Some(mut out) = self.out.take()
            && let Err(error) = out.flush()
        {
            tracing::warn!(%error, "journal dropped without finish; its tail may be lost");
        }
    }
}
```

**Notes.**
- `BufWriter` and `File` discard errors when dropped, so without `finish` a full disk is silent (H-10). Callers end with `journal.finish()?`.
- `Drop` never panics, blocks or awaits (H-14). It logs, because there is no caller left to return to. Async resources get `async fn close(self)`.
- `process::exit`, `panic = "abort"` and a panic during unwinding skip `Drop` entirely: never rely on it for correctness.

## 24. Hand back the consumed argument

**Use when.** A fallible function takes a non-`Copy` value by value, and a caller may want it back to retry, reroute or report.

**Rules.** ERR-14, H-08, ERR-01, OUT-08

```rust
use std::collections::VecDeque;
use std::fmt;

/// The job was not queued. [`Full::into_inner`] returns it.
#[derive(thiserror::Error)]
#[error("queue is full ({capacity} jobs)")]
pub struct Full<T> {
    job: T,
    capacity: usize,
}

impl<T> Full<T> {
    /// The rejected job, without a clone.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.job
    }
}

// Written by hand: no `T: Debug` bound, and the payload (perhaps a secret) stays out of logs.
impl<T> fmt::Debug for Full<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Full").field("capacity", &self.capacity).finish_non_exhaustive()
    }
}

/// A bounded FIFO of jobs.
#[derive(Debug)]
pub struct JobQueue<T> {
    jobs: VecDeque<T>,
    capacity: usize,
}

impl<T> JobQueue<T> {
    /// An empty queue that holds at most `capacity` jobs.
    #[must_use]
    pub const fn new(capacity: usize) -> Self {
        Self { jobs: VecDeque::new(), capacity }
    }

    /// Queues `job`.
    ///
    /// # Errors
    /// [`Full`], carrying `job` back, when the queue is at capacity.
    pub fn push(&mut self, job: T) -> Result<(), Full<T>> {
        if self.jobs.len() >= self.capacity {
            return Err(Full { job, capacity: self.capacity });
        }
        self.jobs.push_back(job);
        Ok(())
    }
}
```

**Notes.**
- Call site: `if let Err(full) = queue.push(job) { overflow.push(full.into_inner()); }`. No `job.clone()` before the call "in case it fails" (H-08).
- std does the same: `FromUtf8Error::into_bytes`, `SendError<T>`, `Arc::try_unwrap` returning `Err(Arc<T>)`, `<[T; N]>::try_from(Vec<T>)` returning `Err(Vec<T>)`, `IntoInnerError::into_inner`.
- `Full<T>` is `Error + Send + Sync + 'static` whenever `T` is (ERR-01).

## 25. On-stack `dyn` for a local runtime choice

**Use when.** A function picks one of a few concrete types at runtime and the value never leaves the function.

**Rules.** API-09, PERF-08

```rust
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Counts the bytes in `path`, or in stdin when `path` is `-`.
///
/// # Errors
/// The I/O error from opening or reading the input.
pub fn byte_count(path: &Path) -> io::Result<u64> {
    // Both temporaries live to the end of the enclosing block (1.79+): no `Box`.
    let input: &mut dyn Read = if path == Path::new("-") {
        &mut io::stdin().lock()
    } else {
        &mut File::open(path)?
    };
    io::copy(input, &mut io::sink())
}
```

**Notes.**
- `Box<dyn Read>` would allocate; calling a generic helper from both arms would compile the rest of the function twice.
- If the value is returned or stored, this shape no longer fits: use an enum over the known types, or `Box<dyn Trait>` for an open set (API-09).

## 26. Closure-friendly role trait

**Use when.** A port or strategy has one method, and tests or callers would like to pass a closure, but several items need to name the role.

**Rules.** API-18, ARC-03, TST-09

```rust
use std::time::SystemTime;

/// Port: the current time (ARC-03).
pub trait Clock {
    /// The current time.
    fn now(&self) -> SystemTime;
}

/// Any `Fn() -> SystemTime` is a clock, so a test passes `move || fixed`.
impl<F: Fn() -> SystemTime> Clock for F {
    fn now(&self) -> SystemTime {
        self()
    }
}

/// The production clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Stamps records with the time they were made.
#[derive(Debug)]
pub struct Stamper<C> {
    clock: C,
}

impl<C: Clock> Stamper<C> {
    /// Uses `clock` for every stamp.
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }

    /// Pairs `record` with the current time.
    pub fn stamp<'r>(&self, record: &'r str) -> (SystemTime, &'r str) {
        (self.clock.now(), record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closure_is_a_clock() {
        let fixed = SystemTime::UNIX_EPOCH;
        let stamper = Stamper::new(move || fixed);
        assert_eq!(stamper.stamp("paid"), (fixed, "paid"));
    }
}
```

**Notes.**
- The named trait replaces `F: Fn() -> SystemTime` repeated on every item, and `Stamper<C>` reads as a role, not a closure type.
- The blanket impl coexists with `impl Clock for SystemClock` and with a named wrapper such as `DynClock` (pattern 29): a local type that is not a closure never implements `Fn`.
- When the role produces a varying type, give the trait `type Output` and write `impl<F: FnMut() -> T, T> Source for F { type Output = T; }`. Users then write `Poller<S>`, not `Poller<F, T>`.

## 27. Kind-parameterized API

**Use when.** A value's kind (protocol, backend, unit system) is fixed where it is created, and some operations exist for only one kind.

**Rules.** API-13, API-12, API-21

```rust
use std::path::{Path, PathBuf};

mod sealed {
    pub trait Sealed {}
}

/// A transfer protocol. Sealed: the set of kinds is closed (API-12).
pub trait Protocol: sealed::Sealed {}

/// NFS transfers, which need a mount point.
#[derive(Debug)]
pub struct Nfs {
    mount_point: PathBuf,
}

/// HTTP transfers.
#[derive(Debug)]
pub struct Http;

impl sealed::Sealed for Nfs {}
impl sealed::Sealed for Http {}
impl Protocol for Nfs {}
impl Protocol for Http {}

/// A download whose protocol is fixed at creation.
#[derive(Debug)]
pub struct Download<P> {
    file: PathBuf,
    protocol: P,
}

impl<P: Protocol> Download<P> {
    /// The file being fetched, for every protocol.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }
}

impl Download<Nfs> {
    /// Starts an NFS download.
    pub fn nfs(file: impl Into<PathBuf>, mount_point: impl Into<PathBuf>) -> Self {
        let protocol = Nfs { mount_point: mount_point.into() };
        Self { file: file.into(), protocol }
    }

    /// Exists only for NFS: no `Option`, no runtime check.
    #[must_use]
    pub fn mount_point(&self) -> &Path {
        &self.protocol.mount_point
    }
}

impl Download<Http> {
    /// Starts an HTTP download.
    pub fn http(file: impl Into<PathBuf>) -> Self {
        Self { file: file.into(), protocol: Http }
    }
}
```

**Notes.**
- This replaces an `Option<PathBuf>` that is `Some` only for NFS and a method documented "returns `None` unless NFS". Typestate (pattern 5) does the same for order; this does it for kind.
- The bound sits on the `impl`, not the struct (API-21).
- When the kind arrives at runtime, or downloads of different kinds share one collection, use an enum and convert at that boundary.

## 28. Tree walk: `Visitor` with `ControlFlow`, moving `Fold`

**Use when.** Several passes (checks, queries, rewrites) run over one recursive data type, such as an AST or a document tree.

**Rules.** CRF-03, H-03, H-08, PERF-10

```rust
use std::ops::ControlFlow;

/// An integer expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A literal.
    Num(i64),
    /// A named variable.
    Var(String),
    /// A sum.
    Add(Box<Expr>, Box<Expr>),
    /// A negation.
    Neg(Box<Expr>),
}

/// A read-only pass. Override only the hooks you need.
pub trait Visitor {
    /// Why the pass stopped early.
    type Break;

    /// Visits `expr`; the default walks its children.
    fn visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        walk_expr(self, expr)
    }
}

/// Visits the children of `expr`: the one exhaustive match on the tree's shape.
pub fn walk_expr<V: Visitor + ?Sized>(visitor: &mut V, expr: &Expr) -> ControlFlow<V::Break> {
    match expr {
        Expr::Num(_) | Expr::Var(_) => ControlFlow::Continue(()),
        Expr::Add(lhs, rhs) => {
            visitor.visit_expr(lhs)?;
            visitor.visit_expr(rhs)
        }
        Expr::Neg(inner) => visitor.visit_expr(inner),
    }
}

struct FindVar;

impl Visitor for FindVar {
    type Break = ();

    fn visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
        if let Expr::Var(_) = expr {
            return ControlFlow::Break(());
        }
        walk_expr(self, expr)
    }
}

/// True when `expr` has no variables; stops at the first one.
#[must_use]
pub fn is_constant(expr: &Expr) -> bool {
    FindVar.visit_expr(expr).is_continue()
}

/// A rewriting pass. Nodes are taken by value, so unchanged subtrees move
/// through with no clone.
pub trait Folder {
    /// Rewrites `expr`; the default rewrites its children.
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        fold_children(self, expr)
    }
}

/// Rewrites the children of `expr`, reusing each `Box`.
pub fn fold_children<F: Folder + ?Sized>(folder: &mut F, expr: Expr) -> Expr {
    match expr {
        leaf @ (Expr::Num(_) | Expr::Var(_)) => leaf,
        Expr::Add(lhs, rhs) => Expr::Add(fold_box(folder, lhs), fold_box(folder, rhs)),
        Expr::Neg(inner) => Expr::Neg(fold_box(folder, inner)),
    }
}

fn fold_box<F: Folder + ?Sized>(folder: &mut F, mut node: Box<Expr>) -> Box<Expr> {
    *node = folder.fold_expr(*node);
    node
}

/// Replaces `Num + Num` with its sum; a sum that would overflow stays as it is.
#[derive(Debug)]
pub struct FoldConstants;

impl Folder for FoldConstants {
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        match fold_children(self, expr) {
            Expr::Add(lhs, rhs) => match (&*lhs, &*rhs) {
                (Expr::Num(a), Expr::Num(b)) if let Some(sum) = a.checked_add(*b) => Expr::Num(sum),
                _ => Expr::Add(lhs, rhs),
            },
            other => other,
        }
    }
}
```

**Notes.**
- A new variant breaks exactly `walk_expr` and `fold_children` (CRF-03); passes that override one hook are untouched.
- `ControlFlow` plus `?` stops a search early without a flag. Unsupported nodes are an error variant or a `Break`, never `panic!` or `unimplemented!` (H-03).
- Recursion depth equals tree depth, and a stack overflow aborts the process: cap nesting where the tree is parsed (PERF-10).

## 29. Service handle and named `dyn` wrapper

**Use when.** A service (client, repository, registry) is shared across tasks, or an implementation is chosen at runtime, and the public API must not expose `Arc`.

**Rules.** API-23, H-08, API-09, ARC-03

```rust
use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

/// Port: the current time.
pub trait Clock: Send + Sync {
    /// The current time.
    fn now(&self) -> SystemTime;
}

#[derive(Debug)]
struct Inner {
    endpoint: String,
    max_retries: u32,
}

/// Handle to the directory service. Cheap to clone: a clone copies one
/// pointer, never the service.
#[derive(Debug, Clone)]
pub struct Directory {
    inner: Arc<Inner>,
}

impl Directory {
    /// Connects to `endpoint`.
    pub fn new(endpoint: impl Into<String>, max_retries: u32) -> Self {
        let inner = Inner { endpoint: endpoint.into(), max_retries };
        Self { inner: Arc::new(inner) }
    }

    /// The service address.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.inner.endpoint
    }
}

/// A clock chosen at runtime. Callers name this type, not `Arc<dyn Clock>`.
#[derive(Clone)]
pub struct DynClock(Arc<dyn Clock>);

impl DynClock {
    /// Wraps any clock.
    pub fn new(clock: impl Clock + 'static) -> Self {
        Self(Arc::new(clock))
    }
}

impl Clock for DynClock {
    fn now(&self) -> SystemTime {
        self.0.now()
    }
}

// Written by hand: `dyn Clock` is not `Debug`.
impl fmt::Debug for DynClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DynClock")
    }
}
```

**Notes.**
- `Directory::clone` is the cheap-handle clone H-08 allows; say "cheap to clone" in the type's docs. Mutable state goes inside `Inner`, behind its own lock (OWN-12), never `Arc<Mutex<Directory>>` at call sites.
- Business code stays generic (`Stamper<C>` with `impl<C: Clock>`, pattern 26). The composition root builds `Stamper<DynClock>` when configuration picks the clock.
- `Arc` in a signature fixes the sharing strategy in the API and makes every caller write `Arc::new`; changing it later is a breaking change.

## 30. Snapshot swap (read-copy-update)

**Use when.** Many readers need a consistent view of read-mostly data (routing table, configuration, projection) for a whole request, and updates are rare.

**Rules.** OWN-11, PERF-12, ASY-14, H-02

```rust
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Where each path prefix is routed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Routes {
    by_prefix: BTreeMap<String, String>,
}

impl Routes {
    /// The target for `prefix`.
    #[must_use]
    pub fn target(&self, prefix: &str) -> Option<&str> {
        self.by_prefix.get(prefix).map(String::as_str)
    }
}

/// Shares the current [`Routes`]. Readers work on a snapshot, never under the lock.
#[derive(Debug, Default)]
pub struct RouteTable {
    current: Mutex<Arc<Routes>>,
}

impl RouteTable {
    /// The current routes. The lock is held only to copy one pointer.
    #[must_use]
    pub fn snapshot(&self) -> Arc<Routes> {
        Arc::clone(&self.lock())
    }

    /// Routes `prefix` to `target`, returning the previous target. Readers
    /// holding a snapshot keep the old version.
    pub fn set(&self, prefix: String, target: String) -> Option<String> {
        // Mutates in place when no snapshot is alive; otherwise copies once.
        Arc::make_mut(&mut self.lock()).by_prefix.insert(prefix, target)
    }

    fn lock(&self) -> MutexGuard<'_, Arc<Routes>> {
        // Each critical section is one pointer copy or one `insert`, so a panic
        // elsewhere cannot leave the routes half-updated: recover from poison.
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
```

**Notes.**
- A reader writes `let routes = table.snapshot();` and then works for as long as it likes; an update never waits for it, and it never sees a half-applied change.
- `Arc::make_mut` is clone-on-write: no copy when the table is unshared. Never write `Arc::new((*old).clone())`, which always copies (OWN-11).
- Updates are methods, not callbacks, so no caller code runs under the lock (ASY-14). While a snapshot is alive, `set` copies the table under the lock: the price of a rare update.
- `Mutex`, not `RwLock`: readers hold it only to copy a pointer (PERF-12). `arc-swap` removes even that lock, as a new dependency (H-16).

## 31. Checked value-object arithmetic

**Use when.** A numeric domain value (money, quantity, capacity) must not overflow silently, and callers need to decide what overflow means.

**Rules.** API-22, ERR-09, API-02

```rust
use std::num::NonZeroU64;

/// An amount of money, in cents. No `Add`: overflow must reach the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Cents(u64);

impl Cents {
    /// Nothing.
    pub const ZERO: Self = Self(0);

    /// Wraps a raw amount.
    #[must_use]
    pub const fn new(cents: u64) -> Self {
        Self(cents)
    }

    /// The raw amount.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The sum, or `None` when it does not fit.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }

    /// The difference, or `None` when `other` is larger.
    #[must_use]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Self)
    }

    /// Equal shares and the remainder. The divisor's type rules out division by zero.
    #[must_use]
    pub fn split(self, parts: NonZeroU64) -> (Self, Self) {
        (Self(self.0 / parts), Self(self.0 % parts))
    }
}

/// The total of `amounts`, or `None` on overflow. Never `.sum()`, which panics or wraps.
#[must_use]
pub fn total(amounts: &[Cents]) -> Option<Cents> {
    amounts.iter().try_fold(Cents::ZERO, |sum, &amount| sum.checked_add(amount))
}
```

**Notes.**
- `impl Add for Cents` would hide a choice: `+` on `u64` panics in debug builds and wraps in release. Expose `checked_*`, or document saturating or wrapping semantics on the operator (API-22).
- Callers turn `None` into their own error: `.ok_or(LedgerError::Overflow { account })?` (ERR-07).
- `split` returns the remainder, so no cent disappears. Value objects compare by value and have no identity; entities get ids (pattern 32).

## 32. Typed aggregate id

**Use when.** Many aggregates share one id representation and each needs its own id type (`Id<Order>`, `Id<Customer>`).

**Rules.** API-02, ARC-13, OWN-16

```rust
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// Identifies one `T`. An `Id<Order>` is not an `Id<Customer>`.
pub struct Id<T> {
    raw: u64,
    // `fn() -> T`: the id owns no `T`, so it is `Send + Sync + Copy` for every `T`.
    entity: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    /// Wraps a raw id from storage.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self { raw, entity: PhantomData }
    }

    /// The raw value, for storage.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.raw
    }
}

// Written by hand: derives would demand `T: Clone`, `T: PartialEq`, ... of the entity.
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for Id<T> {}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Id").field(&self.raw).finish()
    }
}
```

**Notes.**
- One generic type replaces a newtype per entity (pattern 2) when every id has the same representation. Add `Ord` the same way if ids are map keys in a `BTreeMap`.
- `PhantomData<T>` would make `Id<T>` `!Send` whenever `T` is, and tell drop check that the id owns a `T`. `PhantomData<fn() -> T>` does neither.
- `type OrderId = Id<Order>;` is fine: the alias names a distinct type, unlike an alias of `u64` (API-02).

## Review checklist
- [ ] Is a value with rules a newtype with a validating `TryFrom`/`FromStr` and private field? (API-02, API-03)
- [ ] Are ids distinct types rather than bare integers? (API-02)
- [ ] Do optional settings use a params struct or builder instead of positional arguments or bools? (API-10)
- [ ] Is a required construction step enforced by types where misuse is likely, with a compile-fail test? (API-13, TST-08)
- [ ] Is every public trait deliberately sealed or open? (API-12)
- [ ] Does cleanup live in a guard's `Drop`, bound to a named variable? (API-11, H-07)
- [ ] Do state transitions move data (`mem::take`/`replace`) instead of cloning, and keep state on rejection? (OWN-08, H-08)
- [ ] Are business decisions pure (`decide`/`evolve`) and I/O behind consumer-owned ports with hand-written fakes? (ARC-03, ARC-13, TST-09)
- [ ] Do library errors carry typed context and sources, with `anyhow` only in the binary? (ERR-03, ERR-06, H-09)
- [ ] Do accessors return slices or iterators instead of fresh `Vec`s? (OWN-13)
- [ ] Are cyclic structures arenas with index handles rather than `Rc<RefCell<_>>`? (OWN-11)
- [ ] Is shared mutable async state owned by an actor, with bounded channels and an owner for every task? (ASY-04, ASY-07, ASY-09)
- [ ] Does shutdown cancel cooperatively, wait with a deadline, then abort? (ASY-05, H-13)
- [ ] Is concurrency bounded with `buffer_unordered` or a `Semaphore`? (PERF-10)
- [ ] Does external data go through a DTO and `TryFrom`, with versioned, upcast schemas for anything persisted? (OUT-03, OUT-05)
- [ ] Does CLI output go to locked, buffered stdout with explicit flush and documented exit codes? (OUT-06, PERF-07)
- [ ] Are secrets wrapped so `Debug` and serialization cannot leak them, with a test proving it? (OUT-08)
- [ ] Does a type with fallible teardown expose `finish`/`close(self) -> Result`, with `Drop` only logging? (ERR-13)
- [ ] Does a fallible function that consumes its argument hand it back in the error? (ERR-14)
- [ ] Is a local runtime choice `&mut dyn Trait` on the stack rather than a `Box`? (API-09)
- [ ] Is a repeated closure bound a named trait with a blanket impl? (API-18)
- [ ] Is a kind fixed at creation a sealed type parameter rather than an `Option` field? (API-13)
- [ ] Do tree passes share one `walk`/`fold` match, stop with `ControlFlow`, and move nodes instead of cloning? (CRF-03, H-08)
- [ ] Is shared service state behind a cheap-clone handle or named wrapper, with no `Arc` in public signatures? (API-23)
- [ ] Do readers of read-mostly shared data work on an `Arc` snapshot, updated with `Arc::make_mut`? (OWN-11, PERF-12)
- [ ] Does domain arithmetic use `checked_*` instead of operators that can overflow? (API-22, ERR-09)
- [ ] Are generic ids `PhantomData<fn() -> T>` with hand-written impls? (API-02)
