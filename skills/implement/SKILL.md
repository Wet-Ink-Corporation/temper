---
name: implement
description: Drive a Rust change end to end under the rust-craft standard. Plans with the architect, resolves human decisions up front, implements test-first with the implementer, gates, and loops with an independent reviewer until the change is approved.
argument-hint: "<what to build, change or fix>"
disable-model-invocation: true
---

# /temper:implement

Task: $ARGUMENTS

You are the orchestrator for this change. Keep your own context lean. Subagents do the exploration, implementation and review, and you hold the plan, the decisions and the loop.

## 0. Load the standard
Invoke the Skill tool with `temper:rust-craft` now, unless it is already loaded in this session. Rust edits in this context are refused until it is.

## 1. Frame (you)
Restate the task as goal, constraints and non-goals in 3 to 6 lines. If the task is ambiguous in a way that changes the design, ask the human now with AskUserQuestion (at most 3 questions, with a recommended option first). Don't ask about anything the standard already decides.

## 2. Plan
- **Trivial change** (one function, no public API change, under about 30 lines): write a 5-line plan into `.temper/plans/<slug>.md` yourself, including the test list.
- **Otherwise**: delegate to the `temper:rust-architect` subagent with the framed goal. It returns a plan path and its open HITL decisions.

Present every H-16 decision (dependency, API break, exception, crate boundary, migration, test rewrite) to the human with AskUserQuestion before any code is written. Record the answers in the plan under "Decisions". If the human is not present, take the conservative option (no new dependency beyond `preapprovedDependencies` in `.claude/temper.json`, no API break), record that choice, and continue. A test the plan needs but a missing dependency would block is reported under "Decisions needed", never silently dropped.

## 3. Implement
Delegate to the `temper:rust-implementer` subagent with the plan path, the recorded decisions and a clear definition of done ("all items in the plan's test list pass and the gate is green"). For large plans, split the work into slices of a few test-list items each, and run the implementer once per slice in order.

When it returns `BLOCKED`, bring its question to the human, record the answer in the plan, and resume it. When it returns `WIP`, resume it with the remaining items.

## 4. Gate (you)
Run `node "${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs" gate`. If it is RED, send the failures back to the implementer. Do not fix code yourself unless the fix is a one-liner.

## 5. Independent review loop
Delegate to the `temper:rust-reviewer` subagent with only the goal and the plan path. Do not summarize the implementation for it, because the reviewer must form its own view. Then:
- **approved**: go to step 6.
- **changes-requested**: send the findings verbatim to the implementer (step 3), gate (step 4), and review again. The reviewer gets a fresh context each round.
- After 3 rounds without approval, stop and bring the disagreement to the human, with each side's evidence.

A finding you believe is wrong still gets a response. Have the implementer rebut it with evidence in its report, and let the next review round judge the rebuttal. Never drop a finding silently.

## 6. Report (you)
- What changed (crates and files), in 3 to 8 lines.
- The decisions made, and who made them (human, or conservative default).
- Follow-ups the implementer or reviewer suggested but left out of scope.
- The Status block from the standard, §5.

The Stop hook verifies the gate and the approving verdict for the current tree. If it blocks you, do what it says. It is checking the same thing this workflow promised.
