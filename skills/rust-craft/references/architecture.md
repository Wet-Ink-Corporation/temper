# Architecture

Workspace layout, crate and module boundaries, dependency direction, ports and adapters, event sourcing, configuration, dependency policy and binaries. Read this before creating a crate or module, moving code between them, adding a dependency, or designing a use case. The default architecture is event-sourced, domain-driven and hexagonal: a pure domain core, an application layer that owns ports, adapters at the edge, and thin binaries that wire them. Tests reach from the domain up to view models (Crux style) without touching I/O.

Rules covered: ARC-01, ARC-02, ARC-03, ARC-04, ARC-05, ARC-06, ARC-07, ARC-08, ARC-09, ARC-10, ARC-11, ARC-12, ARC-13, ARC-14, ARC-15

## Workspace layout

```text
ledger/
├── Cargo.toml              virtual manifest: [workspace], shared package/deps/lints (ARC-01)
├── Cargo.lock              committed, for binaries and libraries alike
├── clippy.toml, rustfmt.toml, deny.toml
├── AGENTS.md               workspace charter: layer map, release-profile deviations
├── crates/
│   ├── ledger-domain/      pure: types, invariants, decide/evolve/view. No I/O, no async.
│   ├── ledger-app/         use cases + port traits the use cases need. Depends on domain.
│   ├── ledger-adapter-fs/  implements app ports with std::fs. Depends on app + domain.
│   ├── ledger-adapter-http/ one adapter crate per external system (DB, HTTP, queue)
│   └── ledger-cli/         bin: config, tracing, composition root, exit codes
└── xtask/                  optional: repo automation as a Rust binary (not shipped)
```

Every crate has `AGENTS.md` (its charter, ARC-08) and a `CLAUDE.md` containing `@AGENTS.md`. Root manifest essentials come from `templates/workspace/Cargo.toml`: `resolver = "3"`, `[workspace.package]` with `edition = "2024"` and `rust-version`, every third-party crate declared once in `[workspace.dependencies]`, and the full `[workspace.lints]` tables. Members inherit with `edition.workspace = true`, `thiserror = { workspace = true }` and `[lints] workspace = true`.

### ARC-01 · Virtual workspace with shared config
**Why.** One manifest for editions, versions and lints means a new crate cannot silently opt out of the escape-hatch lints or drift to a different dependency version.
**Do.**
```toml
# crates/ledger-domain/Cargo.toml
[package]
name = "ledger-domain"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
thiserror = { workspace = true }
serde = { workspace = true }
```
**Don't.**
```toml
[package]
name = "ledger-domain"
edition = "2021"            # drifts from the workspace
[dependencies]
serde = "1.0.150"           # version pinned per crate, bypasses [workspace.dependencies]
# no [lints] table: this crate compiles with unwrap, as-casts and #[allow] permitted
```
**Detect.** Review every new `Cargo.toml` for `[lints] workspace = true` and `workspace = true` on each dependency. `cargo metadata --format-version 1 | jq '.packages[] | select(.source == null) | .name, .edition'`.

## Layer rules

| Layer | Contains | May depend on | Must never contain |
|---|---|---|---|
| Domain | Entities, value newtypes, commands, events, `decide`/`evolve`/`view`, rejection enums | `std`, `thiserror`, `serde` derives, small pure crates | I/O, clocks, randomness, env vars, async runtimes, framework or DB types, `tracing` subscribers |
| Application | Use cases, port traits, app-level error enums, orchestration (load, decide, persist, publish) | Domain | Concrete adapters, SQL, HTTP types, runtime choice (`tokio::main`), `anyhow` in `pub` signatures |
| Adapter | Port implementations: DB, HTTP client, file system, queue; wire DTOs for that system | Application, domain, the one external library it wraps | Business rules, other adapters, a dependency back from app or domain |
| Binary | Arg parsing, config loading, tracing init, composition root, runtime, exit codes | Everything | Business logic, SQL, anything a test would want to call |

### ARC-02 · Dependencies point inward
**Why.** When the domain imports nothing effectful, every rule is testable as a pure function and adapters can be swapped without touching invariants. An inward arrow reversed once makes the whole core depend on a database.
**Do.** Domain `[dependencies]` list only pure crates (see ARC-01 example).
**Don't.**
```toml
# crates/ledger-domain/Cargo.toml
[dependencies]
sqlx = { workspace = true }     # domain now knows the database
tokio = { workspace = true }    # domain now chooses a runtime
```
**Detect.** `cargo tree -e normal -p ledger-domain` must not show `tokio`, `sqlx`, `reqwest`, `hyper`, `axum`, or any `ledger-adapter-*`. `cargo tree -e normal -i ledger-adapter-fs` must show only binaries as dependents.

### ARC-03 · Ports are owned by the consumer
**Why.** The application declares the capability it needs in its own vocabulary, so adapters depend on the app and not the reverse. Adapter types (rows, HTTP responses, SDK errors) stay behind the trait.
**Do.** See the example below: `EventStore` lives in `app`, speaks `StreamId` and domain `Event`, and hides failures behind an associated `Error`.
**Don't.**
```rust,ignore
// in ledger-app
pub trait EventStore {
    fn load(&self, id: &str) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error>; // adapter types leak inward
}
```
**Detect.** Grep port traits for foreign crate paths in signatures. A port defined in an adapter crate is a finding.

## Ports and adapters

The example simulates crates as modules of one crate: `app` stands for `ledger-app`, `adapter_fs` for `ledger-adapter-fs`, and `crate::account` (next section) for `ledger-domain`. In a real workspace each is its own crate and the `use crate::...` paths become crate names.

```rust
//! Ports and adapters. Each `mod` here stands in for a crate:
//! `app` is `ledger-app`, `adapter_fs` is `ledger-adapter-fs`.

/// Application layer: use cases and the ports they need.
pub mod app {
    use thiserror::Error;

    use crate::account::{self, Command, Event, Rejection};

    /// Validated stream name: 1..=64 ASCII alphanumerics or `-`.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct StreamId(String);

    /// The stream name failed validation.
    #[derive(Debug, Error, PartialEq, Eq)]
    #[error("invalid stream id {0:?}: expected 1-64 ASCII alphanumerics or '-'")]
    pub struct InvalidStreamId(String);

    impl StreamId {
        /// Validate and wrap a stream name.
        ///
        /// # Errors
        ///
        /// Returns [`InvalidStreamId`] for empty, long or non-`[A-Za-z0-9-]` names.
        pub fn parse(raw: impl Into<String>) -> Result<Self, InvalidStreamId> {
            let raw = raw.into();
            let valid = (1..=64).contains(&raw.len())
                && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
            if valid {
                Ok(Self(raw))
            } else {
                Err(InvalidStreamId(raw))
            }
        }

        /// The validated name.
        #[must_use]
        pub fn as_str(&self) -> &str {
            &self.0
        }
    }

    /// Port: event persistence. Owned here, implemented by adapters.
    pub trait EventStore {
        /// Adapter-specific failure. The app never inspects it, only wraps it.
        type Error: std::error::Error + Send + Sync + 'static;

        /// Load a stream's events in order; an unknown stream is empty.
        ///
        /// # Errors
        ///
        /// Returns the adapter's error when the stream cannot be read.
        fn load(&self, stream: &StreamId) -> Result<Vec<Event>, Self::Error>;

        /// Append events if the stream still holds `expected_len` events.
        ///
        /// # Errors
        ///
        /// Returns the adapter's error on a concurrency conflict or I/O failure.
        fn append(
            &mut self,
            stream: &StreamId,
            expected_len: usize,
            events: &[Event],
        ) -> Result<(), Self::Error>;
    }

    /// Failure of [`handle`]: a business rejection or a store failure.
    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum HandleError<E> {
        /// The domain refused the command.
        #[error("command rejected")]
        Rejected(#[from] Rejection),
        /// The event store failed.
        #[error("event store failed")]
        Store(#[source] E),
    }

    /// Use case: load, fold, decide, append. The imperative shell around the pure core.
    ///
    /// # Errors
    ///
    /// [`HandleError::Rejected`] when the domain refuses, [`HandleError::Store`] on persistence failure.
    pub fn handle<S: EventStore>(
        store: &mut S,
        stream: &StreamId,
        command: &Command,
    ) -> Result<Vec<Event>, HandleError<S::Error>> {
        let history = store.load(stream).map_err(HandleError::Store)?;
        let events = account::decide(command, &account::fold(&history))?;
        store
            .append(stream, history.len(), &events)
            .map_err(HandleError::Store)?;
        Ok(events)
    }
}

/// Adapter: one JSON Lines file per stream.
pub mod adapter_fs {
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, BufReader, BufWriter, Write};
    use std::path::PathBuf;

    use thiserror::Error;

    use crate::account::Event;
    use crate::ports::app::{EventStore, StreamId};

    /// File-backed [`EventStore`]. Single writer: not safe across processes.
    #[derive(Debug)]
    pub struct JsonlStore {
        root: PathBuf,
    }

    /// File store failure. Adapter detail; never reaches the domain.
    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum FsStoreError {
        /// File system failure on a path.
        #[error("i/o on {}", .0.display())]
        Io(PathBuf, #[source] io::Error),
        /// A stored record is not a valid event.
        #[error("corrupt stream file {}", .0.display())]
        Corrupt(PathBuf, #[source] serde_json::Error),
        /// Another writer appended first: `(expected, found)` lengths.
        #[error("stream changed: expected {0} events, found {1}")]
        Conflict(usize, usize),
    }

    impl JsonlStore {
        /// Store streams under `root`.
        pub fn new(root: impl Into<PathBuf>) -> Self {
            Self { root: root.into() }
        }

        fn path(&self, stream: &StreamId) -> PathBuf {
            // Safe as a file name: `StreamId` admits only `[A-Za-z0-9-]`.
            self.root.join(format!("{}.jsonl", stream.as_str()))
        }
    }

    impl EventStore for JsonlStore {
        type Error = FsStoreError;

        fn load(&self, stream: &StreamId) -> Result<Vec<Event>, FsStoreError> {
            let path = self.path(stream);
            let file = match File::open(&path) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(e) => return Err(FsStoreError::Io(path, e)),
            };
            serde_json::Deserializer::from_reader(BufReader::new(file))
                .into_iter()
                .collect::<Result<_, _>>()
                .map_err(|e| FsStoreError::Corrupt(path, e))
        }

        fn append(
            &mut self,
            stream: &StreamId,
            expected_len: usize,
            events: &[Event],
        ) -> Result<(), FsStoreError> {
            let found = self.load(stream)?.len();
            if found != expected_len {
                return Err(FsStoreError::Conflict(expected_len, found));
            }
            let path = self.path(stream);
            let io_err = |e| FsStoreError::Io(path.clone(), e);
            fs::create_dir_all(&self.root).map_err(io_err)?;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(io_err)?;
            let mut writer = BufWriter::new(file);
            for event in events {
                serde_json::to_writer(&mut writer, event).map_err(|e| io_err(e.into()))?;
                writer.write_all(b"\n").map_err(io_err)?;
            }
            writer.flush().map_err(io_err)
        }
    }
}
```

The in-memory fake implements the same port for tests. Put it in `#[cfg(test)]` when one crate uses it, or in a `testing` module behind a `testing` Cargo feature when several crates do.

```rust
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use thiserror::Error;

    use super::app::{EventStore, HandleError, StreamId, handle};
    use crate::account::{Command, Event, Rejection};

    /// In-memory fake: same contract as the real adapter, no I/O.
    #[derive(Debug, Default)]
    struct InMemoryStore {
        streams: BTreeMap<StreamId, Vec<Event>>,
    }

    #[derive(Debug, Error)]
    #[error("expected {0} events, found {1}")]
    struct Conflict(usize, usize);

    impl EventStore for InMemoryStore {
        type Error = Conflict;

        fn load(&self, stream: &StreamId) -> Result<Vec<Event>, Conflict> {
            Ok(self.streams.get(stream).cloned().unwrap_or_default())
        }

        fn append(
            &mut self,
            stream: &StreamId,
            expected: usize,
            events: &[Event],
        ) -> Result<(), Conflict> {
            let existing = self.streams.entry(stream.clone()).or_default();
            if existing.len() != expected {
                return Err(Conflict(expected, existing.len()));
            }
            existing.extend_from_slice(events);
            Ok(())
        }
    }

    #[test]
    fn rejected_commands_store_nothing() {
        let (mut store, stream) = (
            InMemoryStore::default(),
            StreamId::parse("acc-1").expect("valid"),
        );
        let result = handle(&mut store, &stream, &Command::Deposit(5));
        assert!(matches!(
            result,
            Err(HandleError::Rejected(Rejection::NotOpen))
        ));
        assert!(store.load(&stream).expect("fake never fails").is_empty());
    }
}
```

## Functional core, imperative shell

### ARC-04 · Decisions are pure; effects live at the edges
**Why.** A pure function over data needs no mocks, runs in microseconds, and is property-testable. Effects mixed into decisions force every test through fakes and make ordering bugs invisible.
**Do.** The shell (`app::handle`) does I/O, then calls pure `fold` and `decide`, then does I/O again. Time, randomness and ids arrive as values or injected ports (see Dependency injection), never read inside the core.
**Don't.**
```rust,ignore
pub fn withdraw(&mut self, amount: u64) -> Result<(), Error> {
    let balance = db::load_balance(self.id)?;        // I/O inside the decision
    if SystemTime::now() > self.frozen_until { .. }  // hidden clock
    db::save(self.id, balance - amount)?;            // effect interleaved with the rule
    Ok(())
}
```
**Detect.** In domain code, flag `std::fs`, `std::net`, `std::env`, `SystemTime::now`, `Instant::now`, `rand::`, `.await`. Tests that need a fake to check a business rule point at a rule living in the shell.

Test up to the view model: the core exposes `view(&State) -> ViewModel`, and tests assert on what the screen would show. The shell (CLI, HTTP, UI) only renders the view model.

## Event sourcing with the Decider

### ARC-13 · Decider shape
**Why.** `decide` and `evolve` separate "may this happen?" from "what does it change?". State is a fold over history, so replay, projections and audit fall out for free, and every rule is a given-when-then test.
**Do.**
```rust
//! Account aggregate in Decider shape: pure, no I/O, no async.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Intent to change an account. Imperative; may be rejected. Amounts in cents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Open the account for the named owner.
    Open(String),
    /// Add funds.
    Deposit(u64),
    /// Remove funds.
    Withdraw(u64),
}

/// A fact that happened. Past tense, immutable, persisted forever.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    /// The account was opened for the named owner.
    Opened(String),
    /// Funds were added.
    Deposited(u64),
    /// Funds were removed.
    Withdrawn(u64),
}

/// Everything `decide` needs to know, and nothing more.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum State {
    /// No `Opened` event yet.
    #[default]
    Unopened,
    /// Open, with the balance in cents.
    Open(u64),
}

/// Why a command was refused. Business outcomes, not infrastructure failures.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rejection {
    /// `Open` on an open account.
    #[error("account is already open")]
    AlreadyOpen,
    /// Money movement before `Open`.
    #[error("account is not open")]
    NotOpen,
    /// Zero-value deposits and withdrawals are meaningless.
    #[error("amount must be positive")]
    ZeroAmount,
    /// Withdrawal larger than the balance: `(balance, requested)`.
    #[error("insufficient funds: balance {0}, requested {1}")]
    InsufficientFunds(u64, u64),
    /// Deposit would overflow the balance.
    #[error("deposit would overflow the balance")]
    Overflow,
}

/// Decide which events a command produces, given the current state.
///
/// # Errors
///
/// Returns a [`Rejection`] when the command violates an account invariant.
pub fn decide(command: &Command, state: &State) -> Result<Vec<Event>, Rejection> {
    match (command, *state) {
        (Command::Open(_), State::Open(_)) => Err(Rejection::AlreadyOpen),
        // Clone: the event owns its data and outlives the command.
        (Command::Open(owner), State::Unopened) => Ok(vec![Event::Opened(owner.clone())]),
        (Command::Deposit(_) | Command::Withdraw(_), State::Unopened) => Err(Rejection::NotOpen),
        (Command::Deposit(0) | Command::Withdraw(0), State::Open(_)) => Err(Rejection::ZeroAmount),
        (Command::Deposit(amount), State::Open(balance)) => {
            balance.checked_add(*amount).ok_or(Rejection::Overflow)?;
            Ok(vec![Event::Deposited(*amount)])
        }
        (Command::Withdraw(amount), State::Open(balance)) if *amount > balance => {
            Err(Rejection::InsufficientFunds(balance, *amount))
        }
        (Command::Withdraw(amount), State::Open(_)) => Ok(vec![Event::Withdrawn(*amount)]),
    }
}

/// Apply one event. Total and infallible: events are facts `decide` already checked.
#[must_use]
pub fn evolve(state: State, event: &Event) -> State {
    match (state, event) {
        (State::Unopened, Event::Opened(_)) => State::Open(0),
        (State::Open(balance), Event::Deposited(amount)) => {
            State::Open(balance.saturating_add(*amount))
        }
        (State::Open(balance), Event::Withdrawn(amount)) => {
            State::Open(balance.saturating_sub(*amount))
        }
        // Not produced by `decide`; a total function keeps the state unchanged.
        (state @ (State::Unopened | State::Open(_)), _) => state,
    }
}

/// Rebuild state from history.
#[must_use]
pub fn fold<'a>(history: impl IntoIterator<Item = &'a Event>) -> State {
    history.into_iter().fold(State::default(), evolve)
}

/// View model: what a screen shows, already formatted. Tests assert on this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountView {
    /// `"open"` or `"not opened"`.
    pub status: &'static str,
    /// Balance as `units.cents`, empty when unopened.
    pub balance: String,
}

/// Project state into the view model. Pure, so everything up to the screen
/// is testable without a UI or I/O.
#[must_use]
pub fn view(state: &State) -> AccountView {
    match *state {
        State::Unopened => AccountView {
            status: "not opened",
            balance: String::new(),
        },
        State::Open(cents) => AccountView {
            status: "open",
            balance: format!("{}.{:02}", cents / 100, cents % 100),
        },
    }
}
```
Tests read as specifications:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn opened_with(cents: u64) -> [Event; 2] {
        [Event::Opened("ada".to_owned()), Event::Deposited(cents)]
    }

    #[test]
    fn withdrawing_more_than_balance_is_rejected() {
        let state = fold(&opened_with(500));
        let result = decide(&Command::Withdraw(600), &state);
        assert_eq!(result, Err(Rejection::InsufficientFunds(500, 600)));
    }

    #[test]
    fn withdrawal_shows_in_the_view() {
        let mut history = opened_with(500).to_vec();
        history.extend(decide(&Command::Withdraw(200), &fold(&history)).expect("funds suffice"));
        assert_eq!(view(&fold(&history)).balance, "3.00");
    }
}
```
**Don't.**
```rust,ignore
pub enum Event { Deposit { amount: u64 }, UpdateBalance { new_balance: u64 } } // imperative names; state snapshots, not facts
pub fn evolve(state: &mut State, event: &Event) -> Result<(), Error> { .. }      // fallible evolve: replay can fail on history
```
**Detect.** Events named in the imperative, `evolve` returning `Result`, I/O in `decide`, `Rejection` variants holding `io::Error`.

Rules of the shape:
- Commands are imperative (`Withdraw`), events past tense (`Withdrawn`). An event is a fact: never rejected, never edited, never deleted.
- `decide` returns `Result<Vec<Event>, Rejection>`. `Rejection` is a business outcome enum, separate from infrastructure errors (`HandleError::Store`).
- `evolve` is total and infallible. Validation belongs in `decide`; history is already true.
- `State` holds only what `decide` needs. Read models are separate projections (`view`, or a `BTreeMap` built by folding events into a query-side structure).

**Versioning.** Stored events outlive code. Change them additively (new optional fields with `#[serde(default)]`, new variants). For a breaking change, freeze the old shape as a private `V1` type, add the new shape, and upcast on read so `decide`/`evolve` see only the current type. Changing a persisted event shape is a schema migration and a HITL decision (H-16). The full compiling pattern is in `structured-output.md` §Versioning and upcasting.
```rust,ignore
#[derive(Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum StoredEvent {
    Deposited(u64),                     // v1: currency implicit (EUR); frozen, never edited
    DepositedV2(u64, Currency),         // v2
}
fn upcast(stored: StoredEvent) -> Event {  // decide/evolve only ever see the current Event
    match stored {
        StoredEvent::Deposited(cents) => Event::Deposited(cents, Currency::Eur),
        StoredEvent::DepositedV2(cents, currency) => Event::Deposited(cents, currency),
    }
}
```

## Module organization

### ARC-05 · One concept per module; `lib.rs` is a table of contents
**Why.** A reader (or agent) orients from `lib.rs` in seconds when it lists modules and the deliberate public surface. Implementation in `lib.rs` hides the architecture and grows without bound.
**Do.**
```rust,ignore
//! Ledger domain: accounts as event-sourced deciders.

mod account;
mod money;

pub use account::{Command, Event, Rejection, State, decide, evolve, fold, view};
pub use money::Cents;
```
**Don't.** A 900-line `lib.rs` with structs, impls and helpers; `pub mod` on everything so the module tree becomes the API by accident.
**Detect.** `lib.rs` containing `fn` bodies or `impl` blocks beyond trivial re-exports; `pub mod` without a reason.

### ARC-06 · `foo.rs` + `foo/`, soft size caps
**Why.** `mod.rs` files make editor tabs indistinguishable and hide where a module lives; `#[path]` breaks the file-to-module mapping every tool relies on.
**Do.**
```text
src/lib.rs            mod account;
src/account.rs        mod decide; mod evolve;  (the module's own surface)
src/account/decide.rs
src/account/evolve.rs
```
**Don't.** `src/account/mod.rs`; `#[path = "../shared/x.rs"] mod x;`.
**Detect.** `clippy::mod_module_files` (warn in template), `clippy::too_many_lines` (threshold 60 in `clippy.toml`), `find src -name mod.rs`, `grep -rn '#\[path' src`.

Size caps: about 400 lines per file, about 50 per function (CRF-02). Split a module when it has two reasons to change, when its private items form two clusters that do not call each other, or when a reader needs to scroll to find the public items. Split along concepts (`account/decide.rs`), never by kind (`types.rs`, `traits.rs`, `impls.rs`).

## When to split a crate

### ARC-07 · Split by dependency profile and reason to change
**Why.** Crates are the unit of compilation, dependency and visibility. A split that follows dependency direction lets `cargo tree` enforce the architecture; a split by type category just adds boilerplate and cycles.
**Do.** Split when at least one holds:
1. The code needs a different dependency profile (pure domain vs `sqlx` adapter).
2. It changes for a different reason or on a different cadence (a protocol adapter vs business rules).
3. It is a reuse boundary (two binaries need it, or it is published).
4. Compile time: a large, stable subtree recompiles on every edit of an unrelated part.
5. Visibility: you need `pub(crate)` to hide internals from a sibling that currently sees them.

Do not split for "organization", for one struct, or to share three helper functions (copy them or give them a real home).
**Don't.** `crates/common`, `crates/utils`, `crates/models`, `crates/types`: dumping grounds every crate depends on, which couple everything to everything.
**Detect.** Crate names that are categories rather than concepts. A crate with fan-in from every other crate and no single purpose. A new crate is a crate-boundary change: HITL (H-16).

## Crate charter template

### ARC-08 · Every crate has a charter
**Why.** The charter is the first thing an agent reads (PROC-01). It records decisions that code cannot express: what the crate is for, what it must never depend on, which invariants hold. Stale charters mislead, so update them in the same change.
**Detect.** A crate directory without `AGENTS.md`; a diff that changes dependencies, public surface or invariants without touching the charter.

Put this in `crates/<name>/AGENTS.md`, and `@AGENTS.md` in `crates/<name>/CLAUDE.md`:

```markdown
# <crate-name>

## Purpose
One or two sentences: what this crate does and for whom. What it deliberately does not do.

## Layer
domain | application | adapter (<external system>) | binary

## Allowed dependencies
- Workspace crates: <list, inward only>
- Third-party: <crate — why> (each approved as a HITL decision, see log)
- Forbidden: <e.g. tokio, sqlx, any ledger-adapter-*>

## Public surface
Re-exported from lib.rs: <types, traits, functions>. Everything else is private or pub(crate).
Stability: <internal to workspace | published, semver-checked>

## Invariants
- <e.g. Balance never negative; enforced by `decide`, tested by `withdrawing_more_than_balance_is_rejected`>
- <e.g. `evolve` is total and never fails>

## Error types
- <ErrorEnum>: <operation it belongs to, #[non_exhaustive], which variants callers match on>

## Test strategy
- Unit: <given/when/then over decide/evolve; view-model assertions>
- Property: <e.g. fold(history) never panics; balance equals deposits minus withdrawals>
- Integration: <adapter contract tests shared by fake and real impl>
- Snapshot: <schemas, CLI output>

## Do-not list
- Do not <read env vars, call SystemTime::now, add I/O, expose adapter types, ...>

## Owner and decisions log
Owner: <human or team>
| Date | Decision | Reason | Approved by |
|---|---|---|---|
| YYYY-MM-DD | <e.g. added serde for event persistence> | <why> | <name> |
```

## Coupling and cohesion heuristics

### ARC-09 · Low coupling
**Why.** Every dependency edge is a reason your code changes when someone else's does. Global mutable state is an invisible edge to every caller, and cyclic modules cannot be split into crates later.
**Do.** Depend on the narrowest port (`EventStore`, not `Database`). Inject collaborators through constructors or parameters. Keep fan-out of a module small; high fan-in is fine for stable, pure types.
**Don't.**
```rust,ignore
static STORE: LazyLock<Mutex<HashMap<String, Vec<Event>>>> = LazyLock::new(Default::default); // global mutable state
mod a { use super::b::helper; }  mod b { use super::a::Thing; }                               // module cycle
```
**Detect.**
- `cargo tree -e normal -p <crate>` for fan-out; `cargo tree -e normal -i <crate>` for fan-in.
- Module cycles: two modules that `use` each other's items. Break by moving the shared concept into a third module or inverting with a trait.
- `static` with `Mutex`, `RwLock`, `RefCell` or atomics holding domain state.

### ARC-10 · High cohesion: tell, don't ask
**Why.** When behavior lives with the data whose invariant it maintains, the invariant is enforced in one place. Callers that fetch fields, decide, and write back duplicate the rule and eventually disagree.
**Do.** `account::decide(&Command::Withdraw(amount), &state)`: the account decides.
**Don't.**
```rust,ignore
if account.balance() >= amount { account.set_balance(account.balance() - amount); } // caller owns the rule
```
**Detect.** Setters on domain types; the same guard (`balance >= amount`) in more than one place; getters called only to compute something the type could compute.

## Dependency injection

Default to generic ports: `fn handle<S: EventStore>(store: &mut S, ..)` or `struct Stamper<C: Clock>`. Static dispatch inlines, needs no allocation, and keeps the core free of `dyn`. Use `Arc<dyn Port>` at the composition root when the implementation is chosen at runtime, when many heterogeneous implementations share a collection, or when monomorphization measurably hurts compile time or binary size. Implementing the port for `Arc<dyn Port>` lets the generic core accept the erased form unchanged.

```rust
//! Dependency injection: generic ports in the core, `Arc<dyn Port>` at the root.

use std::sync::Arc;
use std::time::SystemTime;

/// Port: the current time. Injected so decisions stay deterministic in tests.
pub trait Clock: Send + Sync {
    /// The current wall-clock time.
    fn now(&self) -> SystemTime;
}

/// Adapter: the operating-system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Fake: a clock frozen at one instant.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub SystemTime);

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

/// Core service: generic, statically dispatched, monomorphized per clock.
#[derive(Debug)]
pub struct Stamper<C> {
    clock: C,
}

impl<C: Clock> Stamper<C> {
    /// Build a stamper around any clock.
    pub fn new(clock: C) -> Self {
        Self { clock }
    }

    /// Pair a value with the time it was stamped.
    pub fn stamp<T>(&self, value: T) -> (SystemTime, T) {
        (self.clock.now(), value)
    }
}

/// `Arc<dyn Clock>` is itself a `Clock`, so the root can erase the type once.
impl Clock for Arc<dyn Clock> {
    fn now(&self) -> SystemTime {
        self.as_ref().now()
    }
}

/// Composition root: pick the implementation at runtime, share one instance.
#[must_use]
pub fn build_stamper(frozen_at: Option<SystemTime>) -> Stamper<Arc<dyn Clock>> {
    let clock: Arc<dyn Clock> = match frozen_at {
        Some(instant) => Arc::new(FixedClock(instant)),
        None => Arc::new(SystemClock),
    };
    Stamper::new(clock)
}
```

## Configuration

### ARC-11 · Parse once, pass validated types down
**Why.** An env var read deep in a library is hidden input: untestable without mutating process state, re-parsed on every call, and failing late. A validated `Config` fails at startup with one clear message.
**Do.**
```rust
//! Configuration: parse once at startup into validated types.

use std::env::VarError;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// How the binary formats diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable lines.
    #[default]
    Human,
    /// One JSON object per line.
    Json,
}

/// Validated configuration. Private fields: construction proves validity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    data_dir: PathBuf,
    log_format: LogFormat,
}

/// Why configuration could not be loaded.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigError {
    /// A required variable is absent.
    #[error("{0} is required")]
    Missing(&'static str),
    /// A variable is not valid Unicode.
    #[error("{0} is not valid Unicode")]
    NotUnicode(&'static str),
    /// The log format is not `human` or `json`.
    #[error("{LOG_FORMAT}={0:?}: expected human or json")]
    LogFormat(String),
}

const DATA_DIR: &str = "LEDGER_DATA_DIR";
const LOG_FORMAT: &str = "LEDGER_LOG_FORMAT";

impl Config {
    /// Load from a variable lookup. The binary passes `|k| std::env::var(k)`;
    /// tests pass a map. Library code never calls `std::env` itself.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] for the first missing or invalid variable.
    pub fn from_lookup(
        lookup: impl Fn(&str) -> Result<String, VarError>,
    ) -> Result<Self, ConfigError> {
        let get = |name: &'static str| match lookup(name) {
            Ok(value) => Ok(Some(value)),
            Err(VarError::NotPresent) => Ok(None),
            Err(VarError::NotUnicode(_)) => Err(ConfigError::NotUnicode(name)),
        };
        let data_dir = get(DATA_DIR)?
            .map(PathBuf::from)
            .ok_or(ConfigError::Missing(DATA_DIR))?;
        let log_format = match get(LOG_FORMAT)? {
            None => LogFormat::Human,
            Some(value) => match value.as_str() {
                "human" => LogFormat::Human,
                "json" => LogFormat::Json,
                _ => return Err(ConfigError::LogFormat(value)),
            },
        };
        Ok(Self {
            data_dir,
            log_format,
        })
    }

    /// Directory holding event streams.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Diagnostic output format.
    #[must_use]
    pub fn log_format(&self) -> LogFormat {
        self.log_format
    }
}
```
**Don't.**
```rust,ignore
pub fn batch_size() -> usize {
    std::env::var("LEDGER_MAX_BATCH").ok().and_then(|v| v.parse().ok()).unwrap_or(100) // deep env read; bad values silently become 100
}
```
**Detect.** `std::env::var`, `env::vars`, `dotenv` outside the binary crate. Config structs with public `String` fields that are parsed later.

Library crates take the pieces they need (`&Path`, `NonZeroUsize`), not the whole `Config`.

## Dependency policy

### ARC-12 · Justify every dependency
**Why.** Each dependency is code you ship, audit, license and compile. Unused default features add compile time and attack surface.
**Do.**
```toml
# root Cargo.toml
[workspace.dependencies]
serde = { version = "1", default-features = false, features = ["std", "derive"] }
tokio = { version = "1", default-features = false, features = ["rt-multi-thread", "macros", "net"] }
```
**Don't.** `tokio = { version = "1", features = ["full"] }` in a library; adding a crate for one 10-line function; a version declared directly in a member.
**Detect.** `cargo deny check` (licenses, advisories, bans, duplicate versions) with a committed `deny.toml`; `cargo tree -d` for duplicates; `cargo tree -e features -p <crate>` for feature bloat. A new entry in `[workspace.dependencies]` is a HITL decision (H-16): state maintenance status, size, license and alternatives, and log it in the charter.

## Thin binaries and the composition root

### ARC-14 · Binaries parse, wire, run and map exit codes
**Why.** Logic in `main` cannot be unit-tested or reused. The composition root is the one place that knows every concrete type, so everything else can stay generic.
**Do.**
```rust
//! Composition root: parse input, wire adapters to use cases, run, map to exit codes.

use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

use anyhow::{Context, bail};
use arch_demo::account::Command;
use arch_demo::config::{Config, LogFormat};
use arch_demo::ports::adapter_fs::JsonlStore;
use arch_demo::ports::app::{self, HandleError, StreamId};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

/// Exit codes (house convention, OUT-06): 0 ok, 1 failure, 2 usage,
/// 3 the domain refused the command, 4 the failure could not even be reported.
const EXIT_REJECTED: u8 = 3;
const EXIT_UNREPORTED: u8 = 4;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => report(&error),
    }
}

/// Reports a fatal error on stderr. This must not depend on logging: configuration
/// errors happen before the subscriber exists, and `tracing` without one is a no-op.
fn report(error: &anyhow::Error) -> ExitCode {
    if writeln!(io::stderr().lock(), "error: {error:#}").is_err() {
        // stderr is closed: the exit code is the only channel left.
        return ExitCode::from(EXIT_UNREPORTED);
    }
    ExitCode::FAILURE
}

fn run() -> anyhow::Result<ExitCode> {
    let config = Config::from_lookup(|key| std::env::var(key)).context("loading configuration")?;
    init_tracing(config.log_format())?;
    let (stream, command) = parse_args(std::env::args_os().skip(1))?;
    let mut store = JsonlStore::new(config.data_dir());
    match app::handle(&mut store, &stream, &command) {
        Ok(events) => {
            let mut out = io::stdout().lock();
            serde_json::to_writer(&mut out, &events).context("writing events")?;
            writeln!(out).context("writing events")?;
            Ok(ExitCode::SUCCESS)
        }
        Err(HandleError::Rejected(rejection)) => {
            tracing::warn!(stream = %stream.as_str(), %rejection, "command rejected");
            Ok(ExitCode::from(EXIT_REJECTED))
        }
        Err(other) => {
            Err(other).with_context(|| format!("handling command on {}", stream.as_str()))
        }
    }
}

/// Installs the global subscriber. `try_init` rather than `init`: `init` panics
/// if a subscriber is already set (H-03).
fn init_tracing(format: LogFormat) -> anyhow::Result<()> {
    let builder = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_writer(io::stderr);
    let installed = match format {
        LogFormat::Human => builder.try_init(),
        LogFormat::Json => builder.json().try_init(),
    };
    installed.map_err(|error| anyhow::anyhow!(error).context("installing the tracing subscriber"))
}

/// `<stream> open <owner> | <stream> deposit <cents> | <stream> withdraw <cents>`
fn parse_args(args: impl Iterator<Item = OsString>) -> anyhow::Result<(StreamId, Command)> {
    let args: Vec<String> = args
        .map(|arg| {
            arg.into_string()
                .map_err(|raw| anyhow::anyhow!("non-UTF-8 argument {}", raw.display()))
        })
        .collect::<Result<_, _>>()?;
    let Ok([stream, verb, value]) = <[String; 3]>::try_from(args) else {
        bail!("usage: ledger <stream> <open|deposit|withdraw> <value>");
    };
    let stream = StreamId::parse(stream)?;
    let command = match verb.as_str() {
        "open" => Command::Open(value),
        "deposit" => Command::Deposit(value.parse().context("amount in cents")?),
        "withdraw" => Command::Withdraw(value.parse().context("amount in cents")?),
        other => bail!("unknown verb {other:?}"),
    };
    Ok((stream, command))
}
```
Rejections (business outcomes) and failures (infrastructure) get different exit codes. `anyhow` appears only here (ERR-06). Tracing is initialized here and nowhere else (OUT-08).
**Don't.**
```rust,ignore
fn main() {
    let dir = std::env::var("DIR").unwrap();                        // H-02
    let balance: u64 = fs::read_to_string(..).unwrap().parse().unwrap();
    if balance < 50 { println!("no"); std::process::exit(3); }      // business rule in main; clippy::exit
}
```
**Detect.** `main.rs` over about 100 lines; `if` on domain values in the binary; `std::process::exit` (denied by `clippy::exit`).

## Macros policy

### ARC-15 · Macros are a last resort
**Why.** Macros hide control flow and types from readers, rust-analyzer and error messages, and a `return` or `?` inside one surprises every caller. Functions, generics and traits cover almost every case with better diagnostics.
**Do.** Use a `macro_rules!` only for what the type system cannot express: repetitive trait impls over a list of types, or compile-time literal validation. Keep it private, document it, and make its expansion obvious.
**Don't.**
```rust,ignore
macro_rules! try_or_log { ($e:expr) => { match $e { Ok(v) => v, Err(e) => { warn!(%e); return; } } } } // hidden early return, discards error
```
**Detect.** New `macro_rules!` or proc-macro crates in a diff; macros containing `return`, `?`, `break` or `continue`. A new proc-macro crate is a crate-boundary change (H-16).

## xtask (CONSIDER)

For repo automation that outgrows a shell one-liner (codegen, schema export, release checks), add an `xtask` binary crate and an alias in `.cargo/config.toml`: `[alias] xtask = "run --package xtask --"`. It is ordinary Rust under the same lints, runs on every platform, and keeps build logic out of `build.rs`. Do not use it to wrap or skip the gate (H-12).

## Review checklist

- [ ] Does every member inherit `[workspace.package]`, `[workspace.dependencies]` and `[lints] workspace = true`? (ARC-01)
- [ ] Does `cargo tree -e normal -p <domain>` show no I/O, runtime or adapter crates? (ARC-02)
- [ ] Are ports defined in the consuming app crate, with no adapter types in their signatures? (ARC-03)
- [ ] Are decisions pure, with time, ids and randomness injected? (ARC-04)
- [ ] Is `lib.rs` only module declarations and deliberate re-exports? (ARC-05)
- [ ] No `mod.rs`, no `#[path]`, files under about 400 lines and functions under about 50? (ARC-06)
- [ ] Does any new crate satisfy a split criterion, and is it not a `utils`/`common`/`models` crate? (ARC-07)
- [ ] Does every touched crate have a current `AGENTS.md` charter? (ARC-08)
- [ ] No global mutable state, no module cycles, narrowest port injected? (ARC-09)
- [ ] Does behavior live with the data whose invariant it protects? (ARC-10)
- [ ] Is configuration parsed once in the binary into validated types? (ARC-11)
- [ ] Is every new dependency approved (H-16), declared in the workspace with minimal features, and clean under `cargo deny`? (ARC-12)
- [ ] Decider shape: past-tense events, total `evolve`, `Rejection` separate from infra errors, versioned events? (ARC-13)
- [ ] Is the binary only parsing, wiring, running and mapping exit codes? (ARC-14)
- [ ] Is every new macro something functions, generics or traits cannot do, with no hidden control flow? (ARC-15)
