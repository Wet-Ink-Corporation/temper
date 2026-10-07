# temper

The **rust-craft** engineering standard for Rust, plus the machinery that makes coding agents follow it, whether or not they "decide" to.

- **The standard** (`skills/rust-craft`) is a portable Agent Skill. It covers ownership and lifetimes, public API design, crate and module architecture, typed errors, TDD, performance and resources, async and concurrency, serde and structured output, modern edition-2024 idioms, and a catalogue of escape hatches with the correct shape for each. Every rule has an ID (`OWN-08`, `ERR-03`, `H-02`), which implementers cite in plans and reviewers cite in findings. There are 182 rules; `references/rules.md` is the registry, and `references/sources.md` records which outside guidance (the API Guidelines, Effective Rust, the Performance Book, Rust Atomics and Locks, the Cargo Book, Microsoft's guidelines and others) informed which rules, and what was deliberately left out. `templates/example-workspace/` is a complete event-sourced, hexagonal reference workspace that passes the gate, including under Miri. `tests/doc_examples.py` checks the Rust examples in the standard: "Do" examples are compiled and linted under the template lints (illustrative fragments are only scanned), and "Don't" examples that name a diagnostic must fire it.
- **The enforcement** (hooks, subagents, workflow commands, repository templates) runs in Claude Code.

## Why a plugin and not only a skill

A skill is advice that the model chooses to load. Rules the model can't skip have to live somewhere else: in the compiler, in hooks, or in a separate agent whose output is machine-checked. temper places each rule at the lowest layer that can enforce it:

| Layer | Mechanism | Deterministic? | What it covers |
|---|---|---|---|
| 1. Compiler | `[workspace.lints]`, `clippy.toml`, `rustfmt.toml`, `deny.toml`, toolchain pin | Yes | Every escape hatch a lint can see: unwrap, panic, indexing, `as` casts, `#[allow]`, print macros, leaks, `&String` params, panicking std calls (`thread::spawn`, `env::args`, float `Duration`), guards in scrutinees, tracing spans across `.await`... |
| 2. Just-in-time context | `SessionStart` injection (after startup, resume, clear and compaction), `.claude/rules/*.md` scoped by `paths:`, a charter per crate (`AGENTS.md`, imported by `CLAUDE.md`) | Loading is deterministic; adherence is not | The iron rules and each crate's local invariants arrive in context when the files are touched |
| 3. Skill gate | `PreToolUse` denies `.rs` and `Cargo.toml` edits until `temper:rust-craft` is loaded in this agent context | Yes | "The agent chose not to load the skill" |
| 4. Pre-write scanner | `PreToolUse` lexes the file as it would look after the edit and refuses newly introduced escape hatches, citing rule and fix. It also reads code in doctest fences, which clippy never sees. A member `Cargo.toml` without `[lints] workspace = true`, or with its own `[profile]`, is refused. | Yes | Catches hatches before they reach disk, with actionable feedback. Also catches the ones no lint sees: silent `Err` arms, `map[&k]`, `unwrap` in doc examples |
| 5. Human gates | `PreToolUse` → `ask` for lint and CI config edits, new dependencies and crates, `#[expect]` exceptions, removed tests, manifest switches that stop tests running (`test`, `doctest`, `harness`, `autotests`), `[patch]`, `rust-version` and edition changes, blind snapshot acceptance | Yes | H-05, H-12, H-16: the decisions agents must not make alone |
| 6. Anti-bypass | `PreToolUse:Bash` refuses shell writes to `.rs` files and `Cargo.lock`, `--no-verify`, `--ignore-rust-version`, and `-A`/`--cap-lints`; `Cargo.lock` edits are refused outright (PROC-13); the gate re-scans every changed file | Yes (heuristic for shell) | Routing around layers 3 and 4 |
| 7. Completion gate | `Stop` runs fmt, the scan, a manifest check (lint inheritance, no member profiles), clippy `-D warnings` (or the baseline ratchet), tests and rustdoc (cached per tree fingerprint), then requires an **approving review bound to that exact fingerprint** | Yes | "Done" means green and independently reviewed. `Status: WIP` or `Status: BLOCKED` is the honest exit. Stop holds the turn open but is not a hard stop: it does not run in plan mode, and after one block it lets the turn end if the tree has not changed ([the loop breaker](docs/troubleshooting.md#finishing-a-turn-the-stop-hook)). The pre-commit hook and CI (layer 9) are the hard stops. |
| 8. Independent review | `temper:rust-reviewer` runs in a fresh context with the skill preloaded and read-only tools (`gate --check`). Its verdict JSON is validated by `SubagentStop`: an approval with majors, a red gate, or unread changed files is refused. | The validation is deterministic | Judgment calls tools can't make: clone justification, API shape, architecture |
| 9. CI | The same gate in `.github/workflows/rust.yml`, SHA-pinned and `--locked`, plus each crate built alone, feature combinations (`cargo hack`), unused dependencies (`cargo machete`), the MSRV, and scheduled latest-deps, aarch64 and Miri jobs; Dependabot for bumps | Yes | Everything that happened outside Claude Code |
| 10. Measurement | `temper report` (per-crate compliance scorecard, published by CI), `temper stats` (telemetry of denials, blocks and verdicts) and `claude plugin eval` suites in `evals/` | n/a | Where the codebase stands, and whether the enforcement changes outcomes |

## Install

Requirements:
- **Claude Code 2.1.286 or later.** The end-to-end smoke test passes on 2.1.286, 2.1.288 and 2.1.289. Every hook in `hooks/hooks.json` ran on these versions except the `PreToolUse` `SubagentHandback` hook, which they never call; `SubagentStop` validates and records the reviewer's verdict instead ([results](docs/history/evals.md#smoke-runs)). Older releases are untested.
- **Node 18 or later** on the `PATH` that Claude Code sees. On Windows it must be a real `node.exe`; see [troubleshooting](docs/troubleshooting.md#windows).
- **git**, and the Rust toolchain that `rust-toolchain.toml` pins (1.98.1 in the template), with clippy and rustfmt.
- **Access to `Wet-Ink-Corporation/temper`.** The repository is internal to the organization, so `marketplace add` works only where git has credentials for it ([troubleshooting](docs/troubleshooting.md#cloud-sessions-and-claudeai)).
- **The Cargo workspace at the git root.** A workspace in a subdirectory of the repository (`rust/Cargo.toml` in a monorepo) is not supported yet: `temper baseline --init` refuses it, and `baseline --check` fails, with `temper needs the Cargo workspace at the git root … Nested workspaces are not supported yet.`

```bash
# as a marketplace (this repository)
claude plugin marketplace add Wet-Ink-Corporation/temper
claude plugin install temper@temper

# or for one session while developing
claude --plugin-dir /path/to/temper
```

Then, in a Rust repository, run `/temper:setup`. It installs the workspace lints, clippy, rustfmt and deny config, the toolchain pin, the `CLAUDE.md`/`AGENTS.md` hierarchy, the path-scoped rules, CI, a pre-commit hook and `.claude/temper.json`, and it writes a charter for every crate. The committed `.claude/settings.json` enables the plugin for everyone who opens the repository.

In Claude Code cloud sessions, install the plugin from the environment's setup script, because the repository's settings alone do not install it there. See [Cloud sessions and claude.ai](docs/troubleshooting.md#cloud-sessions-and-claudeai).

### First run

For a repository with no Rust code yet, follow [New project](#new-project). For existing code, follow the [Brownfield adoption](#brownfield-adoption) rollout, which records a baseline so that legacy findings do not block.

Then ask the agent for a small change. You will probably see some of these along the way. Each names the rule and what to do, and the agent acts on it:
- A Rust edit before the skill is loaded: `temper: the rust-craft standard is not loaded in this agent context, so this Rust edit was not applied.` The agent loads `temper:rust-craft` and retries.
- An edit that adds an escape hatch: `temper: this edit introduces N escape hatch(es), so it was not applied.`, then one line per hatch with its rule ID.
- Finishing without a review: `The gate is green, but the current tree (fingerprint …) has no approving independent review (PROC-08).` The agent delegates to `temper:rust-reviewer`.

[Troubleshooting](docs/troubleshooting.md) lists every message.

## Commands and agents

| | Purpose |
|---|---|
| `/temper:implement <task>` | Orchestrates the whole loop: frame, architect plan, human decisions up front, `rust-implementer` test-first, gate, `rust-reviewer`, fix loop (up to 3 rounds), Status report |
| `/temper:review [base]` | Independent review of the current diff in a forked reviewer context |
| `/temper:gate` | Run and summarize the gate |
| `/temper:setup` | Install templates and write crate charters |
| `/temper:audit [crate]` | Brownfield assessment and remediation plan (`.temper/audit.md`) |
| `temper:rust-architect` | Explore, then write a plan (placement, signatures, errors, test list, HITL decisions) |
| `temper:rust-implementer` | Red, green, refactor, self-review, gate, and a Status block. Returns `BLOCKED` on human decisions. |
| `temper:rust-reviewer` | Adversarial, read-only, fresh-context review with a validated, fingerprint-bound verdict |

## Configuration (`.claude/temper.json`)

```json
{
  "skillGate": true,
  "review": "required",
  "askOnNewDependency": true,
  "preapprovedDependencies": ["thiserror", "anyhow", "serde", "serde_json", "tracing", "tracing-subscriber", "proptest", "insta", "trybuild", "divan", "criterion", "tempfile"],
  "gate": { "allFeatures": true, "doc": true, "scan": true, "stepTimeoutSec": 900, "steps": null },
  "protected": ["**/clippy.toml", ".claude/**", "..."],
  "scanner": { "exempt": [{ "rule": "H-01", "paths": ["crates/ffi-sys/**"] }] },
  "manifests": { "ownLints": [] }
}
```

`review` can be `required`, `advisory` or `off`. `manifests.ownLints` lists members a human approved to carry their own full `[lints]` table instead of inheriting the workspace's (an FFI `-sys` crate, an arithmetic-lint opt-in); the hook asks on any change to those tables. The `TEMPER_DISABLE=1` environment variable turns every hook off; that is an emergency switch for a human, not something for agents ([how to use it](docs/troubleshooting.md#the-emergency-switch-temper_disable1)).

## CLI

`node scripts/temper.mjs <command>`. It has no dependencies, needs Node 18 or later, and runs on Windows, macOS and Linux. Hooks call it in exec form (`node <script>`), so no shell quoting is involved. `/temper:setup` also copies it into the repository at `.claude/temper/`, so CI and the pre-commit hook can run it without the plugin.

| Command | Purpose |
|---|---|
| `gate [--check] [--no-cache] [--json]` | fmt, scan, manifests, clippy, tests, rustdoc; ratchet mode when a baseline exists |
| `scan [FILES..] [--all] [--json]` | the escape-hatch scanner |
| `baseline [--init \| --update \| --tighten \| --check [--base REF] [--findings-out FILE]] [--json] [--locked] [--no-cache]` | record, lower or check the lint baseline (`--init` and `--update` are human decisions). Without a baseline, `--check` requires zero findings. |
| `report [--findings FILE] [--base REF] [--out DIR] [--format json,md,html] [--locked]` | the compliance scorecard: `report.json`, `report.md`, `report.html` (`--out` and `--findings-out` stay under `target/` or `.temper/`) |
| `status`, `stats`, `setup [--dry-run] [--force \| --vendor-only]` | tree and gate state; enforcement telemetry ([what it records](#local-data-and-telemetry)); install the templates (`--vendor-only`: refresh only the vendored CLI in `.claude/temper/`) |

## New project

A repository with no Rust code yet needs no baseline. The steps:

1. `git init`, then run `/temper:setup` (or `node <temper>/scripts/temper.mjs setup`). The template `Cargo.toml` is a workspace with `members = ["crates/*"]` and no members, so the gate stays red (`fmt FAILED`) until a crate exists.
2. Create the first crate with `cargo new --lib --vcs none crates/<name>`. Inside the workspace, `cargo new` already adds `[lints] workspace = true` to the crate's manifest; do not add it a second time, because a duplicate key breaks the manifest. Without `--vcs none`, `cargo new` can create a nested git repository in the crate directory, and temper then does not see the crate's files as changes.
3. Make sure `.gitignore` ignores `/target`, and commit `Cargo.lock`, because CI builds with `--locked` (PROC-13).
4. Expect the first gate to fail clippy on the stock crate: `missing documentation for the crate`, `missing documentation for a function` and ``this function could have a `#[must_use]` attribute``. Add a `//!` crate doc and a doc comment on each public item (H-15, API-15), and `#[must_use]` where clippy asks (API-11).
5. Give the crate a charter (ARC-08): run `/temper:setup` again, which writes one for each crate without an `AGENTS.md`, or copy `templates/crate/AGENTS.md` and `CLAUDE.md` into the crate and fill them in.
6. Turn on the pre-commit hook with `git config core.hooksPath .githooks`, and run `cargo machete` before the first push.

## Brownfield adoption

Legacy code must not stop work, and new code must meet the standard. temper does both with ratchets:

- **The pre-write scanner and the manifest check** compare against the session's base revision, so only violations the change introduces block.
- **Clippy, rustdoc and the scanner at the gate** compare against a committed **lint baseline**, `.claude/temper-baseline.json`, which counts findings per crate and lint. With a baseline present, the gate, CI and the pre-commit hook fail only on a finding in a line the change added or modified, or on a crate's count for a lint rising (which catches a new warning on an untouched line). Everything else is legacy debt: reported, not blocking. Without a baseline, zero findings are required, as before.
- **Touched means yours.** A legacy finding on a line you edit or move counts as new code (CRF-08). Renames, including unstaged ones and renamed crates, are recognised. Moving legacy code between files or functions is a change like any other: fix what you move.
- **Suppressions cannot hide behind the baseline.** `#[allow]` or `#[expect]` of a lint the workspace forbids always fails, as it would under `-D warnings`.
- **Tests are not baselined.** A red legacy suite is fixed, or triaged with `#[ignore = "reason"]` as a human decision, before adoption.
- **One platform.** Findings depend on the host's `cfg`s. Record and tighten the baseline on the platform CI runs. `--check`, `--init` and `--update` warn when the host differs from the one the baseline records; `--tighten` refuses, because counts lowered on another platform can fail CI on lines nobody touched. Pass `--any-host` only when the workspace has no platform-specific code. The cost: once linting moves to the ratchet, other platforms no longer lint. A repository whose CI ran clippy on Windows and macOS keeps building and testing there, but code under `cfg(windows)` or `cfg(target_os = "macos")` is linted only where the baseline is recorded.
- **A raise is shown, not failed.** Inside Claude Code a raise is stopped at the source: the CLI refuses it and the hook asks you. A raise made anywhere else (your own terminal, another agent) appears as `BASELINE RAISED`, with the change for each lint, in the gate, the step summary and the pull-request comment, and the merge goes through. A red build means the code regressed; a raise is a question for the reviewer.
- **For a hard sign-off**, as on a team or client repository, add `/.claude/temper-baseline.json @owner-or-team` and `/.claude/temper/ @owner-or-team` to `.github/CODEOWNERS` and turn on "Require review from Code Owners" in branch protection or a ruleset. CI runs the vendored CLI from the pull request itself, so a change to `.claude/temper/` can change what the gate reports; review it like CI configuration. It has three costs:
  - It gates `--tighten` too, which slows paying debt down and leaves the baseline loose.
  - GitHub never counts an author's approval of their own pull request, so a sole owner needs a ruleset bypass for their own changes.
  - Code owners on private repositories need a paid GitHub plan.

The rollout:

1. Run `cargo fmt --all` and commit that on its own first. Otherwise the first gate run reformats legacy lines and they count as changed; `baseline --init` refuses an unformatted tree for this reason.
2. On a branch, run `/temper:setup`. It installs the templates, vendors the CLI into `.claude/temper/`, merges the workspace lint table (you approve each protected edit), writes a charter per crate and offers to record the baseline. Review every "(inferred, confirm)" line in the charters: they are what agents read first.
3. Work through each `manual` line in `setup --dry-run`'s output. Files you already have stay yours, so setup reports what it would have changed:
   - **Strict lint steps.** A workflow step or documented command with `-D warnings` (`cargo clippy -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `RUSTFLAGS`) fails on every legacy finding once the lint table is merged, baseline or not. Lint with `temper baseline --check` (`rust.yml`'s `Lint (temper)` step), build and document without `-D warnings`, and update the commands in `CLAUDE.md`, `AGENTS.md` and the PR template to match.
   - **`clippy.toml`.** An existing one keeps its settings but needs the template's `allow-*-in-tests` keys. Without them, `unwrap_used`, `panic` and the other restriction lints fire in test code, and the baseline records every test as debt.
   - **`rust.yml`.** If you keep your own workflow, compare it with the template's. Its `msrv` job reads `rust-version` from `Cargo.toml`, and every job that compiles has a `Baseline mode` step.
   - **Unused dependencies.** Run `cargo machete` before the first push: `rust.yml` fails on an unused dependency.
4. Record the baseline (`TEMPER_ALLOW_RAISE=1 temper baseline --init`; inside Claude Code the hook asks you) and commit it with the setup. From then on, new code meets the full standard and legacy code keeps building.
5. Run `/temper:audit` for the remediation plan (`.temper/audit.md`), and pay the debt down crate by crate, each crate as its own reviewed change. Run `temper baseline --tighten` after each; the count only goes down. Raising it (`--update`) is a human decision (H-12): the CLI refuses it inside an agent session unless `TEMPER_ALLOW_RAISE=1` is set, and the hook asks. On a pull request, CI compares the baseline with the base branch's and flags any raise in the report and the PR comment.
6. Watch it in CI: every run publishes `temper report` as an artifact and a step summary, and a separate job with the only write permission comments the change on pull requests.

## Upgrading

An adopted repository runs two copies of temper:
- **The plugin** runs the hooks inside Claude Code. It updates through the marketplace.
- **The vendored CLI** in `.claude/temper/` runs in CI and the pre-commit hook. It changes only when you re-run setup and commit the result. Its version is in `.claude/temper/VERSION`.

Move both to the new release together, in one pull request:

1. **Update the plugin.** Run `claude plugin marketplace update temper` and then `claude plugin update temper@temper`, then restart Claude Code. From then on the hooks run the new version, while CI still runs the old vendored copy until step 4 is merged.
2. **Read the CHANGELOG** sections between the version in `.claude/temper/VERSION` and the new one. Releases that change the templates or the vendored CLI include upgrade steps.
3. **Preview, on a branch.** Run `/temper:setup` (it shows the plan before writing), or run `node <temper>/scripts/temper.mjs setup --dry-run` from a temper checkout. Three kinds of line matter:
   - A `drift` line for `.claude/temper/`: the vendored CLI is out of date. It names the plugin's version, the copy's version when it records one, the files that differ, and how to replace the copy. `up to date with temper X` means it is current.
   - `skip   <file> (exists; compare with <template>)`: setup never overwrites an existing template file without `--force`. It does not check whether that file differs from the template.
   - `manual Cargo.toml: …`: setup never edits the root manifest. This line appears only when it has no `[workspace.lints]`.
4. **Refresh.** If the vendored CLI is the only drift, which is the case when the release changes `scripts/` but not `templates/workspace/`, run `setup --vendor-only` and go on to step 5. It replaces `.claude/temper/`, removing files the new release no longer ships, and touches nothing else: no template file, no lint table, no `.gitignore`. Preview it with `setup --vendor-only --dry-run`.

   Otherwise, choose "install and overwrite" in `/temper:setup`, or run `setup --force`. This replaces the vendored CLI, removing files the new release no longer ships. It also overwrites **every template file that exists**, and prints `update` for each one whether or not it changed, so `git status` and `git diff` are the real record. Then:
   - Restore the files you customised: `git checkout -- AGENTS.md CLAUDE.md .claude/temper.json` and any others you edited (usually `.claude/settings.json`, `.claude/rules/*.md` and `.github/workflows/rust.yml`). If the template changed too, merge its change in by hand.
   - Merge any change to `[workspace.lints]` or `[workspace.package]` in the new `templates/workspace/Cargo.toml` into your root manifest by hand. Each one is a protected edit, so Claude Code asks you to approve it.
5. **Re-run the checks.** Run `cargo fmt --all`, then `node .claude/temper/temper.mjs gate --no-cache` and `node .claude/temper/temper.mjs baseline --check`.
   - New lints or a toolchain bump can add findings. Fix them, or record them with `baseline --update`. That is a human decision (H-12): run it as `TEMPER_ALLOW_RAISE=1 node .claude/temper/temper.mjs baseline --update` in your own terminal, on the platform CI runs.
   - If the counts went down, run `baseline --tighten`.
6. **Review the diff** before committing:
   - `.claude/temper/`: the changed files and `VERSION` should match the release. A change there should only ever come from `setup --vendor-only` or `setup --force`; `setup --dry-run` reports any difference from the plugin's copy.
   - `.github/workflows/rust.yml`, `rust-toolchain.toml` and `.github/dependabot.yml`: new jobs, new SHA pins, a new toolchain.
   - `.claude/settings.json`: the marketplace and plugin names, for example the 1.2.2 rename to `temper@temper`.
   - The lint tables, `clippy.toml` and `deny.toml`: what the gate now enforces.
   - `.claude/temper-baseline.json`: any raise shows as `BASELINE RAISED` in the pull-request report. Approve it deliberately.
7. **Merge** the upgrade as its own pull request (`temper X.Y.Z`), so CI runs the new vendored CLI on an otherwise unchanged tree. Ask everyone who works in the repository to update their plugin too, so that the hooks and CI enforce the same release.

A temper checkout serves as `<temper>` in step 3. The plugin's own copy, under Claude Code's plugin directory, works as well.

## Compliance reporting

`temper report` turns the findings into a scorecard: findings by severity (MUST, SHOULD, CONSIDER, and lints the standard does not cite), iron-rule findings called out, and per crate a status (`clean`, `debt`, `regressed`), counts, new and fixed against the baseline, lint inheritance, charter presence and top rules; then the top rules and files. It writes JSON for tooling, Markdown for the step summary and pull-request comment, and a self-contained HTML page. Per-change evidence stays with the change: the gate result and the reviewer's scored verdict (12 rubric dimensions, findings by rule ID) are stored per tree fingerprint under `.git/temper/`.

A sample, built from a test fixture of a six-crate workspace with legacy debt, one regressed crate and a raised baseline: [`docs/examples/sample-report.md`](docs/examples/sample-report.md). The `.html` and `.json` beside it are the same report in the other two formats.

## Context cost

The figures below are approximate: file sizes divided by four, as of this release.
- **Every session:** the `SessionStart` context, about 3 KB (0.8k tokens), again after resume, clear and compaction. The root `CLAUDE.md` and `AGENTS.md` from the template add about 5 KB (1.2k tokens) before you fill them in, and each `.claude/rules/*.md` file (1.4 to 3.3 KB) loads when matching files are touched.
- **Every agent context that edits Rust:** `skills/rust-craft/SKILL.md`, 17 KB (about 4.4k tokens). The skill gate refuses Rust edits until it is loaded.
- **Per task:** the topic reference the skill points to, 25 to 90 KB each (about 6k tokens for `process.md`, up to about 22k for `patterns.md`, which the skill says to grep rather than read whole). A typical task loads the skill and one or two references, about 15k to 30k tokens, and more when one of them is `api-design.md` or `patterns.md`. The rule registry `rules.md` (about 12k tokens) is meant to be grepped, not read; all the references together come to about 175k tokens, and nothing asks for that.
- **Per review round:** the reviewer runs in its own context, so the main window stays clean, but its tokens are billed. Its fixed load is about 10k tokens before the diff: its prompt (0.9k), the preloaded skill (4.4k), `review-rubric.md`, which it reads in full (4.1k), and the `SubagentStart` context (0.2k). A topic reference adds more. Each Stop that finds no approving verdict for the current tree can start another round.

The only measured cost is the smoke test: one session that adds a function, runs the gate and gets a review cost $0.26 to $0.36 in 11 to 13 turns ([results](docs/history/evals.md#smoke-runs)).

## Local data and telemetry

temper keeps its state in `.git/temper/` (in the main repository's git directory when you work in a worktree, and in `target/temper/` when the Cargo project is not inside a git repository): gate results and review verdicts per tree fingerprint, step logs, per-session state, and `telemetry.jsonl`. Inside `.git` it is never committed or pushed. The `target/temper/` fallback stays out of git only while `.gitignore` ignores `/target`, so check that before you `git init` such a project. temper's own code makes no network calls; the gate runs cargo, which can fetch crates.

`telemetry.jsonl` is what `temper stats` summarizes. The hooks append one JSON line, about 100 to 200 bytes, for each session start, skill load, denial, question, reviewer verdict and Stop outcome. Each line holds a timestamp, the Claude Code session ID, the agent type and the event name, and for some events a repository-relative file path, rule IDs, a fingerprint or a count. The file is never pruned, and only `stats` reads it, so delete it whenever you like: `rm "$(git rev-parse --git-common-dir)/temper/telemetry.jsonl"`, which also works in a worktree, where `.git` is a file. There is no separate switch to turn it off; `TEMPER_DISABLE=1` turns off the hooks, and with them the telemetry.

## Troubleshooting

[`docs/troubleshooting.md`](docs/troubleshooting.md) covers:
- what each hook denial, question and Stop block means, and how to clear it;
- installing temper in Claude Code cloud sessions, and what works in claude.ai chat;
- gate timeouts on large workspaces;
- Windows notes;
- how to report a problem.

The short version:
- Run `node .claude/temper/temper.mjs status` to see the gate and review state for the current tree.
- Read the full step logs in `.git/temper/logs/`.
- If temper itself is broken, start Claude Code with `TEMPER_DISABLE=1` to turn every hook off. That is for a human, never an agent. CI and the pre-commit hook still run.
- The hooks fail open: a hook error, a timeout or the Stop loop breaker lets the action go ahead rather than wedge the session. CI is the layer that fails closed ([what fails open](docs/troubleshooting.md#what-fails-open)).

## Using the standard with other agents

`skills/rust-craft/` is a plain Agent Skill: its frontmatter uses spec fields only and its paths are relative. Copy it to wherever Codex, Gemini or Junie read skills, and use the root `AGENTS.md` template, which carries the iron rules in tool-neutral form. Enforcement layers 1 and 9 (lints and CI) apply to every agent and every human. Layers 3 to 8 are specific to Claude Code.

## Development

```bash
tests/ci.sh                         # everything below that needs no model call, as CI runs it (--quick skips Rust builds)
node --test tests/*.test.mjs        # scanner, hook, manifest, baseline, report and vendor unit tests
python3 tests/doc_examples.py       # the skill's Rust and TOML blocks: Do blocks compile, pass clippy and tests
                                    # under the template lints; Don't blocks that carry expect= fire the
                                    # diagnostic they claim; Don't blocks without expect= are not compiled
                                    # and are reported without failing
                                    # (--fetch on a machine without a registry cache)
python3 tests/registry.py           # every rule has one section; every cited rule ID exists; counts match
python3 tests/lint-claims.py        # every lint and scanner key the standard cites exists and is enabled
python3 tests/gen_rulemap.py --check  # scripts/lib/rulemap.json matches rules.md
claude plugin validate --strict .claude-plugin/plugin.json   # plugin manifest + frontmatter
claude plugin validate --strict .   # the marketplace manifest
tests/e2e/brownfield.sh             # the lint baseline end to end on a legacy fixture (no model calls)
tests/e2e/smoke.sh                  # a real headless session with the plugin (a few model calls)
claude plugin eval . --allow-tools Write Edit --scaffold --trust-plugin --runs 3   # behavioral evals vs no-plugin baseline
```

CI (`.github/workflows/ci.yml`) runs the manifest checks and `tests/ci.sh` on every push to `main` and every pull request. The smoke test and the evals call a model, so they run by hand before a release.

The repository also keeps the record behind the code: `docs/history/` holds the design specs, independent reviews and source findings each release was built from, `.kb/_intake/decisions/` holds the decisions settled through Weigh-In, and `docs/examples/` holds a sample compliance report.

Eval runs that grant `Bash` need an OS sandbox (WSL2 on Windows) and the sandbox hides `~/.rustup`, so the suite grades static outcomes (skill loaded, no escape hatches in the produced file, honest reply, review verdict) rather than running cargo. `tests/e2e/smoke.sh` covers the gate end to end.

The eval command above grants only `Write` and `Edit`, so the with-plugin runs cannot run the gate. Their turns end `BLOCKED` or `WIP`, and the suite measures the plugin's refusals rather than the whole loop. Granting `Bash` also needs `bubblewrap` and `socat` on Linux, and a Rust toolchain reachable without `rustup` ([how it was run](docs/history/evals.md#full-suite-with-bash-and-a-sonnet-judge-2026-10-04)). The eval sandbox has no network, so a case cannot test adding a crate.
