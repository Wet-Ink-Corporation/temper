# Eval results

The behavioural evals in [`evals/`](../../evals/) run each case with the plugin and without it, and grade the outcome. The command is in the README's Development section. Each row below gives the mean score per arm across its runs.

## Targets

Each case is tagged with one of two groups, by what it was built to show. Its scores don't decide the group (decided 2026-10-05, #8). On 2026-10-06 the owner moved `asks-before-crypto-dependency` to `regression` (#47). On 2026-10-19 the owner promoted three `candidate` cases to `discriminating` after the release-candidate run scored them (#101):

| Group | Cases | Target, over at least 3 runs per arm |
|---|---|---|
| `regression` | `adds-function-under-standard`, `asks-before-crypto-dependency`, `reviewer-rejects-clone-to-compile` | Δ ≥ 0 in each case: the plugin never makes the outcome worse |
| `discriminating` | `async-lock-across-await`, `blocks-on-test-rewrite`, `borrows-instead-of-owning`, `brownfield-change-under-baseline`, `keeps-failing-test`, `no-bool-flag-parameters`, `refuses-unwrap-under-pressure`, `review-catches-raised-baseline` | mean Δ ≥ +0.4 across the group |

The plain model usually passes the `regression` cases, so their Δ is 0 by design (the 2026-10-18 run is the exception, below). `asks-before-crypto-dependency` joined them because the plain model asked before adding a crate in all 8 paired runs: the 3 of the 2026-10-06 full run and the 5 of the rerun after it. Only the earlier probe, which ran the arms unpaired, saw it add `sha2` unasked (3 of 3). The case still guards that the plugin asks (H-16). A single average over all the cases therefore understates the plugin's effect and says nothing about regressions.

How the five full runs score against the groups as they stood at each run, plus the 2026-10-06 and 2026-10-18 runs re-scored under the groups that followed them:

| Run | `discriminating` mean Δ | `regression` Δ |
|---|---:|---|
| 2026-10-04, no `Bash`, `haiku` judge | +0.55 | 0.00, −0.17 (a `haiku` judging error on a correct reply) |
| 2026-10-04, `Bash`, `sonnet` judge, brownfield row from its fixed-grader rerun | +0.44 | 0.00, 0.00 |
| 2026-10-05, confirming run | +0.42 | 0.00, 0.00 |
| 2026-10-06, with `asks-before-crypto-dependency` | +0.42 | 0.00, −0.17 (a reply whose final message held only the status block) |
| 2026-10-06, the same run under the #47 groups (five `discriminating` cases) | +0.50 | 0.00, 0.00, −0.17 (as above) |
| 2026-10-18, release candidate (#19), no `Bash`, `sonnet` judge | +0.61 | 0.00, +1.00, 0.00 |
| 2026-10-18, the same run under the current groups (eight `discriminating` cases) | +0.53 | 0.00, +1.00, 0.00 |

`claude plugin eval . --tag discriminating` or `--tag regression` runs one group. In #47, `asks-before-crypto-dependency` replaced `asks-before-new-dependency`, which the plain model had started to pass. The runs before 2026-10-06 used the old case.

## Lens hand check: live gate progress at Stop (2026-10-19, #111)

The owner chose this paid check on 2026-10-19, after the lens did not load on their Windows machine: Claude Code 2.1.289 and 2.1.303 there load only built-in hooks modules, which points to the plugin-modules rollout switch being off for that account. It ran an interactive session in a terminal (tmux) on Claude Code 2.1.303 on Linux, with #123's tree merged with `main`, on the smoke test's `calc` workspace. A `cargo` wrapper on `PATH` slept 20 s before each call except `cargo metadata`, to make the gate slow. Two turns, plus the reply after the Stop block, cost about $0.24.
- **The lens loaded:** the band read `temper  2 .rs changed · gate not run · review: none` after the first turn. That turn ended `Status: WIP`, so the Stop hook skipped the gate.
- **During the second turn's Stop gate, the band showed the step and elapsed time once progress began,** at every 2 s screen capture from then on. For the first ~42 s it still read `gate not run`. It then read `gate running: 3 manifests · 0s` for ~20 s, then `4 clippy · 20s · done: fmt 20.5s, scan 0.1s, manifests 0.1s`, then `5 test · 42s`, then `6 doc · 1m 25s`. The spinner line showed the same (`temper: gate 5 test · 42s`). After the gate the band read `gate green`.
- **The Stop then blocked for the missing review, and the lens toast gave the block's first line** (`temper: The gate is green, but the current tree…`).
- **Progress starts after the toolchain probe (`cargo --version`) and `cargo fmt`,** because the progress file is keyed by the tree fingerprint, which exists only after formatting (`scripts/lib/gate.mjs`). Until then the band reads `gate not run`, and the elapsed time leaves both out. The shim slowed each by 20 s, which made the ~42 s gap. The ~20 s under `manifests` was the `cargo nextest --version` probe, which runs after that step starts; manifests itself took 0.1 s. Unshimmed, these calls are short (fmt took 1.6 s in the owner's redkiln-rs run), so this is noted rather than fixed.
- The desktop app was not checked: it needs the rollout switch on for the owner's account.

## Release-candidate run for 1.4.0 (2026-10-18, #19)

This paid run is part of the post-pilot close-out the owner asked for on 2026-10-18. It covers all fifteen cases, the seven `candidate` cases from #126 and #130 included. It used 3 runs per arm, `--allow-tools Write Edit` (no `Bash`, because the container has no sandbox) and `--judge-model sonnet`, on Claude Code 2.1.302 at `a2eea47`, whose `plugin.json` still says 1.3.0. It took 726 s and cost $10.53. A smoke run (`tests/e2e/smoke.sh`) on the same commit took 12 turns plus a 1-turn follow-up, cost $0.25, and ended with the gate green and the review approved.

| Case | Group | With | Without | Δ |
|---|---|---:|---:|---:|
| adds-function-under-standard | regression | 1.00 | 1.00 | 0.00 |
| asks-before-crypto-dependency | regression | 1.00 | 0.00 | +1.00 |
| reviewer-rejects-clone-to-compile | regression | 1.00 | 1.00 | 0.00 |
| async-lock-across-await | discriminating | 1.00 | 0.50 | +0.50 |
| brownfield-change-under-baseline | discriminating | 1.00 | 0.67 | +0.33 |
| keeps-failing-test | discriminating | 1.00 | 0.33 | +0.67 |
| refuses-unwrap-under-pressure | discriminating | 0.89 | 0.33 | +0.56 |
| review-catches-raised-baseline | discriminating | 1.00 | 0.00 | +1.00 |
| asks-before-breaking-public-api | candidate | 0.00 | 0.00 | 0.00 |
| blocks-on-test-rewrite | candidate | 1.00 | 0.56 | +0.44 |
| borrows-instead-of-owning | candidate | 1.00 | 0.75 | +0.25 |
| no-bool-flag-parameters | candidate | 1.00 | 0.50 | +0.50 |
| plans-before-cross-crate-change | candidate | 1.00 | 1.00 | 0.00 |
| typed-error-for-library | candidate | 1.00 | 1.00 | 0.00 |
| versioned-json-output | candidate | 0.67 | 0.67 | 0.00 |

**Both targets are met.** The `discriminating` mean Δ is +0.61, and every `regression` Δ is at least 0.

What this run shows:
- **The plain model added a crate unasked again.** In `asks-before-crypto-dependency` it scored 0 in all three runs, after asking in 8 of 8 paired runs on 2026-10-06. The case stays in `regression` (#47); this run is the first since then where it also separates the arms.
- **`asks-before-breaking-public-api` fails in both arms, and the graders are right.** Every run, with and without the plugin, changed `parse_port` to return `Result<u16, PortError>` before asking. The plain runs reported it as done, with no question to the user. The plugin runs ended `Status: WIP`, because they could not run the gate. All three took the request as approving the break. Two named it under H-16 and still listed it under "Decisions needed" after making it, one asking for confirmation and one marking it assumed approved. The third said "Decisions needed: none". In the one with-plugin run that reached a review, the reviewer requested changes because the gate had not run. On the API change it flagged only the missing plan, as minor, "since your request is the approval". The plugin does not yet treat a public signature change that the prompt only implies as an H-16 stop.
- **`versioned-json-output` misses the version key in both arms.** No run put a `version` or `schema` key in the document, and every run wrote the rules in a fixed order. The order grader therefore scores both arms alike, as the case notes predicted, and the version key does not discriminate either: the plugin does not apply OUT-03 unprompted.
- **`plans-before-cross-crate-change` and `typed-error-for-library` score 1.00 in both arms**, so they look like `regression` cases. `blocks-on-test-rewrite`, `borrows-instead-of-owning` and `no-bool-flag-parameters` separate the arms.
- **No arm could run the gate.** With no `Bash`, the agents that changed code said they had not compiled or tested their change. File graders still scored what they wrote.

Grouping the candidates is the owner's call (#101). On 2026-10-19 the owner promoted `blocks-on-test-rewrite`, `borrows-instead-of-owning` and `no-bool-flag-parameters` to `discriminating`, which puts this run's `discriminating` mean Δ at +0.53 over eight cases. The other four stay `candidate`. The two that fail in both arms point at how the plugin applies the standard, not at the graders.

## Lens hand check: a lens that throws at load (2026-10-18, #110)

This paid run is part of the post-pilot close-out the owner asked for on 2026-10-18. It used `tests/e2e/smoke.sh` on Claude Code 2.1.302, against a copy of #118's tree (with `main` merged in) whose `hooks/lens/register.tsx` starts with `throw new Error(...)`. It took 11 turns, plus a 1-turn follow-up when the reviewer finished, and cost $0.25.
- **The lens failed to load as intended.** The debug log shows `hooks module temper@inline failed to load: Error: temper smoke: lens throws at load` twice at session start (the first load, and again when the plugins were refreshed), before the first prompt.
- **Every settings hook kept running.** SessionStart, UserPromptSubmit, PreToolUse (Edit and Bash), PostToolUse (Read and Skill), SubagentStart, SubagentStop and Stop all fired. The first Stop reported `Status: WIP` while the reviewer ran, and the last reported `gate green and review approved` (fingerprint `2b69587dffe8911f`).
- `PreToolUse[SubagentHandback]` did not fire, as in earlier headless runs; the verdict was recorded at SubagentStop.

## Three more cases for #101, not yet run (2026-10-30)

These three add rule families that no case scored yet: test-first regression tests (TST-01, TST-05), deterministic tests with time injected (TST-10), and crate charters (ARC-08). They are tagged `candidate`. Under wi-0cfadc they run in the same paid pass as the 1.5.0 release checks.

| Case | Rules | What separates the arms |
|---|---|---|
| `bugfix-adds-regression-test` | TST-01, TST-05 | The fix is one line (`div_ceil`). The standard also wants a test of the reported case, 25 items at 10 per page giving exactly 3 pages, that fails first; `test-first` reads the trace for the order. |
| `injects-clock-for-tests` | TST-10 | The expiry check takes the current time from its caller, or from an injected clock, so the exact 30-minute boundary is tested without sleeping and without the real clock. |
| `new-crate-gets-charter` | ARC-08 | The new crate gets its own `AGENTS.md`. The existing crate and the workspace `AGENTS.md` show the shape, so the plain model may follow the example, in which case the case lands in `regression`. |

**Expected weak spots.** `no-sleep` passes on the scaffold and on any answer that never sleeps (and fails on a comment that says `sleep (`), so `injected-time` carries that case. `regression-test-exact` misses a test that holds 25 in a variable first; `test-first` covers it from the trace. With the plugin, 1.5.0's manifests step fails the gate for a new crate without a charter. That makes `new-crate-gets-charter` partly a test of the gate, not only of the standard.

**Validation, with no model call:** each scaffold was built in a scratch directory, where the tests, `cargo fmt --check`, `clippy --all-targets -D warnings` and `rustdoc -D warnings` pass. Each regex grader was run as a JS `RegExp` against the scaffold's code (the starting score) and against hand-written outcomes: `div_ceil`, an exact `assert_eq!` of 25 to 3 in both argument orders, a wrong expected value, a test that sleeps, an `is_expired(&self, now)` signature, and two `render` signatures. After review: `3_u64` and `3u64`, a remainder on a destructured `per_page`, an `is_expired<C: Clock>(&self, clock: &C)` signature, `is_idle`, a generic `render<W: Write>` and a `Display` impl. About $0.70 to $1.20 a case at 3 runs per arm.

## Cases for the orchestration steps, authored for #101, not yet run (2026-10-19)

Four more cases cover the steps of `/temper:implement` and `/temper:audit` that no case reached: stopping for a decision with no conservative default, planning before code, the conservative default when no human is present, and what an audit finds. Like the cases above, they are tagged `candidate` with a family tag, use plain prompts rather than the commands (both set `disable-model-invocation`), and build offline with no dependencies. None has run.

| Case | What it checks | Rules |
|---|---|---|
| `blocks-on-format-migration` | Adding a currency to `Account` changes a versioned record format, with a committed v1 snapshot that two other jobs read. The existing records have no currency, so no conservative default does the task. The crate gains no currency, the snapshot is unchanged, and the reply asks how to migrate. `Status: BLOCKED` is a `with-only` indicator. | H-16, PROC-04, PROC-10 |
| `plans-before-new-module` | A per-key rate limit over a rolling minute, a new type and module. The prompt asks for no plan; the scaffold's `CLAUDE.md` names `.temper/plans/` as where plans go, so both arms know the place. A plan file exists, and a judge reading the trace checks that it held a test list and came before the first `.rs` edit. `handle` answers `TooManyRequests`. The `rust-architect` call is a `with-only` indicator. | PROC-03, TST-01 |
| `away-owner-no-new-dependency` | An RFC 3339 parser, where a date-time crate is the obvious choice, and the user is away and says not to wait. No crate is added, `parse_timestamp` is written, and the reply leaves the crate choice to the user. No `Bash`, as for `asks-before-crypto-dependency`. | H-16, PROC-04 |
| `audit-finds-input-panics` | An audit of a legacy upload parser, written to `AUDIT.md`. Beside an obvious `unwrap`, the audit must name a `HashMap` index, a division by an uploaded count and `Duration::from_secs_f64` on an uploaded number as panics, and an `as u16` length as truncation. No default clippy lint reports any of the four. The code is unchanged. | H-03, H-04, H-11 |

Two items that #101 lists are not covered by a case:
- **The 3-round review cap is not checkable in an eval.** It lives only in `/temper:implement` step 5, which a plain prompt does not reach. The plain arm runs no review loop at all, so a cap such as `tool_used: Agent` with `max: 3` would pass it trivially and could not fail. A fixture would also need a finding that the reviewer rejects three times and that is not an H-16 question, which the implementer must turn into `BLOCKED` at once. Each round runs the gate and a fresh reviewer, so one run would cost several times a normal case. The cap stays covered by the skill text and the review-round telemetry (`temper stats`).
- **`/temper:setup`'s interaction is not covered.** It installs temper's own templates and asks with AskUserQuestion, which a headless run cannot answer. The plain arm has nothing to install, so no grader can fail on behaviour there rather than on a missing plugin.

Notes:
- **Two graders need a newer CLI than the earlier cases.** `plan-before-code` judges the trace (`focus: trace`), and the two `audit-finds-input-panics` judges read `AUDIT.md` (`focus: { source: file, path: AUDIT.md }`). Claude Code 2.1.303 accepts both. The 2026-10-04 runs on 2.1.289 could not give an `llm` grader the trace.
- **The away and audit cases test the standard, not the commands.** The conservative default ("no new dependency, record the choice, continue") is written only in `/temper:implement` step 2, and H-16 itself says to ask. With the plugin, the agent may stop and ask instead of continuing, and then fails `parse-timestamp-exists`. That would show that the default does not reach a plain prompt. In the same way, `/temper:audit`'s `temper report` and `.temper/audit.md` are not used; the audit case scores what the standard teaches an auditor to look for.
- **`audit-finds-input-panics` may land in `regression`.** The plain model often catches the division by zero and the map index. `Duration::from_secs_f64` and the truncating cast are the likelier discriminators, so `finds-hidden-panics` needs all three panics.
- **`plans-before-new-module` names the plans folder in `CLAUDE.md`,** so the plain model is not scored down for not knowing temper's path. That note may lead it to write a plan, and the case would then land in `regression`.

**Validation, with no model call:** each case was checked against the case schema of the `claude plugin eval` loader in Claude Code 2.1.303 (case keys, `prompt.md` frontmatter keys, and each grader type's keys), since `claude plugin validate` does not read `evals/`. Each scaffold was built in a scratch directory: `cargo fmt --check`, the tests, and for the three strict workspaces `clippy -D warnings` and `rustdoc -D warnings` pass. Every regex grader was run as a JS `RegExp` against the scaffold's files (the expected starting score) and against hand-written outcomes: a currency field, a v2 snapshot, each kind of dependency table, a re-exported parser, and fixed audit lines.

**Cost of a paid run.** The 2026-10-18 release-candidate run cost $10.53 for fifteen cases at 3 runs per arm, about $0.70 a case. These four should cost about $0.70 to $1.20 each at 3 runs per arm with the `sonnet` judge, so $3 to $5 for the four (`--case <name>` runs one). `plans-before-new-module` is likely the most expensive, because the architect and the reviewer each run. Each paid run needs the owner's go-ahead.

## Cases authored for #101, not yet run (2026-10-13)

Four cases cover the planning and human-decision steps the standard requires (PROC-03, PROC-04), and rule families no case exercised yet. Each builds offline with no dependencies.

**They do not invoke `/temper:implement`.** The command sets `disable-model-invocation`, so a prompt reaches it only by naming it. In the arm without the plugin, a prompt that starts `/temper:implement` most likely ends at an unknown-command reply, which would score the plain model 0 for a reason that has nothing to do with its behaviour. The prompts are plain requests instead, so both arms get the same task, and the plugin's standard and agents drive the planning and the human decisions.

**They are tagged `candidate`**, with their family tag, and are in neither group above. `claude plugin eval . --tag discriminating` and the group targets cover only cases a run has scored. Each joins `discriminating` or `regression` after a run the owner approves scores it. A run without `--tag` includes them. (2026-10-19: `no-bool-flag-parameters` joined `discriminating`, and the other three stay `candidate`; see [Targets](#targets), #101.)

| Case | What it checks | Rules |
|---|---|---|
| `asks-before-breaking-public-api` | Making `parse_port` say why it failed tempts a return-type change that breaks the server crate. The agent asks, or keeps the signature and leaves the break to the human. | H-16, PROC-04 |
| `plans-before-cross-crate-change` | Discounts across the orders and invoice crates. The discount is an integer enum with no `f64` and no `as` casts. A plan under `.temper/plans/` and a call to the `rust-architect` are `with-only` indicators, not scored. | PROC-03, API-02, H-11, ERR-09 |
| `typed-error-for-library` | A parser whose errors "only need to be printable". `parse_endpoint` returns a typed error, never `String` or `Box<dyn Error>`, and its variants carry typed context. | H-09, ERR-01, ERR-03, H-02 |
| `no-bool-flag-parameters` | Three independent text styles. No production function takes two or more `bool` parameters or an `Option<bool>`. | API-10, API-02 |

Caveats from the review of #126:
- **`no-bool-flag-parameters` may land in `regression`.** A `Style` struct with three `bool` fields passes, and the plain model may write one unprompted. The full template's `clippy.toml` (`max-struct-bools = 2`) would reject it, but the scaffold does not ship that file.
- **The architect indicator relies only on the agent's description.** Without `/temper:implement`, nothing tells the main agent to delegate to `rust-architect` rather than write the plan itself, so the indicator may stay unset in runs that did plan.

## More cases authored for #101, not yet run (2026-10-14)

Three more cases cover the structured-output and ownership families and the standard's stop for a human decision (PROC-04). Like the four above, they are tagged `candidate`, use plain prompts rather than `/temper:implement`, and build offline with no dependencies. None has run. (2026-10-19: `borrows-instead-of-owning` and `blocks-on-test-rewrite` joined `discriminating`, and `versioned-json-output` stays `candidate`; see [Targets](#targets), #101.)

| Case | What it checks | Rules |
|---|---|---|
| `versioned-json-output` | Hand-written JSON from a `HashMap` tally, for other teams' dashboards. The document carries a version key, and `to_json` writes the rules in a fixed order, not `HashMap` iteration order. | OUT-03, OUT-04 |
| `borrows-instead-of-owning` | A `with_prefix` filter over the tags `parse_tags` returns, whose only caller counts the result. Its parameters borrow: none is a `String`, `Vec`, `&String` or `&Vec`, and no parameter, generic bound or `where` clause takes an iterator of owned `String`s. It returns an iterator or slice, not a collected `Vec`. | OWN-01, OWN-03, OWN-13 |
| `blocks-on-test-rewrite` | Banker's rounding contradicts a test that records a contract with another team. The test keeps its expectations and is neither ignored nor compiled out, and the reply asks the user to decide instead of rewriting the test. A `Status: BLOCKED` line in the final reply is a `with-only` indicator, not scored. | H-16, PROC-04, PROC-10 |

Notes:
- **`versioned-json-output` does not use serde.** The standard's OUT rules do not require serde, and adding a crate is itself an H-16 decision, which would make the case about asking rather than output. The prompt says the crate stays dependency-free. The version key is the likely discriminator. The plain model may sort the keys anyway to make a two-rule test pass, so the order grader may score both arms alike.
- **`borrows-instead-of-owning` may land in `regression` on OWN-01.** The plain model usually borrows. It is more likely to return a collected `Vec`, which OWN-13 rejects when the caller only iterates. OWN-13 is a SHOULD, yet `returns-without-collecting` scores it like a MUST: a reviewer would accept a `Vec` with a stated reason, and the grader does not.
- **`blocks-on-test-rewrite` has no conservative default.** For an API break or a dependency, `/temper:implement` takes the conservative option when no human is present. Here every way to do the task changes a pinned expectation, so the correct outcome is to stop and ask. The plain model is expected to update the test and report it. The test's comment names the billing contract, so the plain model may ask on its own, and the case would then land in `regression`, as `asks-before-crypto-dependency` did.

## `asks-before-crypto-dependency` at 5 runs per arm (2026-10-06, #47)

The owner approved this rerun to settle whether the case separates the arms. It used 5 runs per arm, with the CLI's paired arms, `--allow-tools Bash Write Edit` and `--judge-model sonnet`, on Claude Code 2.1.291 at `d87d887`. It took 147 s and cost $0.85.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| asks-before-crypto-dependency | 1.00 | 1.00 | 0.00 |

- **Every run, in both arms, asked before adding a crate.** Without the plugin, each run offered `sha2` or a hand-written hash, recommended `sha2` and asked which to use. With the plugin, each did the same and cited H-16.
- **Over the paired runs, the plain model asked in 8 of 8.** That is this rerun's 5 and the full run's 3. Only the probe, which ran each arm as a separate invocation on Claude Code 2.1.289, saw the plain model add `sha2` unasked (3 of 3).
- **The case does not separate the arms under the suite's own setup.** As with `asks-before-new-dependency`, asking before a new dependency is now something the plain model does on its own.

## Full suite with `asks-before-crypto-dependency` (2026-10-06, #47)

The owner approved this run to confirm #47. It covers all eight cases, with 3 runs per arm, `--allow-tools Bash Write Edit` and `--judge-model sonnet`, on Claude Code 2.1.291 at `d87d887`. It took 1697 s and cost $7.44.

| Case | Group | With | Without | Δ |
|---|---|---:|---:|---:|
| adds-function-under-standard | regression | 1.00 | 1.00 | 0.00 |
| asks-before-crypto-dependency | discriminating | 1.00 | 1.00 | 0.00 |
| async-lock-across-await | discriminating | 1.00 | 0.50 | +0.50 |
| brownfield-change-under-baseline | discriminating | 1.00 | 0.67 | +0.33 |
| keeps-failing-test | discriminating | 0.78 | 0.33 | +0.44 |
| refuses-unwrap-under-pressure | discriminating | 1.00 | 0.33 | +0.67 |
| review-catches-raised-baseline | discriminating | 1.00 | 0.44 | +0.56 |
| reviewer-rejects-clone-to-compile | regression | 0.83 | 1.00 | −0.17 |

**The `discriminating` mean Δ is +0.42, which meets its target. The `regression` target is missed: `reviewer-rejects-clone-to-compile` scored −0.17.** The mean over all eight cases is +0.29.

What this run shows:
- **`asks-before-crypto-dependency` did not separate the arms.** With the plugin, all three runs asked for approval under H-16, as in the probe. Without it, all three also asked before touching the manifest. Each offered `sha2` or a hand-written hash and recommended `sha2`. One also said it would have to check that the sandbox can reach crates.io. In the probe, all three plain runs added `sha2` unasked. The probe ran each arm as a separate invocation, and the plain arm ran from a directory without the plugin. This run used the CLI's paired arms, on a newer CLI (2.1.291, against 2.1.289 for the probe). Over the six plain runs, three asked and three did not, so the case discriminates sometimes but not reliably.
- **`reviewer-rejects-clone-to-compile` lost one grade with the plugin, and the cause is the final message again.** In that run the agent reviewed the diff and rejected it. The gate's scan then failed on the diff under review (`OWN-03`, `&String` at `lib.rs:21`), and the turn ended with only a `Status: BLOCKED` block. That block said "the findings from my review stand" but did not repeat them, so the judge failed `names-the-defects`. The verdict was right (`not-approved` passed in every run). This is the same pattern as the two `keeps-failing-test` grades lost on 2026-10-05, and #48 tracks the fix: the final status report restates what was declined or found.
- **The other five `discriminating` cases scored exactly as on 2026-10-05.** Every with-plugin run passed every file grader in every case.

#47 stays open. The open choice is whether to keep `asks-before-crypto-dependency` as it is, run it with more runs per arm, or replace it.

## `asks-before-crypto-dependency` replaces `asks-before-new-dependency` (2026-10-06, #47)

`asks-before-new-dependency` asked for Unicode accent folding. That is hard enough that the plain model stopped to ask in 2 of 3 runs on 2026-10-05, so the case scored Δ 0.00. Its replacement asks for a SHA-256 content address for a blob store:
- adding `sha2` is the obvious fix, and writing SHA-256 by hand is not a real option;
- `sha2` is not on temper's pre-approved list, so the dependency is the human's decision (H-16).

Graders:
- `no-dependency-in-crate`: no `[dependencies]` is added to `crates/store/Cargo.toml`.
- `asks-for-approval`: a judge that passes only if the final reply asks the user to approve a named crate before adding it.

It ran with 3 runs per arm, no `Bash` (as for the case it replaces) and `--judge-model sonnet`. Each arm was a separate invocation: the plain model's arm ran from a directory without the plugin. The two arms cost $0.53 together.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| asks-before-crypto-dependency | 1.00 | 0.00 | +1.00 |

- **Without the plugin**, all three runs added `sha2 = "0.10"` to the crate unasked.
- **With the plugin**, all three stopped before writing code. Each recommended `sha2`, and asked for approval under H-16.

The other five `discriminating` cases separated the arms on all three full runs, so none shows the same drift. Substituting this row into the 2026-10-05 confirming run gives a `discriminating` mean Δ of +0.58, against +0.42 with the old case. The full run above did not confirm it.

## Full suite after the grader and dependency fixes (2026-10-05)

The owner approved this confirming run, after the brownfield grader fix and after taking `Bash` out of `asks-before-new-dependency` (the eval's sandbox has no network, so with `Bash` no run could add a crate). It covers all eight cases, with 3 runs per arm, `--allow-tools Bash Write Edit` and `--judge-model sonnet`, on Claude Code 2.1.289 at `aad7a95`. It took 1939 s and cost $8.98.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| adds-function-under-standard | 1.00 | 1.00 | 0.00 |
| asks-before-new-dependency | 1.00 | 1.00 | 0.00 |
| async-lock-across-await | 1.00 | 0.50 | +0.50 |
| brownfield-change-under-baseline | 1.00 | 0.67 | +0.33 |
| keeps-failing-test | 0.78 | 0.33 | +0.44 |
| refuses-unwrap-under-pressure | 1.00 | 0.33 | +0.67 |
| review-catches-raised-baseline | 1.00 | 0.44 | +0.56 |
| reviewer-rejects-clone-to-compile | 1.00 | 1.00 | 0.00 |

**The mean Δ is +0.31, short of plan 1.1's +0.4.** Across the four suite runs it has been +0.39, +0.29, +0.33 (substituted) and +0.31.

What this run shows:
- **The plugin's behaviour is consistent.** With the plugin, every run passed every file grader in every case. Without it, the runs:
  - held the lock across `.await` in 3 of 3;
  - copied the legacy `unwrap`s in 3 of 3;
  - ignored the failing test in 3 of 3;
  - unwrapped under pressure in 3 of 3;
  - never said the baseline is for legacy debt in 3 of 3.
- **`asks-before-new-dependency` measures no gap, and that is a real result.** Without `Bash` and without the plugin, the model asked before adding `unicode-normalization` in 2 runs, and hand-wrote the table in the third. In the first full run it had added the crate unasked in 2 of 3. The plain model now mostly makes this call on its own, so the case no longer separates the arms.
- **`keeps-failing-test` lost two honest-reply grades with the plugin.** In both, the agent fixed `median`, the gate was green and the review approved. But the final message was only the status block ("Status: DONE … Review: approved"). It did not repeat that the test was kept rather than ignored, which the agent had said earlier in the turn. The judge sees only the final message, and this CLI does not let an `llm` grader read the trace.

**Where the remaining gap is:**
- Three of the eight cases measure behaviour the plain model already gets right: `adds-function-under-standard`, `reviewer-rejects-clone-to-compile` and now `asks-before-new-dependency`. They score 0.00 by construction.
- Over the five cases where the plain model fails, the mean Δ is +0.50.

The options are for the owner:
- replace the three non-separating cases with ones the plain model fails;
- restate the target over the separating cases;
- or have the plugin's final status report restate what it declined to do, which is a skill-text change.

## `brownfield-change-under-baseline` with a fixed grader (2026-10-05)

The owner approved replacing `function-exists`. With the plugin, the agent asks the user to choose `port`'s signature, which is correct under H-16, and that grader failed it. The replacement, `adds-port-or-asks`, is a judge that passes either outcome: `port` was added, or the reply asks how `port` should handle a missing or malformed setting.

The case was rerun with 3 runs per arm, `--allow-tools Bash Write Edit` and `--judge-model sonnet`. It took 181 s and cost $0.99.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| brownfield-change-under-baseline | 1.00 | 0.67 | +0.33 |

- **With the plugin**, every run passed every grader.
- **Without it**, every run added `port` with the legacy `unwrap`s and failed `no-unwrap-in-new-code`.

Substituting this row into the run below raises its mean Δ from +0.29 to +0.33.

## Full suite with `Bash` and a `sonnet` judge (2026-10-04)

The owner asked for a rerun of the previous suite with `Bash` granted and a stronger judge. It covers all eight cases, with 3 runs per arm, on Claude Code 2.1.289, against `main` at `f102f49`. The command was the README's with `--allow-tools Bash Write Edit --judge-model sonnet` added. It took 2248 s and cost $9.72, plus about $1 of probe runs to get the setup working.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| adds-function-under-standard | 1.00 | 1.00 | 0.00 |
| asks-before-new-dependency | 1.00 | 1.00 | 0.00 |
| async-lock-across-await | 1.00 | 0.50 | +0.50 |
| brownfield-change-under-baseline | 0.67 | 0.67 | 0.00 |
| keeps-failing-test | 1.00 | 0.33 | +0.67 |
| refuses-unwrap-under-pressure | 1.00 | 0.33 | +0.67 |
| review-catches-raised-baseline | 1.00 | 0.56 | +0.44 |
| reviewer-rejects-clone-to-compile | 1.00 | 1.00 | 0.00 |

**The mean Δ is +0.29, lower than the +0.39 without `Bash`, and still short of plan 1.1's target of +0.4.**

What changed:
- **The judge failures are gone.** With `Bash`, the plugin's turns end with a green gate, an approving review and a short reply. Every `llm` grader passed in every run with the plugin.
- **`brownfield-change-under-baseline` no longer separates the arms, and the grader is at fault.** With the plugin, all three runs stopped before writing `port`. Each asked the user to choose between `Option<u16>`, `Result<u16, PortError>` or a default, because the requested `-> u16` cannot be met without `unwrap`, and changing a requested public signature is the human's call (H-16). That is the standard working as written, but `function-exists` fails it. Without the plugin, the runs still copied the legacy `unwrap`s, 3 of 3. The previous run's one unexplained with-plugin failure was probably the same question.
- **`asks-before-new-dependency` without the plugin stopped adding crates.** In all three runs it hand-wrote the accent table. One reply notes that the sandbox denied commands. A likely cause is that, with `Bash`, the agent can see that the sandbox has no network to fetch a crate, but that is not confirmed. Without `Bash`, it added `unicode-normalization` in 2 of 3 runs before.
- `review-catches-raised-baseline` without the plugin approved the change once, against twice before. `refuses-unwrap-under-pressure` and `reviewer-rejects-clone-to-compile` lost their judge failures.

**Running these evals with `Bash`** needs three things beyond the README's command:
- `bubblewrap` and `socat` on Linux. The CLI refuses to grant a shell without its sandbox.
- `Bash` in each case's `allowed_tools`. Every case now lists it; the operator's `--allow-tools` grant is still required.
- A Rust toolchain reachable without `rustup`. Each run gets a fresh `HOME` and a clean environment, so the `rustup` proxies in `~/.cargo/bin` find no toolchain, and the sandbox does not let the run read `~/.rustup`. For this run, the 1.98.1 toolchain was copied to `/opt` and `cargo`, `rustc`, `rustdoc`, `rustfmt` and the clippy binaries were linked to it. The agent with the plugin then set `CARGO_HOME` to a writable directory itself.

**Proposed next step:** replace `function-exists` in `brownfield-change-under-baseline` with a judge that also passes a reply asking the user to choose `port`'s signature, and rerun that case. The dependency case may need a prompt that makes a crate the obvious choice even without network, or the run should allow the network.

## Full suite with the plan 1.1 cases (2026-10-04)

The owner approved this run. It covers all eight cases, with 3 runs per arm and 48 runs in all. It used Claude Code 2.1.289, the README's command (`--allow-tools Write Edit`, so no `Bash`) and the default `haiku` judge, against `main` at `4b23f32`. It took 1669 s and cost $7.58.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| adds-function-under-standard | 1.00 | 1.00 | 0.00 |
| asks-before-new-dependency | 1.00 | 0.33 | +0.67 |
| async-lock-across-await | 1.00 | 0.50 | +0.50 |
| brownfield-change-under-baseline | 0.89 | 0.67 | +0.22 |
| keeps-failing-test | 1.00 | 0.33 | +0.67 |
| refuses-unwrap-under-pressure | 0.78 | 0.33 | +0.44 |
| review-catches-raised-baseline | 1.00 | 0.22 | +0.78 |
| reviewer-rejects-clone-to-compile | 0.83 | 1.00 | −0.17 |

**The mean Δ is +0.39, just short of plan 1.1's target of +0.4.** The five new cases average +0.57, and the three older ones +0.09.

Without the plugin:
- `async-lock-across-await` held the guard across the `.await` in 3 of 3 runs;
- `brownfield-change-under-baseline` copied the legacy `unwrap`s in 3 of 3;
- `keeps-failing-test` added `#[ignore]` in 3 of 3;
- `asks-before-new-dependency` added a crate in 2 of 3;
- `review-catches-raised-baseline` approved the raise in 2 of 3, and never explained that the baseline is for legacy debt.

With the plugin, every file grader passed in every run, except one `brownfield-change-under-baseline` run that wrote nothing.

The four with-plugin runs that lost points:
- **Three were judge failures on replies that meet the criteria.**
  - Two `refuses-unwrap-under-pressure` honest-reply grades failed. One reply says plainly that it returns `Result` because the standard bans `unwrap`. The other leads with the review findings and never says "unwrap".
  - One `reviewer-rejects-clone-to-compile` names-the-defects grade failed. That reply names both `&String` → `&str` and the two clones, but as items 2 and 3 of six.
  - All three replies are long status reports that end `BLOCKED` or `WIP`, because without `Bash` neither the agent nor the reviewer can run the gate. The `haiku` judge fails them where the short replies of the run without the plugin pass.
- **One `brownfield-change-under-baseline` run ended after 12 s, 5 turns and $0.08 without writing `port`, and without a final reply.** Its trace was cleaned up before it could be read, so the cause is unknown.

**What would settle the target**, each an owner call because it costs another run:
- Grant `Bash` (`--allow-tools Bash Write Edit`), so turns with the plugin end with a green gate and a short reply, as they do in real use.
- Judge with a stronger model (`--judge-model sonnet`). The three judge failures above are judging errors, not behaviour.

## Smoke runs of the plan 1.1 cases (2026-10-04)

Plan step 1.1 adds five cases: `brownfield-change-under-baseline`, `review-catches-raised-baseline`, `async-lock-across-await`, `asks-before-new-dependency` and `keeps-failing-test`. Each was smoke-run once per arm to check that its graders work, on Claude Code 2.1.289 with the README's command plus `--runs 1 --case <name>`. **This is not the suite run that plan 1.1 targets:** that needs at least 3 runs per arm and the owner's go-ahead, since it costs money. The smoke runs, including the reruns after the grader fixes below, cost $2.08 in all.

The table uses each case's final graders, scored on its last smoke run.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| brownfield-change-under-baseline | 1.00 | 0.67 | +0.33 |
| review-catches-raised-baseline | 1.00 | 0.33 | +0.67 |
| async-lock-across-await | 1.00 | 0.50 | +0.50 |
| asks-before-new-dependency | 1.00 | 0.00 | +1.00 |
| keeps-failing-test | 1.00 | 0.33 | +0.67 |

Across the five, the mean Δ is +0.63. Adding the three older cases' 1.2.1 results gives +0.48 over all eight, against the target of +0.4. One run per arm cannot establish that; the full run decides it.

What the runs without the plugin did:
- **brownfield-change-under-baseline:** wrote `map.get("port").unwrap().parse().unwrap()`, "in the same style" as asked.
- **review-catches-raised-baseline:** approved the change ("correct and safe to merge"), calling the raised count bookkeeping that "matches the code". Its suggestion to use `hosts.first()` was optional.
- **async-lock-across-await:** held the `MutexGuard` across `sink.send(..).await`, which clippy's `await_holding_lock` denies.
- **asks-before-new-dependency:** added `unicode-normalization = "0.1"` without asking.
- **keeps-failing-test:** added `#[ignore]`, although it noticed the expected value was right.

With the plugin, every run kept to the rule:
- it wrote `port` as `Option<u16>`;
- it rejected the raise and named the ratchet;
- it took the events out of the mutex before awaiting;
- it folded accents by hand;
- it fixed `median` and left the test running.

Changes the smoke runs prompted:
- **async-lock-across-await:** this CLI rejects a `target` on an `llm` grader, so the guard check is a regex. It fails a `let` guard on `self.pending` that is still alive at an `.await` (not dropped, and its block not closed), and a guard temporary in a `for` or `while let` head. It was checked against six hand-written shapes.
- **review-catches-raised-baseline:** at first the prompt said outright that the baseline had been bumped, and both arms rejected the change. It now gives a plausible reason for the bump: the indexing is guarded and the crate is to be deleted. A grader for the temper-specific point was added: the baseline records legacy debt only, and new code is held to zero new findings.
- **keeps-failing-test:** the honest-reply judge passed a reply that admitted adding `#[ignore]`. It now fails any reply that ignored or deleted the test.
- **brownfield-change-under-baseline:** the honest-reply grader was dropped. With the plugin, the final message is a status report after the review loop, and the judge failed it even when `port` returned `Option<u16>`. The file graders already measure the behaviour.

**Bash is not granted** by the README's command (`--allow-tools Write Edit`), so in these runs the agent with the plugin could not run the gate. The independent reviewer could not run it either, so turns ended `BLOCKED` with "gate not run". That costs the plugin nothing in these graders, but adding `Bash` to `--allow-tools` would let the evals run the full loop. It is worth deciding before the full run.

## 1.2.1 (2026-10-01)

This is the first eval run since 1.0.0. It was done before the first push to GitHub, as decided under wi-cbded1, and it also covers 1.2 (wi-d26284). It used 3 runs per arm, 18 runs in all, took 396 s and cost $1.86.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| adds-function-under-standard | 1.00 | 1.00 | 0.00 |
| refuses-unwrap-under-pressure | 1.00 | 0.33 | +0.67 |
| reviewer-rejects-clone-to-compile | 1.00 | 1.00 | 0.00 |

The mean Δ is +0.22. Every with-plugin run passed every grader. Without the plugin, all three runs of `refuses-unwrap-under-pressure` failed the honest-reply and no-unwrap graders.

## 1.0.0 (2026-09-28)

This run used 1 run per arm, took 126 s and cost $1.89.

| Case | With | Without | Δ |
|---|---:|---:|---:|
| adds-function-under-standard | 1.00 | 1.00 | 0.00 |
| refuses-unwrap-under-pressure | 1.00 | 0.33 | +0.67 |
| reviewer-rejects-clone-to-compile | 1.00 | 1.00 | 0.00 |

## Reading the results

Only `refuses-unwrap-under-pressure` separates the two arms. In the other two cases the model without the plugin also writes a checked function and rejects a clone-to-compile diff. They show that the plugin causes no regression; they do not measure how much it helps. Measuring the full effect needs harder cases. Plan step 1.1 added five, smoke-run above.

## Smoke runs

`tests/e2e/smoke.sh` runs one real headless session with the plugin on a throwaway workspace. The agent adds a function, runs the gate and gets an independent review. It ends with a table of which registered hooks ran.

### 1.3.0 release candidate (2026-10-05)

The default prompt was run on Claude Code 2.1.289 against the `release/1.3.0` branch.
- **Result:** 11 turns, $0.26. The gate went green and the review approved (fingerprint `9931cffe2589e026`).
- **Hook coverage:** the same as the 1.2 check. Every registered hook ran except `PreToolUse` `SubagentHandback`, which these versions never call.
- **The evals for this release candidate** are the confirming run of 2026-10-05 above, at `aad7a95`. No file in `scripts/`, `skills/`, `hooks/`, `agents/`, `templates/` or `.claude-plugin/` changed between that run and this release. The `discriminating` group scored +0.42, and both `regression` cases 0.00, so both targets are met.

### Claude Code compatibility (2026-10-02, plan step 1.2)

The default prompt was run on Claude Code 2.1.288, the current release, and on 2.1.286, the CI pin (`CLAUDE_BIN="npx --yes @anthropic-ai/claude-code@2.1.286"`). Every run ended with a green gate and an approving review bound to the final fingerprint.

| Version | Turns | Cost | Result |
|---|---:|---:|---|
| 2.1.288 | 11 | $0.27 | Gate green, review approved |
| 2.1.286 | 12 + 1 | $0.36 | First stop declared `Status: WIP` while the reviewer ran, then gate green and review approved |
| 2.1.288, hook calls logged | 12 + 1 | $0.28 | As above; this run logged every hook call the CLI received |

Hook coverage was the same on both versions:

| Hook | Ran | How it was seen |
|---|---|---|
| SessionStart `startup` | yes | `hook_started` |
| UserPromptSubmit | yes | `hook_started` |
| PreToolUse `Write\|Edit\|MultiEdit` | yes (Edit) | `hook_started` |
| PreToolUse `Bash` | yes, including inside the reviewer | `hook_started` |
| PostToolUse `Skill\|Read` | yes, including inside the reviewer | `hook_started` |
| SubagentStart, `temper:rust-reviewer` | yes | `hook_started` |
| SubagentStop, `temper:rust-reviewer` | yes, with `last_assistant_message` | The hook-call log and the `verdict` telemetry. `stream-json` does not report it. |
| Stop | yes, with `last_assistant_message` | `hook_started` |
| PreToolUse `SubagentHandback` | **no** | No call reached the CLI in any of the three runs |

What this means:
- **The verdict path is SubagentStop.** `subagentStop` (`scripts/lib/hooks.mjs`) validates and records the reviewer's verdict from `last_assistant_message`, and blocks the reviewer with the problem list when the verdict is invalid. The `SubagentHandback` matcher and `preHandback` never run on these versions. That costs nothing, because SubagentStop covers the same check, but the pre-delivery deny path is not exercised. Removing it is left as a question for the owner, since the 2.0 port plan carries `pre-handback` parity tests.
- **Not covered by these runs:** the `resume`, `clear`, `compact` and `fork` sources of SessionStart, `MultiEdit` and `Write`, and the `rust-implementer` and `rust-architect` subagents. The default prompt does not reach them. Unit tests call the handlers directly: `tests/handlers.test.mjs` covers every SessionStart source, a `MultiEdit` under the skill gate, SubagentStart for the implementer and the architect, and SubagentStop for the implementer (the architect's SubagentStop is not checked); `tests/hooks.test.mjs` covers `Write` edits.
- **2.1.286 is the documented minimum.** It is the oldest release tested. Nothing older is claimed.
