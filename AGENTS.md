# temper (this repository)

This repository is temper itself: the Claude Code plugin that ships the rust-craft standard and its enforcement. It is also the `temper` marketplace that users install from. This file is for anyone working **on** temper, human or agent. The README explains how to **use** temper.

## `main` is what users get

Marketplace users install whatever `main` holds, so every merge to `main` reaches them.
- All changes go through a pull request with green CI. A ruleset protects `main`: the `Checks`, `Unit tests (Windows)` and `CHANGELOG entry` checks are required, and force-push is blocked.
- Keep `main` releasable. A change that is not ready for users waits on its branch.
- Versions and releases follow the checklist below.

## Layout

| Path | What it is |
|---|---|
| `skills/rust-craft/` | The standard: `SKILL.md`, and `references/` with the rule registry `rules.md` (182 rules) and its sources |
| `skills/{setup,implement,review,gate,audit}/` | The `/temper:*` commands |
| `agents/` | `rust-architect`, `rust-implementer`, `rust-reviewer` |
| `hooks/hooks.json` | Hook registrations; every hook runs `node scripts/temper.mjs hook <event>` |
| `scripts/temper.mjs`, `scripts/lib/` | The CLI and hook engine: Node 18+, no dependencies |
| `templates/workspace/` | What `/temper:setup` installs in a user repository |
| `templates/example-workspace/` | A complete reference workspace that passes its own gate. Its `.claude/temper/` is a vendored copy of `scripts/`. |
| `tests/` | Node unit tests, Python consistency checks, e2e scripts, fixtures |
| `evals/` | `claude plugin eval` cases |
| `docs/plans/` | Live plans: `v1-readiness.md` (the road to v1) and `temper-2-rust-port.md` (the 2.0 port) |
| `docs/history/` | Specs, reviews, findings and the decision log behind each release |
| `.kb/_intake/decisions/` | Weigh-In decision records (generated; see below) |
| `.claude-plugin/` | `plugin.json` (the version lives here) and `marketplace.json` |

## Commands

| Purpose | Command |
|---|---|
| Everything CI runs that needs no model call | `bash tests/ci.sh` (`--quick` skips the steps that compile Rust) |
| Unit tests | `node --test tests/*.test.mjs` |
| Release check: `plugin.json`, the CHANGELOG heading, the vendored `VERSION` and the tag agree | `bash tests/release-check.sh [--tag vX.Y.Z]` |
| Rule registry, lint claims, rule map | `python3 tests/registry.py`, `python3 tests/lint-claims.py`, `python3 tests/gen_rulemap.py --check` |
| The code blocks in the standard | `python3 tests/doc_examples.py` (add `--fetch` without a warm registry cache) |
| Plugin and marketplace manifests | `claude plugin validate --strict .claude-plugin/plugin.json` and `claude plugin validate --strict .` |
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
  ```
- **The rule map.** After editing `skills/rust-craft/references/rules.md`, run `python3 tests/gen_rulemap.py` to regenerate `scripts/lib/rulemap.json`, then sync the vendored copy.
- **Code in the standard is tested.** `tests/doc_examples.py` checks the Rust and TOML blocks in `skills/rust-craft/`. "Do" blocks (` ```rust `) are compiled and linted and must pass. "Don't" blocks that carry `expect=` must fire the diagnostic they claim. "Don't" blocks without `expect=` are not compiled; the harness lists them in its report without failing. `rust,fragment` blocks are scanned, not compiled, and TOML blocks are parsed. Other languages are counted and skipped.
- **Rule IDs are permanent.** Never renumber or reuse one. Every cited ID must exist (`tests/registry.py`).
- **Decisions.** Questions that need the owner go through Weigh-In. The records in `.kb/_intake/decisions/` are generated, so do not hand-edit them, except to repoint an anchor. Each decision is also listed in `docs/history/decisions.md`.
- **No `CLAUDE.md` at the repository root.** The repository root is the plugin root, and `claude plugin validate --strict` rejects a root `CLAUDE.md`, because a plugin never loads it. Project memory for working here is `.claude/CLAUDE.md`, which imports this file.
- **Scratch work** goes in `.temper/` (gitignored). A plan that others need goes in `docs/plans/`.
- **Pinned versions are bumped by hand, in their own PR.** That covers the Rust toolchain in `ci.yml` and `templates/*/rust-toolchain.toml`, and the Claude Code CLI version in `ci.yml`. Dependabot bumps the SHA-pinned actions.

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
1. Branch `release/X.Y.Z` from `main`. Bump `version` in `.claude-plugin/plugin.json`, and turn `## Unreleased` in `CHANGELOG.md` into a `## X.Y.Z (date)` section. If the release changes `scripts/` or `templates/workspace/`, the section ends with an "Upgrading from X.Y" note for adopted repositories: what drifts, and anything beyond the README's Upgrading steps.
2. Sync the vendored copy (above), including `VERSION`.
3. `bash tests/release-check.sh --tag vX.Y.Z` passes. It needs the tags (`git fetch --tags`), and fails when the versions or the tag disagree, when an `## Unreleased` section is left above the release, or when the "Upgrading from" note is missing. `bash tests/ci.sh` is green; it runs the release check without the tag. For a minor or major release, also run `tests/e2e/smoke.sh` and the evals, and append the results to `docs/history/evals.md`.
4. Open the release PR to `main`.
5. Run the Release workflow on `release/X.Y.Z` (Actions, or `gh workflow run release.yml --ref release/X.Y.Z -f version=X.Y.Z`). Its `check` job runs the release check with the tag, the manifest validation and `tests/ci.sh`. Its `publish` job waits for the owner's approval in the `release` environment, then tags `vX.Y.Z` on the checked commit and creates the GitHub release from the CHANGELOG section. A re-run after the release exists does nothing.
6. The owner merges the release PR with a merge commit, not a squash, so the tag stays in `main`'s history. The merge is the release for marketplace users.

## When the 2.0 port starts

Phase 0a of `docs/plans/temper-2-rust-port.md` adds a Cargo workspace at the repository root. This file then gains the workspace charter: the crate map, the layer rules and a decisions log, in the shape of `templates/workspace/AGENTS.md`. The commands above gain the cargo steps.
