# Eval results

The behavioural evals in [`evals/`](../../evals/) run each case with the plugin and without it, and grade the outcome. The command is in the README's Development section. Each row below gives the mean score per arm across its runs.

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
