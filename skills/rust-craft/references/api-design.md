# API design

How to shape anything another module or crate calls: names, types, traits, parameters, visibility, docs, features and semver. Read this before adding or changing a public type, trait, function or module surface, and before answering any "is this a breaking change?" question. Ownership of parameters and returns is in `ownership.md`; error types are in `errors.md`.

Rules covered: API-01, API-02, API-03, API-04, API-05, API-06, API-07, API-08, API-09, API-10, API-11, API-12, API-13, API-14, API-15, API-16, API-17, API-18, API-19, API-20, API-21, API-22, API-23

## Naming

### API-01 · Standard names

**Why.** Callers predict cost and ownership from the prefix. `as_` promises a free borrow, `to_` warns of work, `into_` says the receiver is gone. A wrong prefix lies about performance. `clippy::wrong_self_convention` checks only that the receiver matches the prefix, never the cost, so cost is a review check.

| Pattern | Contract | Examples |
|---|---|---|
| `as_x(&self) -> &X` | Free borrowed view: O(1), no allocation, no validation. A conversion that checks something is `to_` (`Path::to_str` checks UTF-8). | `as_str`, `as_slice`, `as_bytes` |
| `to_x(&self) -> X` | Expensive: allocates, copies, validates or computes. On a `Copy` type, every owned-to-owned conversion is `to_x(self)`. | `to_string`, `to_m3u`, `f64::to_radians` |
| `into_x(self) -> X` | Consumes a non-`Copy` receiver. Usually cheap (moves). A wrapper around one value exposes it as `into_inner(self)`. | `into_inner`, `into_bytes`, `into_tracks` |
| Getter `x(&self)` | The field name, never `get_x`. | `name()`, `endpoint()` |
| `get` | Keyed or indexed lookup returning `Option`, or the single obvious contained value (`Cell::get`, `NonZeroU32::get`, a newtype's `get()`). Wrappers expose `get_ref`/`get_mut`. | `map.get(&k)`, `StoreId::get` |
| `x_mut(&mut self)` | Mutable twin of an accessor. `mut` goes where it sits in the return type. | `last_mut`, `get_mut`, `as_mut_slice` (not `as_slice_mut`) |
| `is_x` / `has_x` | Pure `bool` predicate, no side effects. | `is_empty`, `has_permission` |
| `new` | Primary constructor. Infallible, or `Result` when it validates (then also implement `TryFrom`, API-07). | `Playlist::new` |
| `with_x` | Constructor variant taking one extra setting. | `with_capacity` |
| `from_x` | Named constructor only when `From` cannot express it (extra arguments or a unit in the name). | `Duration::from_secs` |
| `try_x` | Fallible twin of an infallible `x`. | `try_reserve`, `try_from` |
| `iter` / `iter_mut` / `into_iter` | Borrowing, mutably borrowing, consuming iteration. Each returns a crate-owned type named after the method (`Iter`, `IterMut`, `IntoIter`; likewise `Keys`, `Drain`), plus `IntoIterator` for `&T`, `&mut T` and `T` so `for` loops work. | below |
| Conversions | Traits, not methods: `From`, `TryFrom`, `AsRef`, `FromStr` (API-07). | `impl TryFrom<&str> for EmailAddress` |

**Named iterator types.** Return a newtype over the std iterator, never `std::slice::Iter<'_, T>`: a std type in the signature pins the storage, so moving to a `VecDeque` becomes a break. `IntoIterator for &T` needs a nameable `IntoIter` anyway, because `impl Trait` in associated types is unstable. Mark the newtype `#[must_use]`, forward `size_hint`, and add `DoubleEndedIterator`, `ExactSizeIterator` and `FusedIterator` when the inner iterator has them. A free function whose iterator type never needs naming may return `impl Iterator` (API-08).

**Do.**
```rust
//! Naming conventions (API-01).

use std::iter::FusedIterator;
use std::slice;

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

    /// Iterates over the titles through [`Iter`], named after the method.
    pub fn iter(&self) -> Iter<'_> {
        Iter(self.tracks.iter())
    }
}

/// Iterator over a [`Playlist`]'s titles, returned by [`Playlist::iter`].
#[derive(Debug, Clone)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Iter<'a>(slice::Iter<'a, String>);

impl<'a> Iterator for Iter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(String::as_str)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(String::as_str)
    }
}

impl ExactSizeIterator for Iter<'_> {}

impl FusedIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a Playlist {
    type Item = &'a str;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
```

**Don't.**
```rust,ignore,expect=clippy::wrong_self_convention+scan:get-prefix
# pub struct Playlist { name: String, tracks: Vec<String> }
impl Playlist {
    pub fn get_name(&self) -> &str { &self.name }                    // `get_` prefix on a getter
    pub fn as_string(&self) -> String { self.tracks.join("\n") }     // `as_` that allocates: `to_m3u`
    pub fn to_inner(self) -> Vec<String> { self.tracks }             // consumes, so `into_tracks`
    pub fn iter(&self) -> std::slice::Iter<'_, String> { self.tracks.iter() } // pins the `Vec`
}
```

**Detect.** `clippy::wrong_self_convention` (receivers only), `iter_without_into_iter`, `into_iter_without_iter`, `iter_not_returning_iterator`, `should_implement_trait`. Scan `get-prefix` (advisory). Review: `as_` methods that allocate or validate, std iterator types in public signatures.

## Types that make illegal states unrepresentable

### API-02 · Newtypes and enums, not primitives and flags

**Why.** A `bool` plus two `Option`s has eight states when the domain has two; every consumer must re-derive which combinations are legal, and one forgets. An enum whose variants carry exactly their own data has only legal states, and `match` forces every consumer to handle each. A newtype stops a store id being passed where a user id was meant. A `type` alias does not: `type UserId = u64;` accepts every `u64`, so it documents without protecting. Aliases only abbreviate long types (`type Result<T> = std::result::Result<T, LoadError>;`). Never encode absence or failure in-band (`-1`, `0`, `""`, `u32::MAX`, `NaN`); that is what `Option`, `Result` and enums are for.

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
```rust,ignore,expect=scan:type-alias-primitive
pub type StoreId = u32;            // an alias: any u32 still fits

pub struct Order {
    pub is_delivery: bool,         // true + store = Some(..) is representable and meaningless
    pub store: Option<StoreId>,    // only set when !is_delivery: the comment is the enum trying to get out
    pub address: Option<String>,
    pub morning: Option<bool>,     // three states named by a bool
    pub quantity: i32,             // -1 means "not counted yet": an in-band sentinel
}
```

**Detect.** Review: `bool` fields next to `Option` fields, `Option<bool>`, raw `u32`/`String` ids in signatures, comments saying when a field is valid ("only set when ..", "must be 0 if .."), in-band sentinels, and `Option<Vec<T>>`/`Option<String>` where absent and empty mean the same. `clippy::struct_excessive_bools` (threshold 2 in `clippy.toml`), `option_option`. Scan `type-alias-primitive` (advisory).

### API-03 · Parse, don't validate

**Why.** A `validate(&str) -> bool` check leaves the value as `&str`, so every later function either re-checks or trusts blindly. Parsing converts once into a type whose private fields can only hold valid data; after that the invariant is a fact the compiler carries. Store the parsed structure (here: local and domain parts), not the raw string plus a promise.

- **Privacy is per module, not per type.** Any code in the same module can write `EmailAddress { .. }` and skip `TryFrom`. Give each invariant-carrying type its own module, holding only the type, its constructors and the methods that must see its fields.
- **No infallible `From<raw>`.** `impl From<String> for EmailAddress` is a back door around the parser. The ways in are `TryFrom`, `FromStr` and a fallible `new`.
- **Values known at compile time** use a const-generic constructor with an inline `const { assert!(..) }`: a bad literal fails the build, and there is no runtime panic path. Runtime input still goes through `TryFrom`.

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

```rust
//! A constructor checked at compile time (API-03).

/// A calendar month, 1 to 12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Month(u8);

/// The number is not a month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not a month in 1..=12")]
pub struct InvalidMonth(u8);

impl Month {
    /// Builds a month known at compile time: `Month::of::<13>()` fails the build.
    #[must_use]
    pub const fn of<const N: u8>() -> Self {
        const { assert!(N >= 1 && N <= 12, "month must be in 1..=12") };
        Self(N)
    }
}

impl TryFrom<u8> for Month {
    type Error = InvalidMonth;

    fn try_from(raw: u8) -> Result<Self, Self::Error> {
        if (1..=12).contains(&raw) { Ok(Self(raw)) } else { Err(InvalidMonth(raw)) }
    }
}

/// The start of the second quarter.
pub const APRIL: Month = Month::of::<4>();
```

**Don't.**
```rust,ignore
pub fn is_valid_email(s: &str) -> bool { s.contains('@') }
pub fn send(to: &str) { .. }        // did the caller check? callee cannot know, so it re-checks
pub struct Email(pub String);       // public field: `Email("nope".into())` bypasses parsing
impl From<String> for EmailAddress { .. }   // infallible: a back door around `TryFrom`
```

**Detect.** Review: `is_valid_*`/`validate_*` returning `bool` or `Result<(), _>`; the same check appearing in two places; newtypes with `pub` inner fields; `new` taking unchecked input without returning `Result`; `impl From<raw> for` a validated type; a validated type sharing its module with code that builds it directly.

### API-04 · Private fields unless plain data

**Why.** A public field is a promise that any value is valid and that the field will exist forever. Keep fields private whenever an invariant exists, and expose getters. Plain data with no invariant (a `Receipt`, a params struct, a DTO) may use `pub` fields.

**Detect.** Review every `pub` field: "can a caller put a value here that breaks this type?" If yes, it is a finding. (The restriction lint `clippy::exhaustive_structs` can list them, but it is not enabled in the template, and a member crate cannot add lints next to `[lints] workspace = true`. This check is review-only.)

### API-05 · `#[non_exhaustive]`, deliberately

**Why.** Adding a variant or field to an exhaustive public type is a semver break (API-17). `#[non_exhaustive]` reserves the right to grow, at the cost that downstream must write a `_` arm and cannot build the struct with a literal. Use it on errors, config, events, options, and types that mirror an external system that evolves on its own schedule. Leave closed domain sets (`Fulfilment` above) exhaustive so downstream `match` breaks loudly when you add a case, which is what you want for a domain concept.

- On an enum it permits new variants only. A struct-like variant that may gain fields (an error variant gaining context, ERR-03) needs the attribute on the variant as well: `#[non_exhaustive] TooLarge { len: u64, max: u64 }`. Downstream then matches it with `..` and cannot construct it.
- Decide at first publication. Adding the attribute to an existing type or variant is itself a break.
- Never emulate it with a private `_priv: ()` field.

**Don't.**
```rust,ignore,expect=clippy::manual_non_exhaustive
pub struct Limits {
    pub max_items: u32,
    pub max_bytes: u64,
    _priv: (),              // hand-rolled `#[non_exhaustive]`
}
```

**Detect.** `clippy::manual_non_exhaustive`. Review each new public `enum`/`struct` and each struct-like variant: is "may grow" or "closed" a stated decision? `clippy::exhaustive_enums` is not enabled.

## Trait implementation checklist

### API-06 · Implement standard traits eagerly

**Why.** Downstream crates cannot add std trait impls to your types (orphan rule). A missing `Debug` blocks `{:?}` and `#[derive(Debug)]` on every struct that contains your type; a missing `Hash` blocks use as a map key; a missing `FromIterator` makes callers write `from_vec` helpers instead of `collect()`.

| Trait | Implement when | Notes |
|---|---|---|
| `Debug` | Always, on every public type. | Hand-write it to redact secrets (OUT-08). `missing_debug_implementations` warns. |
| `Clone` | The type has value semantics. | Not for handles to unique resources (sockets, guards). |
| `Copy` | Small (≤ 2–3 words), plain value, will stay that way. | Removing `Copy` later is breaking (OWN-16). |
| `PartialEq`, `Eq` | Equality is meaningful. `Eq` unless a float is inside. | Needed by `assert_eq!` in downstream tests. |
| `Hash` | With `Eq`, when usable as a key. | Must agree with `Eq`. |
| `PartialOrd`, `Ord` | A natural total order exists. | Derived order compares fields and variants in declaration order, so reordering them changes sort output and `BTreeMap` iteration (OUT-04): pin it with a test or implement it by hand. No `PartialOrd` without `Ord` unless the order is genuinely partial. |
| `Default` | An obvious neutral value exists. | Not for types whose invariant needs input. |
| `Display` | User-facing: ids, errors, quantities. | Not "a nicer `Debug`" (OUT-10). |
| `From<T>` | Infallible, lossless, obvious. | Gives `Into` for free. Never implement `Into` directly, and never `From<raw>` for a validated type (API-03). |
| `TryFrom<T>` | Fallible conversion or validation. | The parse-don't-validate entry point (API-03). |
| `FromStr` | The type has a text form. | Enables `"..".parse::<T>()` and clap/serde integration. |
| `AsRef<T>` | Cheap borrow as `T`. | `AsRef<str>`, `AsRef<[T]>`, `AsRef<Path>`. |
| `Borrow<T>` | Only if `Eq`, `Hash` and `Ord` agree with `T`'s. | Enables `map.get(&str)` for `String`-like keys. |
| `IntoIterator` | Collection-like: for `&T`, `&mut T`, `T`. | Makes `for x in &c` work. `IntoIter` is a named type (API-01). |
| `FromIterator`, `Extend` | Collection-like, when any sequence of items keeps the invariant (a sorted container sorts on insert). | Not for invariants an arbitrary iterator can break (`NonEmpty`). `Leaderboard` (API-19) shows both. |
| `Iterator` (your own) | Every hand-written iterator. | Override `size_hint` whenever the remaining count is bounded, so `collect` and `extend` pre-size. Add `ExactSizeIterator`, `DoubleEndedIterator` and `FusedIterator` only when their contracts hold. |
| `LowerHex`, `UpperHex`, `Octal`, `Binary` | Bit-set and bitmask newtypes combined with `\|` and `&`. | Not for quantities (`Nanoseconds`). |
| `Error` | Error types. | Plus `Send + Sync + 'static` (ERR-01). |
| `Serialize`/`Deserialize` | Behind a `serde` feature in libraries (API-16). | Wire contracts: `structured-output.md`. |
| `Send`, `Sync` | Automatic. Keep them. | Assert at compile time, as the `EmailAddress` example above does. |
| `Deref` | Smart pointers, guards, a collection's own view. | Never for inheritance (API-19). |

**Detect.** `missing_debug_implementations`, `clippy::iter_without_into_iter`, `into_iter_without_iter`. Review: public type missing `Clone`/`PartialEq` with no stated reason; collection types without `FromIterator`/`Extend`; hand-written iterators that keep the default `size_hint`.

### API-07 · Conversions are traits, on the more specific type

**Why.** Conversions through traits compose with `?`, `.into()`, `.parse()` and generic bounds; a `from_x` method composes with nothing. Put each conversion on the type that carries the extra invariant, as std does (`str::from_utf8`, `String::into_bytes`): `impl TryFrom<&str> for EmailAddress`, never a `StrExt::to_email()` extension method or a free `parse_email(&str)`, which pile every domain's conversions onto general types. `TryFrom<&str>` suits a parser that builds new storage; a by-value `TryFrom<String>` that keeps the buffer hands it back inside its error on failure (ERR-14).

**Do.** The `EmailAddress` example under API-03: `TryFrom<&str>` and `FromStr` on the specific type, `Display` for the way back.

**Don't.**
```rust,ignore,expect=clippy::from_over_into
# pub struct EmailAddress { local: String, domain: String }
# pub struct EmailError;
pub trait StrExt { fn to_email(&self) -> Result<EmailAddress, EmailError>; } // conversion hung on `str`
pub fn email_from_str(raw: &str) -> Option<EmailAddress> { None }           // ad hoc, and `None` loses why
impl Into<String> for EmailAddress {                                          // implement `From`, get `Into` free
    fn into(self) -> String { format!("{}@{}", self.local, self.domain) }
}
```

**Detect.** `clippy::should_implement_trait` (a method named `from_str`, `add`, `next`, ...), `from_over_into`, `infallible_try_from`. Review: `to_<domain type>` methods on extension traits for `str`/`String`, free `parse_x` or `x_from_str` functions beside a type with no `TryFrom`/`FromStr`.

## Operators

### API-22 · Operators keep their algebraic meaning and never overflow

**Why.** A reader assumes `a + b` is cheap, pure and behaves like addition. `Add` for "append" or "merge", or a `+` that allocates, does I/O or runs in O(n), breaks that assumption. Operators cannot return `Result`, so `Self(self.0 + rhs.0)` on a bounded integer panics in debug builds and wraps silently in release (H-03, ERR-09).

- Implement operators only for algebraic types, as a consistent set: `Add`, `Sub`, `Neg`, the `*Assign` forms and the `&T` forms agree (`a - b == a + -b`).
- A type whose arithmetic can overflow or be invalid (money, quantities) exposes `checked_*` returning `Option` or `Result`. It implements the operator only when wrapping or saturation is its documented meaning.

**Do.**
```rust
//! Checked arithmetic instead of an overflowing operator (API-22).

/// An amount of money in cents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Cents(u64);

impl Cents {
    /// Wraps a raw cent count.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Adds two amounts, or `None` on overflow. No `impl Add`: `+` could not report it.
    #[must_use]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(sum) => Some(Self(sum)),
            None => None,
        }
    }

    /// Subtracts `rhs`, or `None` if it is larger than `self`.
    #[must_use]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(rest) => Some(Self(rest)),
            None => None,
        }
    }
}
```

**Don't.**
```rust,ignore,expect=scan:ops-impl
# #[derive(Clone, Copy)] pub struct Cents(u64);
impl std::ops::Add for Cents {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)       // panics in debug builds, wraps in release
    }
}
```

**Detect.** `clippy::suspicious_arithmetic_impl` and `suspicious_op_assign_impl` catch only a mismatched operator. Scan `ops-impl` (advisory). Review every `std::ops` impl: algebraic meaning, a consistent set, O(1), and no overflow path. `clippy::arithmetic_side_effects` would catch the overflow but is not enabled (an H-05 decision, ERR-09).

## Generic inputs, concrete outputs

### API-08 · Accept broadly, return precisely

**Why.** `impl AsRef<Path>` accepts `&str`, `String`, `&Path` and `PathBuf` without the caller converting. Returning a concrete type (or `impl Iterator` when the type is unnameable) keeps callers free to use every method and avoids allocating a `Vec` they only iterate (OWN-13).

- **Sans-IO.** Parsers, decoders and encoders take `impl BufRead`, `impl Read` or `impl Write` (or `&str`/`&[u8]`), never a `&Path` or a `File`. Opening the file is the caller's job, so tests feed bytes and never touch the filesystem (TST-10). A thin path convenience may wrap the core. Generic readers and writers are taken by value (OWN-01).
- **Ranges.** A parameter that denotes a range is `impl RangeBounds<T>`, so callers write `3..`, `..=9` or `2..5`. Never `(lo, hi)` or two scalars.
- **Callbacks.** Bound by the weakest trait the body needs: `FnOnce` if called at most once, `FnMut` if called repeatedly from one place, `Fn` only when shared or called through `&`. Accept `impl FnMut(..)`, not a `fn` pointer, which rejects capturing closures. Over-bounding forces callers into clones or `Arc` (H-08).

**Do.**
```rust
//! Generic inputs, concrete outputs (API-08).

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::ops::RangeBounds;
use std::path::Path;

/// Writes one `name<TAB>total` line per entry. Sans-IO: tests pass `&mut Vec<u8>`.
pub fn write_totals<W: Write>(mut out: W, totals: &[(&str, u64)]) -> io::Result<()> {
    for (name, total) in totals {
        writeln!(out, "{name}\t{total}")?;
    }
    out.flush()
}

/// Creates the file at any path-like value and writes the totals into it.
pub fn save_totals(path: impl AsRef<Path>, totals: &[(&str, u64)]) -> io::Result<()> {
    write_totals(BufWriter::new(File::create(path)?), totals)
}

/// Returns the non-blank lines of `text` without allocating a `Vec`.
pub fn non_blank(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| !line.trim().is_empty())
}

/// Calls `visit` on each non-blank line. `FnMut`: the closure may count or collect.
pub fn each_non_blank(text: &str, visit: impl FnMut(&str)) {
    non_blank(text).for_each(visit);
}

/// Returns the sequence numbers inside `range`: `3..`, `..=9` and `2..5` all work.
pub fn seqs_in(seqs: &[u64], range: impl RangeBounds<u64>) -> impl Iterator<Item = u64> {
    seqs.iter().copied().filter(move |seq| range.contains(seq))
}
```

**The turbofish caveat.** Argument-position `impl Trait` is an anonymous generic parameter, and callers can never name it with turbofish. Switching a published `fn load<P: AsRef<Path>>(p: P)` to `fn load(p: impl AsRef<Path>)` therefore breaks any caller that wrote `load::<&str>(..)`, and the reverse change adds a parameter to their turbofish. When a function mixes both forms, callers can turbofish only the named parameters. Pick one form at publication and keep it; use a named parameter when callers may need to specify the type (the type appears only in the return, or inference is ambiguous).

**Detect.** Review: `&PathBuf`/`&String` parameters (OWN-03), `-> Vec<T>` from a function whose callers only iterate, `Box<dyn Iterator>` returns where `impl Iterator` works, parsers that take a `&Path` or `File`, `(lo, hi)` range parameters, `Fn` bounds on callbacks called once, `fn(..)` pointer parameters for callbacks.

### API-21 · Bounds on impls, not on type definitions

**Why.** A bound on `struct Lookup<S: Store>` must be repeated on every `impl`, every function that names `Lookup<S>` and every type that contains one, and every test fake (TST-09) must satisfy it even where nothing uses it. `#[derive]` already scopes each derived impl to the parameters that need it. Put bounds on the `impl` blocks and functions that use them. Adding a bound to a published definition later is a break (API-17).

**Do.**
```rust
//! Bounds on impls, not on the definition (API-21).

/// Port: loads records.
pub trait Store {
    /// Loads the record with `id`, if present.
    fn load(&self, id: u64) -> Option<String>;
}

/// A use case generic over its store. No bound here, so `Debug` and `Clone`
/// apply whenever `S` has them, and any `S` can be stored.
#[derive(Debug, Clone)]
pub struct Lookup<S> {
    store: S,
}

impl<S: Store> Lookup<S> {
    /// Wraps `store`.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Returns the record's length in bytes, if present.
    #[must_use]
    pub fn len_of(&self, id: u64) -> Option<usize> {
        self.store.load(id).map(|record| record.len())
    }
}
```

**Don't.**
```rust,ignore,expect=scan:struct-bounds
# pub trait Store {}
#[derive(Debug, Clone)]
pub struct Lookup<S: Store + Clone + std::fmt::Debug> {   // every user and every fake now needs all three
    store: S,
}
```

**Exceptions.**
- A field names an associated type: `struct Cursor<S: Store> { last: Option<S::Record> }`.
- `?Sized` relaxes rather than restricts: `struct Framed<W: ?Sized>`.
- A `Drop` impl needs the bound: `impl<W: Write> Drop for Journal<W>` requires `struct Journal<W: Write>`, or the compiler rejects it with E0367 (ERR-13).

**Detect.** No clippy lint exists. Scan `struct-bounds` (advisory). Review every generic `struct`/`enum` definition.

## Static vs dynamic dispatch

### API-09 · Generics by default, `dyn` on purpose

**Why.** Generics monomorphize: calls inline, no vtable, no allocation, but one copy of the code per type. `dyn Trait` is one copy with an indirect call, and is the only way to hold different concrete types in one collection or to choose an implementation at runtime.

| Situation | Choose | Why |
|---|---|---|
| One concrete type per call site, hot path | `fn f<T: Trait>(x: &T)` or `impl Trait` | Inlining, zero-cost (PERF-08). |
| Heterogeneous collection (`Vec` of mixed handlers) | `Box<dyn Trait>` / `&dyn Trait` | Generics cannot mix types. |
| Plug-ins or strategy chosen at runtime from config | `Box<dyn Trait>`; in public types, a named wrapper over `Arc<dyn Trait>` (API-23) | Type unknown at compile time. |
| Runtime choice among a few types, used only inside one function | On-stack `&mut dyn Trait` bound from an `if`/`match` | No `Box`, and the code after the choice is compiled once. Needs 1.79+ temporary lifetime extension. |
| Closed, known set of variants, including commands | `enum` + `match` | Exhaustive, no vtable, no allocation. A command is data one function interprets (ARC-13). |
| Open table of stateless handlers | `fn(&Args) -> Result<..>` pointers | `Copy`, `'static`, no allocation. `Box<dyn Fn>` only when a handler captures state. |
| Many instantiations bloating binary or compile time | `dyn`, or a generic shim over a non-generic inner fn | Measured code-size win. |
| A service type users name nests generics (`App<Retry<Cached<PgRepo>>>`) | Erase at the composition root behind a named wrapper (API-23) | Keep named types to one level of nesting and about two parameters. |
| `dyn` trait with some generic, `Self`-returning or `-> impl Trait` methods | Mark those methods `where Self: Sized` | They leave the vtable; the rest stays callable through `dyn`. Or split off a dyn-compatible subtrait. |
| Async trait used as `dyn` | Boxed future through a named alias (ASY-03) | Native `async fn` in traits is not dyn-compatible; `Self: Sized` only removes it from the vtable. |

**Dyn compatibility is a contract.** A public trait used as `dyn` (plug-ins, `Arc<dyn Port>`) says so in its docs and proves it with `const _: Option<&dyn Trait> = None;` beside the trait. Without the assertion, E0038 ("the trait `X` is not dyn compatible") appears only at a downstream `dyn` use, often in another crate, and losing dyn compatibility is a semver break (API-17).

**Do.**
```rust
//! Static versus dynamic dispatch (API-09).

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// A named counter. Used as `dyn Metric`, so it stays dyn-compatible.
pub trait Metric {
    /// Current value.
    fn value(&self) -> u64;

    /// Writes the value as one line. Generic, so `Self: Sized` keeps it out of the vtable.
    fn write_to<W: Write>(&self, mut out: W) -> io::Result<()>
    where
        Self: Sized,
    {
        writeln!(out, "{}", self.value())
    }
}

// Fails here (E0038), in this crate, if a method breaks dyn compatibility.
const _: Option<&dyn Metric> = None;

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

/// Writes `report` to `path`, or to stdout when there is none: on-stack `dyn`.
pub fn emit(path: Option<&Path>, report: &str) -> io::Result<()> {
    let out: &mut dyn Write = match path {
        Some(path) => &mut File::create(path)?,
        None => &mut io::stdout().lock(),
    };
    out.write_all(report.as_bytes())?;
    out.flush()
}
```

**Detect.** Review: `Box<dyn Trait>` for a single implementor, `dyn` in a hot loop, `Vec<Box<dyn Trait>>` where the implementor set is closed (use an enum), a public trait used as `dyn` without the `const _` assertion, a `Box<dyn Read>`/`Box<dyn Write>` that never leaves its function, `Vec<Box<dyn Fn..>>` whose closures capture nothing.

### API-23 · Handles, not smart pointers, in public signatures

**Why.** `pub fn state(&self) -> Arc<Mutex<State>>` publishes the locking strategy: every caller now locks, can deadlock, and breaks when you move to an actor or a `RwLock`. `Rc<RefCell<Config>>` in a parameter forces single-threaded shared mutation on every caller. Keep sharing inside (OWN-11) and expose `&T`, `&mut T` or `T`.

- A service many callers share is a handle: a cheap `Clone` over a private `Arc<Inner>`, with methods on the handle. That clone copies a pointer, which is the ownership reason H-08 asks for.
- A public type that holds a runtime-chosen implementation holds a named wrapper (`DynClock`) that implements the trait, never `Arc<dyn Clock>`. The core stays generic (`Stamper<C>`), and the composition root names `Stamper<DynClock>`.
- Exceptions: the pointer is the product (a container or cache type), or a benchmark justifies exposing it. `patterns.md` has the full handle pattern.

**Do.**
```rust
//! Handles and named `dyn` wrappers (API-23).

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug)]
struct Inner {
    endpoint: String,
}

/// A directory service handle. `Clone` copies one pointer, never the service.
#[derive(Debug, Clone)]
pub struct Directory {
    inner: Arc<Inner>,
}

impl Directory {
    /// Creates a handle for the service at `endpoint`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { inner: Arc::new(Inner { endpoint: endpoint.into() }) }
    }

    /// Returns the service endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.inner.endpoint
    }
}

/// Port: the current time.
pub trait Clock {
    /// Returns the current time.
    fn now(&self) -> SystemTime;
}

/// A clock chosen at runtime, behind a name instead of `Arc<dyn Clock>`.
#[derive(Clone)]
pub struct DynClock(Arc<dyn Clock + Send + Sync>);

impl DynClock {
    /// Wraps any clock.
    pub fn new(clock: impl Clock + Send + Sync + 'static) -> Self {
        Self(Arc::new(clock))
    }
}

impl Clock for DynClock {
    fn now(&self) -> SystemTime {
        self.0.now()
    }
}

impl fmt::Debug for DynClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DynClock(..)")
    }
}
```

**Don't.**
```rust,ignore,expect=scan:pub-smart-ptr
# use std::sync::{Arc, Mutex};
# pub struct State;
# pub struct Engine { state: Arc<Mutex<State>> }
impl Engine {
    pub fn state(&self) -> Arc<Mutex<State>> {   // callers now own your locking strategy
        Arc::clone(&self.state)
    }
}
```

**Detect.** Scan `pub-smart-ptr` (advisory). Review: `Arc`, `Rc`, `RefCell`, `Mutex` or `RwLock` in a `pub fn` signature or a `pub` field, and `Arc<dyn Trait>` as a type argument of a public type.

## Parameters: bools, params structs, builders

### API-10 · At most one bool parameter; params struct or builder past four

**Why.** `fetch(url, true, false)` is unreadable at the call site, and swapping the two bools still compiles. An enum names the choice. A single bool that reads naturally (`set_visible(true)`) is fine. Two or more bools, any `Option<bool>`, more than four parameters, or optional settings become enums plus a params struct with `Default` (plain data) or a builder (private fields, invariants, or many optional parts). clippy enforces the outer bounds: `max-fn-params-bools = 1` and `too-many-arguments-threshold = 4`.

**Builder contract** (`patterns.md` follows it):
- The builder is `FooBuilder`, created only by `Foo::builder(required..)`, which takes every required input so a missing one cannot compile. There is no public `FooBuilder::new`.
- Setters are named after the field (`timeout(..)`, never `set_timeout(..)`), infallible, and chainable.
- Cross-field validation happens once, in `build()`, which returns `Result` when anything can fail (API-03).

**Parameter order.** Parameters that recur across a crate keep one order. Call-specific inputs come first; ubiquitous context (`&Ctx`, a logger) and a closure come last. A function takes at most one closure. Newtypes (API-02) catch most swapped arguments, and a fixed order catches the rest.

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
```rust,ignore,expect=clippy::fn_params_excessive_bools+clippy::too_many_arguments
# pub struct Response;
pub fn fetch(url: &str, follow: bool, strict: bool) -> Response { Response }   // fetch(u, false, true)?
pub fn connect(host: &str, port: u16, tls: Option<bool>, retries: u32, timeout: u64) {}
```

**Detect.** `clippy::fn_params_excessive_bools` (threshold 1), `clippy::too_many_arguments` (threshold 4), `clippy::must_use_candidate`, `clippy::return_self_not_must_use`. Review: `set_` setters, a public `FooBuilder::new`, required values passed through setters and checked in `build`, recurring parameters in different orders.

## Typestate

### API-13 · Make misuse a compile error

**Why.** When an operation is only valid in some states (send after recipient, query after connect), encoding the state as a type parameter deletes the invalid method from the invalid type. The error moves from a runtime `Err` to a compile error, and there is no `Option` to unwrap later. The state type can carry the data that state owns (`Recipient(String)`). Prove the guarantee with a `trybuild` test; a `compile_fail` doctest documents it but does not check the error on stable (TST-08).

**Kinds fixed per code path.** The same tool fits a kind (protocol, backend, unit system) that every code path knows at compile time and that never changes at runtime. Make it a sealed type parameter (API-12): `struct Download<P>`, shared methods in `impl<P: Protocol> Download<P>`, kind-only methods in `impl Download<Nfs>`. No `Option` field that only one kind fills, and no runtime check. When the kind arrives at runtime, or kinds share a collection, use an enum and convert at that boundary.

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

**Detect.** Review: `Option` fields that are `Some` only after a certain call or only for one kind; methods that begin with "if not yet configured, return error" or are documented "returns `None` unless ...".

## Sealed traits

### API-12 · Seal traits downstream must not implement

**Why.** Once downstream crates can implement a public trait, adding any method (even a defaulted one can conflict) or changing a bound is a breaking change. Sealing keeps the trait usable in bounds and as `dyn` while reserving implementation to this crate. Say so in the trait's docs, so users learn it before they hit the compiler error.

**Do.**
```rust
//! Sealed traits (API-12).

mod private {
    /// The seal. It is public inside a private module, so downstream code can
    /// see it as a bound but can never name it to implement it.
    pub trait Sealed {}
}

/// A shape with an area.
///
/// This trait is sealed: it cannot be implemented outside this crate.
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

**Detect.** Review: new public trait with no stated position on downstream implementations; sealed trait whose docs do not say it is sealed.

## Trait shape

### API-18 · Small role traits; extension traits for foreign types

**Why.** A trait is a contract every implementor must fulfil; a ten-method trait forces fakes (TST-09) to implement nine methods a test does not care about. One role per trait.

- Keep required methods to the minimal primitives, and add conveniences as provided methods written in terms of them, so implementors and fakes write only the primitives.
- Use an associated type when each implementor has one natural choice (`Iterator::Item`, `Handler::Reply` in API-20), and a type parameter when one type implements the trait many times (`From<T>`, `Handler<C>`).
- A single-method role may be accepted as `impl Fn(..)`. When the same closure bound repeats or needs a name, make it a named trait with a blanket impl for closures, so closures still qualify and a test can pass `|| fixed_time` as the fake.
- Extension traits add behaviour to types you do not own. They never carry conversions into your types (API-07).
- CONSIDER: a library type's essential operations are inherent methods, and its trait impls forward to them, so callers need no import. Ports (ARC-03) are exempt, because adapters are used through the port.
- Trait impls keep the trait's parameter names (`clippy::renamed_function_params`).

**Do.**
```rust
//! Small role traits, provided methods, closure impls, extension traits (API-18).

use std::time::{Duration, SystemTime, SystemTimeError};

/// The one capability a use case needs from time: a role, not a god trait.
pub trait Clock {
    /// Returns the current time. The only required method.
    fn now(&self) -> SystemTime;

    /// Returns the time elapsed since `earlier`. Provided, so fakes write only `now`.
    ///
    /// # Errors
    ///
    /// Returns [`SystemTimeError`] if `earlier` is later than [`Clock::now`].
    fn since(&self, earlier: SystemTime) -> Result<Duration, SystemTimeError> {
        self.now().duration_since(earlier)
    }
}

/// Any closure returning a time is a clock, so a test passes `|| fixed` as its fake.
impl<F: Fn() -> SystemTime> Clock for F {
    fn now(&self) -> SystemTime {
        self()
    }
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

#[cfg(test)]
mod tests {
    use super::Clock;
    use std::time::{Duration, SystemTime, SystemTimeError};

    #[test]
    fn closure_is_a_clock() -> Result<(), SystemTimeError> {
        let start = SystemTime::UNIX_EPOCH;
        let clock = || start + Duration::from_secs(5);
        assert_eq!(clock.since(start)?, Duration::from_secs(5));
        Ok(())
    }
}
```

**Detect.** `clippy::renamed_function_params`. Review: traits with more than about five required methods, traits named after a type (`UserServiceTrait`) rather than a role, `impl Trait` blocks full of `unimplemented!()` in fakes, the same `Fn*` bound spelled out on three or more items.

## Visibility and the facade

### API-14 · Minimal surface

**Why.** Every `pub` item is a semver commitment and a doc obligation. The less that is public, the more you can change without an H-16 decision.

- Private by default. `pub(crate)` for items shared across modules. `pub` only for the deliberate API, re-exported from `lib.rs`. A `pub` enum's variant payloads and a `pub` trait's methods are API too.
- `lib.rs` is the table of contents (ARC-05): module declarations and `pub use` re-exports, no logic. Users import `mycrate::Order`, not `mycrate::domain::order::model::Order`.
- **One public path per item.** A module whose items are re-exported is private (`mod order;`), never `pub mod order;` plus `pub use order::Order;`. When an item moves, remove the old path in the same change (an H-16 break for a published crate). In `cargo doc`, a crate-local item listed under "Re-exports" is the symptom.
- **Re-export by name.** `pub use module::*` publishes whatever later becomes `pub` in that module, and `clippy::wildcard_imports` does not flag it. The only allowed glob selects a `#[cfg]` platform module. No `prelude` module: needing one means the layout is wrong.
- `#[doc(hidden)]` only on items that must be `pub` for macro expansion or sealing internals; everything else is `pub(crate)`. An impl that mentions a private type needs no attribute, because rustdoc omits it.
- `unreachable_pub` warns on `pub` items that are not actually reachable; make them `pub(crate)`.
- Never widen visibility for a test (TST-02). Test through the public API, or put the unit test beside the item.
- Do not leak dependency types: a `reqwest::Error` or `sqlx::Row` in your public signature makes their major version part of your semver. Wrap it (an opaque error, a newtype) unless the dependency is deliberately part of the contract (a `serde` impl, a `bytes::Bytes` buffer); then re-export the crate (`pub use bytes;`) so users get the matching version. Errors leak the same way: a third-party error in a public variant, or a `#[from] dep::Error` on a public enum, makes that dependency public. In a published crate, keep third-party causes in a private repr (ERR-02's opaque shape).

**Do.**
````rust,fragment
// lib.rs: a table of contents. Module bodies live in their own files.
//! Order management: parse commands, decide, and emit events.
//!
//! ```
//! use orders::{Command, Order, decide};
//!
//! let command: Command = "place sku=AB12CD34 qty=2".parse()?;
//! let events = decide(&command, &Order::default())?;
//! assert_eq!(events.len(), 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Feature flags
//!
//! - `serde`: `Serialize` and `Deserialize` for every public type.

mod command;
mod decide;
mod event;
mod order;

pub use command::{Command, ParseCommandError};
pub use decide::{Rejection, decide};
pub use event::OrderEvent;
pub use order::{Order, OrderId};
````

**Detect.** `unreachable_pub`; scan `pub-glob-reexport` and `prelude-mod` (advisory); `cargo doc --open` and read the surface, including its "Re-exports" section; `cargo public-api` diff when available; review for `pub mod` exposing internals, two paths to one item, and third-party types in signatures and error variants.

## Documentation template

### API-15 · Document every public item

**Why.** Rustdoc is the API's user interface, and doctests are the only examples guaranteed to compile. A summary line is what search, module indexes and IDE hovers show.

- First line: one sentence of about 15 words on one line, saying what it does, in the third person ("Splits ...", "Returns ..."). `clippy::too_long_first_doc_paragraph` is a coarse backstop.
- Say what the signature cannot: behaviour, preconditions, units, invariants, surprises. Name parameters in prose ("Copies `src` to `dst`"). Never write `# Arguments`, `# Parameters` or `# Returns` sections, which restate the signature and go stale, and never describe specific callers.
- `# Examples` on primary entry points: a runnable doctest using `?` via a hidden `fn main() -> Result<..>` or a `# Ok::<(), E>(())` tail, never `unwrap` (TST-11). Show a realistic reason to call the item, not only its syntax; link to a shared example rather than duplicating one.
- `# Errors` on every fallible function: each variant and when it occurs (`clippy::missing_errors_doc`).
- `# Panics` should never be needed: production code does not panic (H-02, H-03). If you feel the need to write one, the design is wrong.
- Library crate roots open with `//!` docs: a one-sentence purpose, a quick-start doctest, and a `# Feature flags` list naming each public feature and marking internal or experimental ones (the API-14 example). Every `pub` module opens with `//!` saying what it holds.
- Link with intra-doc paths (``[`PairError::EmptyKey`]``), never hand-written `*.html` or `#method.` targets, which rustdoc cannot check (`rustdoc::broken_intra_doc_links`).

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

**Don't.**
```rust,ignore,expect=scan:doc-arg-sections
# #[derive(Debug)] pub struct PairError;
/// This function parses a pair. The config loader calls it for each line.
///
/// # Arguments
///
/// * `input` - the input string
///
/// # Returns
///
/// The key and the value. See [split](fn.split_once.html).
pub fn parse_pair(input: &str) -> Result<(&str, &str), PairError> {
    input.split_once('=').ok_or(PairError)
}
```

**Detect.** `missing_docs`, `clippy::missing_errors_doc`, `too_long_first_doc_paragraph`, `rustdoc::broken_intra_doc_links`; scan `doc-arg-sections` (advisory); `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, `cargo test --doc`.

## Features

### API-16 · Additive features, lean defaults

**Why.** Cargo unifies features across the dependency graph: if any crate enables `serde` on your crate, everyone gets it, and your crate cannot tell. A feature that removes an item, changes a type's layout or behaviour, or conflicts with another will break some unrelated crate in the graph.

```toml
[features]
default = ["std"]                   # lean: only what nearly every user needs
std = ["serde?/std"]                # turns on serde's `std` only if `serde` is enabled
serde = ["dep:serde"]               # adds impls, removes nothing
tls = ["dep:rustls", "std"]         # declares the feature it needs
[dependencies]
serde = { workspace = true, optional = true, features = ["derive"] }
rustls = { workspace = true, optional = true }
```

- Name a feature after the capability or optional dependency it enables (`std`, `serde`, `tokio`), never `use-x`, `with-x`, `x-support` or `no-x`. No mutually exclusive features; choose between backends at runtime or in the binary.
- Refer to optional dependencies only as `dep:name`, so they create no implicit feature and can be swapped later. Forward a feature of an optional dependency with `name?/feat`. A feature that needs another declares it.
- Gate impls and modules with `#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]` and `#[cfg(feature = "..")]`. Enabling a feature only adds. Never put `#[cfg(feature = ..)]` on a `pub` struct field (it breaks struct literals and patterns) or on a `pub` trait method that implementors define. An enum that gains variants under a feature is `#[non_exhaustive]`.
- Removing a feature, removing an entry from `default`, removing an optional dependency, or moving existing public items behind a feature is breaking (API-17). Retire a feature by keeping it as a no-op.
- Test `--no-default-features` and `--all-features` in CI, and `cargo hack --feature-powerset check` for full coverage.

**Detect.** `clippy::negative_feature_names`, `redundant_feature_names`; CI `cargo hack`. Review `[features]`: optional dependencies without `dep:`, any feature whose `cfg` removes or changes an item, `not(feature = ..)` guarding public API, `cfg(feature)` on `pub` fields or trait methods.

## Semver hazards

### API-17 · Published crates follow semver; breaks are H-16

**Why.** Downstream builds break silently on `cargo update` if a minor release contains a major change. Many breaks are invisible in the diff because they come from auto traits, inference, features or attributes.

Each of these needs a major version bump, and an H-16 decision before starting:

| Change | Note |
|---|---|
| Removing or renaming a public item, or changing a signature | Rename with `#[deprecated]` plus a `pub use` of the old name. |
| Adding any field, public or private, to a struct whose fields are all public, without `#[non_exhaustive]` | Breaks struct literals and exhaustive patterns. |
| Adding an enum variant, or a field to a variant, without `#[non_exhaustive]` | Breaks exhaustive `match` and variant patterns (API-05). |
| Adding `#[non_exhaustive]` to an existing type or variant | Decide it at first publication. |
| Reordering a public tuple struct's fields, or putting an existing item behind a `cfg` | Indexes shift; the item vanishes from some builds. |
| Adding a trait method without a default | Breaks every implementor. A new defaulted or inherent method can still collide with a downstream name: flag it, or seal the trait (API-12). |
| A public trait stops being dyn-compatible | The `const _` assertion (API-09) catches it in-crate. |
| Adding a blanket impl of a public trait (`impl<T: Bound> Trait for T`) | Conflicts with downstream impls. |
| Changing auto traits | A new field of type `Rc`, `Cell` or `*const` silently removes `Send`/`Sync`. Assert them at compile time (API-06). |
| Tightening bounds | Adding `T: Clone` to a generic function or impl. |
| Changing an `impl Trait` return | The hidden type's `Send`/`Sync`/`Unpin` leak to callers, so a new hidden type can remove them. Capturing more lifetimes (edition 2024 captures all in scope; narrow with `use<..>`, MOD-06) can break borrowing callers. |
| Generalizing a concrete parameter or return to a generic, or a named generic to `impl Trait` | Inference and turbofish failures (API-08). Possibly breaking: flag it. |
| Feature changes | Removing a feature, a `default` entry or an optional dependency (API-16). |
| Raising the MSRV, changing the license | By policy. |
| Incompatible behaviour behind an unchanged signature | Change the signature (a new type or name) so old callers fail to compile rather than misbehave. |

- **Check.** Run `cargo semver-checks --baseline-rev <last-release-tag>` before each release, and in CI for published crates. By its own account it misses changes to field or parameter types, generic and lifetime changes, and breaks that appear only under some feature sets; inference breaks need review too. Check those by hand.
- **Deprecate, then remove.** Mark the item `#[deprecated(since = "1.4.0", note = "use Ledger::open")]` in a minor release and delete it in the next major.
- **Record it.** Published crates keep `CHANGELOG.md` with an `Unreleased` section updated in the same change and breaking changes under a `Breaking` heading (PROC-11). Every release gets an annotated tag (`git tag -a v1.4.0`), which is the next check's `--baseline-rev`.

**Detect.** `cargo semver-checks`; review every change to a `pub` item in a published crate against the table.

## Deref is not inheritance

### API-19 · `Deref` is for pointers, guards and a collection's own view

**Why.** `impl Deref<Target = Vec<u32>> for Leaderboard` makes all of `Vec`'s API yours, and with `DerefMut`, `board.push(7)` and `sort_unstable_by` break the sorted invariant. Method resolution through `Deref` also confuses readers and makes the inner type part of your API. `Deref` is for smart pointers, guards, and an owning collection's own borrowed view, the way `Vec<T>` derefs to `[T]` and `String` to `str`. A collection with an invariant derefs to its view read-only, never `DerefMut`, and also implements `AsRef`, because generic bounds do not see through `Deref`. A pointer's or guard's own operations are associated functions (`Arc::get_mut(&mut a)`, `Guard::release(this)`), so they never shadow the target's methods.

**Do.**
```rust
//! `Deref` to a collection's own view (API-19); collection traits (API-06).

use std::cmp::Reverse;
use std::ops::Deref;

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
}

/// A read-only view of the scores. No `DerefMut`: a `&mut [u32]` could reorder them.
impl Deref for Leaderboard {
    type Target = [u32];

    fn deref(&self) -> &[u32] {
        &self.scores
    }
}

impl AsRef<[u32]> for Leaderboard {
    fn as_ref(&self) -> &[u32] {
        &self.scores
    }
}

/// Any sequence of scores keeps the order, so `extend` and `collect` are safe (API-06).
impl Extend<u32> for Leaderboard {
    fn extend<I: IntoIterator<Item = u32>>(&mut self, iter: I) {
        self.scores.extend(iter);
        self.scores.sort_unstable_by_key(|&score| Reverse(score));
    }
}

impl FromIterator<u32> for Leaderboard {
    fn from_iter<I: IntoIterator<Item = u32>>(iter: I) -> Self {
        let mut board = Self::default();
        board.extend(iter);
        board
    }
}
```

**Don't.**
```rust,ignore,expect=scan:deref-inherit
# use std::ops::{Deref, DerefMut};
# pub struct Leaderboard { scores: Vec<u32> }
impl Deref for Leaderboard { type Target = Vec<u32>; fn deref(&self) -> &Vec<u32> { &self.scores } }
impl DerefMut for Leaderboard {        // board.push(7) now bypasses the ordering invariant
    fn deref_mut(&mut self) -> &mut Vec<u32> { &mut self.scores }
}
```

**Detect.** Scan `deref-inherit` (advisory). Review every `impl Deref`/`DerefMut`: is the type a pointer, a guard (`Box`, `Arc`, `MutexGuard`, `Cow`), or an owning collection derefing to its own borrowed view, read-only if it keeps an invariant? If not, it is a finding. On a pointer or guard, inherent `self` methods are a finding: make them associated functions.

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

- [ ] Do `as_`/`to_`/`into_`, getters, `get`, `is_`/`has_`, `new`/`with_` names match their cost and ownership, and do `iter*` methods return named iterator types? (API-01)
- [ ] Can any public type hold an illegal combination (bool plus options, raw ids, `type` aliases, in-band sentinels)? (API-02)
- [ ] Is validation done once at construction, returning `Result`, in the type's own module, with no infallible `From<raw>`? (API-03)
- [ ] Is every `pub` field on a type without invariants? (API-04)
- [ ] Is `#[non_exhaustive]` a deliberate choice on each new public enum, struct and struct-like variant? (API-05)
- [ ] Do public types implement `Debug` and the other traits that make sense, collections `FromIterator`/`Extend`, and stay `Send + Sync`? (API-06)
- [ ] Are conversions `From`/`TryFrom`/`FromStr`/`AsRef` on the more specific type, rather than ad-hoc methods or extension traits? (API-07)
- [ ] Do inputs accept generic forms (readers and writers, not paths; `RangeBounds`; the weakest `Fn*`) and outputs return concrete types or `impl Trait`? (API-08)
- [ ] Is every `dyn` justified, and does each public trait used as `dyn` carry the `const _` assertion? (API-09)
- [ ] More than one `bool`, any `Option<bool>`, or more than four parameters? Do builders follow the contract? (API-10)
- [ ] Are builders, pure constructors and computed results `#[must_use]`? (API-11)
- [ ] Is every new public trait either sealed, and documented as sealed, or intentionally open? (API-12)
- [ ] Would a typestate or builder turn a likely misuse into a compile error, with a compile-fail test? (API-13)
- [ ] Is anything `pub` that could be private or `pub(crate)`? Does each item have one path, with no glob re-export or prelude? Does a dependency type leak? (API-14)
- [ ] Does every public item have a short summary, entry points a `?`-using example, fallible functions `# Errors`, and no `# Arguments`? Does the crate root have a quick start and feature list? (API-15)
- [ ] Are features additive, positively named, `dep:`-based, and default features lean? (API-16)
- [ ] Is any change on the semver hazard table, and did `cargo semver-checks` run? Is it an H-16 decision? (API-17)
- [ ] Are traits small role interfaces with minimal required methods, with extension traits for foreign types? (API-18)
- [ ] Is `Deref` implemented only on pointers, guards and collections derefing to their own view? (API-19)
- [ ] Would a user-facing trait bound benefit from `#[diagnostic::on_unimplemented]`? (API-20)
- [ ] Are generic `struct` and `enum` definitions free of trait bounds, outside the three exceptions? (API-21)
- [ ] Does every operator impl keep its algebraic meaning, with `checked_*` where arithmetic can overflow? (API-22)
- [ ] Do public signatures avoid `Arc`, `Rc`, `RefCell`, `Mutex` and `RwLock`, using handles and named `dyn` wrappers? (API-23)
