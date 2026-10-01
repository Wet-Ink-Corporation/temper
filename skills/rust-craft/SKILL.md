---
name: rust-craft
description: Engineering standard for writing, changing, planning and reviewing Rust. Covers ownership, borrowing and lifetimes, public API design, crate and module architecture, typed error handling, TDD, performance and resource use, async and concurrency, serde and structured output, modern edition-2024 idioms, and the no-escape-hatch rules (no unwrap, expect, panic, unsafe, lint suppression or borrow-checker-appeasing clones in production code). Use it for any task that creates or edits .rs files or Cargo.toml, designs a Rust type, module, crate or API, fixes a rustc or clippy error, plans Rust work, or reviews a Rust diff, including small one-line edits.
license: MIT
compatibility: Rust edition 2024 on stable 1.96 or newer (the template's MSRV; written against 1.98). Portable Agent Skill; the temper plugin adds hook enforcement in Claude Code.
metadata:
  version: "1.1.0"
  rule-registry: references/rules.md
---

# Rust Craft

This standard produces Rust that is correct by construction, idiomatic, resource-efficient and free of escape hatches. It also defines how that code gets made: explore, plan, test first, implement, self-review, gate, independent review.

Every rule has an ID (`OWN-08`, `ERR-03`, `H-02`). Implementers cite IDs in plans and self-review notes. Reviewers cite IDs in findings. The full registry is `references/rules.md`.

**Levels.** MUST is a merge blocker. SHOULD is the default and needs a stated reason to deviate. CONSIDER is judgment.

**Production code** means everything except `#[cfg(test)]` items and `#[test]` functions. Test relaxations apply only inside those. A plain helper function in `tests/` is not exempt from the lints, so declare shared helpers as `#[cfg(test)] mod support;` or have them return `Result` (TST-14). `benches/` and `examples/` follow production rules: benchmarks compile without `cfg(test)`, and examples teach.

## 1. Iron rules (MUST, no exceptions without a human)

These are enforced by the compiler lints in the repository templates, by the temper hooks before a file is written, and by the reviewer. You cannot work around them. If one seems impossible to satisfy, the design is wrong. Stop and redesign, or ask the human (H-16).

| ID | Rule |
|---|---|
| H-01 | No `unsafe` anywhere: no blocks, fns, impls or `unsafe(...)` attributes. The workspace sets `unsafe_code = "forbid"`. |
| H-02 | No `.unwrap()`, `.expect()`, `unwrap_unchecked`, `unwrap_err()` or `expect_err()` in production code. Propagate with `?`, or handle the case. |
| H-03 | No `panic!`, `todo!`, `unimplemented!` or `unreachable!` in production code. Encode invariants in types so the impossible case cannot be written (exhaustive `match`, newtypes, `NonZero`, typestate). No std call that panics on input you do not control: integer `/` and `%`, `thread::spawn`, `env::args`, `Duration::from_secs_f64` and the rest of the table in `references/escape-hatches.md`. |
| H-04 | No panicking index or slice on data whose bounds are not proven by construction. Use `get`, iterators, slice patterns (`[first, rest @ ..]`), `split_first`, `split_at_checked`, `first_chunk` or `as_chunks`. `map[&k]` and `deque[i]` panic too, and no lint sees them: use `get` or `entry`. |
| H-05 | No lint suppression. `#[allow]` and `#![allow]` are denied. `#[expect(lint, reason = "...")]` is an exception request that a human must approve. Never edit lint config, `clippy.toml`, `rustfmt.toml`, CI or hook config to make a check pass. |
| H-06 | No debug leftovers: no `dbg!` anywhere, no commented-out code, and no `TODO` without a tracking reference. No print macros in production code. Libraries log with `tracing`. Binaries write through a locked handle, `writeln!(io::stdout().lock(), ..)?`, because `println!` panics on a closed pipe. |
| H-07 | No ownership escape hatches: no `Box::leak`, `mem::forget`, `mem::transmute`, `static mut`, or `'static` bounds added to silence lifetime errors. `Rc<RefCell<_>>` and `Arc<Mutex<_>>` need a written justification that the state really is shared and mutable (OWN-11). |
| H-08 | No clone to appease the borrow checker. Every `.clone()`, `.to_owned()` or `.to_string()` in a diff has an ownership reason: the callee stores the value, the value is a cheap handle (`Arc`, `Rc`, `Copy`), or the data really is duplicated. |
| H-09 | No stringly-typed or type-erased errors in library APIs. No `Result<_, String>`, `Box<dyn Error>`, `anyhow::Error` or `eyre::Report` in `pub` signatures of library crates. `anyhow` and `eyre` belong only at the binary edge. |
| H-10 | No silently discarded failures. No `let _ = fallible();`, no `.ok();` to drop an error, no `unwrap_or_default()` that hides a failure the caller should see. |
| H-11 | No lossy `as` numeric casts. Use `From`, `TryFrom`, `u64::from(x)`, `checked_*` or `saturating_*`. The workspace denies `clippy::as_conversions`. |
| H-12 | Never weaken verification to get green. Do not delete, `#[ignore]`, loosen or rewrite a test's expectation, lower a lint level, skip a hook, pass `--no-verify` or `--ignore-rust-version`, or flip a manifest switch that stops tests running (`test`, `doctest`, `harness`, `autotests`). A red check means the code is wrong or the test is wrong. Decide which and say so explicitly. Changing a test's expectation is a HITL decision (H-16). |
| H-13 | No unbounded resources: bounded channels, capped buffers for untrusted input, timeouts on external I/O, and no detached tasks or threads. Every spawned task or thread has an owner that joins or aborts it. |
| H-14 | No blocking inside async code. No lock guard, `RefCell` borrow, entered tracing span or `watch::Ref` held across `.await`. `Drop` never blocks. |
| H-15 | Zero warnings. `cargo fmt`, `clippy -D warnings`, tests and `rustdoc -D warnings` are all green. Every `pub` item has docs, and fallible ones have an `# Errors` section. |
| H-16 | Some decisions belong to a human. Ask before acting on any of these: a new dependency (other than the pre-approved list in `.claude/temper.json`) or a dependency's major-version bump, a `[patch]` override, a `rust-version` raise, a public API change or break, a lint or `unsafe` exception, a crate-boundary change, a schema or data migration, deleting or rewriting a test, or a performance-versus-clarity trade-off users will notice. Record the decision in the plan. |

Tests may use `unwrap`/`expect` (prefer `expect("why this holds")`) and `assert!` macros. They may not use `unsafe`, `dbg!`, `#[allow]`, sleeps (TST-10), or `#[ignore]` (including `cfg_attr(miri, ignore)`) without a reason string.

**When a request conflicts with an Iron rule** (for example, "just unwrap it" or "skip the tests"), don't stall and don't comply silently. Build the closest compliant version, such as a `Result` instead of `unwrap`. Then tell the user in one or two sentences what you did differently and why, and offer the sanctioned route if they still want the original: an exception request (H-05), or a decision recorded under H-16. The human can always overrule the standard, but only knowingly.

## 2. The workflow (implementer)

Follow these phases in order. Each has an exit criterion. The phases scale: a one-line fix still goes through Red, Green and the gate, but its plan fits in one sentence.

1. **Orient** (PROC-01, PROC-02). Read the crate charter (`AGENTS.md` or `CLAUDE.md` in the crate root), the crate's `lib.rs` surface, the tests for the area, and the call sites. Check dependency APIs against the version in `Cargo.lock` by reading the source in the cargo registry or docs.rs. Do not write them from memory. Send wide searches to an exploration subagent and keep only its conclusions. *Exit:* you can say what owns what, and where the change belongs.
2. **Frame**. State the goal, the invariants, the constraints and the non-goals. List any H-16 decisions and ask before building on them. *Exit:* no open question blocks the design.
3. **Plan** (PROC-03). Write a plan for anything over about 30 lines, any new type or module, any public API change, or any cross-crate change. Use the template in `references/process.md`: exact signatures with ownership decided (OWN), placement (ARC), error design (ERR), a test list of behaviors (TST), risks, and HITL decisions. Save it under `.temper/plans/` and update its checklist as you go (PROC-06). *Exit:* the test list exists.
4. **Red** (TST-01). Write the next test from the list. Run it and watch it fail for the right reason. A compile error against a not-yet-written API is a valid first red. Record the red run (test name and failure line) in the plan's Progress section, which is the reviewer's evidence. *Exit:* one meaningful failing test.
5. **Green**. Write the minimum code that passes, with no escape hatch even temporarily. *Exit:* the test passes and all other tests still pass.
6. **Refactor**. With tests green, fix names, ownership, duplication and placement, then run `cargo clippy`. *Exit:* no clippy finding in code you wrote or touched, and green tests.
7. **Repeat** steps 4 to 6 for each behavior. Keep each step small (PROC-05).
8. **Self-review** (PROC-07). Read `git diff` as if a stranger wrote it and work through the checklist in `references/review-rubric.md`. Fix what you find rather than listing it.
9. **Gate** (H-15). Run `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, and `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D warnings"`. The temper gate command runs all of these and caches the result. In a repository with a committed lint baseline (`.claude/temper-baseline.json`), clippy and rustdoc fail only on findings in lines you changed or on a crate's count rising; legacy findings pass. Lower the baseline with `temper baseline --tighten` when you fix legacy debt, and never raise it (H-12).
10. **Independent review** (PROC-08). A fresh-context reviewer reviews the diff. Address every finding, or rebut it with evidence.
11. **Report** (PROC-10). End with the Status block (§5). Say plainly what was not done or not verified.

**When stuck** (PROC-09). After two failed attempts at the same borrow or lifetime error, stop editing. Write down who owns each value and how long each borrow lives, then pick a shape from `references/ownership.md` §Fighting the borrow checker. Adding annotations, clones or `'static` at random is not a strategy.

## 3. Design defaults

Use these unless the plan states a reason to deviate.

| Decision | Default | Rules |
|---|---|---|
| Read-only string, sequence or path parameter | `&str`, `&[T]` or `impl IntoIterator<Item = T>`, `&Path` or `impl AsRef<Path>` | OWN-01, OWN-03 |
| Reader or writer parameter | `R: Read`/`W: Write` by value (callers pass `&mut w`); parsers take `impl BufRead`, not a path | OWN-01, API-08 |
| Parameter the callee stores | Owned `String` or `Vec<T>` (`impl Into<String>` for constructors and builders) | OWN-02 |
| Usually borrows, sometimes allocates | `Cow<'_, str>` | OWN-10 |
| Exposing contents | `impl Iterator<Item = &T> + '_` or `&[T]`, not a fresh `Vec` | OWN-13 |
| Sharing | Single owner, then borrow, then move into the task, then `Arc<T>` immutable, then `Arc<Mutex<T>>` (justified). For graphs, use an arena with index handles. Keep `Arc`/`Mutex` out of `pub` signatures: wrap them in a handle type. | OWN-11, API-23 |
| Validated value | Newtype with private field, `TryFrom`/`FromStr` constructor ("parse, don't validate"), in its own module | API-02, API-03 |
| Generic type | No bounds on the `struct`/`enum`; bounds on the `impl` blocks | API-21 |
| Value-object arithmetic | `checked_add` returning `Option`/`Result`; `impl Add` only when it cannot overflow or its wrapping or saturation is documented | API-22, ERR-09 |
| Closed set of cases | `enum`, never bool flags or `Option<bool>` | API-02, API-10 |
| Many optional parameters | Params struct with `Default`, or a builder. Typestate when call order matters. | API-10, API-13 |
| Polymorphism | Generics by default. `dyn Trait` for heterogeneity or plug-ins. `enum` for a closed set. | API-09 |
| Library error | `thiserror` enum per operation or module, typed context fields, `#[non_exhaustive]` only in published crates. A consuming fallible function hands its argument back in the error. | ERR-01..04, ERR-14 |
| Teardown that can fail | `finish(self) -> Result`; `Drop` only logs as a fallback | ERR-13 |
| Binary error | `anyhow` with `.context()` at each boundary; `main` returns `Result` or `ExitCode` | ERR-06 |
| Visibility | Private, then `pub(crate)`, then deliberate `pub` re-exported from `lib.rs` | API-14, ARC-05 |
| Module layout | `foo.rs` plus `foo/bar.rs`; no `mod.rs`, no `#[path]`; roughly 400 lines per file and 50 per function | ARC-06, CRF-02 |
| Crate split | By dependency direction: domain, then application (ports), then adapters, then bin | ARC-02, ARC-03, ARC-07 |
| Async | Only for I/O-bound work. Libraries stay runtime-agnostic and the binary picks the runtime. | ASY-01, ASY-02 |
| Threads and shared state | `thread::scope` or named `thread::Builder` threads, all joined. A `Mutex` before atomics; every atomic ordering has a justifying comment. | ASY-10, ASY-13..17 |
| Collections | `Vec` first. `HashMap` for lookup, `BTreeMap` when order is observable. Pre-size when the size is known. | PERF-02, PERF-06 |
| External data | Deserialize into a DTO, then `TryFrom` into the domain type | OUT-01, OUT-05 |
| Logging | `tracing` with structured fields and spans; no `println!` in libraries | OUT-08 |

## 4. Reviewer protocol (summary)

The reviewer works in a fresh context and is adversarial by design. Its job is to find what is wrong, not to confirm what is right.

1. Establish the scope: the diff against the base revision, the plan, and the crate charters that are touched.
2. Run the gate yourself in check mode (`gate --check`). Never trust a reported result. Any failure means `changes-requested`.
3. Read the tests first to learn the intent, then the public API, then the implementation.
4. Score each rubric dimension from 0 to 4 using the anchors in `references/review-rubric.md`. Record findings with rule ID, severity (blocker, major, minor or nit), `file:line`, evidence and a concrete fix.
5. Verdict: `approved` only when the gate is green, there are no blocker or major findings, and every applicable dimension scores at least 3. Otherwise `changes-requested`.
6. Emit the verdict JSON block (schema in `references/review-rubric.md`) as the last thing in your final message.

Style preferences are not findings unless they break a rule. Label them as nits. Every finding needs evidence: quote the line.

## 5. Structured outputs (agent-facing)

- **Plan.** Use the template in `references/process.md` and save it under `.temper/plans/<slug>.md`.
- **Status block.** End every turn that changed Rust with this block:

  ```text
  Status: DONE | WIP | BLOCKED
  Gate: fmt ok | clippy ok | test ok (N passed) | doc ok   (or the failing step and its first error)
  Review: approved | changes-requested | not yet requested
  Changed: <crate/path> (+added/-removed), ...
  Decisions needed: <H-16 items, or "none">
  Not verified: <anything you could not check, or "none">
  ```

  `DONE` means the gate is green and review is approved. Use `WIP` when you are pausing mid-task, or `BLOCKED` with the question when you need the human. Never write `DONE` over a red gate.
- **Review verdict.** A fenced `json` block with `"temper_verdict": 1` (schema in `references/review-rubric.md`).

## 6. Where to read more

Read the reference that matches the task before starting it. To look up a rule you have not seen yet, grep its row in `references/rules.md` (`grep '^| ERR-13' references/rules.md`) rather than reading the whole registry.

| Task | Read |
|---|---|
| Any borrow or lifetime error, signature design, smart-pointer choice | `references/ownership.md` |
| New public type, trait, function, module surface, or semver question | `references/api-design.md` |
| Error types, `?` plumbing, `Option` to `Result`, binary error reporting | `references/errors.md` |
| New crate, module layout, dependency direction, ports and adapters, event sourcing | `references/architecture.md` |
| Writing or reviewing tests, TDD steps, fakes, property and snapshot tests | `references/testing.md` |
| Hot paths, allocation, collections, I/O, profiling, release profile | `references/performance.md` |
| async, tasks, channels, cancellation, locks, threads, atomics and memory ordering | `references/async-concurrency.md` |
| serde, JSON or CLI output, schemas, versioning, tracing | `references/structured-output.md` |
| Exploring, planning, context management, HITL, self-review, commits | `references/process.md` |
| Reviewing a diff, scoring, verdict format, self-review checklist | `references/review-rubric.md` |
| "I need an exception" or "the lint won't let me" | `references/escape-hatches.md` |
| Edition 2024 and recent stable features, replacing legacy crates | `references/modern-rust.md` |
| Worked, compiling examples of the patterns above | `references/patterns.md` (long: grep for `^## ` and read only the pattern you need) |
| A complete workspace to imitate (charters, layers, tests, CLI) | `templates/example-workspace/` in the temper plugin |
| Where a rule came from, and outside advice deliberately not adopted | `references/sources.md` |
