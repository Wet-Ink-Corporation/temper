# API design

How to shape anything another module or crate calls: names, types, traits, parameters, visibility, docs, features and semver. Read this before adding or changing a public type, trait, function or module surface, and before answering any "is this a breaking change?" question. Ownership of parameters and returns is in `ownership.md`; error types are in `errors.md`.

Rules covered: API-01, API-02, API-03, API-04, API-05, API-06, API-07, API-08, API-09, API-10, API-11, API-12, API-13, API-14, API-15, API-16, API-17, API-18, API-19, API-20

## Naming

### API-01 · Standard names

**Why.** Callers predict cost and ownership from the prefix. `as_` promises a free borrow, `to_` warns of work, `into_` says the receiver is gone. A wrong prefix lies about performance; `clippy::wrong_self_convention` checks the receiver matches.

| Pattern | Contract | Examples |
|---|---|---|
| `as_x(&self) -> &X` | Free borrowed view. No allocation, no computation. | `as_str`, `as_slice`, `as_bytes` |
| `to_x(&self) -> X` | Expensive: allocates, copies or computes. | `to_string`, `to_ascii_lowercase`, `to_m3u` |
| `into_x(self) -> X` | Consumes the receiver. Usually cheap (moves). | `into_inner`, `into_bytes`, `into_tracks` |
| Getter `x(&self)` | Field name, never `get_x`. Reserve `get` for keyed or indexed lookup returning `Option`. | `name()`, `map.get(&k)` |
| `x_mut(&mut self)` | Mutable twin of an accessor. | `last_mut`, `get_mut`, `as_mut_slice` |
| `is_x` / `has_x` | Pure `bool` predicate, no side effects. | `is_empty`, `has_permission` |
| `new` | Primary constructor. Infallible, or `Result` when it validates (then also implement `TryFrom`, API-07). | `Playlist::new` |
| `with_x` | Constructor variant taking one extra setting. | `with_capacity` |
| `from_x` | Named constructor only when `From` cannot express it (extra arguments or a unit in the name). | `Duration::from_secs` |
| `try_x` | Fallible twin of an infallible `x`. | `try_reserve`, `try_from` |
| `iter` / `iter_mut` / `into_iter` | Borrowing, mutably borrowing, consuming iteration, plus `IntoIterator` for `&T`, `&mut T` and `T` so `for` loops work. | below |
| Conversions | Traits, not methods: `From`, `TryFrom`, `AsRef`, `FromStr` (API-07). | `impl TryFrom<&str> for EmailAddress` |

**Do.**
```rust
//! Naming conventions (API-01).

/// An ordered list of track titles.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Playlist {
    name: String,
    tracks: Vec<String>,
}

impl Playlist {
    /// Creates an empty playlist.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), tracks: Vec::new() }
    }

    /// Returns the name. A getter is the field name, never `get_name`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns `true` if the playlist has no tracks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Borrows the tracks as a slice: free, so `as_`.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.tracks
    }

    /// Renders an M3U document: allocates, so `to_`.
    #[must_use]
    pub fn to_m3u(&self) -> String {
        self.tracks.iter().fold(String::from("#EXTM3U\n"), |mut doc, track| {
            doc.push_str(track);
            doc.push('\n');
            doc
        })
    }

    /// Consumes the playlist and returns its tracks: `into_`.
    #[must_use]
    pub fn into_tracks(self) -> Vec<String> {
        self.tracks
    }

    /// Iterates over the tracks by reference.
    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.tracks.iter()
    }
}

impl<'a> IntoIterator for &'a Playlist {
    type Item = &'a String;
    type IntoIter = std::slice::Iter<'a, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
```

**Don't.**
```rust,ignore
pub fn get_name(&self) -> &str { .. }        // `get_` prefix on a getter
pub fn as_string(&self) -> String { .. }     // `as_` that allocates: say `to_string`
pub fn to_inner(self) -> Vec<String> { .. }  // consumes, so `into_inner`
```

**Detect.** `clippy::wrong_self_convention`, `clippy::iter_without_into_iter`, `clippy::should_implement_trait`. Review: grep for `fn get_` on types with no key parameter.

## Types that make illegal states unrepresentable

### API-02 · Newtypes and enums, not primitives and flags

**Why.** A `bool` plus two `Option`s has eight states when the domain has two; every consumer must re-derive which combinations are legal, and one forgets. An enum whose variants carry exactly their own data has only legal states, and `match` forces every consumer to handle each. A newtype stops a store id being passed where a user id was meant.

**Do.**
```rust
//! Enums instead of flags (API-02).

/// Identifies a store. A newtype, so it cannot be confused with other `u32`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StoreId(u32);

impl StoreId {
    /// Wraps a raw store number.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw store number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// How an order reaches the customer. Each case carries exactly its own data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fulfilment {
    /// The customer collects the order.
    Pickup {
        /// Where to collect it.
        store: StoreId,
    },
    /// The order is delivered.
    Delivery {
        /// Street address.
        address: String,
    },
}
```

**Don't.**
```rust,ignore
pub struct Order {
    pub is_delivery: bool,         // true + store = Some(..) is representable and meaningless
    pub store: Option<u32>,        // which u32? a store, a user, a quantity?
    pub address: Option<String>,
    pub morning: Option<bool>,     // three states named by a bool
}
```

**Detect.** Review: `bool` fields next to `Option` fields, `Option<bool>`, raw `u32`/`String` ids in signatures. `clippy::struct_excessive_bools` (threshold 2 in `clippy.toml`).

### API-03 · Parse, don't validate

**Why.** A `validate(&str) -> bool` check leaves the value as `&str`, so every later function either re-checks or trusts blindly. Parsing converts once into a type whose private fields can only hold valid data; after that the invariant is a fact the compiler carries. Store the parsed structure (here: local and domain parts), not the raw string plus a promise.

**Do.**
```rust
//! Parse, don't validate (API-02, API-03).

use std::fmt;
use std::str::FromStr;

/// Why a string is not an [`EmailAddress`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EmailError {
    /// The input has no `@`.
    #[error("email address has no `@`")]
    MissingAt,
    /// The part before or after the `@` is empty.
    #[error("email address has an empty local or domain part")]
    EmptyPart,
}

/// A syntactically valid email address. The only way in is parsing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EmailAddress {
    local: String,
    domain: String,
}

impl EmailAddress {
    /// Returns the part before the `@`.
    #[must_use]
    pub fn local(&self) -> &str {
        &self.local
    }

    /// Returns the lowercased domain. No re-check: the type guarantees it.
    #[must_use]
    pub fn domain(&self) -> &str {
        &self.domain
    }
}

impl TryFrom<&str> for EmailAddress {
    type Error = EmailError;

    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        let (local, domain) = raw.rsplit_once('@').ok_or(EmailError::MissingAt)?;
        if local.is_empty() || domain.is_empty() {
            return Err(EmailError::EmptyPart);
        }
        Ok(Self { local: local.to_owned(), domain: domain.to_ascii_lowercase() })
    }
}

impl FromStr for EmailAddress {
    type Err = EmailError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::try_from(raw)
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.local, self.domain)
    }
}

// Compile-time proof that the public types stay thread-safe (API-06).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EmailAddress>();
    assert_send_sync::<EmailError>();
};
```

**Don't.**
```rust,ignore
pub fn is_valid_email(s: &str) -> bool { s.contains('@') }
pub fn send(to: &str) { .. }        // did the caller check? callee cannot know, so it re-checks
pub struct Email(pub String);       // public field: `Email("nope".into())` bypasses parsing
```

**Detect.** Review: `is_valid_*`/`validate_*` returning `bool` or `Result<(), _>`; the same check appearing in two places; newtypes with `pub` inner fields; `new` taking unchecked input without returning `Result`.

### API-04 · Private fields unless plain data

**Why.** A public field is a promise that any value is valid and that the field will exist forever. Keep fields private whenever an invariant exists, and expose getters. Plain data with no invariant (a `Receipt`, a params struct, a DTO) may use `pub` fields.

**Detect.** Review every `pub` field: "can a caller put a value here that breaks this type?" If yes, it is a finding. (The restriction lint `clippy::exhaustive_structs` can list them, but it is not in the template: a member crate cannot add lints next to `[lints] workspace = true`. This check is review-only.)

### API-05 · `#[non_exhaustive]`, deliberately

**Why.** Adding a variant or field to an exhaustive public type is a semver break (API-17). `#[non_exhaustive]` reserves the right to grow, at the cost that downstream must write a `_` arm and cannot build the struct with a literal. Use it on errors, config, events and options that will grow. Leave closed domain sets (`Fulfilment` above) exhaustive so downstream `match` breaks loudly when you add a case, which is what you want for a domain concept.

**Detect.** Review each new public `enum`/`struct`: is "may grow" or "closed" a stated decision? This check is review-only; the template does not enable `clippy::exhaustive_enums`.

## Trait implementation checklist

### API-06 · Implement standard traits eagerly · API-07 · Conversions are traits

**Why.** Downstream crates cannot add std trait impls to your types (orphan rule). A missing `Debug` blocks `{:?}` and `#[derive(Debug)]` on every struct that contains your type; a missing `Hash` blocks use as a map key. Conversions via traits compose with `?`, `.into()`, `.parse()` and generic bounds; `from_x` methods compose with nothing.

| Trait | Implement when | Notes |
|---|---|---|
| `Debug` | Always, on every public type. | Hand-write it to redact secrets (OUT-08). `missing_debug_implementations` warns. |
| `Clone` | The type has value semantics. | Not for handles to unique resources (sockets, guards). |
| `Copy` | Small (≤ 2–3 words), plain value, will stay that way. | Removing `Copy` later is breaking (OWN-16). |
| `PartialEq`, `Eq` | Equality is meaningful. `Eq` unless a float is inside. | Needed by `assert_eq!` in downstream tests. |
| `Hash` | With `Eq`, when usable as a key. | Must agree with `Eq`. |
| `PartialOrd`, `Ord` | A natural total order exists. | No natural order: do not derive; callers use `sort_by_key`. |
| `Default` | An obvious neutral value exists. | Not for types whose invariant needs input. |
| `Display` | User-facing: ids, errors, quantities. | Not "a nicer `Debug`" (OUT-10). |
| `From<T>` | Infallible, lossless, obvious. | Gives `Into` for free. Never implement `Into` directly. |
| `TryFrom<T>` | Fallible conversion or validation. | The parse-don't-validate entry point (API-03). |
| `FromStr` | The type has a text form. | Enables `"..".parse::<T>()` and clap/serde integration. |
| `AsRef<T>` | Cheap borrow as `T`. | `AsRef<str>`, `AsRef<[T]>`, `AsRef<Path>`. |
| `Borrow<T>` | Only if `Eq`, `Hash` and `Ord` agree with `T`'s. | Enables `map.get(&str)` for `String`-like keys. |
| `IntoIterator` | Collection-like: for `&T`, `&mut T`, `T`. | Makes `for x in &c` work. |
| `Error` | Error types. | Plus `Send + Sync + 'static` (ERR-01). |
| `Serialize`/`Deserialize` | Behind a `serde` feature in libraries (API-16). | Wire contracts: `structured-output.md`. |
| `Send`, `Sync` | Automatic. Keep them. | Assert at compile time, as the `EmailAddress` example above does. |
| `Deref` | Smart pointers only. | Never for inheritance (API-19). |

**Detect.** `missing_debug_implementations`, `clippy::should_implement_trait` (a method named `from_str`, `add`, `next`, ...), `clippy::from_over_into`. Review: public type missing `Clone`/`PartialEq` with no stated reason.

## Generic inputs, concrete outputs

### API-08 · Accept broadly, return precisely

**Why.** `impl AsRef<Path>` accepts `&str`, `String`, `&Path` and `PathBuf` without the caller converting. Returning a concrete type (or `impl Iterator` when the type is unnameable) keeps callers free to use every method and avoids allocating a `Vec` they only iterate (OWN-13).

**Do.**
```rust
//! Generic inputs, concrete outputs (API-08).

use std::io;
use std::path::Path;

/// Reads a manifest from any path-like value.
///
/// # Errors
///
/// Returns the underlying I/O error if the file cannot be read.
pub fn read_manifest(path: impl AsRef<Path>) -> io::Result<String> {
    std::fs::read_to_string(path)
}

/// Returns the non-blank lines of `text` without allocating a `Vec`.
pub fn non_blank(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| !line.trim().is_empty())
}
```

**The turbofish caveat.** Argument-position `impl Trait` is an anonymous generic parameter, and callers can never name it with turbofish. Switching a published `fn load<P: AsRef<Path>>(p: P)` to `fn load(p: impl AsRef<Path>)` therefore breaks any caller that wrote `load::<&str>(..)`, and the reverse change adds a parameter to their turbofish. When a function mixes both forms, callers can turbofish only the named parameters. Pick one form at publication and keep it; use a named parameter when callers may need to specify the type (the type appears only in the return, or inference is ambiguous).

**Detect.** Review: `&PathBuf`/`&String` parameters (OWN-03), `-> Vec<T>` from a function whose callers only iterate, `Box<dyn Iterator>` returns where `impl Iterator` works.

## Static vs dynamic dispatch

### API-09 · Generics by default, `dyn` on purpose

**Why.** Generics monomorphize: calls inline, no vtable, no allocation, but one copy of the code per type. `dyn Trait` is one copy with an indirect call, and is the only way to hold different concrete types in one collection or to choose an implementation at runtime.

| Situation | Choose | Why |
|---|---|---|
| One concrete type per call site, hot path | `fn f<T: Trait>(x: &T)` or `impl Trait` | Inlining, zero-cost (PERF-08). |
| Heterogeneous collection (`Vec` of mixed handlers) | `Box<dyn Trait>` / `&dyn Trait` | Generics cannot mix types. |
| Plug-ins or strategy chosen at runtime from config | `Box<dyn Trait>` / `Arc<dyn Trait>` | Type unknown at compile time. |
| Closed, known set of variants | `enum` + `match` | Exhaustive, no vtable, no allocation. |
| Many instantiations bloating binary or compile time | `dyn`, or a generic shim over a non-generic inner fn | Measured code-size win. |
| Trait with generic methods or `-> impl Trait` | Generics (not dyn-compatible) | Or split off a dyn-compatible subtrait. |
| Async trait used as `dyn` | Boxed future through a named alias (ASY-03) | Native `async fn` in traits is not dyn-compatible. |

**Do.**
```rust
//! Static versus dynamic dispatch (API-09).

/// A named counter.
pub trait Metric {
    /// Current value.
    fn value(&self) -> u64;
}

/// Static dispatch: one monomorphized copy per `M`, calls can inline.
#[must_use]
pub fn total<M: Metric>(metrics: &[M]) -> u64 {
    metrics.iter().map(Metric::value).fold(0, u64::saturating_add)
}

/// Dynamic dispatch: one copy, and the slice may mix concrete types.
#[must_use]
pub fn total_mixed(metrics: &[Box<dyn Metric + Send + Sync>]) -> u64 {
    metrics.iter().map(|metric| metric.value()).fold(0, u64::saturating_add)
}
```

**Detect.** Review: `Box<dyn Trait>` for a single implementor, `dyn` in a hot loop, `Vec<Box<dyn Trait>>` where the implementor set is closed (use an enum).

## Parameters: bools, params structs, builders

### API-10 · At most one bool parameter; params struct or builder past four

**Why.** `fetch(url, true, false)` is unreadable at the call site, and swapping the two bools still compiles. An enum names the choice. A single bool that reads naturally (`set_visible(true)`) is fine. Two or more bools, any `Option<bool>`, more than four parameters, or optional settings become enums plus a params struct with `Default` (plain data) or a builder (private fields, invariants, or many optional parts). clippy enforces the outer bounds: `max-fn-params-bools = 1` and `too-many-arguments-threshold = 4`.

### API-11 · `#[must_use]` on values that must be used

**Why.** A dropped builder, a computed result nobody reads, or an ignored `Receipt` is a silent bug. `#[must_use]` on the type (builders) or the function (pure computations) turns it into a warning, which the gate denies.

**Do.**
```rust
//! Parameters: enums, params structs, builders (API-10, API-11).

use std::time::Duration;

/// Redirect policy. Replaces a `follow_redirects: bool` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Redirects {
    /// Follow up to the client limit.
    #[default]
    Follow,
    /// Return the 3xx response as-is.
    Refuse,
}

/// Plain-data options with a meaningful default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOptions {
    /// Per-request timeout.
    pub timeout: Duration,
    /// Redirect policy.
    pub redirects: Redirects,
    /// Largest accepted body, in bytes.
    pub max_body_bytes: usize,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            redirects: Redirects::default(),
            max_body_bytes: 1 << 20,
        }
    }
}

/// A finished request. Fields are private: the builder is the only way in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    url: String,
    headers: Vec<(String, String)>,
}

impl Request {
    /// Starts building a request for `url`.
    pub fn builder(url: impl Into<String>) -> RequestBuilder {
        RequestBuilder { url: url.into(), headers: Vec::new() }
    }

    /// Returns the target URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the headers in insertion order.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}

/// Consuming builder: each step moves `self`, `build` moves the fields out.
#[derive(Debug, Clone)]
#[must_use = "a builder does nothing until `build` is called"]
pub struct RequestBuilder {
    url: String,
    headers: Vec<(String, String)>,
}

impl RequestBuilder {
    /// Adds a header.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Finishes the request without copying anything.
    #[must_use]
    pub fn build(self) -> Request {
        Request { url: self.url, headers: self.headers }
    }
}
```

| Builder | Signature | Use when |
|---|---|---|
| Consuming | `fn x(self, ..) -> Self`, `fn build(self) -> T` | One-shot construction. `build` moves fields out with no copies. Chains in one expression. Default choice. |
| Non-consuming | `fn x(&mut self, ..) -> &mut Self`, `fn build(&self) -> T` | The builder is reused to produce many values, or configured across branches (`if verbose { b.label(..); }`). `build(&self)` must copy, so the stored parts get cloned (a real duplication under H-08). |
| Params struct | `pub` fields + `Default`, caller writes `FetchOptions { timeout, ..Default::default() }` | Plain data, no invariants, no ordering. Adding a field breaks callers who write a full literal; for a published crate that will grow, prefer a builder. |
| Typestate builder | Methods move between `Builder<State>` types | Missing required parts or wrong order must not compile (API-13). |

**Don't.**
```rust,ignore
pub fn fetch(url: &str, follow: bool, strict: bool) -> Response { .. }   // fetch(u, false, true)?
pub fn connect(host: &str, port: u16, tls: Option<bool>, retries: u32, timeout: u64) { .. }
```

**Detect.** `clippy::fn_params_excessive_bools` (threshold 1), `clippy::too_many_arguments` (threshold 5), `clippy::must_use_candidate`, `clippy::return_self_not_must_use`.

## Typestate

### API-13 · Make misuse a compile error

**Why.** When an operation is only valid in some states (send after recipient, query after connect), encoding the state as a type parameter deletes the invalid method from the invalid type. The error moves from a runtime `Err` to a compile error, and there is no `Option` to unwrap later. The state type can carry the data that state owns (`Recipient(String)`). Prove the guarantee with a `trybuild` test; a `compile_fail` doctest documents it but does not check the error on stable (TST-08).

**Do.**
````rust
//! Typestate (API-13): `send` does not exist until a recipient is set.
//!
//! ```compile_fail
//! use api::typestate::Draft;
//! let receipt = Draft::new("hello").send(); // no `send` on `Draft<NoRecipient>`
//! ```

/// State: no recipient yet.
#[derive(Debug)]
pub struct NoRecipient;

/// State: a recipient is set. The state carries its own data.
#[derive(Debug)]
pub struct Recipient(String);

/// An unsent message in state `S`.
#[derive(Debug)]
#[must_use]
pub struct Draft<S> {
    subject: String,
    state: S,
}

/// Proof that a message went out. Plain data, so public fields (API-04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// Recipient address.
    pub to: String,
    /// Subject line.
    pub subject: String,
}

impl Draft<NoRecipient> {
    /// Starts a draft with no recipient.
    pub fn new(subject: impl Into<String>) -> Self {
        Self { subject: subject.into(), state: NoRecipient }
    }

    /// Sets the recipient: a transition, so it consumes `self`.
    pub fn to(self, address: impl Into<String>) -> Draft<Recipient> {
        Draft { subject: self.subject, state: Recipient(address.into()) }
    }
}

impl<S> Draft<S> {
    /// Valid in every state.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }
}

impl Draft<Recipient> {
    /// Sends the message. Only exists once a recipient is set.
    #[must_use]
    pub fn send(self) -> Receipt {
        Receipt { to: self.state.0, subject: self.subject }
    }
}
````

**Don't.** Use typestate for a state that is only known at runtime (a connection that may drop at any moment). Then the state is data: an `enum` field and methods returning `Result`. Typestate also multiplies types in signatures; reserve it for protocols where misuse is likely and costly.

**Detect.** Review: `Option` fields that are `Some` only after a certain call; methods that begin with "if not yet configured, return error".

## Sealed traits

### API-12 · Seal traits downstream must not implement

**Why.** Once downstream crates can implement a public trait, adding any method (even a defaulted one can conflict) or changing a bound is a breaking change. Sealing keeps the trait usable in bounds and as `dyn` while reserving implementation to this crate.

**Do.**
```rust
//! Sealed traits (API-12).

mod private {
    /// The seal. It is public inside a private module, so downstream code can
    /// see it as a bound but can never name it to implement it.
    pub trait Sealed {}
}

/// A shape with an area. Downstream crates cannot implement it.
pub trait Shape: private::Sealed {
    /// Area in square units.
    fn area(&self) -> f64;
}

/// A circle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    /// Radius.
    pub radius: f64,
}

impl private::Sealed for Circle {}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.radius * self.radius
    }
}
```

`pub(crate) trait Sealed` does not work: a crate-private supertrait on a public trait trips the `private_bounds` lint. Keep the seal `pub` inside a private module.

**Detect.** Review: new public trait with no stated position on downstream implementations.

## Trait shape

### API-18 · Small role traits; extension traits for foreign types

**Why.** A trait is a contract every implementor must fulfil; a ten-method trait forces fakes (TST-09) to implement nine methods a test does not care about. One role per trait. To add methods to a type you do not own, write an extension trait rather than a wrapper.

**Do.**
```rust
//! Small role traits and extension traits (API-18).

use std::time::SystemTime;

/// The one capability a use case needs from time: a role, not a god trait.
pub trait Clock {
    /// Returns the current time.
    fn now(&self) -> SystemTime;
}

/// Adds parsing helpers to `str` without a wrapper type.
pub trait StrExt {
    /// Splits `key: value`, trimming both sides.
    fn split_header(&self) -> Option<(&str, &str)>;
}

impl StrExt for str {
    fn split_header(&self) -> Option<(&str, &str)> {
        self.split_once(':').map(|(name, value)| (name.trim(), value.trim()))
    }
}
```

**Detect.** Review: traits with more than about five required methods, traits named after a type (`UserServiceTrait`) rather than a role, `impl Trait` blocks full of `unimplemented!()` in fakes.

## Visibility and the facade

### API-14 · Minimal surface

**Why.** Every `pub` item is a semver commitment and a doc obligation. The less that is public, the more you can change without an H-16 decision.

- Private by default. `pub(crate)` for items shared across modules. `pub` only for the deliberate API, re-exported from `lib.rs`.
- `lib.rs` is the table of contents (ARC-05): module declarations and `pub use` re-exports, no logic. Users import `mycrate::Order`, not `mycrate::domain::order::model::Order`.
- `unreachable_pub` warns on `pub` items that are not actually reachable; make them `pub(crate)`.
- Never widen visibility for a test (TST-02). Test through the public API, or put the unit test beside the item.
- Do not leak dependency types: a `reqwest::Error` or `sqlx::Row` in your public signature makes their major version part of your semver. Wrap it (an opaque error, a newtype) unless the dependency is deliberately part of the contract (a `serde` impl, a `bytes::Bytes` buffer); then re-export the crate (`pub use bytes;`) so users get the matching version.

**Do.**
```rust,ignore
// lib.rs: a table of contents. Module bodies live in their own files.
//! Order management domain: types, decisions and events.

mod decide;
mod event;
mod order;

pub use decide::{Rejection, decide};
pub use event::OrderEvent;
pub use order::{Order, OrderId};
```

**Detect.** `unreachable_pub`; `cargo doc --open` and read the surface; `cargo public-api` diff when available; review for `pub mod` exposing internals and third-party types in signatures.

## Documentation template

### API-15 · Document every public item

**Why.** Rustdoc is the API's user interface, and doctests are the only examples guaranteed to compile. A summary line is what search and IDE hovers show.

- First line: one sentence, what it does, third person ("Splits ...", "Returns ...").
- `# Examples` on primary entry points: a runnable doctest using `?` via a hidden `fn main() -> Result<..>`, never `unwrap` (TST-11).
- `# Errors` on every fallible function: each variant and when it occurs (`clippy::missing_errors_doc`).
- `# Panics` should never be needed: production code does not panic (H-02, H-03). If you feel the need to write one, the design is wrong.
- Intra-doc links (``[`PairError::EmptyKey`]``) instead of plain names; `rustdoc::broken_intra_doc_links` checks them.

**Do.**
````rust
//! Documentation template (API-15).

/// Why [`parse_pair`] rejected its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PairError {
    /// The input has no `=`.
    #[error("expected `key=value`, found no `=`")]
    MissingEquals,
    /// The key before `=` is empty.
    #[error("key is empty")]
    EmptyKey,
}

/// Splits a `key=value` pair at the first `=`.
///
/// The value may be empty or contain further `=` characters.
///
/// # Examples
///
/// ```
/// use api::docs::parse_pair;
///
/// # fn main() -> Result<(), api::docs::PairError> {
/// let (key, value) = parse_pair("region=eu-west-1")?;
/// assert_eq!(key, "region");
/// assert_eq!(value, "eu-west-1");
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns [`PairError::MissingEquals`] if `input` has no `=`, and
/// [`PairError::EmptyKey`] if nothing precedes it.
pub fn parse_pair(input: &str) -> Result<(&str, &str), PairError> {
    let (key, value) = input.split_once('=').ok_or(PairError::MissingEquals)?;
    if key.is_empty() {
        return Err(PairError::EmptyKey);
    }
    Ok((key, value))
}
````

**Detect.** `missing_docs`, `clippy::missing_errors_doc`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, `cargo test --doc`.

## Features

### API-16 · Additive features, lean defaults

**Why.** Cargo unifies features across the dependency graph: if any crate enables `serde` on your crate, everyone gets it. A feature that removes an item, changes a type's layout or behaviour, or conflicts with another will break some unrelated crate in the graph.

```toml
[features]
default = []                        # lean: users opt in
serde = ["dep:serde"]               # adds impls, removes nothing
[dependencies]
serde = { workspace = true, optional = true, features = ["derive"] }
```

- Gate impls and modules with `#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]` and `#[cfg(feature = "..")]`. Enabling a feature only adds.
- Positive names: a `std` feature, never a `no_std` feature. No mutually exclusive features; choose between backends at runtime or in the binary.
- Test `--no-default-features` and `--all-features` in CI (`cargo hack --feature-powerset check` for full coverage).

**Detect.** Review `[features]`: any feature whose `cfg` removes or changes an item, any `not(feature = ..)` guarding public API.

## Semver hazards

### API-17 · Published crates follow semver; breaks are H-16

**Why.** Downstream builds break silently on `cargo update` if a minor release contains a major change. Many breaks are invisible in the diff because they come from auto traits and inference.

Breaking changes (need a major version bump, and an H-16 decision before starting):
- **Adding a public field** to a struct without `#[non_exhaustive]` whose fields are all public: breaks struct literals and exhaustive patterns.
- **Adding an enum variant** without `#[non_exhaustive]`: breaks every exhaustive `match`.
- **Adding a trait method without a default**: breaks every implementor. Adding a defaulted method can still cause ambiguity errors (seal the trait, API-12).
- **Changing auto traits**: adding a field of type `Rc`, `Cell` or `*const` silently removes `Send`/`Sync`. Assert them at compile time (API-06).
- **Tightening bounds**: adding `T: Clone` to a generic function or impl.
- **Changing an `impl Trait` return**: the hidden type's `Send`/`Sync`/`Unpin` leak to callers, so a new hidden type can remove them. Capturing more lifetimes (edition 2024 captures all in scope; narrow with `use<..>`, MOD-06) can break borrowing callers.
- Removing or renaming a public item, changing a signature, turning a named generic into argument-position `impl Trait` (turbofish caveat above), removing a feature, raising the MSRV (by policy).

Run `cargo semver-checks` before each release, and in CI for published crates (`--baseline-rev <last-release-tag>`). It catches most of the list; auto-trait and inference breaks still need review.

**Detect.** `cargo semver-checks`; review every change to a `pub` item in a published crate against this list.

## Deref is not inheritance

### API-19 · `Deref` is for smart pointers only

**Why.** `impl Deref<Target = Vec<u32>> for Leaderboard` exposes all of `Vec`'s methods, including `push` and `DerefMut`'s `sort_unstable_by`, which break the sorted invariant. Method resolution through `Deref` also confuses readers and makes the inner type part of your API. Expose what the type means, and use `AsRef`/an accessor for a borrow.

**Do.**
```rust
//! `Deref` is not inheritance (API-19).

/// Scores kept sorted, highest first. Exposes only what keeps that true.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Leaderboard {
    scores: Vec<u32>,
}

impl Leaderboard {
    /// Inserts a score at its sorted position.
    pub fn insert(&mut self, score: u32) {
        let at = self.scores.partition_point(|&existing| existing >= score);
        self.scores.insert(at, score);
    }

    /// Returns the highest score, if any.
    #[must_use]
    pub fn top(&self) -> Option<u32> {
        self.scores.first().copied()
    }
}

impl AsRef<[u32]> for Leaderboard {
    fn as_ref(&self) -> &[u32] {
        &self.scores
    }
}
```

**Don't.**
```rust,ignore
impl Deref for Leaderboard { type Target = Vec<u32>; fn deref(&self) -> &Vec<u32> { &self.scores } }
impl DerefMut for Leaderboard { .. }   // board.push(7) now bypasses the ordering invariant
```

**Detect.** Review every `impl Deref`/`DerefMut`: is the type a pointer or a guard (`Box`, `Arc`, `MutexGuard`, `Cow`)? If not, it is a finding.

## `#[diagnostic::on_unimplemented]`

### API-20 · Custom errors for traits users misimplement

**Why.** "the trait bound `Billing: Handler<Refund>` is not satisfied" says what is missing but not what to do. `#[diagnostic::on_unimplemented]` (stable since 1.78) replaces the message, label and note with domain guidance. It is a hint: compilers may ignore it, and it never changes what compiles.

**Do.**
```rust
//! Custom trait diagnostics (API-20).

/// Handles one command type `C`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot handle `{C}` commands",
    label = "no `Handler<{C}>` impl for this type",
    note = "implement `Handler<{C}>` for `{Self}`, or route `{C}` to a handler that has one"
)]
pub trait Handler<C> {
    /// What handling produces.
    type Reply;

    /// Handles `command`.
    fn handle(&mut self, command: C) -> Self::Reply;
}

/// Routes `command` to `handler`.
pub fn dispatch<H: Handler<C>, C>(handler: &mut H, command: C) -> H::Reply {
    handler.handle(command)
}
```

**Detect.** Review: public traits used as bounds in user-facing generic APIs (handlers, extractors, codecs) where the default error is cryptic.

## Review checklist

- [ ] Do `as_`/`to_`/`into_`, getters, `is_`/`has_`, `new`/`with_` and `iter*` names match their cost and ownership? (API-01)
- [ ] Can any public type hold an illegal combination (bool plus options, raw ids)? (API-02)
- [ ] Is validation done once at construction, returning `Result`, with the invariant never re-checked? (API-03)
- [ ] Is every `pub` field on a type without invariants? (API-04)
- [ ] Is `#[non_exhaustive]` a deliberate choice on each new public enum and struct? (API-05)
- [ ] Do public types implement `Debug` and the other traits that make sense, and stay `Send + Sync`? (API-06)
- [ ] Are conversions `From`/`TryFrom`/`FromStr`/`AsRef` rather than ad-hoc methods? (API-07)
- [ ] Do inputs accept generic forms and outputs return concrete types or `impl Trait`? (API-08)
- [ ] Is every `dyn` justified by heterogeneity, plug-ins or code size? (API-09)
- [ ] Any `bool` or `Option<bool>` parameter, or four or more parameters? (API-10)
- [ ] Are builders, pure constructors and computed results `#[must_use]`? (API-11)
- [ ] Is every new public trait either sealed or intentionally open? (API-12)
- [ ] Would a typestate or builder turn a likely misuse into a compile error, with a compile-fail test? (API-13)
- [ ] Is anything `pub` that could be private or `pub(crate)`? Does a dependency type leak? (API-14)
- [ ] Does every public item have docs, entry points a `?`-using example, fallible functions `# Errors`? (API-15)
- [ ] Are features additive and default features lean? (API-16)
- [ ] Is any change on the semver hazard list, and did `cargo semver-checks` run? Is it an H-16 decision? (API-17)
- [ ] Are traits small role interfaces, with extension traits for foreign types? (API-18)
- [ ] Is `Deref` implemented only on pointer-like wrappers? (API-19)
- [ ] Would a user-facing trait bound benefit from `#[diagnostic::on_unimplemented]`? (API-20)
