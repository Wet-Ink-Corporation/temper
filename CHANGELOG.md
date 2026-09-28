# Changelog

## 1.1.0 (2026-09-28)

The standard was reviewed against ten outside sources: the Rust API Guidelines, Effective Rust, the Rust Performance Book, Rust Atomics and Locks, Rust Design Patterns, the Cargo Book, the std docs, Miri, Microsoft's Pragmatic Rust Guidelines and leonardomso/rust-skills. `skills/rust-craft/references/sources.md` records what was taken from each source, and what was left out and why.

### Standard (rust-craft)

**18 new rules** (182 in total):

| Area | Rule | What it requires |
|---|---|---|
| Ownership | OWN-17 | No out-parameters. |
| API | API-21 | No trait bounds on type definitions. |
| API | API-22 | Operators never overflow. |
| API | API-23 | No `Arc`/`Mutex` in public signatures. |
| Errors | ERR-13 | Explicit `finish(self) -> Result`; `Drop` only logs. |
| Errors | ERR-14 | Hand the consumed argument back in the error. |
| Architecture | ARC-16 | Every crate builds on its own. |
| Architecture | ARC-17 | Build-script rules. |
| Tests | TST-17 | Miri and aarch64 CI for concurrency code. |
| Tests | TST-18 | Pin the dependency behaviour you rely on. |
| Performance | PERF-13 | Cache-line padding, only on measured contention. |
| Concurrency | ASY-13 | Atomic ordering justification. |
| Concurrency | ASY-14 | Lock discipline, including guards in `if let`/`match` scrutinees, which edition 2024 did not fix. |
| Concurrency | ASY-15 | `thread::Builder` instead of the panicking `thread::spawn`. |
| Concurrency | ASY-16 | `Condvar::wait_while`, and no spinning. |
| Concurrency | ASY-17 | No home-made primitives. |
| Craft | CRF-11 | Docs describe the contract, not the history. |
| Process | PROC-13 | Never hand-edit `Cargo.lock`. |

**About 90 existing rules sharpened.** The largest changes:
- The panicking-APIs table grew by about 25 verified entries, and one entry was corrected: `Instant - Instant` saturates.
- The memory-ordering cheat sheet was replaced; "unsure → `SeqCst`" was wrong.
- Other additions:
  - `as_`/`to_`/`into_` cost semantics;
  - readers and writers taken by value;
  - dyn-compatibility guards;
  - one public path per item;
  - no `# Arguments` doc sections;
  - caret-floor dependency requirements;
  - profile defaults versus per-application choices;
  - `#[non_exhaustive]` only at published boundaries.

**Examples.**
- Fixed examples that failed the template's own gate (PERF-03 `chunks_exact`) or taught a bug:
  - the ASY-10 scope panic;
  - the Decider `evolve` wildcard;
  - parsers returning `Option`.
- Full sketches now live once in `patterns.md`, with ten new patterns (23–32).

**`SKILL.md`.** The iron rules, design defaults and router were updated. It is still under the 5k-token re-attachment budget.

### Enforcement

**Template lints.**
- rustc: `ambiguous_negative_literals`, `redundant_imports`.
- clippy:
  - `needless_collect`
  - `or_fun_call`
  - `significant_drop_in_scrutinee`
  - `debug_assert_with_mut_call`
  - `future_not_send`
  - `too_long_first_doc_paragraph`
  - `missing_assert_message`
  - `assertions_on_result_states`
  - `renamed_function_params`
  - `empty_drop`
  - `negative_feature_names`
  - `redundant_feature_names`
  - `cargo_common_metadata`

**`clippy.toml`.**
- `disallowed-methods`: `thread::spawn`, `Scope::spawn`, `env::args`, `env::vars`, float `Duration` constructors and arithmetic.
- `await-holding-invalid-types`: tracing span guards, `watch::Ref`.
- `upper-case-acronyms-aggressive`.

**Profiles.** New `dev` (line tables only; no debug info for dependencies), `profiling` and `bench` profiles.

**Scanner.**
- New rules:
  - `unwrap` inside doctest fences, which clippy never sees;
  - silent `Err` arms;
  - `thread::spawn`;
  - `compiler_fence`;
  - `#[global_allocator]` in libraries;
  - advisories for `map[&k]`, `SeqCst`, `fetch_update`, glob re-exports, struct bounds, operator impls, `get_` getters, primitive type aliases and more.
- Compile-time assertions are no longer flagged.
- `cfg_attr(.., ignore)` without a reason is now caught.

**Hooks.**
- A member `Cargo.toml` that does not inherit the workspace lints, or that declares its own `[profile]`, is refused. Previously a new crate silently got none of the escape-hatch lints. The check is a ratchet: only new violations block. Human-approved exceptions are listed in `.claude/temper.json` under `manifests.ownLints`.
- Asks before:
  - manifest switches that stop tests (`test`, `doctest`, `harness`, `autotests`);
  - `[patch]`/`[replace]`;
  - `rust-version`/`edition` changes;
  - new crates;
  - dependency major-version bumps;
  - new `#[ignore]`s;
  - `cargo add`/`cargo remove`.
- Refuses:
  - `Cargo.lock` edits;
  - shell writes to `Cargo.lock`/`Cargo.toml`;
  - `--ignore-rust-version`.

**Gate.** A `manifests` step checks lint inheritance and member profiles. It also runs when cargo is unavailable.

**CI template.**
- Actions pinned to commit SHAs, `permissions: contents: read`, and `--locked` everywhere.
- The MSRV job now really runs 1.96. It used to be overridden by `rust-toolchain.toml`.
- New steps: a per-crate build (ARC-16), `cargo hack` feature powerset, `cargo machete`.
- Scheduled latest-deps, aarch64 and Miri jobs.
- Dependabot, with `lockfile-only`.
- `deny.toml` fixed so `cargo deny check` passes on real dependency trees.

### Example workspace

- It meets every new rule.
- `evolve` enumerates impossible `(state, event)` pairs.
- Writers are taken by value.
- Exit codes are mapped through a `match`, and the deviation from the house convention is recorded in the charter.
- Crate roots have a quick start and a list of features.
- It passes under Miri: no UB, no leaks, no deadlocks.

### Verification

- `tests/doc_examples.py`: every Do block in the skill (329 blocks) compiles, passes clippy and runs its tests under the template lints. Every annotated Don't block fires the diagnostic it claims.
- `tests/registry.py`: every rule has exactly one section, every cited ID exists, and the counts match.
- `tests/lint-claims.py`: now also checks Detect lines and scanner claims by rule ID.
- New `tests/hooks.test.mjs` and `tests/toml.test.mjs`.

## 1.0.0 (2026-09-27)

Initial release:
- the rust-craft skill (164 rules);
- the implementer, reviewer and architect subagents;
- the skill gate, pre-write scanner, protected config and fingerprint-bound completion gate;
- the workspace and example templates;
- evals.
