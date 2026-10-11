# Process and craft

How Rust work gets done here: exploring, planning, managing context, deciding what needs a human, taking small steps, self-review, reporting, and the craftsmanship rules that apply to every line. The technical references say what good code is. This one says how an agent reliably produces it.

Rules covered: PROC-01..PROC-13, CRF-01..CRF-11, H-15, H-16.

## The loop at a glance

```text
Orient -> Frame -> Plan -> [Red -> Green -> Refactor]* -> Self-review -> Gate -> Independent review -> Report
                     ^                                                                  |
                     +---------------------- changes-requested -------------------------+
```

Every phase has an exit criterion (SKILL.md §2). Skipping a phase doesn't save time. It moves the cost into review rounds, which are the most expensive loop there is.

## Process

### PROC-01 · Explore before editing

**Why.** Most bad Rust from agents comes from writing before knowing: the wrong crate, a duplicate helper, an invented dependency API, or a signature that fights existing ownership.

**Do.** Answer these before writing code, and keep the answers in the plan:

| Question | How to answer it |
|---|---|
| What are the crates and layers? | Root `AGENTS.md`; `cargo metadata --no-deps --format-version 1` |
| Which crate owns this concern? | The crate's `AGENTS.md` charter (purpose, layer, invariants) |
| What is the public surface today? | That crate's `lib.rs` (re-exports); `cargo doc --no-deps -p <crate>` |
| How is it used? | Call sites: search for the type or function name across `crates/` |
| What do the tests say it does? | Tests beside the code and under `tests/` |
| What does the dependency API actually look like? | The locked version in `Cargo.lock`, read from source in `~/.cargo/registry/src/*/<crate>-<version>/` or on docs.rs for that version |
| Which features, defaults and `rust-version` does a dependency have? | `cargo info <crate>@<locked version>` |
| What does the dependency graph allow? | `cargo tree -e normal -p <crate> --depth 1` |
| Why is this crate or feature in the build? | `cargo tree --workspace --target all -e features -i <crate>` |
| Can this call panic or fail on my input? | The item's `# Panics` and `# Errors` sections, and its source when they are vague. `Vec` and `String` methods are documented on the `[T]` and `str` pages; `char` and integer methods on the primitive pages. |

Read `# Panics` for every call on data the code does not control. A documented panic condition that input can reach is an H-03 finding unless a type rules it out; escape-hatches.md lists the common ones.

**Don't.** Write against a dependency API or its feature names from memory. Crates change between versions (for example, `thiserror` 1 to 2, `schemars` 0.8 to 1, `rand` 0.8 to 0.9). Check the version first.

### PROC-02 · Context management

Your context window is working memory. What fills it with noise pushes out the rules and the plan.

- **Delegate wide searches.** "Find every place that constructs `Order`" goes to an exploration subagent, which returns the list and a two-line conclusion. You never see the forty files it read.
- **Keep command output short.** Use `cargo clippy --message-format short`, `cargo test -q`, `cargo test -p <crate> <filter>` for the unit you're on, and `2>&1 | tail -n 40` for long logs. The gate prints only failures, with a path to the full log.
- **Read surgically.** Read the function you need, with line ranges for large files. Don't re-read a file you just edited to "check".
- **Externalize state.** The plan file is your durable memory (PROC-06). Decisions, the test-list checklist and open questions live there, not only in the conversation, so compaction or a handoff loses nothing.
- **Re-anchor after compaction.** When a session continues after compaction, re-read the plan file and the relevant crate charter before the next edit. The temper `SessionStart` hook re-injects the Iron rules and the current gate and review state.

### PROC-03 · Plan template

Write the plan to `.temper/plans/<kebab-slug>.md` (the directory is gitignored). Plan anything over about 30 lines, any new type, module or crate, any public API change, and any cross-crate change. For a trivial fix, the plan is the first three sections, in five lines.

```markdown
# <Title>

## Goal
<One or two sentences: the behavior the user gets.>

## Constraints and non-goals
- <e.g. no new dependencies; must stay runtime-agnostic; not changing the wire format>

## Invariants
- <What must always hold, stated as testable facts.>

## Design
- Placement: <crate::module>, because <layer/charter reason> (ARC-02, ARC-05)
- Types: <e.g. `pub struct OrderId(Uuid)` (API-02); `impl TryFrom<&str> for OrderId` with `type Error = ParseIdError` (API-03)>
- Signatures (ownership decided): <fn ... (&self, lines: &[Line]) -> Result<Total, PricingError>> (OWN-01)
- Errors: <type, variants, context fields> (ERR-01..04)
- Concurrency and resources: <if relevant: tasks, channels, bounds, timeouts> (ASY, H-13)
- Alternatives considered: <one line each, and why not>

## Test list (drive in this order)
- [ ] <behavior 1: the simplest happy path>
- [ ] <behavior 2: an edge case>
- [ ] <behavior 3: an error case -> specific variant>
- [ ] <property: round-trip or invariant> (TST-06)

## Decisions (H-16)
| Decision | Options | Recommendation | Decided by | Outcome |
|---|---|---|---|---|

## Risks
- <What could make this wrong, and how the tests catch it.>

## Progress
- <Updated as you go: what's done, what's next, anything learned.>
```

### H-16 · Human decisions

These are never decided silently:

| Decision | Why a human |
|---|---|
| Adding a dependency, or a major-version bump of one | Supply chain, license, compile time, breaking changes, and long-term maintenance cost |
| A `[patch]` override | It replaces published code for the whole graph |
| Raising `rust-version` | It drops users on older toolchains |
| Changing or breaking a public API | Downstream users and semver |
| Any lint exception (`#[expect]`) or `unsafe` | It suspends a guarantee |
| Creating, merging or re-layering crates | Architecture is expensive to reverse |
| Schema, persistence or event-format migrations | Data outlives code |
| Deleting a test or changing an expected value | A test is the spec. Changing it changes the requirement. |
| Performance-versus-clarity trade-offs users will notice | This is a product decision |

The temper hook asks the human on new dependencies, protected files and sensitive manifest keys (`[patch]`, `rust-version`, test switches), and refuses `--ignore-rust-version`. The plan and review catch the rest.

### PROC-04 · Surface human decisions

Never assume an H-16 decision. Frame each one as options, trade-offs and a recommendation:

```text
Decision needed (H-16): add `backon` for retries?
- A) Add backon 1.x: tested backoff and jitter, about 3 small transitive deps. Recommended.
- B) Hand-roll a 30-line retry with exponential backoff: no deps, but we own the edge cases.
- C) No retries: simplest, but transient 503s surface to users.
```

A main-thread agent asks with the question tool. A subagent returns `Status: BLOCKED` with the decision, and the orchestrator asks. Record the outcome in the plan's Decisions table, and for architecture-level decisions, in the crate charter or root `AGENTS.md` decisions log.

### PROC-05 · Small steps

- One behavior per red-green-refactor cycle. Build and run the focused test after every step.
- When a step fails in a way you don't understand, undo it and take a smaller step.
- When commits are in scope, commit at green, one behavior or refactor per commit, with a conventional message (`feat(ledger): reject overdraft withdrawals`). Never commit a red tree, and never use `--no-verify`.

### PROC-06 · Keep the plan current

Record each red run in Progress (the test name and its failure line), then tick the item when it goes green. This is the reviewer's evidence for TST-01. Add learnings to Progress, such as "the parser already normalizes whitespace, so dropped behavior 4". A reviewer, a resumed session or a human can pick up exactly where you are.

### PROC-07 · Self-review protocol

Before asking for review:
1. Run `git diff <base> --stat` and confirm nothing unrelated is in the diff (CRF-08).
2. Read the full diff as if a stranger wrote it. Work through the checklist in `review-rubric.md` section by section.
3. For every `.clone()`, `Arc`, `Mutex`, `#[expect]`, `pub` and new dependency in the diff, write its justification into your report. If you can't justify one, fix it.
4. Check that no test was removed or loosened.
5. Run the gate.

Fix what you find. A self-review that lists problems without fixing them is a report of known defects.

### PROC-08 · Independent review

Every change that reaches "done" gets a `temper:rust-reviewer` verdict for the exact tree fingerprint. Give the reviewer the goal and the plan path, not your interpretation. Its fresh context is the point. The `Stop` hook refuses to end a turn with Rust changes that lack an approving verdict for the current fingerprint. `Status: WIP` or `Status: BLOCKED` is the honest alternative when you can't finish.

### PROC-09 · When stuck

After two failed attempts at the same compiler error, stop editing:
1. Read the whole diagnostic, including every `note:` and `help:`, and run `rustc --explain E0XXX` (PROC-12).
2. Split a failing chain into one `let` per step, each with its type written out (`let names: Vec<&str> = ...;`). The error moves to the exact step rustc rejects. Recombine once it compiles.
3. Write down who owns each value involved and how long each borrow lives. Draw it as a list if that helps.
4. Find the symptom in ownership.md, "Fighting the borrow checker", and apply the shape it prescribes.
5. If the design itself is the problem (for example, two owners of the same mutable state), go back to the plan and change the design. Don't patch around it.

Random `.clone()`, `'static`, `Rc<RefCell<_>>`, `Box::leak` or lifetime annotations sprinkled until it compiles are escape hatches (H-07, H-08).

### PROC-10 · Status block

Every turn that changed Rust ends with this block:

```text
Status: DONE | WIP | BLOCKED
Gate: fmt ok | clippy ok | test ok (N passed) | doc ok   (or the failing step and its first error)
Review: approved | changes-requested | not yet requested
Changed: <crate/path> (+added/-removed), ...
Decisions needed: <H-16 items, or "none">
Declined: <each request you declined or did differently, with a one-sentence reason, or "none">
Not verified: <anything you could not check, or "none">
```

- `DONE` means the gate is green and review is approved for the current tree. The `Stop` hook verifies it.
- `WIP` means you are pausing mid-task. List what remains.
- `BLOCKED` means a human decision or missing information stops progress. Ask the question.

"Declined" restates every request you refused or carried out differently, with the reason, even if you explained it earlier in the turn. A user who reads only the final message must still see it. Example: `Declined: #[ignore] on the failing test; the code was wrong, not the test (H-12)`.

"Not verified" is where honesty lives. Put there anything you could not run (an integration test needing credentials, a platform you can't build for) and any assumption you made.

### PROC-11 · Docs in the same change

When a change alters a crate's purpose, public surface, dependencies or invariants, update its `AGENTS.md` charter. Update rustdoc for changed public items, the README for changed usage, and `CHANGELOG.md` if the repo has one.

A published crate keeps `CHANGELOG.md` with an `Unreleased` section that each change updates, and lists breaking changes under a `Breaking` heading. Every release gets an annotated tag (`git tag -a v1.4.0`), which is also the baseline for `cargo semver-checks` (API-17).

### PROC-12 · Read the diagnostic

rustc's errors usually contain the fix: `help: consider borrowing here`, `note: ...value moved here`. Read the whole message before editing. `rustc --explain E0502` gives a worked explanation. Clippy's messages link to the lint's documentation, which explains the reason and the fix.

An E0277 ("the trait bound `T: X` is not satisfied") for a type that visibly implements `X` usually means two semver-incompatible versions of the crate that defines `X` are in the graph, and the type implements the other one. Run `cargo tree -d`, then `cargo tree -i <crate>@<version>` for each copy. Align the versions (a major bump is H-16) or use the path the dependency re-exports. Don't rewrite correct code around the error.

### PROC-13 · Never edit `Cargo.lock` by hand

Change `Cargo.lock` only through cargo, resolve lockfile conflicts by re-running cargo, and run CI with `--locked`.

**Why.** Cargo computes the lockfile: the resolved graph, checksums and sources. A hand edit, or a hand-merged conflict, records a graph cargo never resolved, and the next build either rewrites it silently or fails far from the cause.

**Do.**
```text
cargo update -p serde --precise 1.0.228   # pin or roll back one crate
cargo update --workspace                  # after editing versions in Cargo.toml
cargo update --dry-run                    # preview what would change
git checkout <base> -- Cargo.lock && cargo check --workspace   # resolve a lockfile conflict
```

**Don't.** Write, `sed` or merge `Cargo.lock` by hand, or delete it to start fresh, which upgrades every dependency at once.

**Detect.** The temper pre-edit hook denies edits to `Cargo.lock`, shell redirects included. CI passes `--locked` to every cargo command, so a lockfile that no longer matches the manifests fails instead of being rewritten.

## Craftsmanship

### CRF-01 · Names

Use the domain's own words. The charter's vocabulary is the ubiquitous language. Types are nouns (`Invoice`, `LedgerEntry`), functions are verbs (`post_entry`, `reconcile`), predicates read as questions (`is_settled`), and conversions follow API-01 naming. Avoid `data`, `info`, `manager`, `helper`, `util`, `do_stuff`, `process` (unless the domain calls it that), and one-letter names outside tiny closures and generic parameters.

- **Acronyms are one word**: `HttpClient`, `Uuid`, `Stdin` in CamelCase, `is_xid_start` in snake_case. `clippy::upper_case_acronyms` runs in aggressive mode (`clippy.toml`), so `HTTPClient` fails the gate.
- **No `Manager`, `Factory`, `Helper` or `Util` types.** Name the role (`Bookings`, `BookingDispatcher`). A factory is a `FooBuilder` (API-10) or an `impl Fn() -> Foo` parameter. `Service` is fine for DDD application and domain services.
- **Names of one kind share a word order within a crate.** If the crate's errors are `ParseAddrError` and `LoadConfigError`, the next one is `WriteReportError`, not `ReportWriteError`.

Code ported from C# or Java carries shapes that compile but are not Rust:

| From C# or Java | Rust shape |
|---|---|
| Null checks, `ThrowIfNull(x)` | `Option<T>` in the type; `?` and `let ... else` |
| An `IFoo` interface per class | No trait until a second implementation exists (CRF-05), or a port owned by its consumer (ARC-03) |
| A static utility class, `Foo::helper(x)` | A free function in the module; associated functions without `self` construct `Self` |
| `FooManager`, `FooFactory` | The role noun, a `FooBuilder`, or an `impl Fn() -> Foo` parameter |
| `Arc` on every nested field | Owned fields, and one `Arc<Inner>` handle at the service level (API-23, patterns.md 29) |
| Getter and setter pairs | Behaviour on the type that keeps its invariant (ARC-10) |
| Exceptions for control flow | `Result` with typed errors (ERR-01) |

### CRF-02 · Function size and shape

One level of abstraction per function. When a block needs a comment to say what it does, it is a function with that name. Aim for about 50 lines and at most 4 parameters. Beyond them, look for an extraction or a params struct (API-10). Clippy's `too_many_lines` (60) and `too_many_arguments` (4) enforce the outer bound.

### CRF-03 · Flat control flow

```rust
use std::num::ParseIntError;

/// A parsed `name:weight` pair.
#[derive(Debug, PartialEq, Eq)]
pub struct Weighted<'src> {
    /// Item name.
    pub name: &'src str,
    /// Item weight.
    pub weight: u32,
}

/// Why a `name:weight` pair did not parse.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseWeightedError {
    /// No `:` separator.
    #[error("expected `name:weight`")]
    MissingColon,
    /// Nothing before the `:`.
    #[error("name is empty")]
    EmptyName,
    /// The weight is not a whole number.
    #[error("weight is not a whole number")]
    BadWeight(#[source] ParseIntError),
}

/// Parses `name:weight`.
///
/// # Errors
/// [`ParseWeightedError`] naming the malformed part.
pub fn parse_weighted(input: &str) -> Result<Weighted<'_>, ParseWeightedError> {
    let (name, weight) = input.split_once(':').ok_or(ParseWeightedError::MissingColon)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(ParseWeightedError::EmptyName);
    }
    let weight = weight.trim().parse().map_err(ParseWeightedError::BadWeight)?;
    Ok(Weighted { name, weight })
}
```

Use early returns (`?`, `let ... else`, guard `if`s) rather than pyramids. Match exhaustively on enums you own, with no `_ =>` arm, so that adding a variant forces every match to be revisited. Enums inside a workspace stay exhaustive, sibling crates included; `#[non_exhaustive]` marks only types that cross a published API boundary (API-05), and a `_` arm is required only on those.

### CRF-04 · Comments

Comments say *why*: a constraint, a trade-off, a link to the issue or spec. Doc comments state the contract: what, errors, and examples. Never leave commented-out code, because version control remembers it. A TODO carries a tracking reference (`TODO(#123): ...`) or doesn't exist.

### CRF-05 · No dead weight

Leave no unused functions, parameters, features or dependencies, and no speculative abstraction built "for later". `dead_code` and the `unused` lints find the first kind; `cargo machete` in CI finds unused dependencies. Speculative generality needs review, with these anchors:

- a trait with one implementation that is not a port. A port with one adapter and a test fake is fine (ARC-03), and so are an extension trait on a foreign type (API-18), a sealed trait (API-12) and a trait that exists for a blanket impl over closures (API-18);
- a marker trait with no methods and no sealing or typestate purpose;
- a type parameter or `where` clause on internal code that only one type ever fills;
- a feature flag or configuration option that nothing sets.

Write the second implementation of a trait when the second use case exists.

### CRF-06 · Formatting

`cargo fmt` with the repository's `rustfmt.toml`. The gate applies it automatically, and its output is authoritative. As a SHOULD, group imports std, then external crates, then `crate::`/`super::`, each group separated by a blank line (stable rustfmt can't enforce grouping). Import the item you use rather than globs, except a dependency's documented prelude (`use rayon::prelude::*`) and `use super::*` in test modules.

### CRF-07 · Mutability

Bindings are immutable unless they must change. Localize mutation: build a value mutably inside a function, then return it immutable. Shadowing a value through a pipeline (`let input = input.trim();`) is idiomatic.

### CRF-08 · Scope discipline

Leave the code you touched a little better: rename the confusing variable, extract the function you needed to understand. Unrelated improvements become suggested follow-ups, not diff noise. A reviewer should be able to explain every line of the diff by the goal.

### CRF-09 · Named constants and units

```rust
use std::time::Duration;

/// How long to wait for the ledger service. Its slowest normal responses take
/// about 2 s, and the load balancer drops connections at 30 s, so a value
/// above 30 s would never fire.
pub const LEDGER_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest accepted request body, in bytes. Matches the proxy's 1 MiB limit:
/// raising only this value moves the rejection to the proxy.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
```

A constant's doc says why it has this value, what breaks if it changes, and which external system or spec it tracks. "Maximum accepted body" restates the name. Units live in the type (`Duration`, a newtype like `Cents(u64)`) or, failing that, in the name (`_BYTES`, `_MS`).

### CRF-10 · Expression-oriented, not golfed

`let status = match code { ... };` and iterator pipelines read well when each step names a transformation. Write a `for` loop when the body has side effects, early exits or several accumulators. Clarity decides, not cleverness.

`Option` is an iterator of zero or one item. Chain or extend with it directly, and use `iter::once` for one known item:

```rust
use std::iter;

/// Everyone to notify: the owner, the assignee if there is one, then the watchers.
pub fn recipients<'a>(
    owner: &'a str,
    assignee: Option<&'a str>,
    watchers: &[&'a str],
) -> impl Iterator<Item = &'a str> {
    iter::once(owner).chain(assignee).chain(watchers.iter().copied())
}
```

Never loop over an `Option` or map it for a side effect; write `if let Some(name) = assignee { .. }`:

```rust,ignore,expect=for_loops_over_fallibles+clippy::option_map_unit_fn
# fn collect(all: &mut Vec<&'static str>, assignee: Option<&'static str>) {
for name in assignee {
    all.push(name);
}
assignee.map(|name| all.push(name));
# }
```

rustc's `for_loops_over_fallibles` and `clippy::option_map_unit_fn` reject both.

### CRF-11 · Docs describe the current contract

Rustdoc and READMEs describe the contract and behaviour. A `//` why-comment may justify the current design, in the present tense ("one lock: `balance` and `history` change together"). Design history, narratives of alternatives tried, and rule-compliance tables go in the plan or the charter's decisions log (ARC-08).

**Why.** A reader of rustdoc needs what the item does now. "We first tried X" goes stale with the next change and buries the contract, and a table of rule IDs "applied" is a self-report nobody maintains. A present-tense reason next to the code stays true as long as the code does; the plan and the decisions log keep the dated history.

**Do.**
```rust
use std::time::Duration;

/// Delay before retry number `retry` (0-based): doubles from 100 ms, capped at 10 s.
pub fn backoff(retry: u32) -> Duration {
    // The cap keeps a retrying client inside the gateway's 30 s idle timeout.
    Duration::from_millis(100)
        .saturating_mul(2_u32.saturating_pow(retry))
        .min(Duration::from_secs(10))
}
```

**Don't.**
```rust,ignore
/// Computes the backoff.
///
/// Design notes: this used a fixed 1 s delay until review asked for exponential
/// backoff. Jitter was considered and rejected for now.
///
/// | Rule   | How it is met       |
/// |--------|---------------------|
/// | ERR-09 | saturating multiply |
/// | API-10 | one parameter       |
pub fn backoff(retry: u32) -> Duration { .. }
```

**Detect.** Review: "originally", "we tried", "used to", "alternatives considered", or rule-ID tables in doc comments, comments or READMEs. A `//` comment may cite a rule ID when it explains a constraint at that line.

## Definition of done

### H-15 · Zero warnings, everything green

fmt, clippy, tests and docs are green with zero warnings, and every public item is documented. The template lints (`missing_docs`, `clippy::missing_errors_doc`) and the gate enforce the mechanical part; "done" also means:

- [ ] Every item in the plan's test list is green; no test was removed or weakened.
- [ ] The gate is green: fmt, clippy `-D warnings`, tests and doctests, rustdoc `-D warnings`, scan.
- [ ] Self-review is done and its justifications are written up (clones, `Arc`s, `#[expect]`s, new `pub`, new deps).
- [ ] Every H-16 decision is made by a human and recorded.
- [ ] The charter, rustdoc and README are in sync.
- [ ] The independent reviewer approved the current fingerprint.
- [ ] The Status block is reported, with honest "Not verified" entries.

## Review checklist
- [ ] Is there a plan with a test list for this non-trivial change, and does its progress match the diff? (PROC-03, PROC-06)
- [ ] Were dependency APIs and features checked against the locked versions, and `# Panics`/`# Errors` read for calls on untrusted data? (PROC-01)
- [ ] Is every H-16 decision recorded with who made it? (PROC-04)
- [ ] Is the diff limited to the goal? (CRF-08)
- [ ] Do names use the domain language, are functions small, and do comments explain why? (CRF-01, CRF-02, CRF-04)
- [ ] Is there no dead code, speculative generality or commented-out code, and no untracked TODO? (CRF-05, H-06)
- [ ] Do rustdoc and READMEs describe the contract, `//` comments justify the design in the present tense, and history live in the plan or decisions log? (CRF-11)
- [ ] Was `Cargo.lock` changed only by cargo? (PROC-13)
- [ ] Are the charter, docs and CHANGELOG updated? (PROC-11)
- [ ] Does the Status block match reality? (PROC-10)
