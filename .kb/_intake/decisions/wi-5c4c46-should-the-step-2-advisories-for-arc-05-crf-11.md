---
id: "kb-decision-wi-5c4c46"
title: "Keep them skipped"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should the step-2 advisories for ARC-05, CRF-11 and ERR-03 be skipped, since wi-3fa7cf rejected option C, which named them?\", facing wi-3fa7cf rejected option C, which named these three advisories, so adding them could contradict your decision, we decided for Keep them skipped and neglected Add the three advisories, on the premise that rejecting C meant rejecting its advisories, accepting that if wrong: three cheap scanner hints are missing; one small PR adds them later."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-19"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-5c4c46"
question: "Should the step-2 advisories for ARC-05, CRF-11 and ERR-03 be skipped, since wi-3fa7cf rejected option C, which named them?"
door: two-way
blast_radius: module
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-19T16:55:40Z"
tree_hash: "99073d96edf861d0dd1cf9fd318ce6e1e7902b78"
recommended: "A"
flip_condition: "you rejected C for reasons unrelated to these three"
---

# Keep them skipped

## Context and problem statement

Should the step-2 advisories for ARC-05, CRF-11 and ERR-03 be skipped, since wi-3fa7cf rejected option C, which named them?

wi-3fa7cf rejected option C, which named these three advisories, so adding them could contradict your decision.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-5c4c46`. Anchor: `docs/history/decisions.md:53`.

## Decision drivers

- Matches wi-3fa7cf as written

## Considered options

### A · Keep them skipped (chosen)

The three stay review-only, as wi-3fa7cf's rejection of C implies

- Holds if rejecting C meant rejecting its advisories.
- If wrong: three cheap scanner hints are missing; one small PR adds them later.

### B · Add the three advisories

Scanner advisories (non-blocking) for ARC-05, CRF-11, ERR-03

- Holds if C was rejected for other reasons than these three.
- If wrong: advisories you didn't want; a revert removes them.

## Evidence

- `docs/history/decisions.md:53`: wi-3fa7cf: After the pilot report, how should the standard-1-6 follow-ups be mechanised

## Decision outcome

Chosen option: **Keep them skipped**, the recommended option.

Decider's note: Chosen on the decision card 2026-10-19

### Consequences

- Good, because it holds if rejecting C meant rejecting its advisories.
- Bad, because if wrong: three cheap scanner hints are missing; one small PR adds them later.

### Confirmation

Revisit when: you rejected C for reasons unrelated to these three

## Why this might be wrong

C may have been rejected for its blocking parts only

## Provenance

- Decided 2026-10-19T16:55:40Z by human:ryan (user-approved), via cli.
- Raised in session `1b39645b-6b96-5cb8-a65b-033e802edde0`.
- Verbatim: "kind: call"
