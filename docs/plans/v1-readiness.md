# temper: repository evaluation and the plan to v1 live

Written 2026-10-02, against `main` at `57810da`.

## What "v1 live" means here

temper already ships 1.x releases (1.0.0 to 1.2.3). None of them is in use yet: no Rust repository runs it, and no repository depends on it in CI. This plan treats **v1 live** as follows:

> The 1.x line (the Node CLI and hooks) is installed and enforcing in real Wet Ink Rust repositories. Their CI requires the temper gate. A release process, an upgrade path and a support channel exist. The behavioural evals show that the plugin changes outcomes.

The temper 2.0 Rust port (wi-30ed25, D1 to D17) comes **after** v1 live, not before. The decisions for it are made, but no code exists yet. Shipping 2.0 first would put a large rewrite in front of the first real user, and the pilot findings from v1 should shape the port. If v1 should instead mean the Rust 2.0 engine, phases 0 to 2 still apply unchanged, and phase 5 moves before phase 4.

---

## Part 1: evaluation

### Product: further along than its usage

| Area | State | Evidence |
|---|---|---|
| The standard (`skills/rust-craft`) | Mature | 182 rules with IDs, 15 reference files, and a sources record. `tests/registry.py` checks 2754 citations with 0 failures. All 329 code blocks compile and lint under the template lints (`tests/doc_examples.py`, 0 failures). |
| Enforcement (hooks, CLI) | Feature-complete for 1.x | 10 layers, from the compiler to measurement. About 5.6k lines of dependency-free Node across 13 modules. 8 hook events. |
| Brownfield support | Built | Lint baseline ratchet, `temper report`, PR comment job (1.2.0). `tests/e2e/brownfield.sh` covers it. |
| Workflow surface | Built | 5 skills/commands (`setup`, `implement`, `review`, `gate`, `audit`) and 3 agents (`architect`, `implementer`, `reviewer`). |
| Templates | Built | A `workspace` template, and a complete `example-workspace` that passes its own gate, including under Miri. |
| Tests | Strong | 140 unit tests (139 pass, 1 platform skip). Registry, lint-claim, rule-map and vendored-copy checks pass, and so do the doc examples and the brownfield e2e. All ran locally on 2026-10-02. CI runs on Linux and Windows. |
| Behavioural evals | Thin | 3 cases, and only one separates the two arms (`refuses-unwrap-under-pressure`, +0.67). The mean Δ is +0.22. `docs/history/evals.md` already says harder cases are needed. |
| Real-world use | **None** | No adopting repository. Several defaults say "check in the first repository that adopts the template" (for example wi-a9eb36, Dependabot `lockfile-only`). |
| temper 2.0 (Rust port) | Decided, not started | 18 decision records (wi-30ed25 and D1 to D17; D7 to D9 have no record). No Rust crates in the repo. **The port plan the records cite, `.temper/plans/temper-2-rust-port.md`, is not in git** (`.temper/` is gitignored). |

### Source control and repository setup

**What is good:**
- Clean history: 22 commits with descriptive messages and decision trailers (`Weigh-In: wi-… decided A`). The 1.0 to 1.2.1 history was merged in rather than squashed.
- Tags and GitHub releases exist for v1.0.0 to v1.2.3.
- CI is least-privilege: `contents: read`, SHA-pinned actions, `persist-credentials: false`, a concurrency group, and Linux plus Windows jobs. CI is green on `main`.
- Dependabot keeps the action pins current. `.gitattributes` forces LF, `.editorconfig` matches it, and `.gitignore` covers the IDE, OS, Python and temper scratch files.
- Decision records (`.kb/_intake/decisions/`) and design history (`docs/history/`) are committed.

**Gaps that block effective ongoing development:**

| # | Gap | Why it matters |
|---|---|---|
| G1 | Every change is pushed straight to `main`. There have been 0 pull requests and 0 issues. | Nothing reviewed the changes or gated them on CI. Run #3 (the `temper 1.2.3` commit) was red on `main` until the next commit fixed it. A plugin that requires reviewed, gated changes should be built that way itself. |
| G2 | The 2.0 port plan exists only in the owner's local, gitignored `.temper/plans/`. | 18 committed decisions anchor to line numbers in a file nobody else can read. One lost laptop loses the plan. |
| G3 | There is no backlog. No issues, milestones or labels exist, and remaining work lives in chat sessions and `docs/history/` prose. | Nobody, human or agent, can see what is next or what is blocked. |
| G4 | There are no contributor files for this repository: no root `CLAUDE.md`/`AGENTS.md`, `CONTRIBUTING.md`, `CODEOWNERS`, PR or issue templates, or `SECURITY.md`. | Agents working here rediscover the release checklist each time: bump `plugin.json`, the vendored `VERSION`, sync `templates/example-workspace/.claude/temper/`, and update the CHANGELOG. |
| G5 | Releases are made by hand. Five of the six releases were published retroactively, all within 5 seconds at 01:55Z on 2026-10-02. | Nothing checks that the tag, `plugin.json`, the CHANGELOG heading and the vendored `VERSION` agree, or that the evals and smoke test ran. |
| G6 | `docs/history/decisions.md` stops at the repository setup. | It omits the 2.0 decisions (wi-30ed25, D1 to D17), so the index and the records disagree. |
| G7 | Cloud and agent sessions have no environment bootstrap. | `tests/ci.sh` fails at "lint and scanner claims" until the pinned 1.98.1 toolchain is installed (seen in this session). |
| G8 | The repository is `internal`. | `claude plugin marketplace add Wet-Ink-Corporation/temper` works only for org members with GitHub auth. That may be right, but it should be decided before go-live rather than discovered then. |
| G9 | A time-sensitive item is open: D2 says "reserve the names now". | `temper-cli`, `temper-domain`, `temper-app` and `temper-host` were free on crates.io on 2026-10-02. Anyone can take them. |

---

## Part 2: the plan

Sizes: **S** is under half a day, **M** is 1 to 2 days, **L** is 3 days or more. **Owner** marks a step only the repository owner can do: GitHub settings, registries, or decisions. Everything else an agent can do through pull requests.

### Phase 0: get organized (repository and source control)

*Exit:* every change reaches `main` through a PR with green CI. The backlog is visible in GitHub. A new contributor or agent can find how to develop and release without reading chat history.

| Step | Work | Size | Who |
|---|---|---|---|
| 0.1 | **Protect `main`** with a ruleset: require a PR, require the `Checks` and `Unit tests (Windows)` status checks, block force-push and deletion, and give the owner a bypass for emergencies. Protect `v*` tags. (G1) | S | Owner |
| 0.2 | **Commit the 2.0 port plan** to `docs/plans/temper-2-rust-port.md`. Rewrite the decision-record anchors from `.temper/plans/…` to the committed path, or add a note mapping them. (G2) | S | Owner provides the file; agent commits |
| 0.3 | **Add contributor files**: a root `CLAUDE.md`/`AGENTS.md` for developing temper itself (layout, `tests/ci.sh`, the vendored-copy rule, the version-bump checklist), `CONTRIBUTING.md`, `.github/CODEOWNERS`, a PR template with the release checklist boxes, issue templates (bug, false positive or negative, rule proposal), and `SECURITY.md`. (G4) | M | Agent |
| 0.4 | **Set up the backlog**: labels (`area:standard`, `area:hooks`, `area:cli`, `area:templates`, `area:evals`, `type:bug`, `type:false-positive`, `pilot`), milestones (`v1 live`, `2.0`), and one issue per step of phases 1 to 5 of this plan. (G3) | S | Agent; owner approves |
| 0.5 | **Bring `docs/history/decisions.md` up to date** with wi-30ed25 and D1 to D17. Record what happened to D7 to D9 (defaulted, dismissed or still open). (G6) | S | Agent |
| 0.6 | **Bootstrap agent and cloud sessions**: a `SessionStart` hook or environment setup script that installs the pinned toolchain (1.98.1 with clippy and rustfmt) and Node 22, so `tests/ci.sh` runs green in a fresh container. (G7) | S | Agent |
| 0.7 | **Decide the audience and visibility** for v1: internal org only, or public. This decides who can install from the marketplace and whether `docs/history/` needs another look, since wi-96caeb was decided while the repository was private. (G8) | S | Owner (Weigh-In) |
| 0.8 | **Reserve the crates.io names and the `@wet-ink` npm scope** per D1 and D2, if they are still wanted. This is a public, one-way action and does not depend on any port code. (G9) | S | Owner |

### Phase 1: harden 1.x (milestone `v1 live`, released as 1.3.0)

*Exit:* the evals show a clear with/without gap on realistic cases, and the known rough edges are fixed or documented.

| Step | Work | Size |
|---|---|---|
| 1.1 | **Add discriminating evals**, starting with the three `evals.md` already names: a brownfield change under the baseline, a review that must catch a raised baseline, and an async lock held across `.await`. Add two more from areas that today have no case: a `Cargo.toml` dependency ask, and a skipped-test or `#[ignore]` attempt. Target: mean Δ ≥ +0.4 over ≥ 3 runs per arm. | L |
| 1.2 | **Check compatibility with current Claude Code.** CI pins `@anthropic-ai/claude-code@2.1.286` for manifest validation. Confirm every hook event and matcher in `hooks/hooks.json` still fires on the current release (the smoke test covers this), and record the minimum supported Claude Code version in the README. | M |
| 1.3 | **Write an upgrade path for adopted repositories**: how a repository moves from one 1.x vendored copy (`.claude/temper/`) to the next. That means `temper setup` drift detection, then what to re-run and what to review. Add it to the README and CHANGELOG. | S |
| 1.4 | **Write troubleshooting docs**: what each hook denial means, `TEMPER_DISABLE=1` as the human emergency switch, gate timeouts on large workspaces, and Windows notes. | S |
| 1.5 | **Make the release checklist executable.** Add a `tests/release-check.sh`, or a CI job on `v*` tags, that asserts the tag, `plugin.json`, the CHANGELOG heading and `templates/example-workspace/.claude/temper/VERSION` all agree, then creates the GitHub release from the CHANGELOG section. (G5) | M |

### Phase 2: pilot (dogfood on real repositories)

*Exit:* two real repositories have run temper day to day for at least two weeks, every pilot finding has an issue, and no P0 or P1 issue is open.

| Step | Work | Size | Who |
|---|---|---|---|
| 2.1 | **Pick two pilot repositories**: one greenfield or small, and one brownfield Wet Ink Rust repository with real lint debt. | S | Owner |
| 2.2 | **Adopt temper in each**: `cargo fmt` commit, then `/temper:setup` on a branch, baseline `--init` on the CI platform, and confirm the charters, following the README rollout. Make the `temper` CI job a required check in that repository's branch protection. | M | Agent + owner approvals |
| 2.3 | **Collect findings**: `temper stats` telemetry (denials, blocks, verdicts), false positives and negatives, gate latency on the Stop hook (timeout 1200 s), friction in `/temper:implement`, and the review-loop round counts. One issue per finding, labelled `pilot`. | M (ongoing) | Agent + owner |
| 2.4 | **Fix the pilot findings** and re-cut as 1.3.x. Settle the open two-way defaults the pilot was meant to test, such as wi-a9eb36 (Dependabot `lockfile-only`). | M–L | Agent |

### Phase 3: release engineering for v1

*Exit:* a release is one tag push plus the owner's approval, and every release is verified.

| Step | Work | Size |
|---|---|---|
| 3.1 | **Add a release workflow** (from 1.5): tag `v*`, then `tests/ci.sh`, the manifest validation and the version-agreement check, then a GitHub release with notes from the CHANGELOG. Run it from a protected `release` environment that requires the owner's approval, the same pattern D12 sets for 2.0. | M |
| 3.2 | **Write a versioning policy** in `CONTRIBUTING.md`: what counts as breaking for temper. A new MUST rule that fails existing code, a changed hook decision, a template change that needs migration, or a CLI flag or exit-code change is major or minor. A docs-only change is a patch. | S |
| 3.3 | **Pre-release model checks**: `tests/e2e/smoke.sh` and the eval suite run before every minor release. Results are appended to `docs/history/evals.md` and linked from the PR. Optionally make them a `workflow_dispatch` job with an API-key secret in the `release` environment. | S |

### Phase 4: v1 live

**Go-live checklist** (each item ticked in a tracking issue):
- [ ] `main` is protected, and every change since phase 0 arrived through a PR with green CI.
- [ ] The intended audience (from 0.7) can run `claude plugin marketplace add Wet-Ink-Corporation/temper` and `claude plugin install temper@temper`.
- [ ] Both pilot repositories run the temper gate as a required CI check, with a committed baseline.
- [ ] No P0 or P1 issue is open in the `v1 live` milestone.
- [ ] The evals meet the phase 1 target on the release candidate, and the smoke test is green.
- [ ] The README covers install, setup, upgrade, troubleshooting and the emergency switch, plus the minimum Claude Code version.
- [ ] The release workflow published the release (1.3.0, or 1.4.0 if the pilot fixes warrant it).
- [ ] There is a support channel: GitHub issues with templates, or an internal channel, named in the README.
- [ ] An announcement or onboarding note for the other Wet Ink repositories.

### Phase 5: after v1, temper 2.0 (the Rust port)

The decisions are already made (wi-30ed25, D1 to D17). Start from the committed port plan (0.2) and add the pilot findings to it before writing code. The order follows the decisions:
1. A workspace of four crates plus `xtask` (D3), with MSRV 1.96 (D4), the approved dependencies only (D10), and an `MIT OR Apache-2.0` licence (wi-4679b3). Built under temper's own standard, as the first dogfood of temper on temper.
2. A parity harness that holds 2.0 byte-identical to 1.x, apart from the approved divergences (D16).
3. A release pipeline for six targets (D5): npm platform packages under `@wet-ink` (D1), crates.io lockstep publishing (D2), and OIDC trusted publishing from the protected `release` environment (D12).
4. The plugin launcher, the cargo fallback with hybrid pending behaviour (D6), and PATH export at SessionStart (D17).
5. User-side changes: `.claude/temper-version` (D14), CI installing temper via `taiki-e/install-action` (D13), and `temper setup --migrate` (D15). Node removed at 2.0.0 (D11).

---

## Suggested order of work

1. **This week:** 0.1, 0.2, 0.7 and 0.8 (owner), and 0.3 to 0.6 (agent PRs). After this, everything goes through PRs.
2. **Next:** phase 1 and 3.1 to 3.2 in parallel. Start phase 2 adoption (2.1 to 2.2) as soon as 1.2 and 1.3 land, since the pilot needs elapsed time more than effort.
3. **Then:** fix the pilot findings, run the go-live checklist, and release v1.
4. **Then:** temper 2.0.
