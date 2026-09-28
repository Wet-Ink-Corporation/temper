# Error handling

How failures are typed, carried, reported, tested, and never dropped. Read this before designing an error type, writing `?` plumbing across a module or crate boundary, converting `Option` to `Result`, touching `main`, or removing an `unwrap`. Exceptions to the escape-hatch rules are in `escape-hatches.md`.

Rules covered: ERR-01, ERR-02, ERR-03, ERR-04, ERR-05, ERR-06, ERR-07, ERR-08, ERR-09, ERR-10, ERR-11, ERR-12, H-02, H-09, H-10

## Library error design

### ERR-01 · Typed errors in libraries · H-09 · No stringly or erased errors in public signatures

**Why.** A caller can only react to what it can match on. `Result<_, String>` offers a message to parse; `Box<dyn Error>` and `anyhow::Error` offer a downcast guess. A typed enum is a closed contract the compiler checks, and adding `Send + Sync + 'static` lets it cross threads and tasks and sit inside `anyhow` at the binary edge.

**Do.** One enum per operation, derived with `thiserror` 2:
```rust
//! Library error design (ERR-01..ERR-05).

use std::num::{NonZeroU16, ParseIntError};
use std::path::{Path, PathBuf};
use std::{fs, io};

/// Validated settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    workers: NonZeroU16,
}

impl Config {
    /// Returns the worker count.
    #[must_use]
    pub fn workers(&self) -> NonZeroU16 {
        self.workers
    }
}

/// A required setting is absent after parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ValidateError {
    /// No `workers` line was present.
    #[error("required setting `workers` is missing")]
    MissingWorkers,
}

/// Why [`load`] failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadConfigError {
    /// The file could not be read.
    #[error("failed to read {}", path.display())]
    Read {
        /// File that was being read.
        path: PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// A line is not `key = value`.
    #[error("{}:{line}: expected `key = value`", path.display())]
    Syntax {
        /// File containing the line.
        path: PathBuf,
        /// One-based line number.
        line: usize,
    },
    /// A key this version does not know.
    #[error("{}:{line}: unknown setting `{key}`", path.display())]
    UnknownKey {
        /// File containing the line.
        path: PathBuf,
        /// One-based line number.
        line: usize,
        /// The unrecognized key.
        key: String,
    },
    /// The `workers` value is not a positive integer.
    #[error("{}:{line}: `workers` must be a positive integer", path.display())]
    Workers {
        /// File containing the line.
        path: PathBuf,
        /// One-based line number.
        line: usize,
        /// Why the value did not parse.
        #[source]
        source: ParseIntError,
    },
    /// Cross-field validation failed; this layer adds nothing to its message.
    #[error(transparent)]
    Invalid(#[from] ValidateError),
}

/// Reads and validates a config file.
///
/// # Errors
///
/// Returns [`LoadConfigError::Read`] if the file cannot be read, and the
/// other variants if its contents are malformed or incomplete.
pub fn load(path: &Path) -> Result<Config, LoadConfigError> {
    let text = fs::read_to_string(path)
        .map_err(|source| LoadConfigError::Read { path: path.to_owned(), source })?;
    parse(path, &text)
}

fn parse(path: &Path, text: &str) -> Result<Config, LoadConfigError> {
    let mut workers = None;
    for (line, raw) in (1..).zip(text.lines()) {
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        let Some((key, value)) = raw.split_once('=') else {
            return Err(LoadConfigError::Syntax { path: path.to_owned(), line });
        };
        match key.trim() {
            "workers" => {
                let parsed = value.trim().parse().map_err(|source| LoadConfigError::Workers {
                    path: path.to_owned(),
                    line,
                    source,
                })?;
                workers = Some(parsed);
            }
            other => {
                return Err(LoadConfigError::UnknownKey {
                    path: path.to_owned(),
                    line,
                    key: other.to_owned(),
                });
            }
        }
    }
    let workers = workers.ok_or(ValidateError::MissingWorkers)?;
    Ok(Config { workers })
}

#[cfg(test)]
mod tests;
```

| Attribute | Use | Rule |
|---|---|---|
| `#[error("...")]` | The `Display` for this layer only. Fields interpolate: `{line}`, `{}` with an expression. | ERR-05 |
| `#[source]` | Marks the cause, exposed through `Error::source()`. A field named `source` is picked up automatically; write the attribute anyway for readers. | ERR-04 |
| `#[from]` | Adds `From<Cause>` so `?` converts. Implies `#[source]`. Only when exactly one step can produce that cause. | ERR-04 |
| `#[error(transparent)]` | Forwards `Display` and `source` to the inner error. For wrapping a layer that adds nothing, and for opaque wrappers. | ERR-02 |
| `#[non_exhaustive]` | Callers need a `_` arm, so adding a variant is not breaking. Default for public error enums. | API-05 |

**Don't.**
```rust,ignore
pub fn load(path: &Path) -> Result<Config, String>;                  // H-09: nothing to match on
pub fn load(path: &Path) -> Result<Config, Box<dyn std::error::Error>>; // H-09: erased, not Send
pub fn load(path: &Path) -> anyhow::Result<Config>;                  // H-09: anyhow in a library API
```

**Detect.** Scanner (H-09) on `pub fn` signatures in library crates. Review: `String`, `&'static str`, `Box<dyn Error>`, `anyhow::`, `eyre::` in any public `Result`. Compile-time check: `const _: () = { const fn f<T: Send + Sync + 'static>() {} f::<LoadConfigError>(); };`.

### ERR-02 · Scope errors to an operation; go opaque when variants are internal

**Why.** A crate-wide `Error` with 40 variants tells the caller of `load` that it might get `Error::PaymentDeclined`; every `match` needs arms that cannot happen. Scope enums to an operation or module, so the variants listed are the ones that occur. In a larger library, where callers need a category but the variants are implementation detail, expose an opaque struct with a `kind()` accessor: you can then add, split or re-source variants without a semver break.

**Do.**
```rust
//! Opaque error with `kind()` (ERR-02).

use std::collections::HashMap;

/// What went wrong, as far as callers can act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The key does not exist.
    NotFound,
    /// The caller's expected version is stale.
    Conflict,
}

/// Error from [`Store`]. Match on [`StoreError::kind`], not on internals.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct StoreError(Box<Repr>);

// Private: free to change without a semver break. Boxed to keep `Result` small.
#[derive(Debug, thiserror::Error)]
enum Repr {
    #[error("key `{key}` not found")]
    NotFound { key: String },
    #[error("version conflict on `{key}`: expected {expected}, found {actual}")]
    Conflict { key: String, expected: u64, actual: u64 },
}

impl StoreError {
    /// Returns the category callers branch on.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match *self.0 {
            Repr::NotFound { .. } => ErrorKind::NotFound,
            Repr::Conflict { .. } => ErrorKind::Conflict,
        }
    }

    /// Returns the key involved.
    #[must_use]
    pub fn key(&self) -> &str {
        match &*self.0 {
            Repr::NotFound { key } | Repr::Conflict { key, .. } => key,
        }
    }
}

/// A versioned in-memory key-value store.
#[derive(Debug, Default)]
pub struct Store {
    entries: HashMap<String, (u64, String)>,
}

impl Store {
    /// Replaces the value at `key` if its version is still `expected`.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotFound`] if `key` is absent, [`ErrorKind::Conflict`] if
    /// another writer bumped the version first.
    pub fn update(&mut self, key: &str, expected: u64, value: String) -> Result<u64, StoreError> {
        let Some((version, slot)) = self.entries.get_mut(key) else {
            return Err(StoreError(Box::new(Repr::NotFound { key: key.to_owned() })));
        };
        if *version != expected {
            let conflict = Repr::Conflict { key: key.to_owned(), expected, actual: *version };
            return Err(StoreError(Box::new(conflict)));
        }
        // 2^64 updates to one key cannot happen; wrapping keeps this total (ERR-09).
        *version = version.wrapping_add(1);
        *slot = value;
        Ok(*version)
    }
}
```

| Shape | When |
|---|---|
| Public enum per operation | Small crates. Callers act on specific variants with specific data. |
| Opaque struct + `kind()` | Many internal failure modes, few caller-relevant categories. Published crates that must evolve. (`std::io::Error` is this shape.) |
| Nested (`WithdrawError::Rejected(Rejection)`) | Layers: a use case wraps the domain's error and adds its own infrastructure cases (ERR-10). |

**Detect.** Review: one `Error` enum imported by every module; variants no function in the calling path can produce; `match` arms that are provably dead.

### ERR-03 · Context fields, not strings

**Why.** `Parse(String)` holding `format!("bad line {n} in {path}")` loses the data: callers cannot pull out the line, tests cannot assert it, and the allocation happens even when the caller discards the error. Typed fields (`path: PathBuf`, `line: usize`, `key: String`) are matchable, testable, and formatted only when displayed.

**Don't.**
```rust,ignore
#[error("{0}")]
Parse(String),                                                    // preformatted: nothing to match
Err(ConfigError::Parse(format!("line {line}: bad value {v}")))    // allocates, loses types
```

**Detect.** Review: variants whose only field is `String` named `msg`/`message`/`reason`; `format!` inside `Err(..)`.

### ERR-04 · Preserve causes; `#[from]` sparingly

**Why.** `#[source]` keeps the chain so the reporter can print "failed to read app.conf: permission denied". `#[from]` makes `?` convert silently, which is right only when one step can produce that cause type. With two `fs` calls and `#[from] io::Error`, the error cannot say which step failed and has no path. Use `map_err` to attach the step and context instead.

**Don't.**
```rust,ignore
#[error("io error")]
Io(#[from] std::io::Error),       // read? write? rename? which file?
let text = fs::read_to_string(path)?;
fs::write(backup, &text)?;        // both land in `Io`, indistinguishable
.map_err(|_| LoadError::Read)?    // cause discarded: `clippy::map_err_ignore`
```

**Detect.** `clippy::map_err_ignore`. It catches only `|_|`: a named binding such as `|_closed|` discards the cause just the same and slips past it. That form is allowed only with a comment saying why the cause has no diagnostic value, for example a `SendError` that only carries the unsent value, or a panic payload. Review: `#[from]` on `io::Error` or other general-purpose errors; any variant without a `source` that wraps a failure.

### ERR-05 · Message style

**Why.** The reporter joins the chain as `outer: cause: root`. A message that ends with a period, starts with a capital or repeats the cause produces "Failed to load.: Failed to read: No such file.: No such file".

- Lowercase first word (proper nouns and acronyms excepted), no trailing punctuation.
- Describe this layer's failure: "failed to read {path}", "loading configuration". Do not interpolate `{source}`: the chain already prints it.
- Include the identifying data (path, id, line), with user-supplied values in backticks.
- No `Error:` prefix; the reporter adds it.

**Detect.** Review `#[error(..)]` strings and `.context(..)` strings against the list.

## Reporting the cause chain

`Display` on an error prints one layer. A report walks `source()` to the root. Libraries rarely report (they return), but when one logs a handled error (ERR-08), it logs the whole chain:
```rust
//! Walking the cause chain (ERR-05).

use std::error::Error;
use std::fmt;

/// Iterates an error and each of its causes, outermost first.
pub fn chain<'a>(
    err: &'a (dyn Error + 'static),
) -> impl Iterator<Item = &'a (dyn Error + 'static)> {
    std::iter::successors(Some(err), |&current| current.source())
}

/// Displays `outer: cause: root cause` on one line.
#[derive(Debug, Clone, Copy)]
pub struct Report<'a>(pub &'a (dyn Error + 'static));

impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (depth, cause) in chain(self.0).enumerate() {
            if depth > 0 {
                f.write_str(": ")?;
            }
            write!(f, "{cause}")?;
        }
        Ok(())
    }
}
```

At the binary edge, `anyhow::Error` does this for you: `{}` prints the outermost message, `{:#}` prints the chain on one line (`a: b: c`), and `{:?}` prints the message plus a `Caused by:` list and a backtrace when `RUST_BACKTRACE=1`. In `tracing`, record the chain as a field (`error = %Report(&err)`), not only `%err`.

## The binary edge

### ERR-06 · `anyhow` at the edge, context at every boundary, exit codes from `main`

**Why.** A binary's errors end at a human or a supervisor, not a `match`, so type erasure is fine there and `.context()` gives each layer its sentence cheaply. The edge is also where failures become exit codes: classify once, after all handling is done. `main` returns `ExitCode` (or `anyhow::Result<()>` when exit status 1 is enough) and never calls `process::exit` (`clippy::exit`, ARC-14).

**Do.**
```rust
//! Binary edge: `anyhow`, context, exit codes (ERR-06).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use errs::config::{self, LoadConfigError};

// sysexits(3) codes: this tool is run by a supervisor that branches on them.
// Other binaries use the house convention in structured-output.md (OUT-06).
const EX_USAGE: u8 = 64;
const EX_NOINPUT: u8 = 66;
const EX_IOERR: u8 = 74;
const EX_CONFIG: u8 = 78;

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        return report("usage: errbin <config-path>", ExitCode::from(EX_USAGE));
    };
    match run(&path) {
        Ok(()) => ExitCode::SUCCESS,
        // `{:#}` prints the chain on one line; `{:?}` prints it as a list.
        Err(err) => report(format_args!("error: {err:#}"), exit_code(&err)),
    }
}

fn run(path: &Path) -> anyhow::Result<()> {
    let config = config::load(path).context("loading configuration")?;
    writeln!(io::stdout().lock(), "workers = {}", config.workers()).context("writing to stdout")?;
    Ok(())
}

fn report(message: impl std::fmt::Display, code: ExitCode) -> ExitCode {
    if writeln!(io::stderr().lock(), "{message}").is_err() {
        // stderr is closed: the exit code is the only channel left.
        return ExitCode::from(EX_IOERR);
    }
    code
}

/// Classifies the failure once, at the edge, after all handling is done.
fn exit_code(err: &anyhow::Error) -> ExitCode {
    let code = match err.downcast_ref::<LoadConfigError>() {
        Some(LoadConfigError::Read { .. }) => EX_NOINPUT,
        Some(_) => EX_CONFIG,
        None => 1,
    };
    ExitCode::from(code)
}
```

- `.context("static")` for fixed text; `.with_context(|| format!(..))` when formatting, so nothing allocates on success.
- Do not repeat what the inner error already says (here `LoadConfigError` already names the path).
- `downcast_ref` on a typed library error is the one sanctioned downcast: classification at the edge. Never downcast to recover inside the program (MOD-11); if code needs to branch on it, the error must stay typed.
- Print with `writeln!(io::stderr().lock(), ..)`; the print macros are denied (H-06).

**Detect.** Review: `anyhow` in a library crate's `[dependencies]`; `?` across a crate or I/O boundary with no `.context`; `process::exit`; `main` that prints and returns success.

## Option to Result

### ERR-07 · Convert `Option` explicitly

**Why.** `None` carries no reason. The caller must decide what absence means here and name it: `ok_or` (cheap, `Copy` error), `ok_or_else` (allocating or computed error), or `let ... else` when the branch needs more than one expression.

**Do.**
```rust
//! `Option` to `Result` (ERR-07).

use std::collections::HashMap;
use std::num::ParseIntError;

/// Why an email lookup failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LookupError {
    /// No user has this id.
    #[error("unknown user {id}")]
    UnknownUser {
        /// The id looked up.
        id: u64,
    },
    /// The user exists but has not set an email.
    #[error("user {id} has no email address")]
    NoEmail {
        /// The id looked up.
        id: u64,
    },
}

/// User id to optional email.
#[derive(Debug, Default)]
pub struct Directory {
    emails: HashMap<u64, Option<String>>,
}

impl Directory {
    /// Returns the email for `id`.
    ///
    /// # Errors
    ///
    /// [`LookupError::UnknownUser`] or [`LookupError::NoEmail`].
    pub fn email(&self, id: u64) -> Result<&str, LookupError> {
        let entry = self.emails.get(&id).ok_or(LookupError::UnknownUser { id })?;
        let Some(email) = entry.as_deref() else {
            return Err(LookupError::NoEmail { id });
        };
        Ok(email)
    }
}

/// Parses `"80, 443"`. The first bad entry fails the whole list.
///
/// # Errors
///
/// Returns the first [`ParseIntError`].
pub fn parse_ports(list: &str) -> Result<Vec<u16>, ParseIntError> {
    list.split(',').map(|port| port.trim().parse()).collect()
}
```

**Don't.**
```rust,ignore
let entry = self.emails.get(&id).unwrap();                     // H-02
let entry = self.emails.get(&id).ok_or(LookupError::Other(format!("{id}")))?; // allocates on success: ok_or_else
if entry.is_none() { return Err(..) } let email = entry.unwrap(); // check-then-unwrap: let-else
```

**Detect.** `clippy::unwrap_used`, `clippy::manual_let_else`, `clippy::unnecessary_unwrap`. `clippy::or_fun_call` is in the nursery group and not enabled by the template, so review `ok_or(format!(..))` by eye.

## Eliminating `unwrap` and `expect`

### H-02 · No `unwrap`/`expect`/`unwrap_unchecked`/`unwrap_err`/`expect_err` in production code

**Why.** `unwrap` is a panic with a worse message: it turns a recoverable condition into a process abort, and "it can never be `None`" is a claim the compiler cannot check and the next refactor can break. Every `unwrap` hides one of three situations: a real error to propagate, an invariant that belongs in a type, or a constant that can be built at compile time. Tests may `expect("why this holds")`.

| Situation | Replace with |
|---|---|
| `mutex.lock().unwrap()` | If the protected data cannot be torn mid-update: `.unwrap_or_else(PoisonError::into_inner)` with a comment. If it can: an error variant (`Poisoned`). Better still, avoid the shared lock (actor, ASY-09). |
| `Regex::new(LIT).unwrap()` in a static | `LazyLock<Result<Regex, regex::Error>>`, a typed error, and a unit test proving the pattern compiles. |
| `"127.0.0.1:8080".parse().unwrap()`, `"30".parse::<u64>().unwrap()` | Const construction: `Ipv4Addr::LOCALHOST`, `SocketAddr::new(..)`, `Duration::from_secs(30)`, integer literals. |
| `NonZeroU16::new(8).unwrap()` | `const` via `NonZeroU16::MIN.saturating_add(7)`, or take `NonZeroU16` from the caller. |
| `v.first().unwrap()`, `v[0]` | Slice pattern `let [first, rest @ ..] = v else { .. }`, `split_first()`, or a `NonEmpty` type. |
| `v.last().unwrap()` right after `push` | Keep the value you pushed, or `Vec::push_mut` (1.95), which returns `&mut T`. |
| `iter.max().unwrap()` on "known non-empty" data | A `NonEmpty` type, or `.ok_or(Error::Empty)?`. |
| `map.get(&k).unwrap()`, `map[&k]` | `.get(&k).ok_or(..)?`; after an insert, use the reference `entry(..).or_insert(..)` returns. |
| `tx.send(x).unwrap()` | An error that hands the value back (`PoolClosed(job)`), or handle "receiver gone" explicitly (H-10). |
| `.map(\|s\| s.parse().unwrap())` | `.map(str::parse).collect::<Result<Vec<_>, _>>()?`. |
| `path.to_str().unwrap()` | Stay in `&Path`/`&OsStr`; `path.display()` for messages; or `.to_str().ok_or_else(..)?`. |
| `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` | Inject a clock port (TST-10), and map `SystemTimeError` to a variant. |
| `u32::try_from(len).unwrap()` | `?` with a variant carrying the value, or change types so `From` applies (H-11). |
| `serde_json::to_string(&v).unwrap()`, `writeln!(..).unwrap()` | `?`. Serialization and writes can fail (non-string map keys, closed pipe). |
| `handle.join().unwrap()` | Map the panic payload to an error variant; prefer `thread::scope` for borrowed work (ASY-10). |
| `Option` field that is "always `Some` after `init()`" | Construct fully in `new`, or typestate (API-13). |

**Do.** Compile-time constants:
```rust
//! Literals are constants, not parsed strings (H-02).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroU16;
use std::time::Duration;

/// Default listen address: no `"127.0.0.1:8080".parse().unwrap()`.
pub const DEFAULT_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080);
/// Default request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Eight workers, built in a const context without `NonZeroU16::new(8).unwrap()`.
pub const DEFAULT_WORKERS: NonZeroU16 = NonZeroU16::MIN.saturating_add(7);
```
A static regex with fallible initialization:
```rust
//! A static regex without `unwrap` (H-02, MOD-03).

use std::sync::LazyLock;

use regex::Regex;

static RELEASE_TAG: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r"^v\d+\.\d+\.\d+$"));

/// The built-in release-tag pattern failed to compile.
#[derive(Debug, thiserror::Error)]
#[error("built-in release tag pattern is invalid")]
pub struct PatternError(#[source] regex::Error);

/// Returns `true` if `tag` looks like `v1.2.3`.
///
/// # Errors
///
/// Returns [`PatternError`] if the built-in pattern does not compile. The test
/// below proves it does, so a tested build never returns it.
pub fn is_release_tag(tag: &str) -> Result<bool, PatternError> {
    let pattern = RELEASE_TAG.as_ref().map_err(|err| PatternError(err.clone()))?;
    Ok(pattern.is_match(tag))
}

#[cfg(test)]
mod tests {
    use super::{PatternError, is_release_tag};

    #[test]
    fn release_tag_pattern_compiles_and_matches() -> Result<(), PatternError> {
        assert!(is_release_tag("v1.2.3")?);
        assert!(!is_release_tag("1.2")?);
        Ok(())
    }
}
```
Mutex poisoning, where the data cannot be torn:
```rust
//! Mutex poisoning without `unwrap` (H-02).

use std::sync::{Mutex, PoisonError};

/// An append-only audit log shared between threads.
#[derive(Debug, Default)]
pub struct AuditLog {
    // Shared and mutable: request threads append, a flusher drains (OWN-11).
    entries: Mutex<Vec<String>>,
}

impl AuditLog {
    /// Appends an entry.
    pub fn record(&self, entry: String) {
        // Poisoned means another thread panicked while holding the lock. `push`
        // completes or does not happen, so the Vec is never torn: recover it.
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).push(entry);
    }
}
```
Non-empty data by pattern and by type:
```rust
//! `first`/`last` without `unwrap` (H-02, H-04).

/// `argv` had no program name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("argument vector is empty")]
pub struct EmptyArgv;

/// Splits `argv` into program name and arguments with a slice pattern.
///
/// # Errors
///
/// Returns [`EmptyArgv`] if `argv` is empty.
pub fn split_argv(argv: &[String]) -> Result<(&str, &[String]), EmptyArgv> {
    let [program, arguments @ ..] = argv else {
        return Err(EmptyArgv);
    };
    Ok((program, arguments))
}

/// A vector with at least one element, so `first` and `last` cannot fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonEmpty<T> {
    head: T,
    tail: Vec<T>,
}

impl<T> NonEmpty<T> {
    /// Creates a one-element vector.
    pub fn new(head: T) -> Self {
        Self { head, tail: Vec::new() }
    }

    /// Appends an element.
    pub fn push(&mut self, item: T) {
        self.tail.push(item);
    }

    /// Returns the first element.
    pub fn first(&self) -> &T {
        &self.head
    }

    /// Returns the last element.
    pub fn last(&self) -> &T {
        self.tail.last().unwrap_or(&self.head)
    }
}
```
A send that returns the value on failure:
```rust
//! Channel send without `unwrap` (H-02, H-13).

use std::sync::mpsc::SyncSender;

/// A unit of work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job(pub u64);

/// The pool stopped. The job comes back so the caller can retry or report it.
#[derive(Debug, thiserror::Error)]
#[error("worker pool has shut down")]
pub struct PoolClosed(pub Job);

/// Queues `job` on a bounded channel.
///
/// # Errors
///
/// Returns [`PoolClosed`], carrying the job, if every worker has exited.
pub fn submit(queue: &SyncSender<Job>, job: Job) -> Result<(), PoolClosed> {
    queue.send(job).map_err(|rejected| PoolClosed(rejected.0))
}
```

**Detect.** `clippy::unwrap_used`, `expect_used`, `unwrap_in_result` (all denied), the temper scanner. Review: `unwrap_or_else(|_| panic!(..))` and `match .. { None => unreachable!() }` are the same escape hatch (H-03).

## Handle or propagate

### ERR-08 · Never both

**Why.** Logging an error and then returning it produces one log line per layer for one failure, each with less context than the final report, and on-call engineers chase phantom errors. Decide at each site: propagate (with context) and let the handler log, or handle (log once, recover, fall back, skip) and do not return it.

**Do.**
```rust
//! Handle or propagate, never both (ERR-08).

use crate::report::Report;

/// A feed fetch failed.
#[derive(Debug, thiserror::Error)]
#[error("failed to fetch feed {url}")]
pub struct FetchError {
    /// Feed URL.
    pub url: String,
    /// Underlying failure.
    #[source]
    pub source: std::io::Error,
}

/// Fetches every feed. A broken feed is handled here: logged once, skipped.
pub fn fetch_all<F>(urls: &[String], fetch: F) -> Vec<String>
where
    F: Fn(&str) -> Result<String, FetchError>,
{
    urls.iter()
        .filter_map(|url| match fetch(url) {
            Ok(body) => Some(body),
            Err(err) => {
                tracing::warn!(error = %Report(&err), "skipping feed");
                None
            }
        })
        .collect()
}
```

**Don't.**
```rust,ignore
let cfg = load(path).inspect_err(|e| tracing::error!("load failed: {e}"))?;  // logged, then propagated
Err(e) => { tracing::error!(?e); return Err(e.into()); }                     // same, spelled out
```

**Detect.** Review: `inspect_err`/`map_err` blocks that log and then `?`; an `error!`/`warn!` followed by `return Err`; the same failure appearing twice in test log output.

## Arithmetic on untrusted input

### ERR-09 · Checked, saturating or wrapping, chosen on purpose

**Why.** `a * b` on external values panics in debug builds and silently wraps in release: a crash in tests and a wrong invoice in production. Decide per operation: overflow is an error (`checked_*` + variant), a documented cap (`saturating_*`), or modular by specification (`wrapping_*` for hashes, checksums, sequence numbers). `strict_*` (1.91) panics on overflow in every build, so it is a panic (H-03) and not allowed in production code.

**Do.**
```rust
//! Arithmetic on untrusted input (ERR-09).

use std::time::Duration;

/// An order total does not fit in `u64` cents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("total for {quantity} x {unit_cents} cents overflows")]
pub struct TotalOverflow {
    /// Requested quantity.
    pub quantity: u32,
    /// Unit price in cents.
    pub unit_cents: u64,
}

/// Multiplies a caller-supplied price and quantity. Overflow is an error.
///
/// # Errors
///
/// Returns [`TotalOverflow`] if the product exceeds `u64::MAX`.
pub fn line_total(unit_cents: u64, quantity: u32) -> Result<u64, TotalOverflow> {
    unit_cents.checked_mul(u64::from(quantity)).ok_or(TotalOverflow { quantity, unit_cents })
}

/// Longest retry delay.
pub const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Exponential backoff. Documented saturation: huge `attempt` values cap at
/// [`MAX_BACKOFF`] instead of overflowing.
#[must_use]
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(100).saturating_mul(2_u32.saturating_pow(attempt)).min(MAX_BACKOFF)
}
```

**Don't.**
```rust,ignore
let total = unit_cents * u64::from(quantity);  // attacker-chosen quantity: panic or wrap
let idx = offset + len;                        // then used to slice: H-04 as well
let n = header.len as usize;                   // H-11
```

**Detect.** `clippy::as_conversions`. Review: `+ - * /` on values that came from a request, file, or environment. `clippy::arithmetic_side_effects` flags every unchecked operator, but it is not in the template, and a member crate cannot add lints next to `[lints] workspace = true`. To opt a parser, codec or money crate in, the human copies the workspace lint table into that crate's own `[lints]` (dropping `workspace = true`) and adds the lint. That is an H-05 decision, recorded in the charter.

## Domain rejections vs infrastructure failures

### ERR-10 · Separate "no" from "could not ask"

**Why.** "Insufficient funds" is a correct answer: deterministic, shown to the user, never retried. "Store unavailable" means no answer was produced: retry, alert, return 503. One flat enum mixes them and every caller re-derives which is which. Keep the pure decision's error type free of I/O (ARC-04), and let the use case wrap both.

**Do.**
```rust
//! Domain rejections versus infrastructure failures (ERR-10).

/// The business said no. Deterministic; retrying changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Rejection {
    /// Not enough money.
    #[error("insufficient funds: balance {balance}, requested {requested}")]
    InsufficientFunds {
        /// Current balance in cents.
        balance: u64,
        /// Requested amount in cents.
        requested: u64,
    },
}

/// The account store could not answer. Operational; may be retried.
#[derive(Debug, thiserror::Error)]
#[error("account store unavailable")]
pub struct StoreUnavailable(#[source] pub std::io::Error);

/// Port: reads balances.
pub trait Balances {
    /// Returns the balance in cents.
    ///
    /// # Errors
    ///
    /// Returns [`StoreUnavailable`] if the backing store fails.
    fn balance(&self, account: u64) -> Result<u64, StoreUnavailable>;
}

/// Why [`withdraw`] did not complete. The two causes stay distinguishable.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WithdrawError {
    /// The domain refused the command.
    #[error(transparent)]
    Rejected(#[from] Rejection),
    /// Infrastructure failed; the command was not evaluated.
    #[error("could not load balance for account {account}")]
    Store {
        /// Account being read.
        account: u64,
        /// Underlying failure.
        #[source]
        source: StoreUnavailable,
    },
}

/// Pure decision: no I/O, only domain rejections.
///
/// # Errors
///
/// Returns [`Rejection::InsufficientFunds`] if `requested` exceeds `balance`.
pub fn decide(balance: u64, requested: u64) -> Result<u64, Rejection> {
    balance.checked_sub(requested).ok_or(Rejection::InsufficientFunds { balance, requested })
}

/// Use case: loads, then decides.
///
/// # Errors
///
/// [`WithdrawError::Store`] if the balance cannot be loaded,
/// [`WithdrawError::Rejected`] if the withdrawal is refused.
pub fn withdraw(store: &impl Balances, account: u64, requested: u64) -> Result<u64, WithdrawError> {
    let balance =
        store.balance(account).map_err(|source| WithdrawError::Store { account, source })?;
    Ok(decide(balance, requested)?)
}
```

**Detect.** Review: domain crates whose error enums contain `io::Error`, `sqlx::Error` or `Timeout`; HTTP handlers that map every error to 500 or every error to 400.

## Testing errors

### ERR-11 · Assert variants structurally

**Why.** Messages change for readability; variants and fields are the contract. Asserting `err.to_string() == "..."` fails on a harmless rewording and passes when the right text comes from the wrong variant. Match the variant and the fields that matter. `std::assert_matches!` is stable since 1.96 and prints the value on failure; `assert!(matches!(..))` works everywhere. Snapshot the rendered chain with `insta` (TST-07) only when the text itself is the product (a CLI's output).

**Do.**
```rust
use std::assert_matches;
use std::path::Path;

use super::{LoadConfigError, ValidateError, parse};

#[test]
fn rejects_zero_workers_with_line_number() {
    let err = parse(Path::new("app.conf"), "# pool\nworkers = 0\n").unwrap_err();
    assert_matches!(err, LoadConfigError::Workers { line: 2, .. });
}

#[test]
fn reports_missing_workers() {
    let err = parse(Path::new("app.conf"), "").unwrap_err();
    assert!(matches!(err, LoadConfigError::Invalid(ValidateError::MissingWorkers)));
}
```

**Don't.**
```rust,ignore
assert_eq!(err.to_string(), "app.conf:2: `workers` must be a positive integer"); // brittle
assert!(result.is_err());                                                          // any error passes (TST-05)
```

**Detect.** Review test diffs for `to_string()` or `format!("{err}")` inside `assert_eq!`, and bare `is_err()` assertions.

## Discarded results

### H-10 · No silently discarded failures · ERR-12 · Never silence `#[must_use]`

**Why.** A dropped `Result` is a failure nobody will ever see: the write that did not happen, the message never delivered. `#[must_use]` exists so the compiler flags it; `let _ =` and `.ok();` are the two ways to lie to it.

| Pattern | What it hides | Instead |
|---|---|---|
| `let _ = fallible();` | The error. Denied: `let_underscore_must_use`. | `?`, or handle it. |
| `fallible().ok();` | The error. Denied: `unused_result_ok`. | `?`, or handle it. |
| `.unwrap_or_default()` on a `Result` | A parse or I/O failure becomes `0` or `""`. | `?`. Use a default only on an `Option` whose absence means "default" by specification. |
| `.unwrap_or(0)` / `.unwrap_or_else(\|_\| ..)` | Same, with a hand-picked value. | Same; if the fallback is the spec, log once (ERR-08). |
| `.filter_map(Result::ok)` / `.flatten()` on `Result`s | Every bad item, silently. | Collect into `Result<Vec<_>, _>`, or partition and report the rejects. |
| `if let Ok(v) = f() { .. }` with no `else` | The failure path. | `match` with an `Err` arm that propagates or logs. |
| `let _ = mutex.lock();` | Drops the guard at once; nothing is locked. | `let _guard = ..`, or restructure. |

**The rare legitimate discard.** Under the template lints `let _ =` on a `Result` does not compile. When failure really is an expected, harmless outcome, write the branch out and say why:
```rust
//! Discarded results (H-10, ERR-12).

use std::sync::mpsc::SyncSender;

/// Sends a reply to a requester that may have stopped waiting.
pub fn reply(to: &SyncSender<u64>, value: u64) {
    // A closed receiver means the requester timed out and went away. That is an
    // expected outcome of the protocol, not a failure: record it and move on.
    if to.send(value).is_err() {
        tracing::debug!(value, "requester gone; reply dropped");
    }
}
```
For values that are not `#[must_use]`, write the expression as a statement; `let _ =` adds nothing. If a `#[must_use]` value must be dropped without a branch, that is an exception request (H-05), not a `let _`.

**Detect.** `let_underscore_must_use`, `unused_result_ok`, `unused_must_use`, `let_underscore_drop`; scanner; review for `unwrap_or_default`, `filter_map(Result::ok)`, `.ok()` followed by `?`-free code.

## Review checklist

- [ ] Does every public library signature return a typed error that is `Error + Send + Sync + 'static`, with no `String`, `Box<dyn Error>` or `anyhow`? (ERR-01, H-09)
- [ ] Is each error type scoped to an operation or module, with only variants that can occur? Opaque with `kind()` where internals must evolve? (ERR-02)
- [ ] Do variants carry typed context fields rather than preformatted strings? (ERR-03)
- [ ] Is every cause kept with `#[source]`, and is `#[from]` used only where one step produces that cause? (ERR-04)
- [ ] Are messages lowercase, unpunctuated, about this layer only, without interpolating the source? (ERR-05)
- [ ] Does the binary use `anyhow` with context at each boundary, and map failures to exit codes in `main`? (ERR-06)
- [ ] Is every `Option` turned into an error with `ok_or`, `ok_or_else` or `let ... else`? (ERR-07)
- [ ] Is any error both logged and returned? (ERR-08)
- [ ] Does arithmetic on external input use `checked_*`, `saturating_*` or `wrapping_*` deliberately, and no `strict_*`? (ERR-09)
- [ ] Are domain rejections distinct from infrastructure failures? (ERR-10)
- [ ] Do tests assert variants and fields, not `Display` strings or bare `is_err()`? (ERR-11)
- [ ] Is any `#[must_use]` value silenced, or any failure hidden by `let _ =`, `.ok();`, `unwrap_or_default()` or `filter_map(Result::ok)`? (ERR-12, H-10)
- [ ] Is there any `unwrap`, `expect`, `unwrap_err` or `expect_err` outside tests? (H-02)
