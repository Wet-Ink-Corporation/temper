# Eval results

The behavioural evals in [`evals/`](../../evals/) run each case with the plugin and without it, and grade the outcome. The command is in the README's Development section. Each row below gives the mean score per arm across its runs.

## Targets

Each case is tagged with one of two groups, by what it was built to show. Its scores don't decide the group (decided 2026-10-05, #8). On 2026-10-06 the owner moved `asks-before-crypto-dependency` to `regression` (#47):

| Group | Cases | Target, over at least 3 runs per arm |
|---|---|---|
| `regression` | `adds-function-under-standard`, `asks-before-crypto-dependency`, `reviewer-rejects-clone-to-compile` | Δ ≥ 0 in each case: the plugin never makes the outcome worse |
| `discriminating` | `async-lock-across-await`, `brownfield-change-under-baseline`, `keeps-failing-test`, `refuses-unwrap-under-pressure`, `review-catches-raised-baseline` | mean Δ ≥ +0.4 across the group |

The plain model passes the `regression` cases, so their Δ is 0 by design. `asks-before-crypto-dependency` joined them because the plain model asked before adding a crate in 8 of 8 paired runs; it still guards that the plugin asks (H-16). A single average over all eight cases therefore understates the plugin's effect and says nothing about regressions.

How the four full runs score against the groups as they stood at each run, plus the latest run under the current groups:

| Run | `discriminating` mean Δ | `regression` Δ |
|---|---:|---|
| 2026-10-04, no `Bash`, `haiku` judge | +0.55 | 0.00, −0.17 (a `haiku` judging error on a correct reply) |
| 2026-10-04, `Bash`, `sonnet` judge, brownfield row from its fixed-grader rerun | +0.44 | 0.00, 0.00 |
| 2026-10-05, confirming run | +0.42 | 0.00, 0.00 |
| 2026-10-06, with `asks-before-crypto-dependency` | +0.42 | 0.00, −0.17 (a reply whose final message held only the status block) |
| 2026-10-06, the same run under the current groups | +0.50 | 0.00, 0.00, −0.17 (as above) |

`claude plugin eval . --tag discriminating` or `--tag regression` runs one group. In #47, `asks-before-crypto-dependency` replaced `asks-before-new-dependency`, which the plain model had started to pass. The runs before 2026-10-06 used the old case.

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
- **Not covered by these runs:** the `resume`, `clear`, `compact` and `fork` sources of SessionStart, `MultiEdit` and `Write`, and the `rust-implementer` and `rust-architect` subagents. The default prompt does not reach them. The unit tests cover their handlers.
- **2.1.286 is the documented minimum.** It is the oldest release tested. Nothing older is claimed.
