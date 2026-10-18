---
id: "kb-decision-wi-2dd2cf"
title: "P2, after v1"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should the temper lens issues (#108-#113) be P2 and outside v1-live, i.e. after the v1 release?\", facing the v1 go-live checklist counts P0 and P1 v1-live issues, so the lens's label decides whether it gates v1, we decided for P2, after v1 and neglected Band in v1, on the premise that v1 ships on enforcement alone, accepting that if wrong: v1 users wait one minor release for the band."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-11"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-2dd2cf"
question: "Should the temper lens issues (#108-#113) be P2 and outside v1-live, i.e. after the v1 release?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-11T21:33:06Z"
tree_hash: "0679046ed2525710353f1f9b4413d6d8f67a1be6"
recommended: "A"
flip_condition: "you want the band in v1"
---

# P2, after v1

## Context and problem statement

Should the temper lens issues (#108-#113) be P2 and outside v1-live, i.e. after the v1 release?

The v1 go-live checklist counts P0 and P1 v1-live issues, so the lens's label decides whether it gates v1.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-2dd2cf`. Anchor: `CONTRIBUTING.md:13`.

## Decision drivers

- v1 scope

## Considered options

### A · P2, after v1 (chosen)

Lens issues stay P2, outside v1-live.

- Holds if v1 ships on enforcement alone.
- If wrong: v1 users wait one minor release for the band.

### B · Band in v1

#108 and #110 become v1-live P1; the rest stay P2.

- Holds if visibility is a v1 promise.
- If wrong: v1 slips by two PRs and a manual check.

## Evidence

- `CONTRIBUTING.md:18`: P2: everything else that should change ... Fix when convenient.
- `https://github.com/Wet-Ink-Corporation/temper/issues/107`: It is post-v1 work (P2): nothing in it blocks the v1 go-live.

## Decision outcome

Chosen option: **P2, after v1**, the recommended option.

Decider's note: Chosen on the decision card: P2, after v1

### Consequences

- Good, because it holds if v1 ships on enforcement alone.
- Bad, because if wrong: v1 users wait one minor release for the band.

### Confirmation

Revisit when: you want the band in v1

## Why this might be wrong

If pilots say the hidden state is their main pain, B is better

## Provenance

- Decided 2026-10-11T21:33:06Z by human:ryan (user-approved), via cli.
- Raised in session `b650fa60-608c-5624-8b6f-9d5546d278c1`.
- Verbatim: "Should the temper lens issues (#108-#113) be P2 and outside v1-live, i.e. after the v1 release?"
