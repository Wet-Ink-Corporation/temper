---
id: "kb-decision-wi-ddfdfe"
title: "Tick and close"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"May an unattended session tick the fixed boxes in audit issues\", facing about 55 audit checklist items are fixed on main but unticked, and wi-754287 authorised agent tracker edits only for named issues, we decided for Tick and close and neglected Tick only; Report only, on the premise that you are fine with an agent closing owner-labelled issues, accepting that if wrong: reopen the issue; nothing is lost."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-10"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-ddfdfe"
question: "May an unattended session tick the fixed boxes in audit issues"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-10T08:02:06Z"
tree_hash: "eb1d2fcb0ccd6bd59330fb3ab58adb8de9a1df70"
recommended: "A"
flip_condition: "you want to close owner-labelled issues yourself"
---

# Tick and close

## Context and problem statement

May an unattended session tick the fixed boxes in audit issues

About 55 audit checklist items are fixed on main but unticked, and wi-754287 authorised agent tracker edits only for named issues.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-ddfdfe`. Anchor: `docs/history/2026-10-audit/issues.md:1`.

## Decision drivers

- Your time
- Control

## Considered options

### A · Tick and close (chosen)

Agent ticks fixed items with PR citations and closes fully-ticked issues (#57, #65, #69; #59, #60, #68 after follow-ups).

- Holds if you are fine with an agent closing owner-labelled issues.
- If wrong: reopen the issue; nothing is lost.

### B · Tick only

Agent ticks with citations and comments 'ready to close'; you close.

- Holds if you want to sign off each closure.
- If wrong: a few minutes of your clicking.

### C · Report only

Agent posts nothing; leaves a list for you.

- Holds if you want no agent writes to GitHub.
- If wrong: the backlog stays stale until you do it.

## Evidence

- `docs/history/2026-10-audit/issues.md:3`: The other 131 are actionable and are tracked as follows.
- `docs/history/decisions.md`: wi-754287: the agent does the tracker edits (named issues)

## Decision outcome

Chosen option: **Tick and close**, the recommended option.

Decider's note: Tick and close

### Consequences

- Good, because it holds if you are fine with an agent closing owner-labelled issues.
- Bad, because if wrong: reopen the issue; nothing is lost.

### Confirmation

Revisit when: you want to close owner-labelled issues yourself

## Why this might be wrong

An item ticked as fixed may be only partly fixed; the citations let you check.

## Provenance

- Decided 2026-10-10T08:02:06Z by human:ryan (user-approved), via chat.
- Raised in session `5ce92570-ff20-5019-a993-a571d4cc77c1`.
- Verbatim: "kind: question"
