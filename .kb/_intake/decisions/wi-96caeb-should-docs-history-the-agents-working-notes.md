---
id: "kb-decision-wi-96caeb"
title: "Keep all of docs/history"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should docs/history/ (the agents' working notes for 1.1 and 1.2, including a few verbatim sentences quoted from the source books) stay in the repository pushed to GitHub?\", facing the notes include a few verbatim sentences from an all-rights-reserved book, while sources.md promises 'ideas only, paraphrased' for it, and they will go public with the repo, we decided for Keep all of docs/history and neglected Keep all but the findings; Keep only decisions and evals, on the premise that the repo stays private, or the short quotes and internal notes may be public, accepting that if wrong: quoted passages and agent notes become public; removal needs a history rewrite."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-01"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-96caeb"
question: "Should docs/history/ (the agents' working notes for 1.1 and 1.2, including a few verbatim sentences quoted from the source books) stay in the repository pushed to GitHub?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-directed
decided_at: "2026-10-01T19:21:58Z"
recommended: "B"
flip_condition: "the repo stays private for good (then A)"
---

# Keep all of docs/history

## Context and problem statement

Should docs/history/ (the agents' working notes for 1.1 and 1.2, including a few verbatim sentences quoted from the source books) stay in the repository pushed to GitHub?

The notes include a few verbatim sentences from an all-rights-reserved book, while sources.md promises 'ideas only, paraphrased' for it, and they will go public with the repo.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-96caeb`. Anchor: `docs/history/README.md:1`.

## Decision drivers

- Quoted book text in the repo
- Evidence behind rules
- Change now

## Considered options

### A · Keep all of docs/history (chosen)

As committed: the full evidence trail ships with the code.

- Holds if the repo stays private, or the short quotes and internal notes may be public.
- If wrong: quoted passages and agent notes become public; removal needs a history rewrite.

### B · Keep all but the findings

Drop findings/*.md (2,494 lines, where the quotes are); keep specs, reviews, decisions, evals. sources.md stays the per-source summary.

- Holds if the quotes are the only concern.
- If wrong: loses the per-claim verification notes behind 1.1's rules.

### C · Keep only decisions and evals

Drop the specs, reviews and findings; keep decisions.md, evals.md and .kb.

- Holds if working notes don't belong in a product repo.
- If wrong: the 'why' behind 1.1 and 1.2 lives nowhere shared.

## Evidence

- `skills/rust-craft/references/sources.md:14`: Rust Atomics and Locks ... All rights reserved; free to read online | Ideas only, paraphrased
- `docs/history/1.1-resource-review/findings/atomics-and-locks.md:37`: a quoted sentence from the book: "use the dedicated fetch-and-modify methods rather than a compare-and-exchange loop"
- `docs/history/README.md:5`: These files were written for the agents doing the work, and they are kept as written
- `git ls-remote (anonymous)`: Wet-Ink-Corporation/temper asks for credentials: the repo is private now

## Decision outcome

Chosen option: **Keep all of docs/history**, overriding the recommendation (B).

Decider's note: Keep all of it

### Consequences

- Good, because it holds if the repo stays private, or the short quotes and internal notes may be public.
- Bad, because if wrong: quoted passages and agent notes become public; removal needs a history rewrite.

### Confirmation

Revisit when: the repo stays private for good (then A)

## Why this might be wrong

If the repo never goes public, B throws away useful notes for no gain.

## Provenance

- Decided 2026-10-01T19:21:58Z by human:ryan (user-directed), via chat.
- Raised in session `a1b8d21a-0b93-5c31-8a7f-242ae801112f`.
- Verbatim: "Should docs/history/ (the agents' working notes for 1.1 and 1.2, including a few verbatim sentences quoted from the source books) stay in the repository pushed to GitHub?"
