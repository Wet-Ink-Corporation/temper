# temper 1.2: `temper baseline` (ratchet) and `temper report` (compliance scorecard)

The owner decided this (Weigh-In wi-cc1518, option A). The plugin lives at this repository. It is a git repo; do not commit.

## Problem

The gate and CI run `cargo clippy --workspace -- -D warnings`, and the pre-commit hook does the same. Once `/temper:setup` merges the template lint table into an existing codebase, every agent turn that changes Rust ends red on legacy debt, and so does CI. The scanner and the manifest check already ratchet: only new violations block. Clippy and rustdoc do not.

## Design (decided; implement exactly this)

### Findings

A **finding** is one diagnostic:

```json
{ "tool": "clippy|rustdoc|scan", "lint": "clippy::unwrap_used", "crate": "ledger-domain",
  "file": "crates/ledger-domain/src/account.rs", "line": 42, "end_line": 42,
  "message": "used `unwrap()` on an `Option` value", "rules": ["H-02"], "severity": "must|should|consider|unmapped" }
```

- **`lint` values:**
  - clippy lints as `clippy::name`;
  - rustc lints bare (`missing_docs`);
  - rustdoc lints as `rustdoc::name`;
  - scanner hits as `scan:<key>`.
- **`file`** is relative to the git root (or to the project root without git), with forward slashes.
- **`severity`** is the highest level among the finding's rules: MUST > SHOULD > CONSIDER. A lint no rule cites is `unmapped`.

A **collection** is `{ "temper_findings": 1, "at", "commit", "toolchain", "crates": [...], "findings": [...], "errors": [...] }`.
- Each crate is `{ "name", "dir", "lints_inherited": bool, "charter": bool }`.
- `errors` holds real compile errors: E-codes, and anything that is not a lint.

How each source is collected:

- **clippy:** `cargo clippy --workspace --all-targets [--all-features per config] --message-format=json -- --cap-lints warn`.
  - `--cap-lints warn` goes after `--`, so it reaches clippy-driver for workspace crates only. Every lint then reports without failing the build, and dependents still get checked. Do not use `RUSTFLAGS`: it invalidates the dependency cache.
  - Set `CARGO_BUILD_WARNINGS=warn` for the run so a `deny` config doesn't fail the build.
  - Parse the `compiler-message` lines. Keep diagnostics whose `code.code` is a lint name. Use the primary span (`is_primary`); `file_name` is relative to the workspace root.
  - Diagnostics with no lint code, or with an E-code, are `errors`. A collection with errors cannot be ratcheted: the gate fails with them, just as clippy `-D warnings` would.
  - Deduplicate a diagnostic that appears twice (lib and test targets) by (lint, file, line, message).
- **rustdoc:** `cargo doc --workspace --no-deps [--all-features] --message-format=json` with `RUSTDOCFLAGS=--cap-lints warn`. Parse the same way.
- **scan:** the scanner over every workspace `.rs` file, like `scan --all`. Keep `hard` and `ask` hits and drop `soft` ones; `lint` is `scan:<key>`.
- **crate attribution:** use `cargo metadata --no-deps` (each package's manifest dir). The longest matching dir wins.
- **`lints_inherited` and `charter`:** read them from the member manifest (the existing manifest helpers) and from whether `AGENTS.md` exists in the crate root.

### Rule map: `scripts/lib/rulemap.json`

Generate it from `skills/rust-craft/references/rules.md` with a Python generator at `tests/gen_rulemap.py`. It must use the same lint-resolution logic as `tests/lint-claims.py`: bare tokens after a `clippy::` token are clippy lints when clippy knows them, and `rustdoc::` is kept as is.

Shape:

```json
{ "generated_from": "rules.md", "lints": { "clippy::unwrap_used": ["H-02"], "missing_docs": ["H-15","API-15"] },
  "scan": { "print": ["H-06"] },
  "rules": { "H-02": { "level": "MUST", "family": "H", "title": "<first sentence of the Rule cell, backticks kept, <=140 chars>" } } }
```

- Scanner keys come from `scan.mjs`: each `RULES` entry's id, plus the keys `scanSource` pushes directly.
- `tests/lint-claims.py`, or a small new check it calls, must fail when the committed `rulemap.json` differs from a fresh generation.
- Both the plugin and the vendored copy (below) use the JSON; nothing parses `rules.md` at runtime.

### Baseline file: `.claude/temper-baseline.json` (committed)

The file sits under `.claude/`, which is already protected, so edits ask the human. `.temper/` is gitignored by setup, so it cannot live there.

```json
{ "temper_baseline": 1, "created": "ISO", "updated": "ISO", "commit": "sha", "toolchain": "1.98.1",
  "totals": { "<crate>": { "<lint>": 3 } },
  "files":  { "<file>": { "<lint>": 2 } } }
```

- Keys are sorted, and the file is pretty-printed with 2 spaces and a trailing newline, so diffs stay small.
- There are no zero entries.
- It covers clippy, rustdoc and scan findings. Tests are not baselined: a failing legacy suite gets fixed, or triaged with `#[ignore = "reason"]` as an H-12 decision.

### Ratchet check: `ratchet(collection, baseline, { base })`

The check fails when either condition holds:

1. **New code.** A finding sits on a line added or changed since `base`. Compute this with `git diff -U0 --no-color -M <merge-base(base, HEAD)>` against the working tree, and treat every line of an untracked file as new. That catches a new violation even when it swaps with a fix in the same crate. A legacy finding on a line the change touched also fails: you touch it, you fix it (CRF-08).
2. **Totals.** For some `(crate, lint)`, the current count exceeds the baseline total. That catches non-local effects: a signature change that makes an unchanged line warn.

Everything else counts as legacy debt and passes.

- **Improvements.** A `(crate, lint)` whose current count is below the baseline is reported as `fixed`, with the suggestion to run `temper baseline --tighten`.
- **No base** (a push build, or no git): only the totals check applies.
- **No baseline file:** the gate and CI behave exactly as today (`-D warnings`: everything must be clean), so a greenfield repo sees no change.
- **Result shape:** `{ ok, new_code: [finding], increases: [{crate, lint, baseline, current}], fixed: [...], legacy_total, baseline_total }`.

### CLI: `temper baseline`

| Command | Behaviour |
|---|---|
| `temper baseline` | Status: totals now vs the baseline, and whether it is tight. |
| `temper baseline --init` | Collect and write. Refuses if the file exists. |
| `temper baseline --update` | Rewrite from the current state; this may raise counts. |
| `temper baseline --tighten` | Lower counts to current values and drop zeros. It **never raises**. |
| `temper baseline --check [--base REF] [--findings-out FILE] [--json]` | Run the ratchet. Exit 0 on pass, 1 on fail, 2 on a usage error or when a baseline is required but absent. |

- With `--findings-out`, write the collection JSON so `report` can reuse it without a second cargo run.
- `--init` and `--update` are human decisions: raising the baseline weakens verification (H-12). The Bash hook **asks** on them (below). `--tighten` and `--check` run freely.

### Gate integration (`scripts/lib/gate.mjs`)

**When `.claude/temper-baseline.json` exists:**
- The `clippy` and `doc` steps run in ratchet mode: collect (clippy and rustdoc), then call `ratchet()` with the gate's session base.
- The step summary lists new-code findings as `file:line lint message` (at most 30), then any increases.
- One trailing line gives the debt: `legacy: N findings (baseline M); K fixed since the baseline: run temper baseline --tighten`.
- Cache the collection at `<stateDir>/findings-<fp>.json`, so `report` and the reviewer's `gate --check` reuse it.
- `test` and `doctest` are unchanged.
- The fingerprint's relevant pathspecs must include `.claude/temper-baseline.json`, because a baseline change changes the verdict.

**When no baseline exists:** the gate behaves exactly as today.

A `gate.steps` override in `temper.json` still wins.

### Hooks (`scripts/lib/hooks.mjs`)

- **preBash, ask (H-12):** `temper.mjs baseline` with `--init` or `--update`, whether run through `node …/temper.mjs` or `$TEMPER_CLI`. `--tighten`, `--check` and status run freely.
- **preBash, deny:** shell writes (redirects, `sed -i`, `tee`, `cp`/`mv` onto it) to `.claude/temper-baseline.json`. The message says to run `temper baseline --tighten`, or to ask the human for `--update`.
- **Edit/Write** to the baseline file already asks, via the protected `.claude/**` glob. Keep that; a test asserts it.

### `temper report` (`scripts/lib/report.mjs`, export `runReport(argv)`)

`temper report [--findings FILE] [--base REF] [--out DIR] [--format json,md,html]`. It reads `--findings`, or the cached collection for the current fingerprint, or else collects. It also reads the baseline if present and runs the ratchet if `--base` is given. It writes `DIR/report.json`, `DIR/report.md` and `DIR/report.html`; the default `DIR` is `target/temper-report`.

**Content:**
- **Workspace summary.** Findings by severity (MUST / SHOULD / CONSIDER / unmapped). Iron-rule (H-*) findings called out. Compared with the baseline: legacy, fixed, new. The number of crates that are clean, carrying debt, or regressed.
- **Per-crate scorecard.** One row per crate:
  - status: `clean` (no MUST findings), `debt` (findings within the baseline) or `regressed`;
  - MUST / SHOULD / other counts;
  - H-rule count;
  - fixed and new counts against the baseline;
  - whether the crate inherits the workspace lints;
  - whether it has a charter;
  - its top 3 rules.
- **Top 15 rules by count.** Each row shows the rule ID, level, title and count.
- **Top 15 files by MUST count.**
- **New-code findings** (when `--base` is given), with `file:line`.
- **Provenance.** Generated time, commit, toolchain, and the temper version (from `plugin.json`, or `VERSION` in the vendored copy).

**Markdown:** compact enough for `$GITHUB_STEP_SUMMARY` and a PR comment. Keep it under 60 lines, and put details in HTML.

**HTML:** one self-contained file with no external assets.
- Inline CSS, with colours defined as custom properties on `:root`.
- A dark theme under `@media (prefers-color-scheme: dark)`.
- Readable at phone width, with tables that scroll horizontally inside a wrapper.
- Escape every string taken from diagnostics.
- It needs no JS. Sorting is optional.

**JSON:** the machine-readable superset.

### Vendoring (`scripts/lib/vendor.mjs`, export `vendorTool(root, {dry, force})`)

This copies `scripts/temper.mjs` and `scripts/lib/*` (including `rulemap.json`) into `<repo>/.claude/temper/`, plus a `VERSION` file holding the plugin version.
- CI and the pre-commit hook run `node .claude/temper/temper.mjs …` without the plugin.
- If the copy exists and differs, `--force` replaces it; otherwise report the drift.
- The vendored CLI must run standalone. Hook subcommands may exist in the copy, but CI never calls them.
- The `setup` command integration (wiring `vendorTool` into `setup()` in `temper.mjs`) is done by the orchestrator.

### CI template (`templates/workspace/.github/workflows/rust.yml`, mirrored in `templates/example-workspace/`)

**Gate job:**
- `actions/checkout` gets `fetch-depth: 0` (the ratchet needs the merge base).
- Replace the single clippy step with two steps:
  - `Lint (ratchet)`, with `if: hashFiles('.claude/temper-baseline.json') != ''`. It runs `node .claude/temper/temper.mjs baseline --check --base "origin/${GITHUB_BASE_REF:-main}" --findings-out target/temper/findings.json`. On push events, base on `github.event.before` when that is not all zeros; otherwise run the totals-only check.
  - `Lint`, with `if: hashFiles('.claude/temper-baseline.json') == ''`: the existing clippy `-D warnings` command.
- The doc step gets the same split: ratchet mode is covered by the baseline check, so skip `cargo doc -D warnings` when a baseline exists, but keep `cargo doc` for broken builds.
- A `Compliance report` step, with `if: always() && hashFiles('.claude/temper/temper.mjs') != ''`:
  - runs `report --findings target/temper/findings.json` (falling back to collecting);
  - appends `report.md` to `$GITHUB_STEP_SUMMARY`;
  - uploads `target/temper-report` with `actions/upload-artifact`, pinned to a commit SHA resolved via `git ls-remote`.
- A PR comment step, only on `pull_request`, with `continue-on-error: true` because fork PRs have a read-only token:
  - `gh pr comment "$PR" --body-file target/temper-report/report.md --edit-last`, falling back to a new comment when there is no previous one. Verify the `gh` flags against gh's docs or `--help`, and do not guess.
  - It needs job-level `permissions: pull-requests: write` for this job only.

**Pre-commit hook (`templates/workspace/.githooks/pre-commit`):** when both the baseline and the vendored tool exist, run `node .claude/temper/temper.mjs baseline --check --base HEAD`; otherwise keep the current clippy command.

**`templates/workspace/.claude/settings.json`:** allow `Bash(node .claude/temper/temper.mjs baseline --check*)`, `…baseline --tighten*`, `…report*` and `…gate*`. Do not allow `--init` or `--update`.

### Example workspace

The example is clean, so it has no baseline.
- It gets a vendored `.claude/temper/`, via `vendorTool` once that is written, so its CI report step works.
- Add a test (Node, in `tests/`) that fails when the example's vendored copy differs from `scripts/`.

### Tests

Everything stays zero-dependency, Node 18+, Windows-safe (backslashes, CRLF).

**Unit tests:**
- clippy JSON parsing, using real captured output (put fixtures in `tests/fixtures/`);
- diff hunk parsing: added and changed lines, deletions, renames, untracked files;
- ratchet:
  - new code fails;
  - a swap in the same crate fails;
  - a rename with an unchanged count passes;
  - a total increase on an untouched line fails;
  - no base means totals only;
- `tighten` never raises;
- `rulemap` resolution;
- report rendering: the numbers match the input, and HTML-escaping holds against a message containing `<script>`;
- hooks: `--init` asks, `--tighten` is allowed, a shell write to the baseline is denied.

**`tests/e2e/brownfield.sh`** (no model calls):
1. Build a scratch git repo with the template lints and a crate carrying legacy debt: `unwrap`, indexing, a missing doc and an `as` cast.
2. Commit, then `temper baseline --init`, then commit.
3. `temper gate` is **green**, and reports legacy debt.
4. Add a new `unwrap` on a new line: the gate is **red** and names it.
5. Revert, then fix one legacy `unwrap` and add another in the same file (a swap): **red**.
6. Revert, then fix one legacy finding: the gate is green and reports `fixed: 1`. `baseline --tighten` lowers the count, and a follow-up `--check` passes.
7. `temper report` writes all three files, and their numbers match.

It should run in under 2 minutes and use `CARGO_TARGET_DIR` inside the scratch dir.

### Constraints

- Zero-dependency Node 18+, and cross-platform.
- Keep the gate fast: one clippy run per gate, plus rustdoc if enabled, and reuse the cache.
- Never write the baseline or the vendored tool from a hook.
