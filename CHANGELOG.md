# Changelog

## Unreleased

Documentation.
- **Cloud sessions.** `docs/troubleshooting.md` has a new section on why temper is missing from Claude Code cloud sessions, and how to install it from the environment's setup script. A repository's `.claude/settings.json` does not install the plugin there, and a `SessionStart` hook installs it too late. The section also covers what works in claude.ai chat: the skills load, but the hooks and agents do not.
- **The organization sync.** The same section says how claude.ai's organization sync keeps temper current: sync `Wet-Ink-Corporation/temper` with Sync automatically on, and remove any uploaded zip copy. It also corrects an earlier line that called the repository public; it is internal.

Clearer messages for new adopters. Only the text changes: every hook decision, exit code and gate result is the same.
- **The skill-gate denial** lists all sixteen Iron rules, H-01 to H-16. It used to stop at H-12, while `SessionStart` printed all sixteen.
- **Malformed hook input.** When a hook's stdin is not valid JSON, the hook still exits 0 with no decision, but it now prints a `systemMessage`: `temper hook <event>: could not parse the hook input as JSON; nothing was checked`. Before, it printed nothing, which read as a clean pass. With `TEMPER_DISABLE=1` it stays silent.
- **The gate's fmt summary when cargo cannot load the workspace.** `cargo fmt` fails before formatting anything when a `members` glob matches no crate (a fresh setup with an empty `crates/`) or a manifest does not parse, and the summary used to be the tail of rustfmt's help text. It now says that cargo cannot load the workspace, quotes cargo's `error:` and `-->` lines with paths relative to the workspace, and, for a glob that matches nothing, suggests a `cargo new --lib` path inside it (`crates/<name>` for `crates/*`). The fmt step and the gate stay RED.
- **Vendored-CLI drift.** `temper setup`'s drift line ends with `rerun with --vendor-only to replace only the vendored CLI (or --force to also overwrite the other template files; an existing Cargo.toml is never overwritten)`, in place of `rerun with --force to replace it`.

`temper setup` adds `/target/` to `.gitignore` when no line ignores `target` already (`target`, `target/`, `/target` and `/target/` all count), next to the `.temper/` line it already added. `--dry-run` reports it without writing, `--vendor-only` leaves `.gitignore` alone, and a second run adds nothing. `Cargo.lock` is not added: it is committed.

Bounded local state. No hook decision changes; what temper keeps under `.git/temper/` goes from unbounded to bounded.
- **Pruning at startup.** `SessionStart` on startup already pruned session state after 14 days. It now also prunes review verdicts (after 60 days, newest 300 kept), gate results (30 days, newest 300) and step logs (14 days, newest 150 files, 64 MB in all). The records of the tree the session starts on are always kept. A tree whose verdict was pruned needs a fresh review at Stop, and one whose gate result was pruned runs the gate again.
- **Telemetry rotation.** `telemetry.jsonl` was only ever appended. At 1 MiB it is now renamed to `telemetry.1.jsonl`, replacing the previous one, and `temper stats` reads both files.
- `scripts/lib/project.mjs` changed too, beyond the files the upgrading note below names; `temper setup --vendor-only` refreshes it with the rest.

Upgrading from 1.3.x: `scripts/temper.mjs`, `scripts/lib/gate.mjs`, `scripts/lib/hooks.mjs` and `scripts/lib/vendor.mjs` changed, so `temper setup --dry-run` reports the vendored copy as drift. Refresh it with `temper setup --vendor-only`. Nothing in `templates/workspace/` changed. A full `temper setup` also appends `/target/` to `.gitignore` if no `target` line is there.

## 1.3.0 (2026-10-05)

Fixes from the pilot adoptions in weigh-in and redkiln-rs (#32).
- **`rust.yml`'s `msrv` job** reads the MSRV from `rust-version` in the root `Cargo.toml`. Before, it was hard-coded to 1.96, so it failed in any repository whose `rust-version` is newer, because cargo refuses a toolchain older than `rust-version`. It also gains the `Baseline mode` step that the other jobs have, so with a baseline it no longer fails on legacy warnings once the MSRV is 1.97 or later.
- **`temper setup` reports what an adopting repository has to change by hand.** It adds:
  - a `manual` line for each workflow or contributor doc (`CLAUDE.md`, `AGENTS.md`, `CONTRIBUTING.md`, the PR template) that uses `-D warnings`. Those steps fail on every legacy finding once the lint tables are merged.
  - a `manual` line listing the `allow-*-in-tests` keys that an existing `clippy.toml` lacks. Without them, the restriction lints fire in test code and the baseline records it as debt. In redkiln-rs, that was 5,541 findings.
  - a reminder to run `cargo machete` before the first push.
- **The workspace template's `AGENTS.md`** lists `temper baseline --check` as the lint command, in place of `cargo clippy -- -D warnings` and `RUSTDOCFLAGS="-D warnings"`.
- **The README's brownfield rollout** has a new step 3 for the `manual` lines. It also says what moving lint to the ratchet costs: the baseline records one platform, so other platforms no longer lint.

`temper setup --vendor-only` refreshes only the vendored CLI (`.claude/temper/`) (#38). It replaces an out-of-date copy without `--force`, removing files the new release no longer ships. It leaves the templates, the lint tables and `.gitignore` alone, and `--dry-run` previews it. Use it when an upgrade's only drift is the vendored CLI, in place of `setup --force` followed by restoring every customised template file. The README's Upgrading section has the step.

Fixes found by the runner probe on macOS and Windows runners.
- On macOS, paths under a symlinked directory were reported relative to the wrong root. TMPDIR is under `/var`, a link to `/private/var`, and cargo reports the real path, so `crates/legacy/Cargo.toml` came out as `../../../../private/var/.../crates/legacy/Cargo.toml`. The 1.2.3 fix for Windows 8.3 names now applies on every platform: when the lexical path leaves the root, both sides are compared in their real form.
- The example workspace's JSONL store could not repair a torn tail or roll back a failed batch on Windows, because an append-mode handle there lacks the right to truncate (`PermissionDenied`). The store now opens stream files read-write and writes each batch at an explicit offset under its exclusive lock. CI now runs the example workspace's tests on Windows.

Documentation.
- The README has an Upgrading section: how an adopted repository moves its plugin and its vendored CLI (`.claude/temper/`) to a new release, what `temper setup --dry-run` reports, what `--force` overwrites, what to re-run, and what to review in the diff.
- New [`docs/troubleshooting.md`](docs/troubleshooting.md), linked from the README. It covers:
  - every hook denial, question and Stop block, and how to clear it;
  - `TEMPER_DISABLE=1` as the human emergency switch, and how to set it, since hooks inherit Claude Code's environment;
  - gate timeouts on large workspaces: the 900 s step timeout against the 1200 s hook timeout, and warming the cache;
  - Windows notes;
  - how to report a problem.

Release engineering.
- `tests/release-check.sh` makes the release checklist's version checks executable (#12). It fails when `plugin.json`, the top `## X.Y.Z (date)` CHANGELOG heading, the example workspace's vendored `VERSION` and, given `--tag`, the tag disagree. It also fails when a release that changes `scripts/` or `templates/workspace/` since the previous tag has no closing "Upgrading from" note, and, with `--tag`, when an `## Unreleased` section sits above the release. `tests/ci.sh` runs it without the tag.
- A Release workflow (`.github/workflows/release.yml`, #17), run by hand on a `release/X.Y.Z` branch. It runs the release check with the tag, the manifest validation and `tests/ci.sh`. After the owner approves in the protected `release` environment, it tags `vX.Y.Z` and creates the GitHub release from the CHANGELOG section. The owner then merges the release PR to `main`, which is what ships. `tests/release-check.sh --notes FILE` writes that section.

Process.
- `CONTRIBUTING.md` has a versioning policy (#18): what makes a temper release major, minor or patch for an adopted repository.

Compatibility.
- The README now states the minimum Claude Code version: 2.1.286. The smoke test passes on 2.1.286 and 2.1.288, and every hook in `hooks/hooks.json` was confirmed to run on both, with one exception. The `PreToolUse` `SubagentHandback` hook never runs on these versions. The reviewer's verdict is validated and recorded by `SubagentStop` instead, which also blocks an invalid verdict, so nothing is lost. The results are in `docs/history/evals.md`.
- `tests/e2e/smoke.sh` ends with a table of which registered hooks ran. It takes `CLAUDE_BIN` to test a pinned release, and it no longer inherits `TEMPER_BASE` from the session that runs it.

Upgrading from 1.2.x: `scripts/temper.mjs` and `scripts/lib/project.mjs` changed, so `temper setup --dry-run` reports the vendored copy as drift. The workspace template's `.github/workflows/rust.yml` (the `msrv` job) and `AGENTS.md` (the lint command) changed too. Refresh with `temper setup --force`, restore the template files you customised, and merge the `rust.yml` and `AGENTS.md` changes into your copies by hand. Commit the result as its own pull request. Or run `temper setup --vendor-only`, which refreshes just the vendored CLI, and merge the two template changes by hand. The README's Upgrading section has the steps.

## 1.2.3 (2026-10-01)

Windows fixes.
- `temper baseline --init` on an unformatted tree named the file by the verbatim path cargo reports on Windows (`//?/C:/.../src/lib.rs`) instead of `src/lib.rs`. Paths are now made relative after stripping the `\\?\` prefix.
- Paths under a directory with an 8.3 short name (`C:\Users\RUNNER~1\...`, as TEMP often is) were reported relative to the long name git gives the root, as `../../RUNNER~1/...`. Both are now compared in their long form when they differ. This affected every relative path temper prints or keys on, including the nested-workspace hint of `baseline --init`.
- `temper report` and `temper setup` in a copy of temper without `report.mjs` or `vendor.mjs` crashed with `ERR_MODULE_NOT_FOUND` on Windows instead of saying the command is not available in this copy.
- The unit tests skip their symlink cases on a host that cannot create symlinks (Windows without Developer Mode), and CI runs them on Windows.

## 1.2.2 (2026-10-01)

The marketplace in this repository is named `temper`, so the install is now `claude plugin marketplace add Wet-Ink-Corporation/temper`, then `claude plugin install temper@temper`. The `wet-ink` name stays free for a marketplace of all Wet Ink plugins (Weigh-In: wi-d75e63).
- The workspace and example templates' `.claude/settings.json` declare the `temper` marketplace and enable `temper@temper`.
- A repository set up with an earlier version keeps `wet-ink` in its `.claude/settings.json`. Rename the `extraKnownMarketplaces` key to `temper` and the `enabledPlugins` entry to `temper@temper`.

## 1.2.1 (2026-09-28)

Documentation only.
- The README's Brownfield adoption section now explains why a raised baseline is flagged but never fails the build. Inside Claude Code a raise is stopped at the source; anywhere else it is a question for the reviewer.
- It also gives a one-line CODEOWNERS recipe for repositories that want a hard sign-off, with its costs: it gates `--tighten` too, a sole owner cannot approve their own pull request, and private repositories need a paid GitHub plan.

## 1.2.0 (2026-09-28)

This release makes temper usable on existing codebases and reports compliance.

### Lint baseline (brownfield ratchet)

Before this release the gate, CI and the pre-commit hook ran `cargo clippy -- -D warnings` over the whole workspace. Merging the lint table into a legacy repository therefore turned every agent turn and every CI run red. The scanner and the manifest check already blocked only new violations; clippy and rustdoc did not. The README's claim that legacy code does not stop work was wrong for them.

**Recording.** `temper baseline --init` records clippy, rustdoc and scanner findings per crate and lint in `.claude/temper-baseline.json`, which you commit. It also records the crate directories and the host platform.
- Findings are collected with `--cap-lints warn`, so every crate is checked even when its dependencies have deny-level findings.
- Findings raised inside a macro are attributed to the call site.
- `--init` refuses an unformatted tree.

**The ratchet.** With a baseline, the gate, CI and the pre-commit hook fail only on:
- a finding in a line added, modified or moved since the session base or the pull request's merge base (renames are recognised, including unstaged ones and renamed crates);
- a crate's count for a lint rising, which catches a new warning on an untouched line;
- an `#[allow]` or `#[expect]` of a lint the workspace forbids.

Legacy findings are reported, not blocking.

**Without a baseline**, `baseline --check` requires zero findings, as `-D warnings` did. CI therefore always runs one lint step through the vendored CLI.

**Changing the baseline.**
- `--tighten` lowers counts after debt is paid and never raises them. It refuses on a host other than the one the baseline records, since counts lowered there can fail CI on untouched lines; `--any-host` overrides that for workspaces with no platform-specific code.
- `--update` raises them. That is a human decision (H-12): the CLI refuses it inside an agent session unless `TEMPER_ALLOW_RAISE=1` is set, and the Bash hook asks. The hook also asks on restoring an older baseline with `git checkout` or `git restore`, and it refuses shell writes to the file.
- The gate and CI compare the baseline with the base's copy and flag any raise in the gate summary, the pull-request report and the step summary. A raise is shown, never failed, so a human-approved `--update` still merges.

**Rule mapping.** Findings map to rule IDs and levels through `scripts/lib/rulemap.json`, which is generated from `rules.md` and checked for freshness by `tests/lint-claims.py`.

**Tests are not baselined.**

### Compliance report

`temper report` writes `report.json`, `report.md` and a self-contained `report.html`. It contains:
- findings by severity, with iron-rule findings called out;
- a per-crate scorecard: status (`clean`, `debt` or `regressed`), counts, new and fixed findings against the baseline, lint inheritance, charter presence and top rules;
- the top rules and files;
- findings in new code;
- any baseline raise.

CI publishes it on every run as an artifact and a step summary. A separate job, the only one with `pull-requests: write`, keeps one pull-request comment up to date.

### Distribution and setup

- `/temper:setup` copies the CLI into the repository at `.claude/temper/`, so CI and the pre-commit hook run it without the plugin.
- Setup now offers to record a baseline for existing code, after checking formatting and tests.
- `/temper:audit` builds its numbers from the report.

### CI template

- The gate job uses `fetch-depth: 0` and a single `Lint (temper)` step, with plain clippy only when the vendored CLI is absent. It runs a report step and uploads the report as an artifact.
- The pull-request comment lives in its own `report-comment` job.
- Every checkout sets `persist-credentials: false`.
- The MSRV, latest-deps and aarch64 jobs no longer fail on legacy warnings when a baseline exists.

### Fixes

- The tree fingerprint now includes the resolved commit. Before, every clean tree had the same fingerprint, so a gate result cached for one commit could be reused for another. A base from another repository is now ignored instead of producing an empty diff.

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
