# Patterns

A catalog of complete, compiling patterns that apply the rules in the other references. Each one was compiled in context on Rust 1.98.1 (edition 2024), passed `cargo clippy --all-targets -- -D warnings` under the workspace template lints, and has tests (mostly not shown). A few blocks continue an earlier one (a binary using the library above it, a DTO using pattern 1's `Email`), and they say so. Read this when the design defaults in SKILL.md §3 point at a shape (newtype, builder, typestate, port, actor) and you want the idiomatic version to copy, or when a reviewer asks "what should this look like instead?".

Start from the index; read only the pattern you need. Snippets omit `#[cfg(test)]` modules; pattern 11 keeps its fake because the fake is part of the pattern. `use` lines are included so each block compiles as a module.

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

/// Why a [`RequestBuilder`] could not build.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// `url` was never set.
    #[error("request has no url")]
    MissingUrl,
}

/// Builds [`Request`]s. Setters take `&mut self`, so a builder can be
/// configured conditionally and reused.
#[derive(Debug, Clone, Default)]
pub struct RequestBuilder {
    url: Option<String>,
    headers: Vec<(String, String)>,
    timeout: Option<Duration>,
}

impl RequestBuilder {
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

    /// Sets the URL.
    pub fn url(&mut self, url: impl Into<String>) -> &mut Self {
        self.url = Some(url.into());
        self
    }

    /// Appends a header.
    pub fn header(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Overrides the default 30 s timeout.
    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = Some(timeout);
        self
    }

    /// Builds a request; the builder stays usable for the next one.
    ///
    /// # Errors
    /// [`BuildError::MissingUrl`] when no URL was set.
    pub fn build(&self) -> Result<Request, BuildError> {
        let url = self.url.clone().ok_or(BuildError::MissingUrl)?;
        Ok(Request {
            url,
            headers: self.headers.clone(),
            timeout: self.timeout.unwrap_or(Self::DEFAULT_TIMEOUT),
        })
    }
}
```

**Notes.**
- `&mut self -> &mut Self` setters let callers write `if traced { builder.header(..); }` without rebinding.
- `build(&self)` clones fields so the builder stays reusable. When fields are expensive or the builder is single-use, prefer a consuming builder (`fn url(self, ..) -> Self`, `build(self)`) and move instead.
- Missing required data is a `Result` from `build`. If that error is common in practice, make it a compile error with a typestate builder (pattern 5).
- Setters take `impl Into<String>` because the builder stores the value (OWN-02).

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

/// Builds a [`Client`]. `build` exists only once an endpoint is set:
///
/// ```compile_fail
/// let client = my_crate::ClientBuilder::new().build();
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

impl ClientBuilder<NoEndpoint> {
    /// Starts with a 10 s timeout and no endpoint.
    pub const fn new() -> Self {
        Self {
            endpoint: NoEndpoint,
            timeout: Duration::from_secs(10),
        }
    }

    /// Sets the endpoint, unlocking `build`.
    pub fn endpoint(self, url: impl Into<String>) -> ClientBuilder<Endpoint> {
        ClientBuilder {
            endpoint: Endpoint(url.into()),
            timeout: self.timeout,
        }
    }
}

impl Default for ClientBuilder<NoEndpoint> {
    fn default() -> Self {
        Self::new()
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

impl Client {
    /// Endpoint URL.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}
```

**Notes.**
- The `compile_fail` doctest is the proof (TST-08); `cargo test` fails if `build()` ever becomes callable without an endpoint.
- Marker types are `pub` so users can name `ClientBuilder<Endpoint>` in signatures.
- Each required field doubles the state space. With more than two, take the required values in `new(..)` and keep the builder for optional ones.
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
        self.count.fetch_add(1, Ordering::Relaxed);
        InFlightGuard { owner: self }
    }

    /// Requests currently in flight.
    #[must_use]
    pub fn current(&self) -> usize {
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
        self.owner.count.fetch_sub(1, Ordering::Relaxed);
    }
}
```

**Notes.**
- Bind it as `let _guard = stats.enter();`. `let _ = stats.enter();` drops it immediately; `clippy::let_underscore_must_use` (deny) and `let_underscore_drop` catch that.
- `Drop` cannot fail or `.await`. For fallible or async cleanup, add an explicit `fn finish(self) -> Result<..>` (or `async fn close`) and keep `Drop` as the best-effort fallback.
- Never `mem::forget` a guard (H-07). A guard that protects a lock must not live across `.await` (H-14).
- `Relaxed` is enough for a statistics counter; anything that orders other memory needs a stated `Ordering` reason (ASY-10).

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
- Generic `Notes<S: NoteStore>` is static dispatch. Use `Box<dyn NoteStore>` only when the adapter is chosen at runtime.
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

```rust
use std::io;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use tokio::sync::{AcquireError, Semaphore};
use tokio::task::{JoinError, JoinSet};

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

use crate::email::{Email, EmailError}; // pattern 1

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

```rust
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
```

**Notes.**
- `Debug` redacts, so `#[derive(Debug)]` on containing configs and `tracing` fields like `?config` stay safe.
- No `Display`, `Serialize`, `PartialEq` or `Clone`: leaking needs a visible `expose()` call, which reviewers grep for, and there is exactly one copy of the secret to zeroize or drop.
- Compare secrets with a constant-time function (`subtle`), never `==`. Zeroing memory on drop needs `zeroize` or `secrecy`: a new dependency (H-16).

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
- [ ] Are secrets wrapped so `Debug` and serialization cannot leak them? (OUT-08)
