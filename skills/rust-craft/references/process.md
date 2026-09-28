# Process and craft

How Rust work gets done here: exploring, planning, managing context, deciding what needs a human, taking small steps, self-review, reporting, and the craftsmanship rules that apply to every line. The technical references say what good code is. This one says how an agent reliably produces it.

Rules covered: PROC-01..PROC-12, CRF-01..CRF-10, H-15, H-16.

## The loop at a glance

```text
Orient -> Frame -> Plan -> [Red -> Green -> Refactor]* -> Self-review -> Gate -> Independent review -> Report
                     ^                                                                  |
                     +---------------------- changes-requested -------------------------+
```

Every phase has an exit criterion (SKILL.md §2). Skipping a phase doesn't save time. It moves the cost into review rounds, which are the most expensive loop there is.

## PROC-01 · Explore before editing

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
| What does the dependency graph allow? | `cargo tree -e normal -p <crate> --depth 1` |

**Don't.** Write against a dependency API from memory. Crates change between versions (for example, `thiserror` 1 to 2, `schemars` 0.8 to 1, `rand` 0.8 to 0.9). Check the version first.

## PROC-02 · Context management

Your context window is working memory. What fills it with noise pushes out the rules and the plan.

- **Delegate wide searches.** "Find every place that constructs `Order`" goes to an exploration subagent, which returns the list and a two-line conclusion. You never see the forty files it read.
- **Keep command output short.** Use `cargo clippy --message-format short`, `cargo test -q`, `cargo test -p <crate> <filter>` for the unit you're on, and `2>&1 | tail -n 40` for long logs. The gate prints only failures, with a path to the full log.
- **Read surgically.** Read the function you need, with line ranges for large files. Don't re-read a file you just edited to "check".
- **Externalize state.** The plan file is your durable memory (PROC-06). Decisions, the test-list checklist and open questions live there, not only in the conversation, so compaction or a handoff loses nothing.
- **Re-anchor after compaction.** When a session continues after compaction, re-read the plan file and the relevant crate charter before the next edit. The temper `SessionStart` hook re-injects the Iron rules and the current gate and review state.

## PROC-03 · Plan template

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
- Types:
  ```rust,ignore
  pub struct OrderId(Uuid);                                  // API-02
  impl TryFrom<&str> for OrderId { type Error = ParseIdError; ... }
  ```
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

## PROC-04 · Human decisions (H-16)

These are never decided silently:

| Decision | Why a human |
|---|---|
| Adding a dependency | Supply chain, license, compile time, and long-term maintenance cost |
| Changing or breaking a public API | Downstream users and semver |
| Any lint exception (`#[expect]`) or `unsafe` | It suspends a guarantee |
| Creating, merging or re-layering crates | Architecture is expensive to reverse |
| Schema, persistence or event-format migrations | Data outlives code |
| Deleting a test or changing an expected value | A test is the spec. Changing it changes the requirement. |
| Performance-versus-clarity trade-offs users will notice | This is a product decision |

Frame each one as options, trade-offs and a recommendation:

```text
Decision needed (H-16): add `backon` for retries?
- A) Add backon 1.x: tested backoff and jitter, about 3 small transitive deps. Recommended.
- B) Hand-roll a 30-line retry with exponential backoff: no deps, but we own the edge cases.
- C) No retries: simplest, but transient 503s surface to users.
```

A main-thread agent asks with the question tool. A subagent returns `Status: BLOCKED` with the decision, and the orchestrator asks. Record the outcome in the plan's Decisions table, and for architecture-level decisions, in the crate charter or root `AGENTS.md` decisions log.

## PROC-05 · Small steps

- One behavior per red-green-refactor cycle. Build and run the focused test after every step.
- When a step fails in a way you don't understand, undo it and take a smaller step.
- When commits are in scope, commit at green, one behavior or refactor per commit, with a conventional message (`feat(ledger): reject overdraft withdrawals`). Never commit a red tree, and never use `--no-verify`.

## PROC-06 · Keep the plan current

Record each red run in Progress (the test name and its failure line), then tick the item when it goes green. This is the reviewer's evidence for TST-01. Add learnings to Progress, such as "the parser already normalizes whitespace, so dropped behavior 4". A reviewer, a resumed session or a human can pick up exactly where you are.

## PROC-07 · Self-review protocol

Before asking for review:
1. Run `git diff <base> --stat` and confirm nothing unrelated is in the diff (CRF-08).
2. Read the full diff as if a stranger wrote it. Work through the checklist in `review-rubric.md` section by section.
3. For every `.clone()`, `Arc`, `Mutex`, `#[expect]`, `pub` and new dependency in the diff, write its justification into your report. If you can't justify one, fix it.
4. Check that no test was removed or loosened.
5. Run the gate.

Fix what you find. A self-review that lists problems without fixing them is a report of known defects.

## PROC-08 · Independent review

Every change that reaches "done" gets a `temper:rust-reviewer` verdict for the exact tree fingerprint. Give the reviewer the goal and the plan path, not your interpretation. Its fresh context is the point. The `Stop` hook refuses to end a turn with Rust changes that lack an approving verdict for the current fingerprint. `Status: WIP` or `Status: BLOCKED` is the honest alternative when you can't finish.

## PROC-09 · When stuck

After two failed attempts at the same compiler error, stop editing:
1. Read the whole diagnostic, including every `note:` and `help:`, and run `rustc --explain E0XXX` (PROC-12).
2. Write down who owns each value involved and how long each borrow lives. Draw it as a list if that helps.
3. Find the symptom in ownership.md, "Fighting the borrow checker", and apply the shape it prescribes.
4. If the design itself is the problem (for example, two owners of the same mutable state), go back to the plan and change the design. Don't patch around it.

Random `.clone()`, `'static`, `Rc<RefCell<_>>`, `Box::leak` or lifetime annotations sprinkled until it compiles are escape hatches (H-07, H-08).

## PROC-10 · Status block

Every turn that changed Rust ends with this block:

```text
Status: DONE | WIP | BLOCKED
Gate: fmt ok | clippy ok | test ok (N passed) | doc ok   (or the failing step and its first error)
Review: approved | changes-requested | not yet requested
Changed: <crate/path> (+added/-removed), ...
Decisions needed: <H-16 items, or "none">
Not verified: <anything you could not check, or "none">
```

- `DONE` means the gate is green and review is approved for the current tree. The `Stop` hook verifies it.
- `WIP` means you are pausing mid-task. List what remains.
- `BLOCKED` means a human decision or missing information stops progress. Ask the question.

"Not verified" is where honesty lives. Put there anything you could not run (an integration test needing credentials, a platform you can't build for) and any assumption you made.

## PROC-11 · Docs in the same change

When a change alters a crate's purpose, public surface, dependencies or invariants, update its `AGENTS.md` charter. Update rustdoc for changed public items, the README for changed usage, and `CHANGELOG.md` if the repo has one.

## PROC-12 · Read the diagnostic

rustc's errors usually contain the fix: `help: consider borrowing here`, `note: ...value moved here`. Read the whole message before editing. `rustc --explain E0502` gives a worked explanation. Clippy's messages link to the lint's documentation, which explains the reason and the fix.

## Craftsmanship

### CRF-01 · Names

Use the domain's own words. The charter's vocabulary is the ubiquitous language. Types are nouns (`Invoice`, `LedgerEntry`), functions are verbs (`post_entry`, `reconcile`), predicates read as questions (`is_settled`), and conversions follow API-01 naming. Avoid `data`, `info`, `manager`, `helper`, `util`, `do_stuff`, `process` (unless the domain calls it that), and one-letter names outside tiny closures and generic parameters.

### CRF-02 · Function size and shape

One level of abstraction per function. When a block needs a comment to say what it does, it is a function with that name. Aim for about 50 lines and at most 4 parameters. Beyond them, look for an extraction or a params struct (API-10). Clippy's `too_many_lines` (60) and `too_many_arguments` (4) enforce the outer bound.

### CRF-03 · Flat control flow

```rust
/// A parsed `name:weight` pair.
#[derive(Debug, PartialEq, Eq)]
pub struct Weighted<'a> {
    /// Item name.
    pub name: &'a str,
    /// Item weight.
    pub weight: u32,
}

/// Parses `name:weight`, returning `None` for malformed input.
#[must_use]
pub fn parse_weighted(input: &str) -> Option<Weighted<'_>> {
    let (name, weight) = input.split_once(':')?;
    let weight = weight.trim().parse().ok()?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(Weighted { name, weight })
}
```

Use early returns (`?`, `let ... else`, guard `if`s) rather than pyramids. Match exhaustively on enums you own, with no `_ =>` arm, so that adding a variant forces every match to be revisited.

### CRF-04 · Comments

Comments say *why*: a constraint, a trade-off, a link to the issue or spec. Doc comments state the contract: what, errors, and examples. Never leave commented-out code, because version control remembers it. A TODO carries a tracking reference (`TODO(#123): ...`) or doesn't exist.

### CRF-05 · No dead weight

Leave no unused functions, parameters, features or dependencies (`cargo machete` finds unused deps), and no speculative abstraction built "for later". Write the second implementation of a trait when the second use case exists.

### CRF-06 · Formatting

`cargo fmt` with the repository's `rustfmt.toml`. The gate applies it automatically, and its output is authoritative. As a SHOULD, group imports std, then external crates, then `crate::`/`super::`, each group separated by a blank line (stable rustfmt can't enforce grouping). Import the item you use rather than globs, except preludes the crate documents and `use super::*` in test modules.

### CRF-07 · Mutability

Bindings are immutable unless they must change. Localize mutation: build a value mutably inside a function, then return it immutable. Shadowing a value through a pipeline (`let input = input.trim();`) is idiomatic.

### CRF-08 · Scope discipline

Leave the code you touched a little better: rename the confusing variable, extract the function you needed to understand. Unrelated improvements become suggested follow-ups, not diff noise. A reviewer should be able to explain every line of the diff by the goal.

### CRF-09 · Named constants and units

```rust
use std::time::Duration;

/// How long to wait for the ledger service before giving up.
pub const LEDGER_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum accepted request body, in bytes.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
```

Units live in the type (`Duration`, a newtype like `Cents(u64)`) or, failing that, in the name (`_BYTES`, `_MS`).

### CRF-10 · Expression-oriented, not golfed

`let status = match code { ... };` and iterator pipelines read well when each step names a transformation. Write a `for` loop when the body has side effects, early exits or several accumulators. Clarity decides, not cleverness.

## Definition of done (H-15)

- [ ] Every item in the plan's test list is green; no test was removed or weakened.
- [ ] The gate is green: fmt, clippy `-D warnings`, tests and doctests, rustdoc `-D warnings`, scan.
- [ ] Self-review is done and its justifications are written up (clones, `Arc`s, `#[expect]`s, new `pub`, new deps).
- [ ] Every H-16 decision is made by a human and recorded.
- [ ] The charter, rustdoc and README are in sync.
- [ ] The independent reviewer approved the current fingerprint.
- [ ] The Status block is reported, with honest "Not verified" entries.

## Review checklist
- [ ] Is there a plan with a test list for this non-trivial change, and does its progress match the diff? (PROC-03, PROC-06)
- [ ] Were dependency APIs checked against the locked versions? (PROC-01)
- [ ] Is every H-16 decision recorded with who made it? (PROC-04)
- [ ] Is the diff limited to the goal? (CRF-08)
- [ ] Do names use the domain language, are functions small, and do comments explain why? (CRF-01, CRF-02, CRF-04)
- [ ] Is there no dead code or commented-out code, and no untracked TODO? (CRF-05, H-06)
- [ ] Are the charter and docs updated? (PROC-11)
- [ ] Does the Status block match reality? (PROC-10)
