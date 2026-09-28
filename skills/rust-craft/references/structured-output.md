# Structured output

Serde contracts, DTOs, schema versioning, deterministic output, CLI machine mode, JSON Schemas, `tracing`, secrets, and `Display` vs `Debug`. Read this before adding a serde derive to a public or persisted type, changing a wire or file format, writing CLI output, adding logging, or handling credentials or personal data.

Rules covered: OUT-01, OUT-02, OUT-03, OUT-04, OUT-05, OUT-06, OUT-07, OUT-08, OUT-09, OUT-10

## DTOs vs domain types

### OUT-01 · Separate wire types from domain types
**Why.** The wire shape is a contract with outsiders and changes on their schedule; the domain type encodes invariants and changes on yours. One type serving both either leaks internals into the contract or weakens invariants to fit the wire.
**Do.** A DTO per external shape (request, response, file, event record), converted with `From`/`TryFrom`. Reuse the domain type directly only when the shapes are identical and neither must version independently (the decider's `Event` in `architecture.md` is one such case: it is the persisted contract).
**Don't.**
```rust,ignore
#[derive(Serialize, Deserialize)]
pub struct User { pub email: String, pub password_hash: String, pub is_admin: bool } // domain type is the API: hash leaks out, is_admin settable from JSON
```
**Detect.** `Deserialize` on types with private invariants; `pub` fields on types that also have validating constructors; adapter DTOs (rows, SDK types) in app or domain signatures (ARC-03).

### OUT-05 · Validate on the way in
**Why.** `Deserialize` checks shape, not meaning. A derived `Deserialize` on a newtype bypasses its constructor, so "validated" types would hold unvalidated data.
**Do.** Deserialize into a raw DTO, then `TryFrom` into the domain type. The domain type has no `Deserialize`.
```rust
//! Wire DTO in, validated domain type out.

use schemars::JsonSchema;
use serde::Deserialize;
use thiserror::Error;

/// Inbound wire shape. Mirrors the JSON exactly; trusts nothing.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateUserRequest {
    /// Email address as sent by the client.
    pub email: String,
    /// Display name as sent by the client.
    pub display_name: String,
    /// Optional age in years.
    #[serde(default)]
    pub age_years: Option<u16>,
}

/// Domain value: exists only if valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUser {
    email: Email,
    display_name: String,
    age_years: Option<u16>,
}

impl NewUser {
    /// The validated email.
    #[must_use]
    pub fn email(&self) -> &Email {
        &self.email
    }

    /// The trimmed, non-empty display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Age in years, if given.
    #[must_use]
    pub fn age_years(&self) -> Option<u16> {
        self.age_years
    }
}

/// An email address with one `@` and non-empty local and domain parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    /// The address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a request could not become a [`NewUser`].
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidUser {
    /// Email is not `local@domain`.
    #[error("email {0:?} is not of the form local@domain")]
    Email(String),
    /// Display name is blank or longer than 80 characters.
    #[error("display name must be 1-80 characters")]
    DisplayName,
    /// Age is outside 13..=150.
    #[error("age {0} is outside 13..=150")]
    Age(u16),
}

impl TryFrom<CreateUserRequest> for NewUser {
    type Error = InvalidUser;

    fn try_from(dto: CreateUserRequest) -> Result<Self, InvalidUser> {
        let email = match dto.email.split_once('@') {
            Some((local, domain))
                if !local.is_empty() && !domain.is_empty() && !domain.contains('@') =>
            {
                Email(dto.email)
            }
            _ => return Err(InvalidUser::Email(dto.email)),
        };
        let display_name = dto.display_name.trim();
        if display_name.is_empty() || display_name.chars().count() > 80 {
            return Err(InvalidUser::DisplayName);
        }
        if let Some(age) = dto.age_years.filter(|age| !(13..=150).contains(age)) {
            return Err(InvalidUser::Age(age));
        }
        Ok(Self {
            email,
            // Allocation: trimming produced a borrow of the DTO we are consuming.
            display_name: display_name.to_owned(),
            age_years: dto.age_years,
        })
    }
}
```
**Don't.**
```rust,ignore
#[derive(Deserialize)]
pub struct Email(String);            // derive skips the '@' check: any string deserializes
```
**Detect.** `#[derive(Deserialize)]` on a type with a `new`/`parse`/`TryFrom` constructor. If a validated type must deserialize directly, use `#[serde(try_from = "String")]` so serde calls the constructor.

## Serde conventions

### OUT-02 · Pin the conventions on every public contract
**Why.** Serde defaults (Rust field names, externally tagged enums, unknown fields ignored) are accidents that become contracts once someone depends on them. Ignoring unknown fields turns a misspelled config key into a silently default value.
**Do.**
```rust
//! Serde conventions pinned on a public contract.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Job settings as read from a config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobSpec {
    /// Job name.
    pub name: String,
    /// Retry policy; absent means "never retry", so `default` carries meaning.
    #[serde(default)]
    pub retries: u8,
    /// Per-attempt timeout, as integer milliseconds on the wire.
    #[serde(with = "duration_ms")]
    pub timeout: Duration,
    /// Free-form label, omitted from output when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What the job does.
    pub action: Action,
}

/// Closed set of job actions, internally tagged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Fetch a URL.
    Fetch {
        /// Target URL.
        url: String,
    },
    /// Remove files older than `days`.
    Prune {
        /// Age threshold in days.
        days: u32,
    },
}

/// `Duration` as whole milliseconds (`u64`), rejecting values that do not fit.
mod duration_ms {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer, de::Error as _, ser::Error as _};

    pub(super) fn serialize<S: Serializer>(
        value: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let millis = u64::try_from(value.as_millis())
            // TryFromIntError adds nothing to this message (ERR-04)
            .map_err(|_overflow| S::Error::custom("duration exceeds u64 milliseconds"))?;
        serializer.serialize_u64(millis)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        u64::deserialize(deserializer)
            .map(Duration::from_millis)
            .map_err(|e| D::Error::custom(format_args!("timeout in milliseconds: {e}")))
    }
}
```
**Don't.**
```rust,ignore
#[derive(Deserialize)]
pub struct JobSpec {
    pub retrys: u8,                  // typo; without deny_unknown_fields "retries": 3 is silently ignored
    #[serde(default)] pub name: String,   // default where absence is an error: empty name accepted
    pub timeout: Duration,           // serde's {"secs":..,"nanos":..} shape becomes the contract
}
```
**Detect.** Public serde types without `rename_all`; enums without an explicit `tag`/`untagged` choice; inbound command or config types without `deny_unknown_fields`; `#[serde(default)]` on required fields.

| Attribute | Effect | Use |
|---|---|---|
| `rename_all = "camelCase"` / `"snake_case"` | Field or variant names on the wire | Every public type; pick one case per contract |
| (none) externally tagged | `{"A":{"x":1}}` | Rarely; serde default, awkward for most consumers |
| `tag = "t"` internally tagged | `{"t":"A","x":1}` | Default for struct variants. Not for tuple variants holding primitives |
| `tag = "t", content = "c"` adjacent | `{"t":"A","c":{"x":1}}` | Variants holding non-objects, or when payload must stay separate |
| `untagged` | `{"x":1}` | Only for accepting legacy shapes; first matching variant wins, errors are vague |
| `deny_unknown_fields` | Unknown keys are an error | Inbound commands, config. Not with `flatten`. Omit on read models that must tolerate newer producers |
| `default` / `default = "path"` | Missing field takes `Default` / the fn's value | Only where absence has a meaning |
| `skip_serializing_if = "Option::is_none"` | Omits the field on output | Optional output fields; pair with `default` so it round-trips |
| `with = "module"` | Custom `serialize`/`deserialize` fns for one field | Units (`Duration` as ms), formats (timestamps) |
| `serialize_with` / `deserialize_with` | One direction only | Asymmetric formats |
| `try_from = "Raw"` / `into = "Raw"` | Route through a conversion | Validated newtypes that must be serde types |

## Versioning and upcasting

### OUT-03 · Persisted and public schemas carry a version
**Why.** Stored bytes outlive the code that wrote them. Without a version the reader cannot tell an old document from a corrupt one, and every change becomes a guess.
**Do.** Evolve additively (new optional fields, new variants). For a breaking change, freeze the old shape as a private type, add an upcast function, and always write the current version. Reject unknown future versions explicitly.
```rust
//! Versioned envelope with upcasting on read.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version written by this build.
pub const CURRENT_VERSION: u32 = 2;

/// Current shape (v2): `name` split into `display_name`, `email_verified` added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    /// Name shown in the UI.
    pub display_name: String,
    /// Contact address.
    pub email: String,
    /// Added in v2; v1 documents never verified an address.
    pub email_verified: bool,
}

/// Frozen v1 shape. Never edit: it describes bytes already on disk.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileV1 {
    name: String,
    email: String,
}

fn upcast_v1(v1: ProfileV1) -> Profile {
    Profile {
        display_name: v1.name,
        email: v1.email,
        email_verified: false,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeIn {
    version: u32,
    data: serde_json::Value,
}

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    version: u32,
    data: &'a Profile,
}

/// Failure to read a stored profile.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReadError {
    /// Envelope or payload is malformed.
    #[error("malformed profile document")]
    Malformed(#[from] serde_json::Error),
    /// Written by a newer build, or never existed.
    #[error("unsupported profile version {0}")]
    UnsupportedVersion(u32),
}

/// Read any supported version and upcast it to the current shape.
///
/// # Errors
///
/// Returns [`ReadError`] for malformed JSON or an unknown version.
pub fn read_profile(json: &str) -> Result<Profile, ReadError> {
    let envelope: EnvelopeIn = serde_json::from_str(json)?;
    match envelope.version {
        1 => Ok(upcast_v1(serde_json::from_value(envelope.data)?)),
        CURRENT_VERSION => Ok(serde_json::from_value(envelope.data)?),
        other => Err(ReadError::UnsupportedVersion(other)),
    }
}

/// Always write the current version.
///
/// # Errors
///
/// Returns a serialization error (not expected for this type).
pub fn write_profile(profile: &Profile) -> Result<String, serde_json::Error> {
    serde_json::to_string(&EnvelopeOut {
        version: CURRENT_VERSION,
        data: profile,
    })
}
```
**Don't.**
```rust,ignore
pub struct Profile { pub display_name: String, pub email: String } // renamed `name` in place: every stored v1 document now fails to load
```
**Detect.** Edits to a type that is persisted or published without a version bump and upcaster; `#[serde(alias)]` used as an unversioned migration. A schema or data migration is a HITL decision (H-16).

## Deterministic output

### OUT-04 · Same input, same bytes
**Why.** Nondeterministic output breaks snapshot tests, caching, diffing and reproducible builds. `HashMap` iteration order is randomized per process, and `now()` or random ids inside a function make it untestable.
**Do.**
```rust
//! Deterministic output: ordered maps, sorted collections, injected time.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

/// Word-frequency report. Byte-identical for identical input.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report<'a> {
    /// Injected by the caller; never `SystemTime::now()` in here.
    pub generated_at_unix: u64,
    /// `BTreeMap` serializes in key order; `HashMap` would not.
    pub counts: BTreeMap<&'a str, u64>,
    /// Sorted, deduplicated.
    pub longest: Vec<&'a str>,
}

/// Build a report from text. `now_unix` comes from the injected clock.
#[must_use]
pub fn report(text: &str, now_unix: u64) -> Report<'_> {
    // HashMap is fine for counting; order is fixed when converting to output.
    let mut counts: HashMap<&str, u64> = HashMap::new();
    for word in text.split_whitespace() {
        let count = counts.entry(word).or_default();
        *count = count.saturating_add(1);
    }
    let max_len = counts.keys().map(|w| w.len()).max().unwrap_or(0);
    let mut longest: Vec<&str> = counts
        .keys()
        .copied()
        .filter(|w| w.len() == max_len)
        .collect();
    longest.sort_unstable();
    Report {
        generated_at_unix: now_unix,
        counts: counts.into_iter().collect(),
        longest,
    }
}
```
**Don't.**
```rust,ignore
#[derive(Serialize)]
struct Report { counts: HashMap<String, u64>, generated_at: SystemTime } // order varies per run; clock read inside
```
**Detect.** `HashMap`/`HashSet` in `Serialize` types; `SystemTime::now`, `Uuid::new_v4`, `rand` inside output-producing functions; snapshot tests that flake.

## CLI output modes

### OUT-06 · Machine mode, clean streams, meaningful exit codes
**Why.** Scripts and agents parse stdout and branch on exit codes. Diagnostics on stdout corrupt the data stream; a single exit code for "partial success" and "bad usage" forces callers to parse text.

House exit-code convention (use it unless the tool's consumers already expect another):

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | failure (infrastructure, unexpected) |
| 2 | usage error |
| 3 | the domain refused the request, or partial success |
| 4 | the failure could not even be reported (stderr closed) |

When a supervisor or shell script needs finer classes, use sysexits(3) (64 usage, 65 data, 66 no input, 74 I/O, 78 config) for *every* class instead, as errors.md does. Don't mix the two schemes. Document the table in the binary's `--help` and its crate charter.

Map outcomes to codes with an exhaustive `match`, never `as` on a discriminant (H-11): a new variant then fails to compile until it gets a code.
```rust
//! Exit codes as a closed set, mapped without casts.

use std::process::ExitCode;

/// How a run ended; one variant per row of the exit-code table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Everything succeeded.
    Success,
    /// Infrastructure or unexpected failure.
    Failure,
    /// The command line was invalid.
    Usage,
    /// The domain refused the request, or only part of it succeeded.
    Refused,
    /// The failure could not be reported (stderr closed).
    Unreported,
}

impl From<Outcome> for u8 {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Success => 0,
            Outcome::Failure => 1,
            Outcome::Usage => 2,
            Outcome::Refused => 3,
            Outcome::Unreported => 4,
        }
    }
}

impl From<Outcome> for ExitCode {
    fn from(outcome: Outcome) -> Self {
        Self::from(u8::from(outcome))
    }
}
```
```rust,ignore,expect=scan:as-cast
ExitCode::from(Outcome::Usage as u8) // denied by clippy::as_conversions; reordering the enum would change the contract
```
**Do.** Data to stdout through one locked, buffered writer. Diagnostics to stderr through `tracing`. JSON Lines (one object per line) for streams, so consumers can process records before the run ends. Distinct `ExitCode`s for success, partial failure and usage errors. `clap` with `value_enum` is fine for real CLIs; this example hand-parses to stay dependency-light.
```rust
//! `filestat [--format human|json] <path>...`: line and byte counts per file.
//! Data goes to stdout (JSON Lines in machine mode); diagnostics go to stderr.

use std::borrow::Cow;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

// Exit codes (house convention, OUT-06): 0 ok, 1 failure, 2 usage,
// 3 partial success, 4 the failure could not even be reported.
const EXIT_USAGE: u8 = 2;
const EXIT_PARTIAL: u8 = 3;
const EXIT_UNREPORTED: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Human,
    Json,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileStat<'a> {
    /// Lossy for non-UTF-8 paths: serde refuses to serialize those as `Path`.
    path: Cow<'a, str>,
    lines: usize,
    bytes: usize,
}

fn main() -> ExitCode {
    let installed = tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::WARN.into())
                .from_env_lossy(),
        )
        .try_init(); // `init` would panic if a subscriber were already set (H-03)
    if let Err(error) = installed {
        return report(
            &format!("error: installing the log subscriber: {error}"),
            ExitCode::FAILURE,
        );
    }
    let Some((format, paths)) = parse_args(std::env::args_os().skip(1)) else {
        return report(
            "usage: filestat [--format human|json] <path>...",
            ExitCode::from(EXIT_USAGE),
        );
    };
    match run(format, &paths) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(EXIT_PARTIAL),
        // Broken pipe or full disk on stdout: stderr may still work.
        Err(error) => report(
            &format!("error: writing output failed: {error}"),
            ExitCode::FAILURE,
        ),
    }
}

/// Fatal messages go straight to stderr: they must not depend on logging.
fn report(message: &str, code: ExitCode) -> ExitCode {
    if writeln!(io::stderr().lock(), "{message}").is_err() {
        return ExitCode::from(EXIT_UNREPORTED);
    }
    code
}

fn parse_args(mut args: impl Iterator<Item = OsString>) -> Option<(Format, Vec<PathBuf>)> {
    let mut format = Format::Human;
    let mut paths = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--format" {
            format = match args.next()?.to_str()? {
                "human" => Format::Human,
                "json" => Format::Json,
                _ => return None,
            };
        } else {
            paths.push(PathBuf::from(arg));
        }
    }
    (!paths.is_empty()).then_some((format, paths))
}

/// Returns `Ok(false)` when some inputs were skipped; `Err` only for stdout failures.
fn run(format: Format, paths: &[PathBuf]) -> io::Result<bool> {
    let mut out = BufWriter::new(io::stdout().lock());
    let mut all_ok = true;
    for path in paths {
        let (lines, bytes) = match count(path) {
            Ok(counts) => counts,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "skipping unreadable file");
                all_ok = false;
                continue;
            }
        };
        let stat = FileStat {
            path: path.to_string_lossy(),
            lines,
            bytes,
        };
        match format {
            // JSON Lines: one complete object per line, streamable.
            Format::Json => serde_json::to_writer(&mut out, &stat)?,
            Format::Human => write!(
                out,
                "{:>8} {:>10}  {}",
                stat.lines,
                stat.bytes,
                path.display()
            )?,
        }
        writeln!(out)?;
    }
    out.flush()?;
    Ok(all_ok)
}

/// Streams the file in buffer-sized chunks: memory stays bounded for any size.
fn count(path: &Path) -> io::Result<(usize, usize)> {
    let mut reader = BufReader::new(File::open(path)?);
    let (mut lines, mut bytes) = (0_usize, 0_usize);
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok((lines, bytes));
        }
        lines = lines.saturating_add(
            chunk
                .iter()
                .map(|&b| usize::from(b == b'\n'))
                .sum::<usize>(),
        );
        bytes = bytes.saturating_add(chunk.len());
        let consumed = chunk.len();
        reader.consume(consumed);
    }
}
```
**Don't.**
```rust,ignore
println!("Processing {path}...");                // progress on stdout corrupts JSON output
println!("{}", serde_json::to_string_pretty(&all)?); // whole-run buffering, pretty JSON: not streamable
std::process::exit(1);                           // skips destructors and buffered writer flush
```
**Detect.** `print!`/`println!` anywhere (denied by `clippy::print_stdout`); progress or log text on stdout; `std::process::exit`; one array printed at the end of a long stream.

## Schemas

### OUT-07 · Generate and snapshot JSON Schemas
**Why.** A snapshot of the generated schema turns an accidental contract change (renamed field, new required field, loosened type) into a failing test with a readable diff.
**Do.** Derive `schemars::JsonSchema` on public DTOs (as `CreateUserRequest` above) and snapshot the schema with `insta`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_request_converts() {
        let dto: CreateUserRequest =
            serde_json::from_str(r#"{"email":"a@b.io","displayName":" Ada "}"#)
                .expect("valid json");
        let user = NewUser::try_from(dto).expect("valid user");
        assert_eq!(user.display_name(), "Ada");
    }

    #[test]
    fn unknown_fields_are_refused() {
        let error = serde_json::from_str::<CreateUserRequest>(
            r#"{"email":"a@b.io","displayName":"Ada","isAdmin":true}"#,
        )
        .expect_err("isAdmin is not a field");
        assert_eq!(error.classify(), serde_json::error::Category::Data);
    }

    #[test]
    fn schema_is_stable() {
        insta::assert_json_snapshot!(schemars::schema_for!(CreateUserRequest));
    }
}
```
Review the `.snap` diff like an API change. Accepting a changed schema snapshot is a public API change: HITL (H-16), never a reflexive `cargo insta accept` (H-12).
**Don't.** Hand-written schema files that drift from the types; no schema at all for a published contract.
**Detect.** Public or persisted serde types without `JsonSchema` and a snapshot test; `.snap` files changed in a diff without a mention in the plan.

## Observability with tracing

### OUT-08 · Structured, span-scoped, secret-free
**Why.** Structured fields are queryable; interpolated strings are not. Spans give every event its context (request, user, attempt) without threading it through arguments. Stable event names and standard field names let dashboards and alerts survive a reworded message. A library that installs a subscriber or prints fights the application for control of output.
**Do.**
- Libraries emit spans and events only. The binary installs the subscriber once (`tracing_subscriber::fmt().with_env_filter(..).with_writer(io::stderr)`, plus `.json()` for machine consumers), as in `architecture.md` §Thin binaries and the CLI above.
- Filter with `EnvFilter` (`RUST_LOG=ledger=debug,warn`), with a default directive.
- `#[instrument(skip_all, fields(...))]`: skip everything, then record chosen safe fields. Never rely on the default, which records every argument with `Debug`.
- `%value` records with `Display`, `?value` with `Debug`. Prefer `%` for ids and errors.
- Spans for units of work with duration (request, job, command); events for things that happen at a point (decision taken, retry, failure).
- Events that operators query carry a stable `name:` in `<component>.<operation>.<outcome>` form. Fields use the OpenTelemetry semantic-convention key where one exists: `user.id`, `http.request.method`, `http.response.status_code`, `url.path`, `db.operation.name`, `file.path`, `error.type`. Keep the message plain text; `{field}` templates print literally.
- Hot loops log per batch, not per item: count inside the loop and emit one event with the totals. Never put a `format!` result in a field.
```rust
//! Library-side instrumentation: spans and events only, no subscriber.

use tracing::{debug, info, instrument, warn};
#
# /// Stand-in for the redacting `Secret` in the next section.
# #[derive(Debug)]
# pub struct Secret(String);

/// A login attempt.
#[derive(Debug)]
pub struct Login {
    /// Stable account id: safe to log, unlike the name (PII).
    pub user_id: u64,
    /// Account name.
    pub user: String,
    /// Password; redacted in `Debug`.
    pub password: Secret,
}

/// Port: checks a password against the stored hash in constant time.
pub trait Verifier {
    /// Whether `password` is correct for `user`.
    fn verify(&self, user: &str, password: &Secret) -> bool;
}

/// Outcome of [`authenticate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOutcome {
    /// Credentials matched.
    Granted,
    /// Credentials did not match.
    Denied,
}

/// Check a login. `skip_all` records no arguments; `fields` adds only safe ones.
#[instrument(skip_all, fields(user.id = login.user_id, attempt = attempt))]
pub fn authenticate(login: &Login, verifier: &impl Verifier, attempt: u32) -> AuthOutcome {
    debug!("verifying credentials");
    if verifier.verify(&login.user, &login.password) {
        info!(name: "auth.login.granted", "login succeeded");
        AuthOutcome::Granted
    } else {
        warn!(name: "auth.login.denied", "login failed");
        AuthOutcome::Denied
    }
}

/// Import rows, logging once per batch however many rows there are.
pub fn import(rows: &[&str]) -> usize {
    let rejected = rows.iter().filter(|row| row.trim().is_empty()).count();
    info!(name: "import.batch.completed", rows = rows.len(), rejected, "batch imported");
    rows.len().saturating_sub(rejected)
}
```
**Don't.**
```rust,ignore
#[instrument]                                       // records `login` with Debug; any new field leaks
pub fn authenticate(login: &Login) -> AuthOutcome {
    println!("login attempt for {} with {}", login.user, login.password.expose()); // print in a library; secret in output
    info!("user {} failed", login.user);            // interpolated, not a field
}
```
**Detect.** `#[instrument]` without `skip_all` on functions taking credentials or large values; `tracing_subscriber` in a library's `[dependencies]`; `info!("..{}..", x)` with interpolated values instead of fields; operator-facing events without `name:`; an event inside a per-item loop; `print`/`eprint` macros (denied).

## Secrets and PII

Wrap credentials, tokens and personal data in a newtype whose `Debug` redacts and which implements neither `Display` nor `Serialize`. Reading the value requires a named method (`expose`) that is easy to grep in review. A unit test proves the redaction, because a later switch to `#[derive(Debug)]` would silently undo it. If a secret must be persisted, write a dedicated DTO field with an explicit `serialize_with`, never a blanket derive.

```rust
//! Secrets that cannot leak through `Debug`, logs or serialization.

use std::fmt;

/// A credential. `Debug` redacts; no `Display`, no `Serialize`, no `Clone`
/// (one copy to drop), and no `PartialEq` (comparing secrets needs a
/// constant-time verifier).
pub struct Secret(String);

impl Secret {
    /// Wrap a credential.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The one deliberate, greppable way to read the value.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

/// Connection settings: safe to `{:?}` and to log. Not `Clone`: it owns a secret.
#[derive(Debug)]
pub struct DbSettings {
    /// Host name.
    pub host: String,
    /// Password, redacted in `Debug`.
    pub password: Secret,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_secret() {
        let raw = "hunter2-7f3a";
        let settings = DbSettings {
            host: "db.internal".to_owned(),
            password: Secret::new(raw.to_owned()),
        };

        let rendered = format!("{settings:?}");

        assert!(!rendered.contains(raw), "leaked: {rendered}");
        assert_eq!(
            rendered,
            r#"DbSettings { host: "db.internal", password: Secret([redacted]) }"#
        );
    }
}
```

Do not derive `Debug` on a struct holding a raw `String` password. A hand-written `Debug` is never empty: when it hides fields, list the safe ones and end with `finish_non_exhaustive()` (`Session { user_id: 7, .. }`), which also keeps `clippy::missing_fields_in_debug` quiet. For PII (emails, names, addresses) in logs, record a stable id instead of the value.

## Display vs Debug

### OUT-10 · `Display` for users, `Debug` for developers
**Why.** `Debug` output is unstable (derive formats may change, fields get added) and may expose internals. Parsing it, or showing it to users, couples them to implementation details.
**Do.** Implement `Display` for types users see (errors via `thiserror`'s `#[error]`, ids, quantities); derive `Debug` everywhere (API-06), redacting where needed. Provide `FromStr` as the inverse of `Display` when a value round-trips through text: that pair is the type's own canonical text form (`acc-42`, `12.50 EUR`). Machine-readable output goes through `Serialize`/`Deserialize`, so the caller picks the format; never make `Display` or `FromStr` speak JSON or CSV.
**Don't.**
```rust,ignore
let id: u64 = format!("{order:?}").trim_start_matches("OrderId(").trim_end_matches(')').parse()?; // parsing Debug
eprintln!("error: {err:?}");                                                                       // Debug shown to users
impl fmt::Display for Order { /* serde_json::to_string(self) */ }                                 // a wire format posing as text
```
**Detect.** `format!("{:?}")` whose result is parsed, compared or shown in UI; user-facing messages built from `{:?}`; a `Display` impl that calls a serializer. `clippy::to_string_trait_impl` denies `impl ToString`. In binaries, report errors with `{err:#}` (anyhow chain), not `{err:?}`.

## Zero-copy deserialization

### OUT-09 · Borrow from the input for large or hot inputs (CONSIDER)
**Why.** Deserializing into `String` allocates per field. For large documents or hot loops, borrowing from the input buffer removes those allocations; `Cow` falls back to owning only when the text contains escapes.
**Do.**
```rust
//! Zero-copy deserialization: borrow from the input buffer when possible.

use std::borrow::Cow;

use serde::Deserialize;

/// One parsed log record, borrowing from the source text.
#[derive(Debug, Deserialize)]
pub struct Record<'src> {
    /// Level names never contain escapes, so a plain borrow is safe.
    pub level: &'src str,
    /// Messages may contain escapes (`\n`, `\"`): borrow when possible, own when not.
    #[serde(borrow)]
    pub message: Cow<'src, str>,
}

/// Parse one JSON line without copying unescaped strings.
///
/// # Errors
///
/// Returns a `serde_json` error for malformed input, or when `level`
/// contains an escape sequence (a `&str` cannot hold unescaped text).
pub fn parse(line: &str) -> Result<Record<'_>, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrows_plain_strings_and_owns_escaped_ones() {
        let plain = parse(r#"{"level":"info","message":"ready"}"#).expect("valid");
        assert!(matches!(plain.message, Cow::Borrowed("ready")));
        let escaped = parse(r#"{"level":"info","message":"a\nb"}"#).expect("valid");
        assert!(matches!(escaped.message, Cow::Owned(_)));
    }
}
```
Use `&'de str` only for fields that can never contain escapes; use `Cow<'de, str>` with `#[serde(borrow)]` otherwise (serde does not borrow `Cow` without the attribute). The input buffer must outlive the records, so this suits parse-then-process pipelines, not long-lived storage. Measure first (PERF): most inputs do not need it.
**Don't.**
```rust,ignore
#[derive(Deserialize)]
struct Record<'a> { message: &'a str } // fails at runtime on any escaped message such as "a\nb"
```
**Detect.** `&'a str` fields in `Deserialize` types that hold free text; `Cow` fields without `#[serde(borrow)]` (always owned).

## Review checklist

- [ ] Are wire types separate from domain types where shapes or versioning differ, converted with `From`/`TryFrom`? (OUT-01)
- [ ] Do public serde types pin `rename_all`, enum tagging and `deny_unknown_fields` on inbound types, with `default` only where absence means something? (OUT-02)
- [ ] Do persisted and public formats carry a version, with an upcaster for each old version? (OUT-03)
- [ ] Is output deterministic: `BTreeMap`, sorted collections, injected time and ids? (OUT-04)
- [ ] Is external data deserialized into a DTO and validated by `TryFrom`, with no `Deserialize` on validated types? (OUT-05)
- [ ] Does the CLI offer JSON output on stdout, diagnostics on stderr, JSON Lines for streams, and distinct exit codes mapped with a `match`, not `as` (H-11)? (OUT-06)
- [ ] Are public schemas generated with `schemars` and snapshot-tested? (OUT-07)
- [ ] `tracing` only, subscriber in the binary, `skip_all` plus chosen fields, named events with standard keys, one event per batch, no secrets or PII in logs? (OUT-08)
- [ ] Does every secret type have a non-empty redacting `Debug` with a unit test, no `Serialize` and no `PartialEq`? (OUT-08)
- [ ] For large or hot inputs, is borrowing considered and `Cow` fields marked `#[serde(borrow)]`? (OUT-09)
- [ ] Is `Display` used for users and `Debug` never parsed or shown to users, with serialization only through serde? (OUT-10)
