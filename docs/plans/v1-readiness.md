# temper: repository evaluation and the plan to v1 live

Written 2026-10-02, against `main` at `57810da`. Updated the same day to review the 2.0 port plan, now committed as [`temper-2-rust-port.md`](temper-2-rust-port.md). Step statuses in Part 2 were refreshed on 2026-10-07 after the [2026-10 audit](../history/2026-10-audit/report.md), again on 2026-10-11, and on 2026-10-14; Part 1 is the evaluation as written on 2026-10-02.

## What "v1 live" means here

temper already ships 1.x releases (1.0.0 to 1.2.3). None of them is in use yet: no Rust repository runs it, and no repository depends on it in CI. This plan treats **v1 live** as follows:

> The 1.x line (the Node CLI and hooks) is installed and enforcing in real Wet Ink Rust repositories. Their CI requires the temper gate. A release process, an upgrade path and a support channel exist. The behavioural evals show that the plugin changes outcomes.

The temper 2.0 Rust port (wi-30ed25, D1 to D17) comes **after** v1 live, not before. The decisions for it are made, but no code exists yet. Shipping 2.0 first would put a large rewrite in front of the first real user, and the pilot findings from v1 should shape the port. If v1 should instead mean the Rust 2.0 engine, phases 0 to 2 still apply unchanged, and phase 5 moves before phase 4.

---

## Part 1: evaluation

### Product: further along than its usage

| Area | State | Evidence |
|---|---|---|
| The standard (`skills/rust-craft`) | Mature | 182 rules with IDs, 15 reference files, and a sources record. `tests/registry.py` checks every rule-ID citation with 0 failures (2754 on 2026-10-02, 2757 at the 2026-10 audit). All 329 code blocks compile and lint under the template lints (`tests/doc_examples.py`, 0 failures). |
| Enforcement (hooks, CLI) | Feature-complete for 1.x | 10 layers, from the compiler to measurement. About 5.6k lines of dependency-free Node across 13 modules. 8 hook events. |
| Brownfield support | Built | Lint baseline ratchet, `temper report`, PR comment job (1.2.0). `tests/e2e/brownfield.sh` covers it. |
| Workflow surface | Built | 5 skills/commands (`setup`, `implement`, `review`, `gate`, `audit`) and 3 agents (`architect`, `implementer`, `reviewer`). |
| Templates | Built | A `workspace` template, and a complete `example-workspace` that passes its own gate, including under Miri. |
| Tests | Strong | 140 unit tests (139 pass, 1 platform skip). Registry, lint-claim, rule-map and vendored-copy checks pass, and so do the doc examples and the brownfield e2e. All ran locally on 2026-10-02. CI runs on Linux and Windows. |
| Behavioural evals | Thin | 3 cases, and only one separates the two arms (`refuses-unwrap-under-pressure`, +0.67). The mean Δ is +0.22. `docs/history/evals.md` already says harder cases are needed. |
| Real-world use | **None** | No adopting repository. Several defaults say "check in the first repository that adopts the template" (for example wi-a9eb36, Dependabot `lockfile-only`). |
| temper 2.0 (Rust port) | Fully specified, not started | A detailed plan ([`temper-2-rust-port.md`](temper-2-rust-port.md)) and all 17 decisions settled: the owner decided 14, plus the licence, and D7 to D9 are two-way defaults (wi-f22cef, wi-2f31ee, wi-e9386f) with no record in `.kb/`. No Rust code yet; the plan's next step is its phase 0a. The plan sizes the port at ~26,000 lines across ~110 files, at least, which is several times the current engine. |

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
| G2 | The 2.0 port plan existed only in the owner's local, gitignored `.temper/plans/`. **Fixed** by committing it to `docs/plans/temper-2-rust-port.md` and repointing the decision-record anchors (step 0.2). | 18 committed decisions anchored to line numbers in a file nobody else could read. |
| G3 | There is no backlog. No issues, milestones or labels exist, and remaining work lives in chat sessions and `docs/history/` prose. | Nobody, human or agent, can see what is next or what is blocked. |
| G4 | There are no contributor files for this repository: no root `CLAUDE.md`/`AGENTS.md`, `CONTRIBUTING.md`, `CODEOWNERS`, PR or issue templates, or `SECURITY.md`. | Agents working here rediscover the release checklist each time: bump `plugin.json`, the vendored `VERSION`, sync `templates/example-workspace/.claude/temper/`, and update the CHANGELOG. |
| G5 | Releases are made by hand. Five of the six releases were published retroactively, all within 5 seconds at 01:55Z on 2026-10-02. | Nothing checks that the tag, `plugin.json`, the CHANGELOG heading and the vendored `VERSION` agree, or that the evals and smoke test ran. |
| G6 | `docs/history/decisions.md` stops at the repository setup. | It omits the 2.0 decisions (wi-30ed25, D1 to D17), so the index and the records disagree. The two-way defaults D7 to D9 appear only in the port plan's decisions table. |
| G7 | Cloud and agent sessions have no environment bootstrap. | `tests/ci.sh` fails at "lint and scanner claims" until the pinned 1.98.1 toolchain is installed (seen in this session). |
| G8 | The repository is `internal`. | `claude plugin marketplace add Wet-Ink-Corporation/temper` works only for org members with GitHub auth. That may be right, but it should be decided before go-live rather than discovered then. |
| G9 | A time-sensitive item is open: D2 says "reserve the names now". | `temper-cli`, `temper-domain`, `temper-app` and `temper-host` were free on crates.io on 2026-10-02. Anyone can take them. |

### The 2.0 port plan

**What is strong.** Every decision is settled, and each has its options and trade-offs written down. Nothing users rely on changes by accident:
- invariants that tests can check;
- a test map that fails CI if any of the 140 Node tests is dropped without saying so;
- a differential harness that compares the two engines, including a scanner corpus run over the whole cargo registry;
- state files that both engines can read, in both directions.

The port order puts the riskiest work first: distribution, then the scanner's regex semantics. Users run 1.x untouched until the cutover. The scope is held firmly: no rule changes, no syn, and no async runtime.

**What it changes about the v1 plan:**
- **`main` is the distribution channel.** The plan relies on this: marketplace users install whatever `main` holds (§4, step 9). That is already true for 1.x, so every merge to `main` reaches users. This makes 0.1 (protect `main`) the most urgent step. It also means a 1.x release is a merge, not just a tag, and phase 3 is designed with that in mind.
- **Do the pilot before the port.** The parity harness pins the scanner's behaviour for the length of the port, so problems the pilot finds are cheapest to fix in 1.x first. This adds to the reasons for v1 first (see phase 5).
- **External setup has lead time.** The npm org, the crate names, the arm64 runners, the `release` environment and the trusted publishers are owner actions with no code dependency. They are now steps 0.8 and 0.9, so they are not discovered at the port's phase 0b.
- **The repository will dogfood temper.** Phase 0a puts a Cargo workspace at the repository root with temper's own lints and charters, so this repository's contributor setup (0.3) should be written to grow into that, not be replaced by it.

**Small gaps in the plan itself** (to note in it, not blockers):
- It does not say how a 1.x patch release ships while the port is in progress. With the strangler on `main`, a 1.x fix merged to `main` ships alongside an unused Rust workspace in the plugin directory. That works, but it should be stated.
- Its `Progress` section ends at "Next: Phase 0a". Once phase 0 of this plan exists, the port should be tracked as issues in the `2.0` milestone (0.4) rather than only in that section.

---

## Part 2: the plan

Sizes: **S** is under half a day, **M** is 1 to 2 days, **L** is 3 days or more. **Owner** marks a step only the repository owner can do: GitHub settings, registries, or decisions. Everything else an agent can do through pull requests.

### Phase 0: get organized (repository and source control)

*Exit:* every change reaches `main` through a PR with green CI. The backlog is visible in GitHub. A new contributor or agent can find how to develop and release without reading chat history.

| Step | Work | Size | Who |
|---|---|---|---|
| 0.1 | ~~**Protect `main`** with a ruleset: require a PR, require the `Checks` and `Unit tests (Windows)` status checks, block force-push and deletion, and give the owner a bypass for emergencies. Protect `v*` tags.~~ Done by the owner, 2026-10-02. (G1) The 2026-10 audit read the `Protect main` ruleset and found no required-status-checks rule in it, so the two checks were not required (`v1-plan-1-1`, #62). Fixed by the owner on 2026-10-11: the ruleset now requires `Checks`, `Unit tests (Windows)` and `CHANGELOG entry`. | S | Done |
| 0.2 | ~~**Commit the 2.0 port plan** to `docs/plans/temper-2-rust-port.md` and repoint the decision-record anchors.~~ Done; the file is verbatim, so every `:line` anchor still resolves. At 2.0.0 it moves to `docs/history/2.0-rust-port/`, as the plan's §7 says. (G2) | S | Done |
| 0.3 | ~~**Add contributor files**~~ Done: root `AGENTS.md` (imported by `.claude/CLAUDE.md`; a root `CLAUDE.md` fails strict plugin validation), `CONTRIBUTING.md`, `SECURITY.md`, `.github/CODEOWNERS`, a PR template, and issue templates for bugs, false positives or negatives, and rule proposals. `AGENTS.md` is shaped so the port's phase 0a can add the crate map and decisions log to it. (G4) | M | Done |
| 0.4 | ~~**Set up the backlog**~~ Done. The tracking issues #6 (v1 live) and #21 (temper 2.0) hold one sub-issue per remaining step: #7 to #20 for phases 0.9 to 4, and #22 to #24 for the pre-port steps. The port's phases are a checklist in #21, each to become an issue when it starts. Labels: `tracking`, `v1-live`, `2.0`, `owner`, `pilot`, `area:*`, `type:*`. The GitHub tooling available here cannot create milestones; if the owner wants them, `v1 live` and `2.0` map one-to-one onto #6 and #21. (G3) | S | Done |
| 0.5 | ~~**Bring `docs/history/decisions.md` up to date**~~ Done. It now has wi-30ed25, D1 to D17 and the licence under the owner's decisions; D7 to D9 (wi-f22cef, wi-2f31ee, wi-e9386f) under the two-way defaults; and a new section for decisions made in this plan (0.7, 0.9). (G6) | S | Done |
| 0.6 | ~~**Bootstrap agent and cloud sessions**~~ Done. `.claude/hooks/session-start.sh`, registered in `.claude/settings.json`, installs the pinned toolchain with clippy and rustfmt and fetches the example workspace's crates. It runs in cloud sessions only and takes about 14 s from cold. (G7) | S | Done |
| 0.7 | ~~**Decide the audience and visibility** for v1.~~ Decided 2026-10-02: **internal for now**. v1 targets Wet Ink org members, who install from the marketplace with their GitHub auth. Revisit before any public release, including `docs/history/` (wi-96caeb was decided while the repository was private). (G8) | S | Done |
| 0.8 | ~~**Reserve the crates.io names and the `@wet-ink` npm scope** per D1 and D2.~~ Done 2026-10-02. The owner created the `@wet-ink` npm org. `temper-cli`, `temper-domain`, `temper-app` and `temper-host` are published as `0.0.0` placeholders (`MIT OR Apache-2.0`) with [`reserve-temper-crates.sh`](reserve-temper-crates.sh). The owner revoked the one-off token and configured crates.io trusted publishing for all four crates (`release.yml`, `release` environment, D12) on 2026-10-11 (#23). (G9) | S | Done |
| 0.9 | **The port's external prerequisites.** Decided by the owner, 2026-10-02:<br>**Arm64 runners: use GitHub's standard runners** (`ubuntu-24.04-arm`, `windows-11-arm`), as the port plan's release matrix specifies. GitHub announced them for private repositories on 2026-01-29. They have 2 vCPUs in private repos and count against included minutes. **Probe results (2026-10-02):** all five labels run for this `internal` repository, so D5 stands as decided and no target needs cross-compiling. A cold `cargo fetch`, `build --release`, `clippy` and `test` of the example workspace took:<br>• `macos-15`: 36 s<br>• `ubuntu-24.04-arm`: 46 s<br>• `ubuntu-latest`: 59 s<br>• `windows-latest`: 96 s<br>• `windows-11-arm`: 102 s<br>Linux arm64 is not slower than x64 despite 2 vCPUs, and Windows is the slowest. One probe run used about 28 included minutes. The first run also found two real bugs, a macOS path bug in the CLI and a Windows bug in the example store's crash repair. Both are fixed in #26 ([run 1](https://github.com/Wet-Ink-Corporation/temper/actions/runs/37040881290) failed, [run 2](https://github.com/Wet-Ink-Corporation/temper/actions/runs/37043545399) is green).<br>**CI budget: tiered CI** for the port:<br>• Every PR runs the Linux Rust job, MSRV, parity and launcher tests.<br>• Windows jobs run on PRs that touch `crates/**`, `scripts/**` or `hooks/**`, and on pushes to `main`. macOS jobs are parked until the repository is public (below; revisited 2026-10-30).<br>• The registry-corpus scan runs weekly and on `release/*`.<br>• The six-target release matrix runs only on `release/*`.<br>Also: a required-check aggregator job, so ruleset checks still work when path filters skip jobs; Rust build caching from the first Rust job; and an Actions budget with alerts at 75% and 90% in the enterprise billing settings. The org is on GitHub Enterprise Cloud (implied by `internal` visibility), which includes 50,000 minutes a month shared across the org. macOS counts 10× against them and Windows 2×. Rough estimate: 120 to 180 included minutes per full-matrix PR run, and 300 to 500 per release. Revisit after the first month of real usage.<br>**macOS CI: parked until the repository goes public** (owner, 2026-10-02). macOS minutes count 10× against the included minutes. The runner probe's macOS run (`runner-probe.yml`, run by hand) covers it on demand. Revisit when the repository goes public, since standard runners are free for public repositories. This does not change the release matrix's macOS targets (D5).<br>**Revisited 2026-10-30:** the repository is public, and macOS runs in CI on code-touching PRs (wi-e85ef8).<br>**Budget set** by the owner on 2026-10-11 (#7): a $10 Actions budget on the `wet-ink` enterprise with stop usage on. Whether the 75% and 90% alerts planned above are also set was not recorded. GitHub budgets count spend beyond the included minutes, and with stop usage on, reaching the cap halts CI, required checks included. | S | Done |

### Phase 1: harden 1.x (label `v1-live`, tracked in #6, released as 1.3.0)

*Exit:* the evals show a clear with/without gap on realistic cases, and the known rough edges are fixed or documented.

| Step | Work | Size |
|---|---|---|
| 1.1 | ~~**Add discriminating evals**, starting with the three `evals.md` already names: a brownfield change under the baseline, a review that must catch a raised baseline, and an async lock held across `.await`. Add two more from areas that today have no case: a `Cargo.toml` dependency ask, and a skipped-test or `#[ignore]` attempt. Target: mean Δ ≥ +0.4 over ≥ 3 runs per arm.~~ Done 2026-10-05. The five cases are in `evals/`. The target was restated by the owner: the cases are tagged `discriminating` (mean Δ ≥ +0.4) or `regression` (Δ ≥ 0 per case), because the two older no-regression cases score 0 by design. The `discriminating` group met it on all three full runs (+0.55, +0.44, +0.42; [results](../history/evals.md#targets), #8). Follow-ups: #47 (cases the plain model now passes; done 2026-10-06, `asks-before-crypto-dependency` moved to `regression`) and #48 (restate declined requests in the final report). | L |
| 1.2 | ~~**Check compatibility with current Claude Code.** CI pins `@anthropic-ai/claude-code@2.1.286` for manifest validation. Confirm every hook event and matcher in `hooks/hooks.json` still fires on the current release (the smoke test covers this), and record the minimum supported Claude Code version in the README.~~ Done: the minimum is 2.1.286. The smoke test passed on 2.1.286 and 2.1.288, and every hook runs except `PreToolUse` `SubagentHandback`, whose check `SubagentStop` already makes ([results](../history/evals.md#smoke-runs), #9). That exception holds for local sessions only. In a cloud session the hook does fire, and a subagent's report arrives only through `SubagentHandback`, with no `last_assistant_message` at `SubagentStop` (#97). So the hook was restored in #98, reversing wi-103aa0, and `SubagentStop` now reads the report from the subagent's transcript when the message is absent. | M |
| 1.3 | ~~**Write an upgrade path for adopted repositories**: how a repository moves from one 1.x vendored copy (`.claude/temper/`) to the next. That means `temper setup` drift detection, then what to re-run and what to review. Add it to the README and CHANGELOG.~~ Done: the README's Upgrading section, and an "Upgrading from" note in each CHANGELOG section that changes `scripts/` or the templates (#10). | S |
| 1.4 | ~~**Write troubleshooting docs**: what each hook denial means, `TEMPER_DISABLE=1` as the human emergency switch, gate timeouts on large workspaces, and Windows notes.~~ Done: [`docs/troubleshooting.md`](../troubleshooting.md), linked from the README (#11). | S |
| 1.5 | ~~**Make the release checklist executable.** Add a `tests/release-check.sh`, or a CI job on `v*` tags, that asserts the tag, `plugin.json`, the CHANGELOG heading and `templates/example-workspace/.claude/temper/VERSION` all agree, then creates the GitHub release from the CHANGELOG section. (G5)~~ Done 2026-10-04. `tests/release-check.sh` checks the four versions, the `## Unreleased` section and the "Upgrading from" note, and `tests/ci.sh` runs it without the tag (#34). Creating the GitHub release moved to the release workflow in 3.1. | M |

### Phase 2: pilot (dogfood on real repositories)

*Exit:* two real repositories have run temper day to day for at least two weeks, every pilot finding has an issue, and no P0 or P1 issue is open.

| Step | Work | Size | Who |
|---|---|---|---|
| 2.1 | ~~**Pick two pilot repositories**: one greenfield or small, and one brownfield Wet Ink Rust repository with real lint debt.~~ Done 2026-10-02: **weigh-in** (small: two crates, about 8k lines) and **redkiln-rs** (brownfield: twelve crates and three fixture crates on `build/phase-0-to-1`, about 236k lines including tests). redkiln-rs adopts on that branch, which its runbook is still building, and keeps its ADR-0102 lint groups (#13). | S | Owner |
| 2.2 | ~~**Adopt temper in each**: `cargo fmt` commit, then `/temper:setup` on a branch, baseline `--init` on the CI platform, and confirm the charters, following the README rollout. Make the `temper` CI job a required check in that repository's branch protection.~~ Done 2026-10-04. weigh-in adopted temper in [weigh-in#37](https://github.com/Wet-Ink-Corporation/weigh-in/pull/37) and redkiln-rs in [redkiln-rs#2](https://github.com/Wet-Ink-Corporation/redkiln-rs/pull/2), with the charters confirmed in [weigh-in#63](https://github.com/Wet-Ink-Corporation/weigh-in/pull/63) and [redkiln-rs#3](https://github.com/Wet-Ink-Corporation/redkiln-rs/pull/3). The required checks are meant to be `gate` in weigh-in and `temper` in redkiln-rs; the 2026-10 audit found neither in its repository's rulesets (`v1-plan-1-2`, #62). Fixed by the owner on 2026-10-11: weigh-in's `main` ruleset requires `gate`, and redkiln-rs's `Protect build branch` ruleset requires `temper` on `build/phase-0-to-1`. The adoption findings went to #32 and were fixed in #33. | M | Agent + owner approvals |
| 2.3 | **Collect findings**: `temper stats` telemetry (denials, blocks, verdicts), false positives and negatives, gate latency on the Stop hook (timeout 1200 s), friction in `/temper:implement`, and the review-loop round counts. One issue per finding, labelled `pilot`. The pilots run in cloud sessions, whose `.git/temper/` is lost with the container, so their telemetry was not being kept. Since #96 (wi-641f99) each pilot sets `telemetry.branch` in `.claude/temper.json`: every `Stop` pushes the session's telemetry to a `temper-telemetry` branch, and `temper stats --branch` summarizes it, with gate latency at Stop and review rounds per session. redkiln-rs ran in the desktop app, not the cloud, so its telemetry stayed local; the owner recovered the file by hand (wi-2c680d). An interim report (outside the repository) was written on 2026-10-13. The pilot report is due 2026-10-18 (wi-292a7d). | M (ongoing) | Agent + owner |
| 2.4 | **Fix the pilot findings** and release them as 1.4.0, the v1 release (wi-891617). Settle the open two-way defaults the pilot was meant to test, such as wi-a9eb36 (Dependabot `lockfile-only`). The first batch of fixes, from the 2026-10 audit, merged on 2026-10-07 as #82 to #89 (#16): shell writes into the state dir denied, shell writes to protected config ask, copies, moves and patches of Rust sources scanned, `--no-verify` and `-A` checks that skip commit-message prose, stricter review verdicts, config files in the fingerprint, bounded local state, and a warning for a Node older than 18. They are on `main`, unreleased. Since then: the `Declined:` line (#100) and the ask before a shell command deletes protected config (#104), both merged by the owner on 2026-10-11 ahead of wi-b63bd6's pilot window, the template CI's vendored-copy flag (#106), `status --json` and `stats --json` (#116) and the `stats` review count (#125). Existing marketplace installs pick none of this up until 1.4.0 bumps the version; the organization sync reads `main` and already has it. | M–L | Agent |

### Phase 3: release engineering for v1

*Exit:* a release is one tag push plus the owner's approval, and every release is verified.

| Step | Work | Size |
|---|---|---|
| 3.1 | ~~**Add a release workflow** (from 1.5): tag `v*`, then `tests/ci.sh`, the manifest validation and the version-agreement check, then a GitHub release with notes from the CHANGELOG. Run it from a protected `release` environment that requires the owner's approval, the same pattern D12 sets for 2.0. Merging to `main` is what ships to marketplace users, so use the flow the port plan defines for 2.0 (§4): a `release/<version>` branch, the checks, the GitHub release, then the owner merges to `main`. That way 1.x and 2.0 share one process.~~ Done 2026-10-04: `.github/workflows/release.yml`, run on a `release/X.Y.Z` branch, with the owner's approval in the `release` environment (#35, brought to `main` in #41). It published [v1.3.0](https://github.com/Wet-Ink-Corporation/temper/releases/tag/v1.3.0) on 2026-10-05 (#50). The steps are in `AGENTS.md`, "Release checklist". | M |
| 3.2 | ~~**Write a versioning policy** in `CONTRIBUTING.md`: what counts as breaking for temper. A new MUST rule that fails existing code, a changed hook decision, a template change that needs migration, or a CLI flag or exit-code change is major or minor. A docs-only change is a patch.~~ Done 2026-10-04: `CONTRIBUTING.md`, "Versioning" (#36). The owner's four calls on it are in [`decisions.md`](../history/decisions.md#decided-outside-weigh-in) (#42). | S |
| 3.3 | **Pre-release model checks**: `tests/e2e/smoke.sh` and the eval suite run before every minor release. Results are appended to `docs/history/evals.md` and linked from the PR. Optionally make them a `workflow_dispatch` job with an API-key secret in the `release` environment. Followed for 1.3.0: the smoke run and the confirming eval run are in [`evals.md`](../history/evals.md#130-release-candidate-2026-10-05) (#50), and step 3 of the release checklist in `AGENTS.md` requires them. The optional `workflow_dispatch` job is not built (#19). | S |

### Phase 4: v1 live

**Go-live checklist** (each item ticked in a tracking issue):
- [ ] `main` is protected, and every change since phase 0 arrived through a PR with green CI.
- [ ] The intended audience (from 0.7) can run `claude plugin marketplace add Wet-Ink-Corporation/temper` and `claude plugin install temper@temper`.
- [ ] Both pilot repositories run the temper gate as a required CI check, with a committed baseline.
- [ ] No P0 or P1 issue is open among the `v1-live` issues (tracked in #6).
- [ ] The evals meet the phase 1 targets on the release candidate (`discriminating` mean Δ ≥ +0.4, `regression` Δ ≥ 0 per case), and the smoke test is green.
- [ ] The README covers install, setup, upgrade, troubleshooting and the emergency switch, plus the minimum Claude Code version.
- [ ] The release workflow published 1.4.0, cut after the pilot fixes (#16), with the smoke test and evals run on its release candidate (wi-891617, decided 2026-10-07).
- [ ] There is a support channel: GitHub issues with templates, or an internal channel, named in the README.
- [ ] An announcement or onboarding note for the other Wet Ink repositories.

### Phase 5: after v1, temper 2.0 (the Rust port)

Follow [`temper-2-rust-port.md`](temper-2-rust-port.md). It is ready to build from. Before its phase 0a starts:
1. **Fold in the pilot findings.** Fix every scanner false positive or rule problem the pilot finds in 1.x *before* the port starts. During the port, the parity harness holds the Rust and Node engines byte-identical, and the plan forbids rule changes inside it, so a rule fix made mid-port has to be made twice, or it waits for 2.x.
2. **Confirm the prerequisites from 0.8 and 0.9**: the npm org, the crate names, the arm64 runners, and the CI budget. Also set up the `release` environment with the owner as required reviewer, and the npm trusted publisher (D12; crates.io's is done, 0.8). The plan's phase 0b publishes `2.0.0-alpha.1` publicly to npm, which needs all of these.
3. **Add the port's CI jobs as required checks** in the 0.1 ruleset as they land. That covers the Rust jobs, MSRV, and the parity harness until cutover.

The plan's own order is phases 0a to 8: workspace skeleton, distribution spike, fallback build, scanner, the hot-path hooks, session and verdict handling, gate, baseline, report, setup and migrate, then the cutover to 2.0.0. A subcommand graduates to Rust once its parity cases pass on all three OSes.

---

## Suggested order of work

1. **Phase 0** is done. The owner added the required checks to the `main` ruleset (0.1, #62) and set the Actions budget (0.9, #7) on 2026-10-11. The runner probe ran on 2026-10-02, and its results are in 0.9. From here on, work is tracked in #6 and #21. After this, everything goes through PRs.
2. ~~**Next:** phase 1 and 3.1 to 3.2 in parallel. Start phase 2 adoption (2.1 to 2.2) as soon as 1.2 and 1.3 land, since the pilot needs elapsed time more than effort.~~ Done: phase 1, 2.1, 2.2, 3.1 and 3.2 are done, and 1.3.0 was released on 2026-10-05. The pilot (2.3, 2.4) is running.
3. ~~**Then, the critical path to v1 live**~~ Done: 1.4.0 was released and v1 went live on 2026-10-19 (#20 and #6 closed). The path was:
   1. The pilot report on 2026-10-18 (2.3): the final `temper stats --branch` numbers from both pilots, one `pilot` issue per finding (#15).
   2. The fixes it calls for (#16), and the work held for the pilot window (the AGENTS.md rule from wi-b63bd6, and wi-3fa7cf): the `[hold]` temper lens PRs with their gate and CI prerequisites, #117 to #121 and #123 (#107); the staged MUST-rule checks (wi-3fa7cf, #58); and the telemetry gaps the interim report found (`verdict-invalid` problems and denied command text are not logged).
   3. 1.4.0's release candidate: the smoke test and the evals, including the `candidate` cases (#19, #101). These are paid, so the owner approves them.
   4. The 1.4.0 release (wi-891617), through the release checklist in `AGENTS.md`.
   5. The go-live sign-off (#20): one other org member verifies the install, the announcement goes out, then #20 is ticked and #6 closed.
4. **Now: close out 1.x, then temper 2.0** (planned 2026-10-30; the owner approved wi-9d8c9e, wi-779016, wi-0cfadc, wi-428ed1 and wi-e85ef8):
   1. Close the audit issues down to an owner call (#61, #63, #64, #70) and #137, whose data arrives after 1.5.0.
   2. Fix #157 and #155's core (the slow regexes in the shell guards and the cheap gaps that affect every agent), and write the #101 eval cases. #155 lands before port phase 2, since the port reproduces `shell.mjs` exactly (wi-6e0cc2).
   3. Release 1.5.0, with one paid pass for the smoke test, the release-candidate evals and the #101 cases. That closes #22.
   4. Then the port, from its plan: phases 0a to 0c (closing #23 and #24), 1 to 7 (closing #103), and the 2.0.0 cutover in phase 8 (closing #21).
