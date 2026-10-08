---
id: "kb-decision-wi-754287"
title: "Agent does it now"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Close\", facing tracker items are stale: #47 done by PR #53, #7 probe boxes done, #6 body out of date, #19 and #23 need a call, we decided for Agent does it now and neglected You do it, on the premise that you're fine with an agent editing issue bodies, accepting that if wrong: revert an issue edit."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-754287"
question: "Close"
door: two-way
blast_radius: file
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-08T05:04:47Z"
tree_hash: "92eff1b8ace909b89a350c8c67fbf1c176a442fe"
recommended: "A"
flip_condition: "wanting tracker edits only from people"
---

# Agent does it now

## Context and problem statement

Close

Tracker items are stale: #47 done by PR #53, #7 probe boxes done, #6 body out of date, #19 and #23 need a call.

Raised by an agent (marker) as a deferral and captured by Weigh-In as `wi-754287`. Anchor: `docs/plans/v1-readiness.md:155`.

## Decision drivers

- Effort for you

## Considered options

### A · Agent does it now (chosen)

Close #47, tick #7 boxes 1-2, refresh #6 body, keep #19 open for the dispatch job, tick #23 arm64.

- Holds if you're fine with an agent editing issue bodies.
- If wrong: revert an issue edit.

### B · You do it

Leave the list in the #6 report for you.

- Holds if you want to own tracker edits.
- If wrong: they stay stale.

## Evidence

- `docs/plans/v1-readiness.md:102`: #47 ... done 2026-10-06
- `docs/plans/v1-readiness.md:94`: Probe results (2026-10-02): all five labels run

## Decision outcome

Chosen option: **Agent does it now**, the recommended option.

Decider's note: Agent does the tracker edits now

### Consequences

- Good, because it holds if you're fine with an agent editing issue bodies.
- Bad, because if wrong: revert an issue edit.

### Confirmation

Revisit when: wanting tracker edits only from people

## Why this might be wrong

if you track by who ticked what

## Provenance

- Decided 2026-10-08T05:04:47Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: deferral"
