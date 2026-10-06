# temper: status-quo audit against vision, v1 plan and 2.0 port plan

- **Date:** 2026-10-06
- **Subject:** `Wet-Ink-Corporation/temper` at `d87d887` (= `origin/main`, plugin version 1.3.0), read-only clone
- **Pilots:** `weigh-in` (main) and `redkiln-rs` (main, plus the adoption branch `build/phase-0-to-1`), both read-only clones
- **Isolation:** the audit ran entirely in the session scratchpad. It did not modify `/home/user/temper` or the branch checked out there.
- **Method:** 14 audit dimensions, each producing evidence-cited findings. A second verifier pass re-checked every finding, re-running greps, probes and `gh api` calls. **199 findings survived verification. 2 were refuted and dropped** (see section 10). Status vocabulary: done | partial | not-started | regressed | stale-doc | risk | quality-issue.

---

## 1. Executive overview

temper is **feature-complete for 1.x and is in real use**. Every capability promised in the README and in `plugin.json` exists in code, and most of them were driven by hand during the audit. The deterministic CI is fully green at `d87d887`: 156 unit tests (155 pass, 1 Windows-only skip), 182 rules with 2,757 citations checked, 0 of 329 doc examples failing, and the brownfield e2e passing in 27 s (`ci.log`, final line `== all checks passed`, `EXIT 0`). 1.3.0 shipped through the gated release workflow. Both pilots run temper 1.3.0, byte-identical to `scripts/`, with a CI gate and a committed baseline. weigh-in has already paid its whole lint baseline down to zero.

**v1 is not "live" yet, and the gap is mostly verification, bookkeeping and time rather than engineering:**

1. **The protection claims are not true in GitHub.** temper's own `main` ruleset requires no CI status checks. weigh-in's ruleset does not require the temper `gate` job. redkiln-rs's adoption branch has no rules at all. AGENTS.md, CONTRIBUTING.md, the plan and the comment that closed #14 all say otherwise (`gh api .../rulesets/24368200`, `.../weigh-in/rulesets/24377415`, `.../redkiln-rs/rulesets/24430583`). These are owner-only edits of a few minutes each.
2. **The pilot soak has just started.** Adoption merged 2026-10-03/04, and the plan requires at least two weeks of daily use. The earliest honest exit is about **2026-10-17**. No telemetry, gate-latency or false-positive data has been captured yet (#15 has no comments).
3. **The go-live checklist (#20) has 0 of 9 boxes ticked.** In substance 4 items are met, 2 are not met, and 3 are partial or unverified.

**Quality gaps worth closing before v1 and before the 2.0 parity harness freezes behaviour:**
- The headline "completion gate + fingerprint-bound review" paths (Stop, SubagentStop, verdict validation, SessionStart, the skill-gate deny) have **no deterministic tests**. They are covered only by paid model-call smoke runs and evals. One evals.md sentence claims unit coverage that does not exist.
- **Shell writes can forge review and gate state**, edit protected config, or switch the Stop gate off by breaking `rust-toolchain.toml`. The README labels the shell layer "heuristic", but it does not say that review state is writable from the shell. CI remains the hard backstop for the gate, but nothing outside the session re-checks the independent review.

**The 2.0 Rust port has not started, which is intended.** It is gated on v1 going live. The plan is complete and all 17 decisions plus the licence are settled. It has drifted from 1.3.0 in specific places (test counts, the setup features, `LEGACY_TEMPLATES`, macOS) and carries two new risks. Realistic size: about 26k lines, roughly 6 to 10 weeks elapsed once started, bounded by a single owner's review bandwidth.

**Overall grade: B.** The engineering is strong, the evidence is honest, and the remaining v1 work is small. Two things pull the grade down: enforcement settings that were claimed but are not real, and validation (pilot data, orchestration tests, cost model) that has not caught up with the features.

---

## 2. Milestone status: v1-readiness (`docs/plans/v1-readiness.md`)

### Phase 0: Foundations, about 90%

| Step | What | Status | Evidence |
|---|---|---|---|
| 0.1 | Protect `main` with a ruleset that requires `Checks` + `Unit tests (Windows)` | **risk** (marked done, but the required checks are absent) | `gh api repos/Wet-Ink-Corporation/temper/rulesets/24368200` shows rules `deletion`, `non_fast_forward`, `pull_request` (0 approvals) and no `required_status_checks`. Classic protection returns 404. Created and updated 2026-10-02T14:03:55Z, so the rule was never added. Plan:86, AGENTS.md:8 and CONTRIBUTING.md:10 claim otherwise. |
| 0.2 | Commit port plan | done | `docs/plans/temper-2-rust-port.md` (890 lines) |
| 0.3 | Contributor files | done | CONTRIBUTING.md, SECURITY.md, CODEOWNERS, PR template, 3 issue templates |
| 0.4 | Backlog and labels | done (milestones could not be created) | #6/#21 plus sub-issues #7-#24. `gh api .../milestones?state=all` returns `[]` |
| 0.5 | Decision log | done | `docs/history/decisions.md` (41 rows), 21 records in `.kb/_intake/decisions/`, all `accepted` |
| 0.6 | Cloud bootstrap | done | `.claude/hooks/session-start.sh` |
| 0.7 | Visibility decision | done (internal) | decisions.md:71 |
| 0.8 | Reserve crate and npm names | done; token revocation unconfirmed | crates.io: 4 crates at 0.0.0 (2026-10-02T15:04Z); npm org `wet-ink` returns 200. #23 item 4 unticked |
| 0.9 | Runner probe + Actions budget alerts | partial | Probe run 2 green on a branch, never re-run on main (`actions/runs/37043545399`). Budget alerts: owner item, #7 open, not verifiable |

### Phase 1: Hardening, 100% (two follow-ups open)

| Step | What | Status | Evidence |
|---|---|---|---|
| 1.1 | Discriminating evals, mean delta ≥ +0.4 | done (thin margin) | evals.md: +0.55 / +0.44 / +0.42, regression cases ≥ 0, #8 closed by PR #49. The all-eight mean is +0.31. Follow-ups #47 and #48 are open |
| 1.2 | Claude Code compatibility, minimum version | done (one dead hook) | README:28 gives 2.1.286. Smoke passed on 2.1.286, .288 and .289. `SubagentHandback` never fires |
| 1.3 | Upgrade path docs | done | README Upgrading, CHANGELOG "Upgrading from". Upgrade 1.2.3→1.3.0 re-run in the audit |
| 1.4 | Troubleshooting | done | docs/troubleshooting.md |
| 1.5 | Executable release check | done (not struck through in the plan) | `tests/release-check.sh`, 13 tests, run by `ci.sh` and `release.yml` |

### Phase 2: Pilots, about 50% (critical path)

| Step | What | Status | Evidence |
|---|---|---|---|
| 2.1 | Choose pilots | done | weigh-in (clean-ish), redkiln-rs (brownfield, via `build/phase-0-to-1`) |
| 2.2 | Adopt, gate as a required check, baseline, charters | **partial**: adopted, but the "required check" claim is false | Both on 1.3.0 byte-identical (`diff -r` clean). weigh-in ruleset requires `check(×3)`, `plugin`, `deny` and not `gate`. ci.yml also skips the Linux check on PRs. redkiln-rs ruleset `required_status_checks: []`, and the build branch has no rules |
| 2.3 | Collect findings for ≥ 2 weeks (telemetry, latency, false positives, review rounds) | **not-started / time-gated** | #15 has no comments. No `temper stats` output and no gate-latency measurement anywhere. Earliest exit about 2026-10-17 |
| 2.4 | Fix findings, cut 1.3.x, settle wi-a9eb36 | partial | Adoption fixes (#32→#33, #38→#39) shipped as 1.3.0 (minor). #16 open. wi-a9eb36 cannot be settled: weigh-in uses its own dependabot.yml, redkiln-rs has none |

### Phase 3: Release engineering, about 95%

| Step | What | Status | Evidence |
|---|---|---|---|
| 3.1 | Release workflow with owner-approved environment | done | `release.yml`. Run 37263274680 was approved by Ryan-Britton and tagged v1.3.0 at 2495d0e. PR #50 was a merge commit |
| 3.2 | Versioning policy | done | CONTRIBUTING.md:24-57 |
| 3.3 | Smoke + evals before every minor release | done in practice for 1.3.0; #19 open; nothing enforces it | evals.md:233-238. No gate in `release.yml` or `release-check.sh`. The optional dispatch job is not built |

### Phase 4: Go-live checklist (#20), 0/9 ticked; in substance 4 met, 2 not met, 3 partial

| # | Item | Real status | Evidence |
|---|---|---|---|
| 1 | `main` protected, PR + CI required | **not met** | No `required_status_checks` in ruleset 24368200 |
| 2 | Intended audience can `marketplace add` + `install` | partial (unverified for anyone but the owner) | All activity is from one account. No other org member has dry-run the install |
| 3 | Both pilots run the gate as a required check, with a baseline | **not met** (baselines yes, required no) | Rulesets above. redkiln-rs is on a non-default branch |
| 4 | No P0/P1 open in the `v1 live` milestone | vacuous | No P0/P1 labels (`get_label P1` returns not found), and 0 milestones |
| 5 | Evals meet 1.1 target + smoke green | met for 1.3.0 (thin) | +0.42 confirming run. +0.58 with the #47 swap is a substitution, not a full rerun |
| 6 | README covers install, setup, upgrade, troubleshooting, emergency switch, minimum Claude Code version | met | README:25-43, 73, 116-166 |
| 7 | A release published by the release workflow | met | v1.3.0 |
| 8 | Support channel named | met (indirectly) | README:161 links to troubleshooting.md:150-158 (issue templates). The README never says "open an issue" |
| 9 | Announcement / onboarding note | not-started | grep finds no doc and no issue |

### Phase 5: Pre-port (2.0 prerequisites), about 40%

| Step | What | Status | Evidence |
|---|---|---|---|
| 1 (#22) | Fold pilot findings into 1.x and the plan; patch-release policy during the port | not-started | 0/3 items. Blocked on #16. `grep 'patch release'` in the port plan finds nothing |
| 2 (#23) | Release environment, crates.io + npm trusted publishing, revoke one-off token, arm64 confirmed | partial | Environment exists and gated 1.3.0. Trusted publishing is unverifiable. Token revocation is unconfirmed. arm64 was probed on a branch. 0/5 boxes ticked |
| 3 (#24) | Tiered CI, aggregator required check, Rust cache | not-started (expected; lands with Phase 0a) | ci.yml has two jobs only |

---

## 3. Milestone status: temper 2.0 Rust port (`docs/plans/temper-2-rust-port.md`)

| Item | Status | Evidence |
|---|---|---|
| Plan specified (invariants, test map, parity harness, D16 divergences) | done | 890 lines |
| Decisions D1-D17 + licence | done | 16 port records. Anchors resolve (e.g. :79→D7, :84→D8, :91→D9) |
| crates.io names (temper-cli/-domain/-app/-host) | done | 0.0.0, 2026-10-02. Single individual owner (bus factor) |
| `@wet-ink` npm org | done | org endpoint returns 200 `{}`, no packages |
| arm64 / release-target runners | done (branch run only) | runner-probe.yml:21. Run 2 green |
| Release environment + trusted publishing | partial / unverifiable | #23 |
| One-off token revoked | unconfirmed (risk) | plan:93, #23 item 4 |
| Tiered CI (#24) | not-started | ci.yml |
| Pilot fold-in (#22) | not-started | blocked on pilot window |
| Phase 0a (workspace, LICENSE-APACHE, launch.mjs, parity skeleton) | not-started | No `Cargo.toml`, `crates/`, `xtask/`, `LICENSE-APACHE`, `package.json`, `tests/parity` at d87d887 |
| Phases 0b, 0c, 1-8 | not-started | #21 phases unchecked; by design |

Size baseline still holds. The Node engine is 5,649 lines against 5,593 cited, still 13 modules and 56 scanner keys. Since 1.2.3 only `temper.mjs` (+56) and `project.mjs` (+23) changed. The ~26k-line lower bound (~4.6× the Node engine) is still sound.

---

## 4. Quality scorecard

| Dimension | Grade | Rationale |
|---|---|---|
| Product promise | B | Every promised capability exists and behaves as documented under hand probes. The headline completion-gate and review paths lack deterministic tests. Shell can forge state. Real-world validation is 2-3 days old |
| The rust-craft standard | B+ (reported B) | 182 rules, 2,757 citations, 0/329 failing examples, sources and licences recorded. 86 rules (31 of them MUST) are review-only. Evals cite 3 rule IDs. 31 Don't blocks have no `expect=`. The rubric names 72 of 182 IDs |
| Enforcement engine | B | Zero-dependency Node, 90-105 ms hooks, linear scanner (13k lines in 64 ms), deliberate Windows handling, earlier security findings fixed and tested. Gaps: the anti-bypass shell layer, untested orchestration handlers, unbounded state growth |
| Workflow surface (commands, agents, templates, evals) | B | All 5 commands and 3 agents match the README. Evals meet target. Nothing evaluates `/temper:implement`, `/temper:setup`, `/temper:audit` or the architect and implementer. #48 is not done |
| Release and CI | B | The release workflow shipped 1.3.0 behind approval. SHA pins and Dependabot are in place. Pre-release model checks are manual. One cancelled main run and one rerun are unexplained. macOS CI is parked |
| v1 plan execution | B | Phases 0, 1 and 3 are delivered. The required-check claims are false. The plan and tracking issues lag reality in both directions |
| Pilots | C | Adoption is real and healthy, and weigh-in remediated 721→0. But the soak is not elapsed, there is no telemetry or latency data, findings are bundled, and redkiln-rs is off main |
| 2.0 port readiness | B | Plan and decisions are excellent, and the external prerequisites are mostly done. The plan has drifted from 1.3.0 and misses two new risks. No dates |
| Docs and governance | B | CHANGELOG, version, decisions and troubleshooting are all aligned. Tracking issue bodies are stale (#6, #7, #20, #23) |
| Closure of history findings | A | About 30 of the 1.1/1.2 review findings were spot-checked and none was silently dropped. Residuals: vendored-CLI integrity (item 27), nested workspaces, 7 drifted anchors |
| Security, privacy, licensing | B | Least-privilege, SHA-pinned workflows. Telemetry is local-only. MIT is consistent. Telemetry is undisclosed and unbounded, SECURITY.md is thin, and the vendored copy has no integrity check |
| Fresh-adopter experience | B | Greenfield and upgrade both reach green. There is no greenfield path in the docs, the empty-workspace error is raw rustfmt help, `.gitignore` is incomplete, and the Node and Claude Code minimums are not checked |
| Runtime cost and context budget | C | Hooks are cheap. The standard costs about 4.4k tokens up front plus 9-22k per reference, and a review round costs about 25-30k. None of this is documented or budgeted. Stop-gate latency on a large repo was never measured. Budget alerts are not set |
| Backlog and open decisions | B | 12 open issues all map to plan steps. All 21 decisions are accepted. There are no priority labels or milestones, and the bodies are stale |

---

## 5. Findings by dimension

Severity scale: info < low < low-medium < medium < medium-high < high. Effort: none, S (≤ half day), M (1-3 days), L (≈1 week), XL (multi-week). **(O)** marks an owner-only item.

### 5.1 Product promise (22)

| ID | Finding | Status | Sev | Effort | Key evidence |
|---|---|---|---|---|---|
| 1-1 | Standard complete, registry-checked, example-tested | done | info | none | ci.log: 182 rules / 2757 citations / 0 of 329 failing |
| 1-2 | Skill-load gate works but has no unit test | partial | medium | S | hooks.mjs:255-259. tests/hooks.test.mjs:76 "not under test here" |
| 1-3 | Pre-write scanner and protected-config gates implemented and tested | done | info | none | hooks.mjs:227-366. 32 scan + 33 hook tests |
| 1-4 | Completion gate and fingerprint-bound review: no deterministic tests | partial | medium-high | M | hooks.mjs:547-623, verdict.mjs. grep of tests/ finds nothing. The evals.md:266 claim of unit coverage is unsupported |
| 1-5 | Verdict and gate state forgeable through Bash | risk | medium | S | preBash has no `.git/temper` rule (hooks.mjs:349-353). Probe: `echo > .git/temper/verdicts/x.json` is allowed |
| 1-6 | README omits the loop breaker and plan-mode skip | stale-doc | low | S | hooks.mjs:548, 563-591. troubleshooting.md:120 |
| 1-7 | Dead `SubagentHandback` hook; removal is an open owner question | quality-issue | low | S (O) | hooks.json:32-36. evals.md:262-265 |
| 1-8 | Anti-bypass shell heuristics broad and tested | done | info | none | hooks.mjs:368-490. 12 tests |
| 1-9 | Lint-baseline ratchet complete and proven on a brownfield repo | done | info | none | baseline.mjs 504 lines, 31 tests, brownfield e2e 27 s |
| 1-10 | Report, stats, status CLI done | done | info | none | report.mjs, 19 tests, docs/examples |
| 1-11 | Commands and agents match README | done | info | none | README:47-56. agents/rust-reviewer.md:5-8 |
| 1-12 | Setup, vendoring and upgrade shaped by real adoptions | done | info | none | CHANGELOG 1.3.0. weigh-in#75, redkiln-rs#4 |
| 1-13 | Discriminating target met; overall measured lift modest (+0.31 all-eight) | partial | medium | M | evals.md:45, 57, 62, 72, 75 |
| 1-14 | Real-world validation window far shorter than the exit criterion | partial (time-gated) | medium | M | plan:110. #15 no comments |
| 1-15 | Go-live checklist unticked; tracking issues stale | stale-doc | low | S (O) | #20, #6, #7 |
| 1-16 | README accurate but dense, not a quick start; internal status and support channel not stated | partial | low | S | README:10-23. troubleshooting.md:41 |
| 1-17 | README compatibility lags the latest evidence (2.1.289) | stale-doc | low | S | README:28. evals.md:235 |
| 1-18 | macOS claim backed only by one manual probe | partial | low | S | README:77. PR #29 |
| 1-19 | CI reliability: run 69 cancelled, run 70 needed a retry (~53 min) | risk | low | S | actions/runs/37367980815 |
| 1-20 | Release, versioning and distribution machinery works | done | info | none | v1.3.0. Release run 1 |
| 1-21 | 31 of 107 Don't blocks are neither compiled nor tied to a diagnostic | quality-issue | low | M | ci.log "Don't blocks without expect=: 31". doc_examples.py:17 |
| 1-22 | Enforcement boundary outside Claude Code clearly stated | done | info | none | README:168-170 |

### 5.2 The standard (13)

| ID | Finding | Status | Sev | Effort | Key evidence |
|---|---|---|---|---|---|
| 1-1 | 182 rules, counts consistent everywhere | done | info | none | README:5, AGENTS.md:16, plan:21 |
| 1-2 | Every code example compiled or linted, 0 failures | done | info | none | ci.log:967-1001 |
| 1-3 | Lint and scanner claims verified against configuration | done | info | none | 254 + 4 citations, 0 false |
| 1-4 | Sources recorded with licences and non-adoptions | done | info | none | sources.md |
| 1-5 | Only about half the rules (96) have a mechanical enforcer; 84-86 review-only | risk | medium | XL | Parse of the rules.md "Enforced by" column |
| 1-6 | 31 MUST rules enforced by review alone | risk | medium | L | IDs listed in finding (API-03 … PROC-11) |
| 1-7 | Evals cite only 3 rule IDs (H-08, H-16, OWN-03) | partial | low | L | grep of evals/ |
| 1-8 | 13-17 rules lean on scan advisories with no lint backstop | quality-issue | low | M | rules.md:49, 80, 115, 117, 138, 173, 176 |
| 1-9 | Review rubric names only 72 of 182 rule IDs | quality-issue | low | M | review-rubric.md:5 |
| 1-10 | Plan citation count stale (2754 vs 2757) | stale-doc | info | S | plan:21 |
| 1-11 | 31 Don't blocks lack `expect=` | partial | low | M | ci.log:974, 990 |
| 1-12 | Scanner keys `catch-unwind`, `path-attr`, `todo-comment` have no test | risk | low | S | scan.mjs:103, 185, 478 |
| 1-13 | Progressive-loading structure covers the promised areas | done | info | none | SKILL.md 146 lines, 14 references |

### 5.3 Engine (19)

| ID | Finding | Status | Sev | Effort | Key evidence |
|---|---|---|---|---|---|
| 1-1 | Architecture complete: zero-dependency CLI, single entry, 9 registrations | done | info | none | temper.mjs:33-69. hooks.json |
| 1-2 | Deterministic CI green: 156 tests in 8 files | done | info | none | ci.log:941-946 |
| 1-3 | Windows handling deliberate and CI-verified | done | info | none | project.mjs:136-165 |
| 1-4 | Hook and scanner performance good; gate cached per fingerprint | done | info | none | pre-edit 94 ms. 13,461 lines in 64 ms |
| 1-5 | Earlier security findings fixed and regression-tested | done | info | none | project.mjs:226-255 (safeOutputPath) |
| 1-6 | Verdicts and gate cache forgeable by shell write; Stop trusts them | risk | medium | S | gate.mjs:203-207. hooks.mjs:580-581. verdict.mjs:80-84 |
| 1-7 | Shell writes to protected config unguarded; a broken toolchain file turns Stop into a pass-through | risk | medium | S | hooks.mjs:349-353. gate.mjs:179-190. hooks.mjs:561-564 |
| 1-8 | `cp`/`mv`/`git apply`/`patch` of .rs skip the pre-write scanner and the `#[expect]` ask | risk | low | S | hooks.mjs:349 has no `copies: true` |
| 1-9 | Orchestration handlers have no deterministic tests | quality-issue | medium | M | hooks.test.mjs:9 imports only preEdit/preBash |
| 1-10 | Gate latency against the 1200 s Stop budget never measured on a pilot | not-started | low-medium | M | project.mjs:89. hooks.json |
| 1-11 | Fail-open design; malformed stdin exits 0 silently | risk | low | S | temper.mjs:56, 59-65 |
| 1-12 | Verdict validation accepts degenerate evidence (suffix match, all `n/a`) | quality-issue | low | S | verdict.mjs:55-64 |
| 1-13 | `SubagentHandback` hook is dead code | partial | low | S (O) | hooks.json:33. evals.md:262 |
| 1-14 | README says every hook was confirmed to run | stale-doc | low | S | README:28 vs CHANGELOG:43 |
| 1-15 | Fingerprint ignores temper.json, `.cargo/config.toml` and `include_str!` inputs | risk | low | S | project.mjs:317-331 |
| 1-16 | State under `.git/temper` grows without bound | quality-issue | low | S | project.mjs:303-310. gate.mjs:173, 202 |
| 1-17 | `--no-verify` and `clippy -A` match inside quoted text (false positive) | quality-issue | low | S | hooks.mjs:471, 474 |
| 1-18 | Nested workspaces unsupported; only the 2.0 plan says so | stale-doc | low | S | temper.mjs:260-263 |
| 1-19 | 2.0 engine not started (correctly gated) | not-started | info | XL | #21 |

### 5.4 Workflow surface (10)

| ID | Finding | Status | Sev | Effort | Key evidence |
|---|---|---|---|---|---|
| 1-1 | 5 commands and 3 agents consistent with README | done | info | none | README:49-53 |
| 1-2 | Templates complete, including templates/crate | done | info | none | #32/#33, #38/#39 |
| 1-3 | Example workspace in sync; its cargo tests run in GitHub CI only | partial | low | S | ci.yml:67-68. ci.sh:31-45 |
| 1-4 | Eval target met on the discriminating group | done | info | none | evals.md targets. #8 closed |
| 1-5 | +0.58 is a substitution, not a full rerun; #47 open | partial | low | S (O) | evals.md:45 |
| 1-6 | #48: final report does not restate declined requests | not-started | medium | S | skills/implement step 6. evals.md:72, 81 |
| 1-7 | Orchestration skills and architect/implementer agents have no behavioural evals | risk | medium | L | evals.md:266 |
| 1-8 | `SubagentHandback` never observed firing | risk | low | S | hooks.mjs:610 |
| 1-9 | Eval method limits (no Bash by default, judge sensitivity, no network) | quality-issue | low | M | evals.md:49, 118-120, 158 |
| 1-10 | #19 and #20 still open | partial | medium | S (O) | evidence-github.md:22-23 |

### 5.5 Release and CI (13)

| ID | Finding | Status | Sev | Effort | Key evidence |
|---|---|---|---|---|---|
| 1-1 | Release workflow built and used for 1.3.0 | done | info | none | release.yml. run 37263274680 |
| 1-2 | Executable release check wired into CI and release | done | info | none | release-check.sh. ci.sh:35-36 |
| 1-3 | Versioning policy written | done | info | none | CONTRIBUTING.md:24-55 |
| 1-4 | Deterministic CI comprehensive and green at HEAD | done | info | none | run 73 |
| 1-5 | Pre-release model checks manual and not enforced | partial | low | S | release-check.sh has no smoke or evals check |
| 1-6 | Protection and environment reviewer not verifiable from the repo (later shown false for checks, see v1-plan 1-1) | risk | low | S (O) | AGENTS.md:8 |
| 1-7 | Run 69 cancelled; concurrency also cancels main pushes (50, 55, 56) | risk | low | S | ci.yml:12-14 |
| 1-8 | Dependabot for action SHAs; small pin gaps (`dtolnay` master SHA, `npx --yes`) | done | info | S | dependabot.yml |
| 1-9 | macOS and arm64 not in CI; probe never re-run on main | partial | low | S (O) | runner-probe.yml:21. PR #29 |
| 1-10 | Unreleased changes on main after 1.3.0; no 1.3.x | partial | low | S | CHANGELOG:3-8 |
| 1-11 | Tracking issues and go-live checklist stale | stale-doc | low | S | #6, #20, #47 |
| 1-12 | 2.0 pre-port CI and release environment work not started | not-started | medium | L (O for #23) | #22-#24 |
| 1-13 | Local CI green with one Windows-only skip | done | info | none | ci.log |

### 5.6 v1 plan (17)

| ID | Finding | Status | Sev | Effort | Key evidence |
|---|---|---|---|---|---|
| 1-1 | **temper main ruleset requires no CI status checks** | risk (claimed done, not enforced) | medium-high | S (O) | `gh api .../rulesets/24368200` |
| 1-2 | **Pilot gates are not required checks in either pilot; the #14 closure claim is false** | partial | medium-high | S (O) | weigh-in ruleset 24377415. redkiln-rs 24430583 `[]`. temper#14 comment 5977012383 |
| 1-3 | Pilot findings period is the critical path; earliest exit about 2026-10-17 | partial | medium | M | #15. weigh-in#82 |
| 1-4 | 2.4: only adoption fixes shipped; wi-a9eb36 cannot be settled by these pilots | partial | medium | M | decisions.md:41. pilot dependabot files |
| 1-5 | Plan document stale in both directions | stale-doc | low | S | plan:3, 86, 106, 115, 125-127, 155 |
| 1-6 | Tracking issues #6, #7, #19, #20, #23 stale | stale-doc | low | S | #23 item 1 done: environment created 2026-10-04T20:37Z |
| 1-7 | Phase 0 steps 0.2-0.8 delivered; 31 PRs; tag ruleset | done | info | none | ruleset 24470245 |
| 1-8 | Budget alerts unverified; one-off crates.io token not revoked | partial | low | S (O) | plan:93-94. #7, #23 |
| 1-9 | Phase 1 complete | done | info | none | evals.md:5-24 |
| 1-10 | Phase 3 complete; 1.3.0 shipped through the gated flow | done | info | none | approvals API |
| 1-11 | Go-live items 6 and 8 met | done | info | S | README:25-166 |
| 1-12 | Items 5 and 7 met for 1.3.0, but which release is v1 is undecided | partial | medium | M | plan:138 |
| 1-13 | Item 2 (org members can install) unverified beyond the owner | partial | low | S (O) | PR #52 |
| 1-14 | Item 9 (announcement) not started | not-started | medium | S (O) | plan:140 |
| 1-15 | Item 4 trivially true: no priority labels or milestones | risk | low | S | labels list |
| 1-16 | redkiln-rs adoption only on `build/phase-0-to-1` | risk | low | S (O) | main 353ea01 (2026-09-11), no `.claude` |
| 1-17 | Critical path to v1 live (see section 6) | partial | medium | M | — |

### 5.7 Pilots (14)

| ID | Finding | Status | Sev | Effort | Key evidence |
|---|---|---|---|---|---|
| 1-1 | weigh-in: 1.3.0 vendored, byte-identical | done | info | none | `diff -r` clean |
| 1-2 | weigh-in: CI `gate` runs the ratchet; baseline fully remediated 721→0 | done | info | none | rust.yml:21, 69-77. totals `{}` |
| 1-3 | weigh-in: charters confirmed | done | info | none | weigh-in#63 |
| 1-4 | redkiln-rs: 1.3.0 on the build branch, job `temper`, baseline 6,606, 12 charters | done | info | none | build branch 62dd3b8 |
| 1-5 | redkiln-rs main has no temper and no Rust (213 commits behind) | risk | low | S (O) | `git rev-list --count` = 213 |
| 1-6 | Two-week soak not elapsed | partial | medium | M | adoption 2026-10-03 |
| 1-7 | No `temper stats` telemetry or gate-latency evidence from either pilot | not-started | medium | M | telemetry is local `.git/temper` only |
| 1-8 | Findings not filed one per issue; only bundled #32 | partial | low | S | evidence-github.md §1-2 |
| 1-9 | Pilot-driven fixes shipped in 1.3.0; upgrade path exercised | done | info | none | #33, #39, weigh-in#75, redkiln-rs#4 |
| 1-10 | wi-a9eb36 unsettled | not-started | low | S | decisions.md:41 |
| 1-11 | Required-check status only asserted in a comment (since shown false; see v1-plan 1-2) | risk | low | S (O) | temper#14 |
| 1-12 | redkiln-rs baseline (6,606) not tightened since adoption; no remediation owner | partial | low | L | baseline a06af34 |
| 1-13 | #20 item 3 unticked | stale-doc | low | S | plan:133 |
| 1-14 | temper CI flakiness relevant to pilot trust | risk | low | S | run 69/70 |

### 5.8 2.0 port (19)

| ID | Finding | Status | Sev | Effort | Key evidence |
|---|---|---|---|---|---|
| 1-1 | Plan fully specified; D1-D17 + licence settled, anchors valid | done | info | none | plan:860-881 |
| 1-2 | No phase started; correctly gated | not-started | info | XL | `ls` of Phase 0a artifacts all absent |
| 1-3 | crates.io names and npm org reserved | done | info | none (O) | crates.io API, npm org 200 |
| 1-4 | Runners confirmed by probe; issues not updated | done | info | S | runs/37043545399 |
| 1-5 | Release environment + trusted publishing not confirmed | partial | medium | S (O) | #23 0/5 |
| 1-6 | One-off crates.io and npm tokens not confirmed revoked | risk | low-medium | S (O) | plan:93. reserve-temper-crates.sh |
| 1-7 | Tiered CI and aggregator (#24) not started | not-started | low | M | ci.yml two jobs |
| 1-8 | Plan still requires macOS CI and parity for graduation, though macOS is parked | stale-doc | low | S (O) | plan:555, 559, 706, 889 |
| 1-9 | Progress "Next" line lists done prerequisites | stale-doc | low | S | plan:889-890 |
| 1-10 | Baseline metrics slightly stale; size estimate holds | stale-doc | low | S | 5,649 lines. 156 tests vs 140 cited |
| 1-11 | 1.3.0 setup features and release-check tooling missing from the test list and D16 | stale-doc | medium | S | plan grep finds only :185 |
| 1-12 | `LEGACY_TEMPLATES` stops at v1.2.3, but rust.yml changed in 1.3.0 | stale-doc | low | S | plan:684. commit 1398194 |
| 1-13 | Hash-guarded `--migrate` will not rewrite the pilots' customised rust.yml | risk | low | M | sha256 mismatches |
| 1-14 | Plan does not cover the claude.ai org-sync and cloud-session install paths | risk | medium | M | plan:644-651 vs troubleshooting.md:27-41 |
| 1-15 | Pre-port step 1 open; pilot window not elapsed | not-started | medium | M | #22 |
| 1-16 | Port start depends on a go-live checklist that has not begun | risk | medium | S (O) | #20 |
| 1-17 | Effort realism: ~26k lines, about 6-10 weeks elapsed; no dates in the plan | risk | medium | XL | plan:838-858 |
| 1-18 | 1.x release flow already follows the port's §4 shape | done | info | none | release.yml |
| 1-19 | Reserved crates have a single individual owner (bus factor) | risk | low | S (O) | crates.io owners API |

### 5.9 Docs and governance (11)

| ID | Finding | Status | Sev | Effort |
|---|---|---|---|---|
| 1 | CHANGELOG and plugin.json versions aligned (1.3.0) | done | info | none |
| 2 | Rule count in AGENTS.md accurate | done | info | none |
| 3 | Troubleshooting exists and is linked | done | info | none |
| 4 | Contributor files present | done | info | none |
| 5 | Decision records complete and linked (21) | done | info | none |
| 6 | Vendored CLI in sync | done | info | none |
| 7 | Live plans present | done | info | none |
| 8 | #6 body stale (still "in the open PR") | stale-doc | low | S (O) |
| 9 | #20 completely unticked (partly genuinely open) | partial stale-doc | low | S (O) |
| 10 | CI timeout and retry risk (runs 69/70) | risk | low | S |
| 11 | Post-1.3.0 commits unreleased (expected) | done | info | none |

### 5.10 Closure of history findings (16)

| ID | Finding | Status | Sev | Effort |
|---|---|---|---|---|
| 2-1 | 1.1 content review (35) carried through | done | info | none |
| 2-2 | 1.1 machinery review (24) fixed and tested | done | info | none |
| 2-3 | 1.1 adoption spec items all exist | done | info | none |
| 2-4 | 1.2 baseline spec shipped and extended | done | info | none |
| 2-5 | 1.2 review blockers fixed | done | info | none |
| 2-6 | Vendored CLI editable; CI runs it unverified (1.2 item 27). Agent shell writes are now denied, but there is no CI integrity check and no decision | risk | low | S |
| 2-7 | Nested workspaces unsupported (item 19) | partial | low | M |
| 2-8 | Content finding 21 (MOD-13) only partly fixed: fallback docs step `RUSTDOCFLAGS`, no `--keep-going` | partial | low | S |
| 2-9 | 7 of 18 decisions.md anchors drifted | stale-doc | low | S |
| 2-10 | Review and spec docs cite scratchpad evidence not in the repo | stale-doc | low | S |
| 2-11 | wi-a9eb36 not settled by pilots | partial | low | S |
| 2-12 | Spec deviations in CHANGELOG and review, not in decisions.md | quality-issue | low | S |
| 2-13 | Skill-gate deny shows 12 of 16 iron rules (`slice(0,12)`, hooks.mjs:260) | partial | low | S |
| 2-14 | No pilot-observed false-positive tally | partial | medium | M |
| 2-15 | Minor content items not individually checked; DynClock defined 3× | risk | info | S |
| 2-16 | Decision log complete and traceable | done | info | none |

### 5.11 Security, privacy, licensing (12)

| ID | Finding | Status | Sev | Effort |
|---|---|---|---|---|
| 2-1 | Workflows least-privilege, SHA-pinned, no `pull_request_target` | done | info | none |
| 2-2 | SECURITY.md has a private channel but is thin (personal email, no SLA or supported versions) | partial | low | S (O) |
| 2-3 | Telemetry local-only; no prompts or code; no network code | done | info | none |
| 2-4 | Telemetry not disclosed in README; no retention; no telemetry-specific opt-out | quality-issue | low | S |
| 2-5 | Vendored `.claude/temper/` has no integrity check; no CODEOWNERS guidance | risk | low | M |
| 2-6 | No adopter-side check that the vendored copy matches a release (`--dry-run` drift is manual) | partial | low | M |
| 2-7 | MIT consistent with wi-4679b3 | done | info | none |
| 2-8 | LICENSE-APACHE not yet present (Phase 0a) | not-started | low | S (O) |
| 2-9 | Source attributions respect licences | done | info | none |
| 2-10 | No NOTICE / third-party file | quality-issue | low | S |
| 2-11 | Release job input handling safe | done | info | none |
| 2-12 | Internal repo while docs assume public consumption; go-public hardening list | risk | low | S (O) |

### 5.12 Fresh-adopter experience (9)

| ID | Finding | Status | Sev | Effort |
|---|---|---|---|---|
| 2-1 | Setup CLI deterministic and fast; greenfield gate goes green once a crate exists | done | info | none |
| 2-2 | No documented greenfield path | partial | low | S |
| 2-3 | Empty workspace gives a red gate with raw rustfmt help text | quality-issue | low | S |
| 2-4 | Stock `cargo new --lib` fails the gate (docs, must_use); duplicate `[lints]` gives an unhelpful message | risk | low | S |
| 2-5 | `.gitignore` gets only `.temper/`; `target/` untracked | quality-issue | low | S |
| 2-6 | Upgrade 1.2.3→1.3.0 works; `--vendor-only` and `--force` verified | done | info | none |
| 2-7 | Drift message recommends `--force` and the README `--vendor-only`; persistent `manual` lines | quality-issue | low | S |
| 2-8 | Node 18 and Claude Code 2.1.286 minimums never checked; Node 18 untested in CI | partial | low | S |
| 2-9 | Time to first green short (5-15 min) but the steps are partly implicit | partial | low | S |

### 5.13 Runtime cost and context budget (11)

| ID | Finding | Status | Sev | Effort |
|---|---|---|---|---|
| 2-1 | Hook-injected context small and bounded (SessionStart 3,109 B) | done | info | none |
| 2-2 | Hook latency low (90-185 ms) | done | info | none |
| 2-3 | Standard loads ~4.4k tokens plus 9-22k per reference; not documented or budgeted | risk | low | S |
| 2-4 | A review round adds ~10k fixed tokens, ~25-30k with a reference | risk | low | S |
| 2-5 | SessionStart re-injects on resume and compact (documented; cost unquantified) | done | info | none |
| 2-6 | Stop-gate latency on a real workspace not measured | partial | medium | M |
| 2-7 | Eval cost well recorded (~$30-35 total) but uncapped | done | info | S (O) |
| 2-8 | Actions budget alerts not set (#7) | not-started (unconfirmed) | medium | S (O) |
| 2-9 | CI minutes per run not recorded; worst case 120 billable-equivalent minutes | risk | low | S |
| 2-10 | telemetry.jsonl has no cap or rotation | quality-issue | low | S |
| 2-11 | Per-edit pre-hook scans the whole file; not benchmarked on large files | partial | info | S |

### 5.14 Backlog and open decisions (13)

| ID | Finding | Status | Sev | Effort |
|---|---|---|---|---|
| 2-1 | Backlog covers every plan step (12 open, 0 open PRs) | done | info | none |
| 2-2 | No P0/P1 labels, so go-live item 4 cannot be evaluated | not-started | medium | S |
| 2-3 | Milestones `v1 live` and `2.0` do not exist (`[]`), but docs point to them | stale-doc | low | S (O) |
| 2-4 | Tracking issue bodies stale | stale-doc | low | S |
| 2-5 | Owner-only items outstanding: budget alerts, go-live sign-off | not-started | medium | S (O) |
| 2-6 | #15 window and #16 1.3.x still open | partial | medium | M |
| 2-7 | Eval follow-ups #47 and #48 open (~$12+ to re-run) | partial | low | M |
| 2-8 | #19 open though 1.3.0 was checked | partial | low | S |
| 2-9 | No pending Weigh-In decisions; no stray TODOs | done | info | none |
| 2-10 | Two-way defaults lack a review trail | risk | low | S (O) |
| 2-11 | 2.0 pre-port work unstarted and correctly gated | not-started | info | XL |
| 2-12 | CI flake not tracked in an issue | risk | low-medium | S |
| 2-13 | Unreleased commits (#51-#53); no 1.3.x | partial | info | M |

---

## 6. Remaining effort roll-up

### 6.1 To "v1 live": about 3-5 working days of effort over about 2 calendar weeks

The critical path is the pilot soak. Ordered:

1. **Owner, S (minutes each):** add required status checks `Checks` and `Unit tests (Windows)` to temper's `Protect main` ruleset; add `gate` to weigh-in's ruleset; protect redkiln-rs `build/phase-0-to-1` with `temper` required, or record explicitly that a build-branch gate satisfies item 3.
2. **Agent + owner, M, elapsed to about 2026-10-17:** run the soak; file one `pilot` issue per finding with a severity; capture `temper stats` from each pilot and a measured cold and warm Stop-gate time on redkiln-rs.
3. **Agent, M:** fix the findings; decide or re-scope wi-a9eb36; land #48 (final report restates declined requests).
4. **Owner decision, S:** is v1 the existing 1.3.0, or a 1.3.x / 1.4.0 cut after the fixes?
5. **If a new release, agent + owner, S-M:** release PR, smoke run, discriminating eval run (owner approves about $9-12), Release workflow, merge commit.
6. **Owner, S:** one other org member dry-runs the install; send the announcement; create P0/P1 labels or reword item 4; tick #20 and close #6.
7. **Agent, S:** refresh the plan document and issue bodies (#6, #7, #19, #20, #23); fix README drift (2.1.289, SubagentHandback, internal and support line).

### 6.2 Recommended 1.x hardening before the parity harness freezes behaviour: about 4-6 working days

- Deterministic tests for Stop/completionGate, SubagentStop, `validateVerdict`/`extractVerdict`, SessionStart sources, the skill-gate deny and the loop breaker (M). These also serve as the 2.0 parity oracle.
- preBash deny for shell writes to the state dir and to `config.protected` globs; treat "cargo unusable after a toolchain edit" as red (S).
- `copies: true` for `.rs` shell writes; quote-blanking for the `--no-verify` and `-A` checks (S).
- Tighten verdict validation: path-component match, a minimum number of integer scores (S).
- Prune or cap telemetry, gate cache and logs; disclose telemetry in the README (S).
- Greenfield quick start, empty-workspace message, `.gitignore` additions, drift message pointing to `--vendor-only` (S).
- Decide on `SubagentHandback` (owner, S).

### 6.3 To 2.0: XL, about 6-10 weeks elapsed after v1

Pre-port: #22 fold-in (M), #23 owner setup (S-M), #24 tiered CI (M-L). Then phases 0a-8: about 26k lines added and about 14k deleted. Binding constraints are owner review and approval bandwidth, owner-only registry and environment setup, and the Actions budget. Before Phase 0a, rebase the plan to 1.3.x: test counts, setup features, `LEGACY_TEMPLATES`, the macOS criterion, org-sync and cloud install paths, and the patch-release policy.

---

## 7. Owner-only actions

1. Add required status checks to temper `main` (ruleset 24368200).
2. Add `gate` to weigh-in's required checks; decide and implement protection for redkiln-rs `build/phase-0-to-1`.
3. Set Actions budget alerts at 75% and 90% (#7).
4. Revoke the one-off crates.io token; remove `CRATES_TOKEN` and the npm token from the cloud environment (#23).
5. Configure crates.io and npm trusted publishing; confirm the `release` environment reviewer (#23).
6. Decide which release is "v1" (1.3.0 vs 1.3.x / 1.4.0) and approve eval spend for it.
7. Have one other Wet Ink org member verify marketplace install (go-live item 2).
8. Write and send the announcement / onboarding note (item 9).
9. Create P0/P1 labels and `v1 live` / `2.0` milestones, or reword the docs to say "label".
10. Decide `SubagentHandback` removal and wi-a9eb36 (and review the other two-way defaults).
11. Restate the port's macOS graduation criterion given macOS CI is parked.
12. Add a co-owner (team) to the crates.io crates and a second npm org admin.
13. Before going public: strengthen SECURITY.md (shared mailbox, supported versions, private vulnerability reporting), confirm secret scanning and push protection.
14. Tick #20 and close #6 once the above are true.

---

## 8. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Required checks are not enforced on temper or the pilots, while docs claim they are | A red PR could merge to `main`, which is what marketplace users install. The pilot gate is advisory | Owner adds the checks (minutes). Add a CI or scripted ruleset assertion |
| Review and gate state forgeable via shell; config and toolchain edits disable the Stop gate | The "independent review" guarantee is trust-based. A confused or adversarial agent can end a turn "approved" | preBash state-dir and protected-glob rules; treat a broken toolchain as red; consider HMAC'd state |
| Headline orchestration paths untested deterministically | A regression in Stop, verdict or loop-breaker logic ships green. The 2.0 parity harness lacks an oracle | Write Node unit tests now |
| Pilot soak yields little data (no telemetry export, no latency measurement) | v1 declared without evidence that daily use is tolerable; false-positive rate unknown | Make `temper stats` capture and Stop-gate timing explicit deliverables of #15 |
| Stop gate exceeds the 1200 s hook budget on large repos (redkiln-rs) | The hook is killed and the turn ends unenforced, with orphaned cargo processes | Measure. Document expected times. Bound gate steps to the hook budget |
| Thin eval margin (+0.42 vs +0.4) and only 3 rule IDs exercised | A new release could miss the target; most rules are behaviourally unproven | Full rerun after the #47 swap; add orchestration evals over time |
| Context and token cost undocumented (~4.4k up front, 25-30k per review round) | Adopters surprised by cost; no regression guard on standard growth | Document the cost; add a size budget test for SKILL.md and the rubric |
| Single-owner bus factor (merges, approvals, crates.io, releases) | Delays or a blocked release if the owner is unavailable | Second admin for registries; document the delegation path |
| 2.0 port scale and gaps (regex parity, npm optionalDependencies, org-sync/cloud install paths not planned) | Multi-week effort could stall or regress users | Strangler approach keeps users on 1.x. Rebase the plan; spike distribution in Phase 0b |
| Unexplained CI cancellation (run 69) and slow reruns | A flaky required check erodes trust once checks are actually required | Open an issue; inspect run logs; record minutes per run |

---

## 9. What the plans got wrong / stale docs

**Claimed done but not true (most important):**
- v1-readiness 0.1 (plan:86), AGENTS.md:8 and CONTRIBUTING.md:10 say `main` requires `Checks` and `Unit tests (Windows)`. The live ruleset has no status-check rule and never had one (created and updated at the same timestamp).
- v1-readiness 2.2 (plan:115) and the comment closing temper#14 say `gate` and `temper` are required checks in the pilots. Neither ruleset requires them.
- docs/history/evals.md:266 says "the unit tests cover their handlers" for SessionStart resume, clear, compact and fork. No such tests exist.
- README:28 says every hook in hooks.json was confirmed to run. `SubagentHandback` never runs (CHANGELOG:43).

**Done but not marked:** plan steps 1.5, 3.1, 3.2 and 3.3 are not struck through. Line 155 still says "run the runner probe". #7 probe boxes and #23 item 1 (environment) are unticked. #20 is 0/9 ticked though about 4 items are met. #6's body still says "in the open PR".

**Referenced things that do not exist:** `v1 live` / `2.0` milestones (CONTRIBUTING.md:7, plan:135). P0/P1 labels (plan:110, 135). Scratchpad evidence paths in the 1.1/1.2 review docs. The adoption spec's `scratchpad/findings/*.md` names.

**Port plan drift from 1.3.0:** 140 vs 156 tests (143 engine + 13 release-check). Parity "exactly as in 1.2.3". `LEGACY_TEMPLATES` v1.0.0..v1.2.3 though rust.yml changed in 1.3.0. `--vendor-only`, strict-lint and clippy.toml checks and `release-check.sh` absent from the test list and D16. macOS required in CI and for graduation though parked. Progress "Next" lists done prerequisites. No org-sync or cloud-session install path in §5.3. No patch-release policy during the port. No time estimate or milestone dates.

**Smaller drift:** plan:21 citation count (2754 vs 2757). README Claude Code version list (missing 2.1.289). 7 of 18 decisions.md anchors. Review rubric claims "every rule" but names 72 of 182. Baseline spec still shows the dropped `files` table and the in-job PR comment. Plan 2.4 expected "1.3.x" but 1.3.0 shipped. wi-a9eb36 was planned to be settled by pilots whose configuration cannot test it.

---

## 10. Refuted findings (dropped)

Two candidate findings failed verification and are excluded:
1. "Main CI run 69 cancelled after ~15 min and PR #52 needed a rerun: flaky or slow CI" (as a standalone *flaky/slow suite* claim). The underlying events are real and kept as low-severity risks elsewhere, but the claim of a slow or flaky test suite was not supported.
2. "Version tags missing from the audit clone; 1.2.3 simulated from commit a485408". This was an audit-environment artefact (shallow clone without tags), not a product finding.

---

## 11. Key numbers

| Measure | Value |
|---|---|
| Plugin version / audited commit | 1.3.0 / d87d887 |
| Unit tests | 156 (155 pass, 1 Windows-only skip) |
| Rules / citations / doc blocks failing | 182 (95 MUST, 78 SHOULD, 9 CONSIDER) / 2,757 / 0 of 329 |
| Rules with a mechanical enforcer | ~96 of 182 (53%). 31 MUST rules are review-only |
| Discriminating eval delta | +0.55 / +0.44 / +0.42 against a +0.4 target (all-eight mean +0.31) |
| Hook latency | 90-105 ms (most), 165-185 ms (SessionStart, SubagentStart) |
| Node engine size | 5,649 lines, 13 modules, 56 scanner keys |
| PRs merged 2026-10-02..06 | 31 |
| Open issues / open PRs | 12 / 0 |
| Pilots on 1.3.0 | 2 of 2. weigh-in baseline 721→0. redkiln-rs 6,606 |
| Days of pilot soak elapsed / required | ~3 / 14 (earliest exit ~2026-10-17) |
| Go-live checklist | 0/9 ticked; ~4 met in substance |
| 2.0 port phases started | 0 of 11 (0a, 0b, 0c, 1-8). ~26k lines estimated |
| Verified findings / refuted | 199 / 2 |
