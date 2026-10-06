# GitHub evidence (read-only, collected 2026-10-06 via GitHub MCP)
Repo: https://github.com/Wet-Ink-Corporation/temper (visibility: internal, default branch main, MIT). Open issues count 12.
Note: list_pull_requests reports `merged:false` for every PR but sets `merged_at`; treat PRs with merged_at as merged (all PRs below are closed with merged_at, none open).

## 1. Tracking issues
### #6 "v1 live: tracking" (open; labels tracking, v1-live; created 2026-10-02; never updated since)
https://github.com/Wet-Ink-Corporation/temper/issues/6 . Sub-issue summary: 14 total, 9 completed (64%). Body has prose status only (no checkboxes); it still says "0.3, 0.5, 0.6 are in the open PR" (stale: PR #25 merged 2026-10-02) and does not reflect pilots/release 1.3.0. Body is stale-doc.
| # | title | state | labels | closing PR / evidence |
|---|---|---|---|---|
| 7 | 0.9: run the runner probe and set Actions budget alerts | OPEN | v1-live, area:ci, owner | Probe ran (see sec 4: run 1 failed, run 2 success 2026-10-02) and PR #26 fixed its bugs, but issue still open: budget alerts owner item unchecked/unknown |
| 8 | 1.1: add discriminating behavioural evals | closed 2026-10-05 | area:evals | PR #49 "Closes #8"; target discriminating mean >= +0.4 (runs +0.55,+0.44,+0.42), regression each >=0 |
| 9 | 1.2: Claude Code compatibility | closed 2026-10-02 | area:hooks | PR #30 (min CC 2.1.286) |
| 10 | 1.3: upgrade path docs | closed 2026-10-02 | area:docs | PR #27 |
| 11 | 1.4: troubleshooting docs | closed 2026-10-02 | area:docs | PR #28 |
| 12 | 1.5: executable release check | closed 2026-10-04 | area:ci | PR #34 |
| 13 | 2.1: pick two pilot repos | closed 2026-10-03 | owner, pilot | PR #31 (weigh-in, redkiln-rs) |
| 14 | 2.2: adopt temper in pilots | closed 2026-10-04 | pilot | weigh-in#37 merged (baseline 735, 727 after tighten), redkiln-rs#2 merged into build/phase-0-to-1 (baseline 6,606; was 12,147 before test-code exemption); charters confirmed weigh-in#63, redkiln-rs#3; comment says temper check is a required check in both (gate on main; `temper` on build branch) |
| 15 | 2.3: collect pilot findings | OPEN | pilot | no comments; findings tracked in #32 (closed) |
| 16 | 2.4: fix pilot findings and cut 1.3.x | OPEN | pilot | PR #33 fixed adoption gaps; 1.3.0 released (minor, not 1.3.x) |
| 17 | 3.1: release workflow | closed 2026-10-04 | area:ci | PR #35/#41; Release run 1 on release/1.3.0 succeeded 2026-10-05 |
| 18 | 3.2: versioning policy | closed 2026-10-04 | area:docs | PR #36, decisions PR #42 |
| 19 | 3.3: pre-release model checks (smoke + evals) | OPEN | area:evals | evals recorded in PRs #43,#44,#45,#46,#49; smoke "green on CC 2.1.289" per 1.3.0 commit msg |
| 20 | 4: v1 go-live checklist | OPEN | v1-live | all 9 items still unticked (body unchanged since 2026-10-02) |
Not yet in sub-issue list but open under v1-live: #47 (evals: replace discriminating cases plain model passes; PR #53 merged 2026-10-06 replaced dep case, mentions Refs #47, issue still open), #48 (final status report should restate declined requests).
Go-live checklist (#20) items: main protected+PR/CI; org members can marketplace-add/install; both pilots gate required w/ baseline; no P0/P1 open; evals meet 1.1 target + smoke green; README covers install/setup/upgrade/troubleshooting/emergency switch/min CC version; release workflow published v1 release; support channel named in README; announcement to other Wet Ink repos.

### #21 "temper 2.0 (Rust port): tracking" (open; tracking, 2.0; created 2026-10-02; no updates)
https://github.com/Wet-Ink-Corporation/temper/issues/21 . Sub-issues 3, completed 0 (0%). Phase checklist 0a,0b,0c,1..8 all unchecked ("port starts after v1 live"). Decisions D1-D17 settled per body.
| # | title | state | labels |
|---|---|---|---|
| 22 | 2.0 pre-port: fold pilot findings into 1.x and port plan (3 unchecked items; blocked on #16) | OPEN | 2.0 |
| 23 | 2.0 pre-port: release environment and trusted publishing (release env w/ owner reviewer, crates.io + npm trusted publishing, revoke tokens, arm64 runners confirmed) | OPEN | owner, 2.0 |
| 24 | 2.0 pre-port: tiered CI and required checks for the port | OPEN | area:ci, 2.0 |

## 2. All temper issues (23 total; 12 open, 11 closed)
Open: #6,#7,#15,#16,#19,#20,#21,#22,#23,#24,#47,#48. Closed: #8-#14,#17,#18,#32,#38.
Others: #32 "Pilot: setup copies rust.yml and CI assumptions that do not fit an adopting repository" CLOSED 2026-10-04 (labels v1-live, pilot, type:bug; created 2026-10-02; >=7 items incl. #6 msrv job no baseline mode, #7 setup skips existing clippy.toml and its allow-*-in-tests keys; fixed by PR #33). #38 "`temper setup --vendor-only`" CLOSED 2026-10-04 (PR #39). #47 OPEN (area:evals, 2026-10-05). #48 OPEN (area:evals, 2026-10-05).
Pilot-labelled: #13,#14,#15,#16,#32. No P0/P1-labelled issues exist (labels used: tracking, v1-live, 2.0, owner, pilot, area:ci/docs/evals/hooks, type:bug).

## 3. temper PRs (all 31 listed; all merged/closed; none open)
Newest first (number, title, created -> merged):
53 Replace dependency eval asks-before-crypto-dependency (#47) 10-06 -> 10-06; 52 Correct repo visibility, document org sync 10-05->10-06; 51 Document installing temper in Claude Code cloud sessions 10-05; 50 temper 1.3.0 (release/1.3.0) 10-05; 49 Split eval target regression/discriminating (plan 1.1); 46 Run dependency eval without Bash; 45 Fix brownfield eval grader; 44 Record eval rerun w/ Bash+sonnet judge; 43 Record full eval run plan 1.1 cases; 42 Versioning-policy decisions (3.2); 41 Bring release workflow to main (3.1); 40 Five discriminating eval cases; 39 setup --vendor-only; 37 Mark 2.2 done; 36 Versioning policy; 35 Release workflow; 34 Executable release check (1.5); 33 Fix adoption gaps pilots found; 31 Record pilot repos (2.1); 30 CC compatibility min 2.1.286 (1.2); 29 Park macOS CI until repo public; 28 Troubleshooting docs (1.4); 27 Upgrade path docs (1.3); 26 Fix two runner-probe bugs (macOS paths, Windows example store); 25 Phase 0 contributor files/decision log/cloud hook; 5 Research native binaries in plugins; 4 Plan 0.9 arm64 runners + runner probe; 3 Reserve crate names script (0.8); 2 Plans: commit 2.0 port plan; 1 Plan: repo evaluation and road to v1.
Range 2026-10-02 .. 2026-10-06; 31 PRs in 5 days. PR #51 and #52 are post-1.3.0 and doc-only, on main unreleased. (No open PRs.)

## 4. Workflow runs (workflows: CI, Release, Runner probe, Dependabot Updates)
CI on main (latest first): run 73 success 10-06 (d87d887, PR#53 merge); 71 success 10-06 (#52); **69 FAILURE 10-05 (43d70d5, PR#51 merge): both jobs (Unit tests (Windows), Checks) were "cancelled" after ~15 min (20:09->20:24) (snapshot reading: timeout/hang, not a test failure); https://github.com/Wet-Ink-Corporation/temper/actions/runs/37367980815**. *Correction from verification: that reading was refuted (report §10). The jobs stopped about 15 minutes in, well before the 60 and 30 minute timeouts, and `ci.yml` cancels in-progress runs on the same ref, so the run was most likely cancelled by a later push to main, not hung.*; 67 success 10-05 (#50 release merge); 65,63,61,59,57,51 success; 56,55,50 cancelled (back-to-back merges, concurrency cancel).
CI on PRs: all success (runs 52-72); run 70 (PR#52) needed attempt 2 (completed success; the ~53 min wall clock 20:43->21:36 spans both attempts). *Correction: no flaky or slow job was established; the diagnosis was refuted (report §10).*
Main CI run 69's failing commit (PR#51) was followed by green run 71, so main is green at HEAD d87d887 (run 73 success).
Release workflow: run 1, workflow_dispatch on release/1.3.0, success 2026-10-05T04:23-04:25 (https://github.com/Wet-Ink-Corporation/temper/actions/runs/37263274680). Only one Release run exists (v1.0.0-v1.2.3 tags/releases predate the workflow; created 2026-10-01/02).
Runner probe: run 1 FAILED 2026-10-02 (main; found macOS path and Windows example-store bugs, fixed PR #26); run 2 success 2026-10-02 on branch claude/temper-repo-eval-v1-1o7fj0 (windows-latest and windows-11-arm fixed). Never run on main after the fixes. Runners tested incl. arm64 (windows-11-arm).
Required checks (per AGENTS.md): `Checks` and `Unit tests (Windows)`; CI job names confirmed in run 69 jobs. Ruleset itself not readable via available tools.

*Later verification (2026-10-06, same audit).* The GitHub MCP tools used for this snapshot could not read rulesets. During finding verification, the verifier and then the auditor read them over the REST API with `gh api`. Those reads are the source of the headline required-checks finding:
- `gh api repos/Wet-Ink-Corporation/temper/rulesets/24368200` ("Protect main", created and updated 2026-10-02T14:03:55Z): rules `deletion`, `non_fast_forward`, `pull_request` (0 approvals), with the admin role able to bypass at any time. No `required_status_checks` rule.
- `gh api repos/Wet-Ink-Corporation/temper/rules/branches/main`: `deletion`, `non_fast_forward`, `pull_request` only. `gh api repos/Wet-Ink-Corporation/temper/branches/main/protection`: 404 "Branch not protected".
- `gh api repos/Wet-Ink-Corporation/weigh-in/rulesets/24377415` (updated 2026-10-02T17:06:31Z): `required_status_checks` are `check (macos-latest)`, `check (ubuntu-latest)`, `check (windows-latest)`, `plugin` and `deny`. `gate` is not among them.
- `gh api repos/Wet-Ink-Corporation/redkiln-rs/rulesets/24430583`: targets `~DEFAULT_BRANCH` only, with no required checks. `gh api repos/Wet-Ink-Corporation/redkiln-rs/rules/branches/build/phase-0-to-1`: `[]`.

## 5. Pilot repos
### weigh-in (https://github.com/Wet-Ink-Corporation/weigh-in)
Adoption PR weigh-in#37 merged 2026-10-03 (baseline 735); follow-ups #44 (clear clippy findings), #52/#60/#64/#70 baseline tightening (727 -> 722 -> 721), #63 charters, #75 "Upgrade temper to 1.3.0" closed 2026-10-05 (not necessarily merged; search shows closed_at only). Weigh-in itself is at 0.5.4 (released 2026-10-06). Latest main CI runs (2026-10-06, head cf341e5 Release 0.5.4): `ci` success, `rust` success; earlier runs 151-156 on main all success. Issues mentioning temper: #38 "Cloud SessionStart stops at clippy since temper's lint tables (#37)" (closed, label gap), #51 "Tighten temper baseline after #44" (closed). Open weigh-in #76 "Installed plugin 0.5.2 ships no Windows binary: /weigh-in fails on Windows" (opened 10-05, weigh-in's own issue, not temper). Per temper#14 comment, `gate` is a required check on weigh-in main; I could not independently verify branch protection (no ruleset/branch-protection read tool).
### redkiln-rs (https://github.com/Wet-Ink-Corporation/redkiln-rs)
No issues. PRs: #1 (made build branch CI green), #2 adopt temper (merged 10-03 into build/phase-0-to-1, baseline 6,606), #3 charters (merged 10-04), #4 "Upgrade temper to 1.3.0" (merged 10-05 into build/phase-0-to-1). Latest runs on build/phase-0-to-1: `temper` workflow success (run 11, 10-05), `ci` success (run 25). Adoption is on the build branch, NOT on main (main-branch adoption pending the build branch landing). Required check `temper` per temper#14 comment; not independently verifiable. Baseline 6,606 findings = large legacy debt.
Neither pilot repo has a temper-labelled issue tracking findings; findings live in temper#32.

## 6. Releases / tags
v1.3.0 (2026-10-05, sha 2495d0e, release/1.3.0 commit; https://github.com/Wet-Ink-Corporation/temper/releases/tag/v1.3.0), v1.2.3, v1.2.2, v1.2.1, v1.2.0, v1.1.0, v1.0.0. v1.0.0-v1.2.2 releases all published 2026-10-02T01:55:09-14Z within seconds (backfilled retroactively); v1.2.3 dated 2026-10-01T19:36. No prerelease/draft. Note that the "v1 live" tracking is still open although 1.x releases exist: the go-live checklist has not been formally closed.

## 10-line digest
1. Tracking #6 (v1 live): 9/14 sub-issues closed; open: #7 (owner: probe/budget alerts), #15, #16 (pilot findings), #19 (pre-release model checks), #20 (go-live checklist, all 9 boxes unticked). Body text is stale.
2. Tracking #21 (2.0 port): 0/3 pre-port sub-issues done, no port phase issue started; correctly gated on v1 live.
3. temper 1.3.0 released 2026-10-05 via Release workflow (only run, success); tags v1.0.0..v1.3.0 exist.
4. 31 PRs in 2026-10-02..10-06, all merged, none open; main CI at HEAD d87d887 green (run 73).
5. One red main run (69, PR#51): jobs cancelled after ~15 min (hang/timeout) on Windows+Linux; PR#52 CI also needed a rerun (attempt 2) -> CI flakiness/slowness risk.
6. Evals: discriminating group met +0.4 (+0.55/+0.44/+0.42 full runs; replacement dep case on 10-06 -> +0.58 substituted); #47 and #48 still open follow-ups.
7. Pilots: both adopted (weigh-in main, baseline ~721; redkiln-rs build/phase-0-to-1, baseline 6,606), both upgraded to 1.3.0, CI green; required-check status claimed in temper#14 but not verifiable here.
8. Pilot findings (#32, 7+ items) fixed in PR#33/#39 and shipped in 1.3.0; #15/#16 left open pending a real findings period, no 1.3.x patch cut.
9. Post-1.3.0 commits on main (PR#51 cloud-session docs, #52 visibility docs, #53 eval case) are unreleased.
10. Owner-only items outstanding: #7 budget alerts, #23 release environment / trusted publishing, plus branch protection could not be seen by this audit.
