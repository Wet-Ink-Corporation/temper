# Modern Rust

Edition 2024 on current stable (written against 1.98): what changed, which features to adopt, which legacy crates and idioms to replace, and what is still unstable. Read this when you set up a crate or toolchain, upgrade an edition, touch code that uses a pre-2021 idiom, or are unsure whether a std API or syntax is available at the crate's MSRV.

Rules covered: MOD-01, MOD-02, MOD-03, MOD-04, MOD-05, MOD-06, MOD-07, MOD-08, MOD-09, MOD-10, MOD-11, MOD-12, MOD-13

## Toolchain and MSRV

### MOD-01 · Edition 2024, pinned toolchain, declared `rust-version`
**Why.** The edition changes language semantics (capture rules, drop order, `unsafe` requirements), so a mixed-edition workspace behaves differently crate by crate. A pinned toolchain makes local runs, hooks and CI see the same compiler and the same clippy lints; `rust-version` makes the MSRV a checked contract instead of a guess.
**Do.**
```toml
# rust-toolchain.toml (repository root)
[toolchain]
channel = "1.98.1"
components = ["rustfmt", "clippy"]
profile = "minimal"
```
```toml
# Cargo.toml (virtual manifest)
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
rust-version = "1.96"   # oldest compiler you support; raise it deliberately
```
**Don't.**
```toml
# channel = "stable"  -- floats; a new clippy lint turns the gate red overnight
[package]
edition = "2021"       # one crate on an old edition in a 2024 workspace
# no rust-version      -- MSRV is whatever the author's machine had
```
**Detect.** Review `Cargo.toml` and `rust-toolchain.toml` for every new crate; `cargo +<msrv> check` in CI when you promise an MSRV.

**How the pieces interact.**

| Setting | Effect | Gotcha |
|---|---|---|
| `rust-toolchain.toml` `channel` | rustup picks this compiler for every `cargo` call in the tree | Pin an exact version (`"1.98.1"`), not `"stable"`. Bumping it is a reviewed change. |
| `components` | installs `rustfmt`, `clippy` (add `rust-src`, `llvm-tools` only if needed) | Missing `clippy` means the gate cannot run. |
| `rust-version` | cargo refuses to build on older compilers; clippy's `incompatible_msrv` (deny under the template) rejects std APIs newer than it | Edition 2024 requires `rust-version >= 1.85`. With `rust-version = "1.93"`, using `array_windows` (1.94) fails clippy, so either raise the MSRV or use an older API. The template sets 1.96 so every API these references recommend is allowed. |
| `resolver = "3"` | MSRV-aware dependency resolution (1.84+): `cargo update` prefers dependency versions whose `rust-version` your MSRV satisfies | Edition 2024 packages default to resolver 3, but a virtual manifest has no edition: set it explicitly. |
| `build.warnings` (Cargo 1.97+) | Cargo-level warning policy (`warn`, `allow`, `deny`) | See MOD-13. |

Language features (let chains, if-let guards) are gated by the compiler version, not by clippy: they need `rust-version` at least as high as their stabilization, or users on the MSRV get a hard error.

### MOD-13 · `build.warnings = "deny"` in CI, not `RUSTFLAGS=-Dwarnings`
**Why.** `RUSTFLAGS` is part of every crate's build fingerprint, so switching it between local and CI runs rebuilds the whole dependency graph and defeats caches. `build.warnings` (Cargo 1.97+) is applied by Cargo after compilation, fails the command on any warning (including clippy's), and leaves fingerprints alone.
**Do.**
```toml
# .cargo/config.toml used by CI, or: CARGO_BUILD_WARNINGS=deny cargo clippy ...
[build]
warnings = "deny"
```
**Don't.**
```bash
RUSTFLAGS="-Dwarnings" cargo clippy --workspace   # full rebuild whenever RUSTFLAGS differs
```
**Gotcha.** It needs the Cargo in the *toolchain* to be 1.97 or newer, whatever `rust-version` says. Older Cargo ignores both the config key and `CARGO_BUILD_WARNINGS` silently, so an MSRV job on 1.96 passes with warnings. The template's MSRV job therefore uses `RUSTFLAGS=-Dwarnings`, where the cache cost doesn't matter.

**Detect.** Grep CI config for `RUSTFLAGS=.*-D ?warnings`. Local gate runs keep `-- -D warnings` on the clippy command line (that flag reaches only workspace crates, not dependencies). Cargo prints `warnings are denied by build.warnings configuration` when it fires.

### MOD-09 · Lints live in `[workspace.lints]`, members inherit
**Why.** Crate-root `#![deny(...)]` attributes drift per crate and are invisible to the protected-config hook. One table in the root manifest is the single policy, and changing it is an H-05 matter.
**Do.**
```toml
# root Cargo.toml
[workspace.lints.rust]
unsafe_code = "forbid"
[workspace.lints.clippy]
unwrap_used = "deny"

# crates/foo/Cargo.toml
[lints]
workspace = true
```
**Don't.**
```rust,ignore
// lib.rs
#![deny(clippy::unwrap_used)] // policy scattered across crates
#![allow(clippy::pedantic)]   // H-05 violation hidden in a crate root
```
**Detect.** Every member manifest has `[lints] workspace = true`; `rg '^#!\[(allow|deny|warn|forbid)' crates/` returns nothing.

## Edition 2024 changes that affect everyday code

Each claim below was compiled on 1.98.1.

| Change | What breaks or changes | What to do |
|---|---|---|
| RPIT captures all in-scope lifetimes (MOD-06) | `fn f<'a>(x: &'a T, y: &U) -> impl Iterator + ...` now borrows `y` too; callers can't drop `y`'s owner while the iterator lives (E0597) | Add `+ use<'a>` (1.82+) to list exactly what is captured. Remove old `Captures<'_>` / `+ '_` tricks. |
| `if let` scrutinee temporaries drop before `else` | A `MutexGuard` in `if let Some(v) = *m.lock()? { .. } else { m.lock() }` no longer deadlocks | Nothing, but don't rely on the 2021 behavior when porting. |
| Tail-expression temporaries drop before locals | `let c = RefCell::new(..); c.borrow().len()` compiles (was E0597) | Delete `let x = ...; x` workarounds. |
| `extern` blocks must be `unsafe extern`; `#[no_mangle]`, `#[export_name]`, `#[link_section]` must be `#[unsafe(..)]` | Hard errors | All banned by H-01 (`unsafe_code = "forbid"`). FFI needs an exception request (H-05) and a HITL decision (H-16). |
| `std::env::set_var` / `remove_var` are `unsafe` | E0133 without `unsafe` | Banned by H-01. Parse env once at startup into config (ARC-11). For children use `Command::env`. In tests, inject values instead of mutating the environment (TST-10). |
| Never-type fallback is `!`, not `()` | `f()?;` with `fn f<T: Default>() -> Result<T, E>` fails: ``!: Default`` not satisfied (1.92+ also denies the pattern on 2021) | Name the type: `f::<()>()?;`. |
| `gen` is a reserved keyword | `let gen = ..` and methods named `gen` fail to parse | Rename (rand 0.9 renamed `Rng::gen` to `random`), or write `r#gen` for a foreign API. |
| `Box<[T]>: IntoIterator` yields `T` | `boxed.into_iter()` moves items out (was `&T`) | Use `.iter()` when you meant to borrow. |
| `rustfmt` style edition 2024 | Formatting follows the crate edition: version-sorted imports and other layout changes | Commit the reformat separately from logic changes. Never pin an older `style_edition` to avoid the diff (H-05). |
| Resolver 3 is the default for edition 2024 packages | MSRV-aware resolution | See the toolchain table. |

### MOD-06 · Know the 2024 capture rules; narrow with `use<..>`
**Why.** In edition 2024 a return-position `impl Trait` captures every generic and lifetime in scope, so the hidden type is assumed to borrow every reference argument. That is the right default for most accessors but over-constrains callers when the result owns or only borrows one argument.
**Do.**
```rust
/// Names containing `needle`, case-insensitively.
///
/// `use<'a>` narrows the capture to `names`; `needle` may be dropped while
/// the iterator is alive.
pub fn matching<'a>(names: &'a [String], needle: &str) -> impl Iterator<Item = &'a str> + use<'a> {
    let needle = needle.to_lowercase();
    names
        .iter()
        .map(String::as_str)
        .filter(move |name| name.to_lowercase().contains(&needle))
}
```
**Don't.**
```rust,ignore
// No `use<'a>`: the result also captures `needle`'s lifetime.
pub fn matching<'a>(names: &'a [String], needle: &str) -> impl Iterator<Item = &'a str> {..}
let hits = { let q = String::from("AL"); matching(&names, &q) }; // E0597: `q` does not live long enough
```
**Detect.** E0597/E0716 at a call site of an `impl Trait`-returning fn; review any `use<..>` that lists more than it needs, and leftover `+ '_` or `Captures` helpers from 2021.

### Temporary scope, in code
The tail-expression case is `let buf = RefCell::new(s); buf.borrow().len()`, which now compiles. The `if let` case:
```rust
/// Returns the cached value, filling it on a miss.
///
/// Edition 2024 drops the scrutinee's guard before `else`, so the second
/// `lock()` does not deadlock.
///
/// # Errors
/// [`Poisoned`] if a previous holder panicked.
pub fn cached_or_fill(
    cache: &Mutex<Option<u64>>,
    fill: impl FnOnce() -> u64,
) -> Result<u64, Poisoned> {
    if let Some(v) = *cache.lock()? {
        Ok(v)
    } else {
        let v = fill();
        *cache.lock()? = Some(v);
        Ok(v)
    }
}
```
`Poisoned` is a unit error with `impl<T> From<PoisonError<T>> for Poisoned`, so `?` works on `lock()`.

## Everyday idioms

### MOD-02 · let-else, let chains, if-let guards
**Why.** Each removes a level of nesting or a duplicated match. `let ... else` keeps the happy path unindented and forces the else branch to diverge; let chains (1.88, edition 2024) replace pyramids of `if let`; if-let guards (1.95) bind inside a match guard instead of nesting a second `match`.
**Do.**
```rust
/// Parses `key = value`.
///
/// # Errors
/// [`PairError::MissingEquals`] when the line has no `=`.
pub fn parse_pair(line_no: usize, line: &str) -> Result<Pair<'_>, PairError> {
    let Some((key, value)) = line.split_once('=') else {
        return Err(PairError::MissingEquals { line: line_no });
    };
    Ok(Pair {
        key: key.trim(),
        value: value.trim(),
    })
}

/// Returns the configured port when `enabled` is `"true"` and `port` parses.
#[must_use]
pub fn enabled_port(config: &BTreeMap<String, String>) -> Option<u16> {
    if let Some(flag) = config.get("enabled")
        && flag == "true"
        && let Some(raw) = config.get("port")
        && let Ok(port) = raw.parse::<u16>()
    {
        return Some(port);
    }
    None
}

/// Classifies a raw argument.
#[must_use]
pub fn classify(raw: &str) -> Token<'_> {
    match raw {
        s if let Some(name) = s.strip_prefix("--") => Token::Flag(name),
        s if let Ok(n) = s.parse::<i64>() => Token::Number(n),
        s => Token::Word(s),
    }
}
```
**Don't.**
```rust,ignore
let (key, value) = match line.split_once('=') {   // manual let-else
    Some(kv) => kv,
    None => return Err(PairError::MissingEquals { line: line_no }),
};
if let Some(flag) = config.get("enabled") {         // pyramid; use a let chain
    if flag == "true" {
        if let Some(raw) = config.get("port") { /* ... */ }
    }
}
```
**Detect.** `clippy::manual_let_else`, `clippy::collapsible_if`, `clippy::collapsible_match`; review for nested `match` inside a match arm.

### MOD-03 · `LazyLock` / `OnceLock`, not `lazy_static!` or `once_cell`
**Why.** std has had `OnceLock` (1.70) and `LazyLock` (1.80) for years; the crates add a dependency (H-16) and a macro for nothing. A global is still only acceptable when it is immutable after init (ARC-09).
**Do.**
```rust
static RESERVED: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["admin", "root", "system"]));

/// True when `name` is reserved.
#[must_use]
pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(name)
}

/// Text with a lazily computed, cached word count.
#[derive(Debug)]
pub struct Document {
    text: String,
    word_count: OnceLock<usize>,
}

impl Document {
    /// Word count, computed on first call.
    #[must_use]
    pub fn word_count(&self) -> usize {
        *self
            .word_count
            .get_or_init(|| self.text.split_whitespace().count())
    }
}
```
**Don't.**
```rust,ignore
lazy_static! { static ref RESERVED: HashSet<&'static str> = /* ... */; } // extra dep + macro
static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();               // global mutable state (ARC-09)
```
**Detect.** `clippy::disallowed_macros` (template bans `lazy_static::lazy_static`); `cargo tree -i once_cell` for direct uses. Use `LazyCell`/`OnceCell` for the single-threaded equivalents.

### MOD-04 · Native `async fn` in traits and async closures
**Why.** Since 1.75 traits can declare `async fn` / `-> impl Future`, which avoids `#[async_trait]`'s per-call `Box<dyn Future>` allocation and erased `Send`-ness. Async closures (1.85, `AsyncFn*` bounds) can borrow from their captures across `.await`, which `F: Fn() -> Fut` cannot express.
**Do.**
```rust
/// Port: loads blobs by key. Public, so the `Send` bound is spelled out.
pub trait Store {
    /// Loads the bytes stored under `key`.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when `key` is absent.
    fn load(&self, key: &str) -> impl Future<Output = Result<Vec<u8>, StoreError>> + Send;
}

/// Runs `op` up to `attempts` times and returns the first success or the last error.
///
/// # Errors
/// The error from the final attempt.
pub async fn retry<T, E>(
    attempts: NonZeroU32,
    mut op: impl AsyncFnMut() -> Result<T, E>,
) -> Result<T, E> {
    let mut outcome = op().await;
    for _ in 1..attempts.get() {
        if outcome.is_ok() {
            break;
        }
        outcome = op().await;
    }
    outcome
}
```
Implementors may write `async fn load(&self, key: &str) -> ...` in the `impl`. Callers can pass `async || { calls += 1; ... }`.
**Don't.**
```rust,ignore
#[async_trait::async_trait]                       // boxes every call
pub trait Store { async fn load(&self, key: &str) -> Result<Vec<u8>, StoreError>; }

pub async fn retry<F, Fut>(op: F) where F: FnMut() -> Fut, Fut: Future<Output = ..> // can't borrow captures
```
**Detect.** `rg 'async_trait|BoxFuture|Pin<Box<dyn Future'`. rustc's `async_fn_in_trait` lint warns on bare `async fn` in a `pub` trait because callers cannot add `Send`; use the `-> impl Future<..> + Send` form there (ASY-03).
**Exceptions.** Native async-fn traits are not dyn-compatible. If you need `dyn Store`, write an object-safe wrapper trait returning `Pin<Box<dyn Future<Output = _> + Send + '_>>` for that seam only, or keep `async-trait` behind a HITL decision (H-16).

### MOD-05 · `#[expect(lint, reason = "...")]`, never `#[allow]`
**Why.** `#[allow]` silences forever, even after the code no longer triggers the lint. `#[expect]` (1.81) fails with `unfulfilled_lint_expectations` once the lint stops firing, so exceptions expire, and `reason` records who approved it. Under H-05 every `#[expect]` is an exception request that a human approved.
**Do.**
```rust
/// Length of `name` in bytes. Signature frozen for 1.x callers.
#[expect(
    clippy::needless_pass_by_value,
    reason = "signature frozen until 2.0; exception approved in plan 0042 (H-05)"
)]
#[must_use]
pub fn legacy_len(name: String) -> usize {
    name.len()
}
```
**Don't.**
```rust,ignore
#[allow(clippy::needless_pass_by_value)] // denied: clippy::allow_attributes
#[expect(clippy::unwrap_used)]           // denied: no reason, and no approval
```
**Detect.** `clippy::allow_attributes`, `clippy::allow_attributes_without_reason`; the temper scanner flags every new `#[expect`. See `escape-hatches.md`.

### MOD-07 · Modern std APIs over hand-rolled equivalents
**Why.** The std versions are shorter, named for intent, and handle the edge cases (overflow in `div_ceil`, aliasing in the slice form of `get_disjoint_mut`) that hand-rolled code gets wrong. They are not all panic-free: `as_chunks` panics when `N == 0` (guard it with `const { assert!(N > 0) }`, MOD-10), and `HashMap::get_disjoint_mut` panics on overlapping keys. Check each API's version against `rust-version`; clippy's `incompatible_msrv` enforces it.
**Do.**
```rust
/// Removes expired sessions and returns them, in one pass.
pub fn take_expired(sessions: &mut Vec<Session>, now: u64) -> Vec<Session> {
    sessions.extract_if(.., |s| s.expires_at <= now).collect()
}

/// Pages needed to show `items` at `per_page` each.
#[must_use]
pub fn page_count(items: u64, per_page: NonZeroU64) -> u64 {
    items.div_ceil(per_page.get())
}

/// Moves `amount` between two distinct accounts.
///
/// # Errors
/// [`TransferError::Accounts`] for an out-of-range or repeated index;
/// [`TransferError::Insufficient`] when `from` cannot cover `amount`.
pub fn transfer(
    balances: &mut [u64],
    from: usize,
    to: usize,
    amount: u64,
) -> Result<(), TransferError> {
    let [src, dst] = balances.get_disjoint_mut([from, to])?;
    let remaining = src.checked_sub(amount).ok_or(TransferError::Insufficient)?;
    let credited = dst.checked_add(amount).ok_or(TransferError::Overflow)?;
    *src = remaining;
    *dst = credited;
    Ok(())
}
```
In tests, `use std::assert_matches;` (1.96, not in the prelude) and write `assert_matches!(transfer(&mut b, 0, 0, 1), Err(TransferError::Accounts(_)));`.
**Don't.**
```rust,ignore
let pages = (items + per_page - 1) / per_page;        // overflows near u64::MAX
opt.map_or(true, |max| n <= max)                      // is_none_or
let mut i = 0; while i < v.len() { if expired(&v[i]) { out.push(v.remove(i)); } else { i += 1; } }
let (a, b) = (&mut v[from], &mut v[to]);              // E0499, then someone reaches for unsafe
```
**Detect.** `clippy::manual_div_ceil`, `manual_is_multiple_of`, `unnecessary_map_or`, `manual_inspect`, `manual_repeat_n`, `manual_midpoint`, `manual_split_once`; review index loops with `remove`/`swap`.

### MOD-08 · `core::error::Error` and `fmt::from_fn`
**Why.** `core::error::Error` (1.81) lets a `no_std` library define real errors; `std::error::Error` is the same trait re-exported. `fmt::from_fn` (1.93) builds a one-off `Display` value from a closure, so you stream output without a wrapper struct or an intermediate `String` (PERF-04).
**Do.**
```rust
impl core::error::Error for Overflow {}

/// Displays `items` as `a, b, c` without building a `String`.
#[must_use]
pub fn comma_list<'a>(items: &'a [&'a str]) -> impl fmt::Display + use<'a> {
    fmt::from_fn(move |f| {
        let Some((first, rest)) = items.split_first() else {
            return Ok(());
        };
        f.write_str(first)?;
        rest.iter().try_for_each(|item| write!(f, ", {item}"))
    })
}
```
**Don't.**
```rust,ignore
pub fn comma_list(items: &[&str]) -> String { items.join(", ") } // allocates just to be written out
struct CommaList<'a>(&'a [&'a str]); impl Display for CommaList<'_> { .. } // boilerplate for one call site
```
**Detect.** Review `String`-returning helpers whose only caller writes them into a formatter.

### MOD-10 · Const generics, `const fn`, inline `const { }`
**Why.** Compile-time evaluation turns run-time checks into build errors and documents sizes in types. An inline `const { assert!(..) }` on a const generic fails the build for bad instantiations instead of panicking at run time (H-03 stays satisfied).
**Do.**
```rust
/// Bytes in `n` KiB, saturating at `usize::MAX`.
#[must_use]
pub const fn kib(n: usize) -> usize {
    n.saturating_mul(1024)
}

/// Default read buffer, computed at compile time.
pub const READ_BUF_BYTES: usize = kib(64);

/// Splits `data` into `N`-byte frames plus the trailing partial frame.
///
/// `N == 0` is rejected at compile time instead of panicking at run time.
pub fn frames<const N: usize>(data: &[u8]) -> (impl Iterator<Item = &[u8; N]>, &[u8]) {
    const { assert!(N > 0, "frame size must be non-zero") };
    let (whole, rest) = data.as_chunks::<N>();
    (whole.iter(), rest)
}
```
**Don't.**
```rust,ignore
pub fn frames(data: &[u8], n: usize) -> Vec<Vec<u8>> { // size unchecked until run time, allocates per frame
    data.chunks(n).map(<[u8]>::to_vec).collect()        // chunks(0) panics
}
```
**Detect.** Review: magic sizes passed as run-time arguments, `lazy`-initialized values that are compile-time computable. Don't over-apply: const generics that callers must spell everywhere are a cost.

### MOD-11 · Trait upcasting, not `as_any` hacks
**Why.** Since 1.86 `&dyn Sub` coerces to `&dyn Super` (and `Box`, `Arc` likewise). The old workaround (an `as_any()` or `as_super()` method on every impl, then `downcast_ref`) adds boilerplate and turns type-driven dispatch into run-time checks.
**Do.**
```rust
/// Anything with a display name.
pub trait Named {
    /// The display name.
    fn name(&self) -> &str;
}

/// A plugin is also [`Named`].
pub trait Plugin: Named {
    /// Transforms `input`.
    fn apply(&self, input: &str) -> String;
}

/// Formats a label for any named item.
#[must_use]
pub fn label(item: &dyn Named) -> String {
    format!("[{}]", item.name())
}

/// Labels every plugin; `&dyn Plugin` coerces to `&dyn Named`.
#[must_use]
pub fn labels(plugins: &[Box<dyn Plugin>]) -> Vec<String> {
    plugins.iter().map(|p| label(p.as_ref())).collect()
}
```
**Don't.**
```rust,ignore
trait Plugin: Named { fn as_named(&self) -> &dyn Named; fn as_any(&self) -> &dyn Any; }
if let Some(p) = plugin.as_any().downcast_ref::<Upper>() { .. } // control flow by concrete type
```
**Detect.** `rg 'fn as_any|downcast_ref|downcast::<'` in production code. A closed set of plugin kinds is an `enum` (API-09).

### MOD-12 · `cfg_select!` instead of `cfg-if`
**Why.** `cfg_select!` (1.95) is the std version of `cfg_if!`: first matching arm wins, `_` is the fallback, and it works in item and expression position (`cfg_select! { unix => { "unix" } _ => { "other" } }`). One fewer dependency.
**Do.**
```rust
cfg_select! {
    windows => {
        /// Separator between entries of `PATH`.
        pub const PATH_LIST_SEPARATOR: char = ';';
    }
    _ => {
        /// Separator between entries of `PATH`.
        pub const PATH_LIST_SEPARATOR: char = ':';
    }
}
```
**Don't.**
```rust,ignore
cfg_if::cfg_if! { if #[cfg(windows)] { .. } else { .. } } // extra dependency
#[cfg(windows)] const SEP: char = ';';
#[cfg(not(windows))] const SEP: char = ':';            // fine for two arms; error-prone for more
```
**Detect.** `cargo tree -i cfg-if --depth 1` for direct uses; a `cfg_select!` without `_` that fails to build on an untested target.

## Feature adoption table

Versions checked against releases.rs and clippy's `incompatible_msrv`. Each example compiled on 1.98.1. Raise `rust-version` before using anything newer than it.

| Feature | Stable since | Replaces | Minimal example |
|---|---|---|---|
| `let ... else` | 1.65 | `match` with early `return` | `let Some(v) = map.get(k) else { return Err(Missing); };` |
| Generic associated types | 1.65 | lifetime-on-trait workarounds | `type Item<'a> where Self: 'a;` |
| `OnceLock`, `OnceCell` | 1.70 | `once_cell::sync::OnceCell` | `*cache.get_or_init(\|\| 42)` |
| `Option::is_some_and` | 1.70 | `map_or(false, ..)` | `opt.is_some_and(\|v\| v > 0)` |
| unsigned `div_ceil` | 1.73 | `(n + k - 1) / k` | `n.div_ceil(k)` |
| `Result::inspect_err` | 1.76 | `map_err(\|e\| { side_effect; e })` | `.inspect_err(\|_\| *failures = failures.saturating_add(1))` |
| `async fn` / `-> impl Trait` in traits | 1.75 | `async-trait` | `fn load(&self, key: &str) -> impl Future<Output = ..> + Send;` |
| `slice::chunk_by` | 1.77 | manual run grouping | `xs.chunk_by(\|a, b\| a == b)` |
| `#[diagnostic::on_unimplemented]` | 1.78 | cryptic E0277 for your trait | `#[diagnostic::on_unimplemented(message = "..")] pub trait Handler {}` |
| inline `const { }` | 1.79 | helper `const` items | `[const { Vec::new() }; 8]` |
| `LazyLock`, `LazyCell` | 1.80 | `lazy_static!`, `once_cell::Lazy` | `static MIME: LazyLock<HashMap<&str, &str>> = LazyLock::new(\|\| ..);` |
| exclusive range patterns | 1.80 | `200..=299` | `200..300 => "ok",` |
| `Option::take_if` | 1.80 | `if cond { slot.take() }` | `slot.take_if(\|v\| *v > 10)` |
| `split_at_checked` | 1.80 | length check + `split_at` | `buf.split_at_checked(4)` |
| `#[expect(.., reason)]` | 1.81 | `#[allow]` | see MOD-05 |
| `core::error::Error` | 1.81 | std-only errors | `impl core::error::Error for Overflow {}` |
| `use<..>` precise capturing | 1.82 | `Captures` trick | `-> impl Iterator<Item = &'a str> + use<'a>` |
| `Option::is_none_or` | 1.82 | `map_or(true, ..)` | `limit.is_none_or(\|max\| n <= max)` |
| `iter::repeat_n` | 1.82 | `repeat(x).take(n)` | `iter::repeat_n('-', n)` |
| integer `isqrt` | 1.84 | float `sqrt` + cast | `n.isqrt()` |
| edition 2024, async closures | 1.85 | `F: Fn() -> Fut` | `op: impl AsyncFnMut() -> Result<T, E>` |
| trait upcasting | 1.86 | `as_any` / `as_super` | `let named: Box<dyn Named> = plugin;` |
| `get_disjoint_mut` | 1.86 | `split_at_mut` gymnastics, `unsafe` | `let [src, dst] = balances.get_disjoint_mut([from, to])?;` |
| `Vec::pop_if` | 1.86 | `last()` check + `pop()` | `stack.pop_if(\|(urgent, _)\| *urgent)` |
| `Vec::extract_if` | 1.87 | `retain` + second pass, `drain_filter` | `sessions.extract_if(.., \|s\| s.expires_at <= now)` |
| unsigned `is_multiple_of` | 1.87 | `n % k == 0` (panics on `k == 0`) | `n.is_multiple_of(4)` |
| let chains (edition 2024) | 1.88 | nested `if let` | `if let Some(x) = a && x > 1 { .. }` |
| `HashMap::extract_if` | 1.88 | collect keys, then remove | `m.extract_if(\|_, v\| *v == 0)` |
| `Cell::update` | 1.88 | `c.set(c.get() + 1)` | `hits.update(\|n\| n.saturating_add(1));` |
| `slice::as_chunks` | 1.88 | `chunks_exact` + `try_into` | `let (whole, rest) = data.as_chunks::<N>();` |
| `File::lock` | 1.89 | `fs2` / `fd-lock` crates | `file.lock()` |
| `Result::flatten` | 1.89 | `and_then(\|r\| r)` | `r.flatten()` |
| `Duration::from_mins` / `from_hours` | 1.91 | `from_secs(5 * 60)` | `Duration::from_mins(5)` |
| `RwLockWriteGuard::downgrade` | 1.92 | drop write, re-take read (racy) | `RwLockWriteGuard::downgrade(writer)` |
| `fmt::from_fn` | 1.93 | one-off `Display` structs | see MOD-08 |
| `VecDeque::pop_front_if` | 1.93 | `front()` check + `pop_front()` | `q.pop_front_if(\|v\| *v == 0)` |
| `slice::array_windows` | 1.94 | `windows(2)` + indexing | `xs.array_windows().all(\|[a, b]\| a <= b)` |
| `LazyLock::get` | 1.94 | tracking init separately | `LazyLock::get(&MIME).is_some()` |
| if-let guards | 1.95 | nested `match` in an arm | `s if let Ok(n) = s.parse::<i64>() => ..` |
| `cfg_select!` | 1.95 | `cfg-if` | see MOD-12 |
| `Vec::push_mut` | 1.95 | `push` then `last_mut().unwrap()` | `v.push_mut(String::new()).push_str("new");` |
| `assert_matches!` | 1.96 | `assert!(matches!(..))` | `use std::assert_matches;` then `assert_matches!(r, Err(E::Accounts(_)));` |
| Cargo `build.warnings` | 1.97 | `RUSTFLAGS=-Dwarnings` | see MOD-13 |

Skip for everyday code: C-string literals `c"..."` (1.77) and `&raw const` (1.82) serve FFI and raw pointers, which H-01 excludes. `strict_add` and friends (1.91) panic on overflow: not for production code (H-03, ERR-09); use `checked_*` or `saturating_*`. The `dead_code_pub_in_binary` lint (1.97) is allow-by-default and the workspace does not enable it; changing that is a lint-config change (H-05).

## Legacy to modern replacements

| Legacy | Modern | Notes |
|---|---|---|
| `lazy_static!`, `once_cell` | `std::sync::LazyLock` / `OnceLock`; `std::cell::LazyCell` / `OnceCell` | MOD-03. `lazy_static!` is a disallowed macro in the template. |
| `#[async_trait]` | native `async fn` / `-> impl Future + Send` in traits | MOD-04. Not dyn-compatible: for `dyn` seams box the future in one wrapper trait, or keep `async-trait` by HITL decision (H-16). |
| `cfg-if` | `cfg_select!` | MOD-12, 1.95+. |
| `failure`, `error-chain`, hand-written `impl Error` boilerplate | `thiserror` 2 (libraries), `anyhow` at the binary edge | ERR-01, ERR-06. `failure` and `error-chain` are unmaintained. |
| `structopt` | `clap` 4 with `#[derive(Parser)]` | structopt was merged into clap. |
| `try!(expr)` | `expr?` | `try` is a reserved keyword since 2018. |
| `mod.rs` | `foo.rs` + `foo/bar.rs` | ARC-06; `clippy::mod_module_files`. |
| `extern crate foo;`, `#[macro_use]` | nothing; `use foo::macro_name;` for macros | Only `extern crate alloc;` in `no_std` crates remains. |
| `#[allow(lint)]` | `#[expect(lint, reason = "...")]`, and only as an approved exception | MOD-05, H-05. |
| `F: Fn() -> Fut` / `Box<dyn Fn() -> Pin<Box<dyn Future>>>` | async closures, `AsyncFn` / `AsyncFnMut` / `AsyncFnOnce` bounds | MOD-04, ASY-12. |
| `as_any()` + `downcast_ref` to reach a supertrait | trait upcasting | MOD-11. |
| `mem::uninitialized`, `mem::zeroed` | banned (`unsafe`, H-01) | Use `Default`, `Option`, `[const { .. }; N]`, or `array::from_fn`. |
| `std::sync::mpsc` in async code | `tokio::sync::mpsc` (bounded), `oneshot`, `watch`, `broadcast` | `std` receivers block the executor (H-14, ASY-07). |
| `Vec::drain_filter` (nightly) | `Vec::extract_if` | 1.87. |
| `map_or(false, f)` / `map_or(true, f)` | `is_some_and(f)` / `is_none_or(f)` | `clippy::unnecessary_map_or`. |
| `std::env::set_var` in tests | inject config (and `Command::env` for children) | `unsafe` in 2024 (H-01), and shared global state (TST-10). |

## Not stable — do not use

Each of these fails on 1.98.1 without `#![feature]`, and nightly features are not allowed. If a design needs one, redesign.

| Feature | Error on 1.98.1 | Stable alternative |
|---|---|---|
| `try { .. }` blocks | E0658 `try` expression is experimental | extract a function returning `Result`/`Option` and use `?` |
| `gen { yield .. }` blocks / generators | E0658 gen blocks are experimental | `iter::from_fn`, `iter::successors`, a hand-written `Iterator` |
| specialization (`default fn`) | E0658 | separate traits, or an `enum` |
| `type Alias = impl Trait` (TAIT) | E0658 | name the concrete type, return `impl Trait` from a fn, or box |
| `impl Trait` in associated types | E0658 | a named type, or `Box<dyn ..>` |
| `impl Trait` in `let` bindings | E0562 | let inference work, or name the type |
| `!` as a general type (`fn f(x: !)`) | E0658 | `std::convert::Infallible` |
| return type notation (`T::method(..): Send`) | E0658 | declare `-> impl Future<Output = ..> + Send` in the trait (ASY-03) |
| async drop (`AsyncDrop`) | E0658 | explicit `async fn shutdown(self)` plus cancellation (ASY-05) |
| `const trait` / const trait impls | E0658 | `const fn` free functions |
| deref patterns (`match boxed { Some(x) => .. }` through `Box`) | E0308 | `match *boxed` or `match boxed.as_ref()` |
| `Iterator::array_chunks` | E0658 `iter_array_chunks` | `slice::as_chunks`, `chunks_exact` |
| signed `div_ceil` (`i32::div_ceil`) | E0658 `int_roundings` | unsigned `div_ceil`, or `checked_div` plus explicit rounding |

## Upgrading a crate to edition 2024

1. Start from a green gate on the current edition. Commit.
2. Make sure the toolchain in `rust-toolchain.toml` is 1.85 or newer, and `rust-version` is at least 1.85.
3. Run `cargo fix --edition --all-targets --all-features -p <crate>`. It applies the machine-fixable migration lints (`unsafe extern`, `#[unsafe(..)]` attributes, `r#gen`, `if let` rescope, `use<..>` bounds for `impl Trait` capture) while still on the old edition. Tail-expression drop-order changes are only warned about: read each warning.
4. Set `edition = "2024"` (or inherit `edition.workspace = true`). For a virtual workspace also set `resolver = "3"`.
5. Review the diff by hand. `cargo fix` is conservative: it adds `use<..>` bounds that preserve 2021 capture behavior and may rewrite `if let` into `match` to keep drop order. Remove each one that isn't needed; keep those that are. Any `unsafe` it introduced (`unsafe extern`, `unsafe { env::set_var }`) is an H-01 violation: redesign it away or raise an exception request (H-05, H-16).
6. Run `cargo fmt --all`. Style edition 2024 reorders imports; commit the reformat on its own.
7. Run the full gate (H-15): fmt, clippy with `-D warnings`, tests, docs. New clippy findings are real work, not config changes (H-12).
8. Do one crate per change in a workspace, dependencies first, so each step is reviewable.

## Review checklist
- [ ] Is every crate on edition 2024 with inherited `rust-version`, and is the toolchain pinned with `rustfmt` and `clippy`? (MOD-01)
- [ ] Are early exits `let ... else`, nested `if let` chains flattened, and matched-then-matched-again arms if-let guards? (MOD-02)
- [ ] Any `lazy_static!`, `once_cell`, or a global that is mutable after init? (MOD-03)
- [ ] Any `#[async_trait]`, `BoxFuture`, or `F: Fn() -> Fut` where native async traits or async closures fit? Public async traits spell out `+ Send`? (MOD-04)
- [ ] Any `#[allow]`? Is every `#[expect]` approved and does it carry a `reason`? (MOD-05)
- [ ] Does each `impl Trait` return capture only what it borrows (`use<..>` where callers need it)? (MOD-06)
- [ ] Hand-rolled `div_ceil`, `map_or(false/true, ..)`, index loops with `remove`, or two `&mut` into one slice? (MOD-07)
- [ ] One-off `Display` structs or `String`-returning formatters that should be `fmt::from_fn`? (MOD-08)
- [ ] Any crate-root lint attributes, or a member without `[lints] workspace = true`? (MOD-09)
- [ ] Run-time checks on sizes that could be `const { assert!() }` or a const generic? (MOD-10)
- [ ] Any `as_any`, `downcast_ref` or `as_super` shims? (MOD-11)
- [ ] Any new `cfg-if` use? (MOD-12)
- [ ] Does CI use `RUSTFLAGS=-Dwarnings` instead of `build.warnings`? (MOD-13)
- [ ] Does the diff use an API newer than `rust-version`, or anything from the "Not stable" list?
