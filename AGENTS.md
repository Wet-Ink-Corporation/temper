# temper (this repository)

This repository is temper itself: the Claude Code plugin that ships the rust-craft standard and its enforcement. It is also the `temper` marketplace that users install from. This file is for anyone working **on** temper, human or agent. The README explains how to **use** temper.

## `main` is what users get

New marketplace installs and the claude.ai organization sync get whatever `main` holds. An existing marketplace install moves only when `plugin.json`'s version changes. Every merge to `main` reaches some users, so keep it releasable.
- All changes go through a pull request with green CI. A ruleset protects `main`: the `Checks`, `Unit tests (Windows)` and `CHANGELOG entry` checks are required, and force-push is blocked.
- Keep `main` releasable. A change that is not ready for users waits on its branch.
- Versions and releases follow the checklist below.

## Layout

| Path | What it is |
|---|---|
| `skills/rust-craft/` | The standard: `SKILL.md`, and `references/` with the rule registry `rules.md` (182 rules) and its sources |
| `skills/{setup,implement,review,gate,audit}/` | The `/temper:*` commands |
| `agents/` | `rust-architect`, `rust-implementer`, `rust-reviewer` |
| `hooks/hooks.json` | Hook registrations; every hook runs `node scripts/temper.mjs hook <event>`. No `modules`: see `plugins/temper-lens/` |
| `plugins/temper-lens/` | The `temper-lens` plugin: the lens (a mod), its types, plugin tests, and vendored copies of `scripts/temper.mjs`, `scripts/lib/` and `rules.md` |
| `scripts/temper.mjs`, `scripts/lib/` | The CLI and hook engine: Node 18+, no dependencies |
| `templates/workspace/` | What `/temper:setup` installs in a user repository |
| `templates/example-workspace/` | A complete reference workspace that passes its own gate. Its `.claude/temper/` is a vendored copy of `scripts/`. |
| `tests/` | Node unit tests, Python consistency checks, e2e scripts, fixtures. `tests/parity/` is the 1.x-against-Rust differential harness of the 2.0 port, deleted at cutover (its `README.md`) |
| `evals/` | `claude plugin eval` cases |
| `docs/plans/` | Live plans: `v1-readiness.md` (the road to v1) and `temper-2-rust-port.md` (the 2.0 port) |
| `docs/history/` | Specs, reviews, findings and the decision log behind each release |
| `.kb/_intake/decisions/` | Weigh-In decision records (generated; see below) |
| `.claude-plugin/` | `plugin.json` (the version lives here) and `marketplace.json` (lists `temper` and `temper-lens`) |
| `Cargo.toml`, `crates/`, `xtask/` | The 2.0 port's Cargo workspace (see "Workspace charter" below). Users still run the Node engine until the cutover. |
| `LICENSE`, `LICENSE-APACHE` | The plugin is MIT; the crates are `MIT OR Apache-2.0` and each carries copies of both |

## Commands

| Purpose | Command |
|---|---|
| Everything CI runs that needs no model call | `bash tests/ci.sh` (`--quick` skips the steps that compile Rust) |
| The workspace's gate | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo test --workspace --all-features --locked`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked` (or `node scripts/temper.mjs gate`) |
| Licence copies in each crate | `cargo xtask check-licenses` |
| Version sites agree (the 2.0 workspace) | `cargo xtask versions --check` |
| Supply chain | `cargo deny check` |
| MSRV | `cargo +1.96 check --workspace --all-targets --locked` |
| Unit tests | `node --test tests/*.test.mjs` |
| Parity harness library and test-map checker (no binary) | `node --test tests/parity/harness.test.mjs`, `python3 tests/parity/check_test_map.py --self-test` |
| Scan parity cases (Rust engine against 1.x) | `cargo build -p temper-cli && TEMPER_BIN=target/debug/temper node --test tests/parity/scan.test.mjs` |
| 1.x scan tests are mapped to Rust tests | `python3 tests/parity/check_test_map.py` |
| Scanner corpus (release build; add `--root "$DIR"` after `python3 tests/doc_examples.py --extract "$DIR"` for the skill's blocks) | `node tests/parity/corpus.mjs --bin target/release/temper --lock Cargo.toml --lock templates/example-workspace/Cargo.toml --root templates/example-workspace` |
| Release check: `plugin.json`, the CHANGELOG heading, the vendored `VERSION` and the tag agree | `bash tests/release-check.sh [--tag vX.Y.Z]` |
| Rule registry, lint claims, rule map | `python3 tests/registry.py`, `python3 tests/lint-claims.py`, `python3 tests/gen_rulemap.py --check` |
| The code blocks in the standard | `python3 tests/doc_examples.py` (add `--fetch` without a warm registry cache) |
| Plugin and marketplace manifests | `claude plugin validate --strict .claude-plugin/plugin.json`, `claude plugin validate --strict plugins/temper-lens/.claude-plugin/plugin.json` and `claude plugin validate --strict .` |
| The lens: plugin tests, read-only allow-list, type check | `bash tests/lens-check.sh` |
| Brownfield e2e, no model calls | `bash tests/e2e/brownfield.sh` |
| Headless smoke session, a few model calls | `bash tests/e2e/smoke.sh` |
| Behavioural evals (model calls, costs money) | `claude plugin eval . --allow-tools Write Edit --scaffold --trust-plugin --runs 3` |

Needs Node 22+, Python 3.11+, git, and the toolchain in `templates/workspace/rust-toolchain.toml` with clippy and rustfmt. In Claude Code cloud sessions, `.claude/hooks/session-start.sh` installs the toolchain.

## Rules of the road

- **The vendored copy.** `templates/example-workspace/.claude/temper/` must match `scripts/` byte for byte, and its `VERSION` must match `plugin.json`; `tests/ci.sh` checks both. After changing `scripts/`:
  ```bash
  v=templates/example-workspace/.claude/temper
  cp scripts/temper.mjs "$v/" && rm -f "$v"/lib/* && cp scripts/lib/* "$v/lib/"
  node -p 'require("./.claude-plugin/plugin.json").version' > "$v/VERSION"
  l=plugins/temper-lens/scripts
  cp scripts/temper.mjs "$l/" && rm -f "$l"/lib/* && cp scripts/lib/* "$l/lib/"
  cp skills/rust-craft/references/rules.md plugins/temper-lens/references/
  ```
  The lens plugin's `plugins/temper-lens/references/rules.md` must match `skills/rust-craft/references/rules.md` too, and its `plugin.json` version must match temper's (`tests/release-check.sh`).
- **No mod in the temper plugin.** The desktop app's Chat and Cowork turn off every part of a plugin that ships a mod (`modules` in `hooks.json`), skills included, so temper's own `hooks/hooks.json` lists none and `plugin.json` names no `types`; `tests/ci.sh` fails otherwise. Mod code goes in `plugins/temper-lens/`.
- **The rule map.** After editing `skills/rust-craft/references/rules.md`, run `python3 tests/gen_rulemap.py` to regenerate `scripts/lib/rulemap.json`, then sync the vendored copies (and copy `rules.md` to `plugins/temper-lens/references/`).
- **Code in the standard is tested.** `tests/doc_examples.py` checks the Rust and TOML blocks in `skills/rust-craft/`. "Do" blocks (` ```rust `) are compiled and linted and must pass. "Don't" blocks that carry `expect=` must fire the diagnostic they claim. "Don't" blocks without `expect=` are not compiled; the harness lists them in its report without failing. `rust,fragment` blocks are scanned, not compiled, and TOML blocks are parsed. Other languages are counted and skipped.
- **Rule IDs are permanent.** Never renumber or reuse one. Every cited ID must exist (`tests/registry.py`).
- **Decisions.** Questions that need the owner go through Weigh-In. The records in `.kb/_intake/decisions/` are generated, so do not hand-edit them, except to repoint an anchor. Each decision is also listed in `docs/history/decisions.md`.
- **No `CLAUDE.md` at the repository root.** The repository root is the plugin root, and `claude plugin validate --strict` rejects a root `CLAUDE.md`, because a plugin never loads it. Project memory for working here is `.claude/CLAUDE.md`, which imports this file.
- **Scratch work** goes in `.temper/` (gitignored). A plan that others need goes in `docs/plans/`.
- **Pinned versions are bumped by hand, in their own PR.** That covers the Rust toolchain in `ci.yml` (including the `Rust MSRV` job's `1.96`, which follows `rust-version`), `rust-toolchain.toml` and `templates/*/rust-toolchain.toml`, the Claude Code CLI and TypeScript versions in `ci.yml`, and the plugin API types in `tests/lens-types/` (see its README). Dependabot bumps the SHA-pinned actions.

## Unattended sessions

An agent working with no human watching (an AFK session, a scheduled run) follows the rules above plus these:
- **Decide two-way calls; block on one-way ones.** A one-way or high-impact call waits for the owner. Such calls include a release that the plan does not schedule, a new dependency, anything H-16 lists, and a behaviour change to the hooks or the gate while a pilot runs on `main` (as wi-b63bd6 decided for the 2026-10 pilot). While it waits, the session does independent work.
- **Record every call.** Each assumption, deferral or call goes through Weigh-In and is listed in `docs/history/decisions.md`.
- **Merge only when everything is green.** Any agent session may merge a PR that meets this bar, without a per-session ask (wi-880b66). Only the owner can merge to `main`, through the ruleset's bypass, so an agent's merge acts with the owner's authority. The bar is every check on the current head green or skipped, not only the required ones, plus no merge conflict, no unresolved review thread, and an APPROVED verdict from a fresh-context reviewer, cited on the PR: `temper:rust-reviewer` for Rust, an independent review otherwise. Merge with a merge commit. Never merge a PR that waits on a one-way decision or has `[hold]` in its title, and never a release PR: the owner merges those (release checklist, step 6).
- **Hard limits.** Never run the paid model checks (`tests/e2e/smoke.sh`, `claude plugin eval`) without the owner's go-ahead. Never edit rulesets, environments, secrets, billing or repository settings. Never skip, disable or quarantine a test to get to green. Never push to `main` directly or rewrite a branch someone else owns.
- **Releases follow the schedule.** The plan in `docs/plans/v1-readiness.md` sets the schedule, and the release checklist below sets the steps. For a scheduled release, an agent may do steps 1 to 4 and start the Release workflow (step 5): its `check` job is free, and `publish` waits for the owner's approval. A minor or major release also needs step 3's smoke test and evals. Those are paid, so the agent asks for the go-ahead first or leaves them to the owner. The owner approves the `release` environment and merges.
- **Leave a trail.** End with a report of what merged, what was released, what was decided, and what waits on whom. Tick audit boxes in #57 to #70 with their PR citation, as wi-ddfdfe allows.

## Release checklist

A release goes through `.github/workflows/release.yml`, the flow the 2.0 port plan defines (§4):
1. Branch `release/X.Y.Z` from `main`. Bump `version` in `.claude-plugin/plugin.json` and `plugins/temper-lens/.claude-plugin/plugin.json`, and turn `## Unreleased` in `CHANGELOG.md` into a `## X.Y.Z (date)` section. If the release changes `scripts/` or `templates/workspace/`, the section ends with an "Upgrading from X.Y" note for adopted repositories: what drifts, and anything beyond the README's Upgrading steps.
2. Sync the vendored copies (above), including `VERSION`.
3. `bash tests/release-check.sh --tag vX.Y.Z` passes. It needs the tags (`git fetch --tags`), and fails when the versions or the tag disagree, when an `## Unreleased` section is left above the release, or when the "Upgrading from" note is missing. `bash tests/ci.sh` is green; it runs the release check without the tag. For a minor or major release, also run `tests/e2e/smoke.sh` and the evals, and append the results to `docs/history/evals.md`, and have the owner check that `/temper:gate` loads in a new Claude desktop app chat with a folder connected (no CI covers that surface).
4. Open the release PR to `main`.
5. Run the Release workflow on `release/X.Y.Z` (Actions, or `gh workflow run release.yml --ref release/X.Y.Z -f version=X.Y.Z`). Its `check` job runs the release check with the tag, the manifest validation and `tests/ci.sh`. Its `publish` job waits for the owner's approval in the `release` environment, then tags `vX.Y.Z` on the checked commit and creates the GitHub release from the CHANGELOG section. A re-run after the release exists does nothing.
6. The owner merges the release PR with a merge commit, not a squash, so the tag stays in `main`'s history. The merge is the release for marketplace users.

## Workspace charter (the 2.0 port)

The root is also a Cargo workspace, added in Phase 0a of `docs/plans/temper-2-rust-port.md`. It ports the CLI and hook engine from Node to Rust. Until the 2.0.0 cutover, `hooks/hooks.json` runs `scripts/temper.mjs` and nothing in the workspace reaches users. The workspace follows rust-craft, like the code the plugin governs, and temper's own hooks gate it.

### Crate map

| Crate | Layer | Purpose | Depends on |
|---|---|---|---|
| `crates/temper-domain` | domain | Scanner, manifest, config and verdict policy, models, renderers, message texts; no I/O | std (D10's crates as phases need them) |
| `crates/temper-app` | application | Use cases per subcommand and hook; owns the ports (`Files`, `ProcessRunner`, `Clock`) | domain |
| `crates/temper-host` | adapter | Process runner with timeouts, files, clock | app, domain |
| `crates/temper-cli` | binary | `temper`: arguments, wiring, output, exit codes | everything |
| `xtask` | tooling, unpublished | `cargo xtask check-licenses`; release tooling from Phase 0b | none |

Dependencies point inward: domain, then application, then adapters, then the binary (ARC-02). Each crate's `AGENTS.md` is its charter (ARC-08). Every member inherits `[lints] workspace = true` and `[workspace.package]`; sibling crates are declared once in `[workspace.dependencies]`.

Toolchain 1.98.1 (`rust-toolchain.toml`, a copy of the template's). MSRV 1.96 (D4); raising it is an H-16 decision. `clippy.toml`, `rustfmt.toml`, `deny.toml` and `.cargo/config.toml` are copies of `templates/workspace/`, which `tests/ci.sh` checks; the config only appends the `xtask` alias. The `exclude` list keeps the templates' own workspaces out of this one.

### Decisions log

The port's decisions D1 to D17 are recorded in full, with their Weigh-In records, in the port plan's "Decisions (H-16) record". In short:

| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | D1: npm packages `@wet-ink/temper-<platform>-<arch>` | H-16 | owner (wi-836680) |
| 2026-10-02 | D2: publish 4 crates to crates.io | H-16 | owner (wi-222f58) |
| 2026-10-02 | D3: four crates plus `xtask` | ARC-07, H-16 | owner (wi-0b4dc2) |
| 2026-10-02 | D4: MSRV 1.96, toolchain 1.98.1 | H-16 | owner (wi-5fc813) |
| 2026-10-02 | D5: six release targets | H-16 | owner (wi-4a9bfe) |
| 2026-10-02 | D6: hybrid hook behaviour while the fallback build is pending | H-16 | owner (wi-4abeef) |
| 2026-10-02 | D7: hand-written release workflow plus `xtask` | H-16 | two-way default (wi-f22cef) |
| 2026-10-02 | D8: port the masking lexer, no `syn` | H-16 | two-way default (wi-2f31ee) |
| 2026-10-02 | D9: `regex` plus explicit predicates | H-16 | two-way default (wi-e9386f) |
| 2026-10-02 | D10: dependencies `regex`, `toml`, `sha2`, `wait-timeout`; no `clap` and the like | ARC-12, H-16 | owner (wi-b03273) |
| 2026-10-02 | D11: 2.0.0 ships Rust only | H-16 | owner (wi-bf7535) |
| 2026-10-02 | D12: trusted publishing from the `release` environment | H-16 | owner (wi-6aac03) |
| 2026-10-02 | D13: user CI installs through `taiki-e/install-action` | H-16 | owner (wi-37c1df) |
| 2026-10-02 | D14: the pin lives in `.claude/temper-version` | H-16 | owner (wi-3b5428) |
| 2026-10-02 | D15: hash-guarded `setup --migrate` | H-16 | owner (wi-cdcecc) |
| 2026-10-02 | D16: the divergences and retired tests in §2.4 | H-12, H-16 | owner (wi-348e0b; amended by wi-5fbb56 and #64) |
| 2026-10-02 | D17: export the binary on PATH in Claude sessions | H-16 | owner (wi-1d4cd6) |
| 2026-10-02 | Crates are `MIT OR Apache-2.0`; the plugin stays MIT | H-16 | owner (wi-4679b3) |
| 2026-10-30 | macOS runs in CI on every code-touching PR | H-16 | owner (wi-e85ef8) |
| 2026-10-31 | Workspace version `2.0.0-alpha.0` and `publish = false` until Phase 0b's first alpha; no third-party dependency in Phase 0a | H-16 | two-way calls (Phase 0a) |
| 2026-11-01 | `serde` (pre-approved; `std` only, no `derive`) added in Phase 1d.2a; `wait-timeout` 0.2.1 (D10) added in 1d.2b, to `temper-host` only | ARC-12, H-16 | pre-approved; D10 (wi-b03273) |
| 2026-11-01 | The scanner corpus job (`.github/workflows/corpus.yml`): Linux, Windows and macOS; weekly, on `release/**`, by hand and on PRs that touch the engines or the scanner's inputs; advisory, not a required check. The parity cases and the test map run in `tests/ci.sh` under `Checks` | H-16 | owner (wi-295cf1, wi-68036c); OSes two-way (wi-fc6a1c) |
