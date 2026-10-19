---
id: "kb-decision-wi-39a8f0"
title: "Context plus minor-only can't overturn"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"How should temper keep reviewer verdicts stable when the same unchanged tree is reviewed again (#136)?\", facing on one unchanged redkiln-rs tree the reviewer went approved, changes-requested, changes-requested, approved, and the last one recorded decides Stop, we decided for Context plus minor-only can't overturn and neglected Context only; Approval sticks; Leave as is, on the premise that real defects always come with a blocker or major finding, accepting that if wrong: a minor-only objection to an approved tree no longer blocks Stop; revert one rule."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-19"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-39a8f0"
question: "How should temper keep reviewer verdicts stable when the same unchanged tree is reviewed again (#136)?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-19T17:50:25Z"
tree_hash: "1985a2ff6d87b63c0a5cb03e87e9789af40bd07b"
recommended: "A"
flip_condition: "minor-only changes-requested verdicts turn out to matter"
---

# Context plus minor-only can't overturn

## Context and problem statement

How should temper keep reviewer verdicts stable when the same unchanged tree is reviewed again (#136)?

On one unchanged redkiln-rs tree the reviewer went approved, changes-requested, changes-requested, approved, and the last one recorded decides Stop.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-39a8f0`. Anchor: `scripts/lib/hooks.mjs:1149`.

## Decision drivers

- Enforcement kept
- Stability

## Considered options

### A · Context plus minor-only can't overturn (chosen)

The reviewer is shown the earlier verdict for the same tree; a changes-requested with no blocker or major finding does not replace an approval of that tree.

- Holds if real defects always come with a blocker or major finding.
- If wrong: a minor-only objection to an approved tree no longer blocks Stop; revert one rule.

### B · Context only

The reviewer sees the earlier verdict; the last verdict still wins.

- Holds if seeing the earlier findings is enough for reviews to converge.
- If wrong: flip-flops continue, only less often.

### C · Approval sticks

Once an unchanged tree is approved, no later verdict overturns it.

- Holds if a second review of the same tree never finds a real blocker.
- If wrong: a real blocker found later is ignored until the tree changes.

### D · Leave as is

Keep last-verdict-wins and gather more data.

- Holds if the redkiln-rs flip-flop was a one-off.
- If wrong: turns keep ending or blocking by luck of the last review.

## Evidence

- `scripts/lib/verdict.mjs:107`: recordVerdict overwrites verdicts/<fp>.json: the last verdict always wins
- `scripts/lib/verdict.mjs:96`: only an approval is checked for blocker/major findings; a changes-requested verdict can carry none

## Decision outcome

Chosen option: **Context plus minor-only can't overturn**, the recommended option.

Decider's note: Chosen on the decision card 2026-10-19

### Consequences

- Good, because it holds if real defects always come with a blocker or major finding.
- Bad, because if wrong: a minor-only objection to an approved tree no longer blocks Stop; revert one rule.

### Confirmation

Revisit when: minor-only changes-requested verdicts turn out to matter

## Why this might be wrong

a reviewer may file a real defect as minor

## Provenance

- Decided 2026-10-19T17:50:25Z by human:ryan (user-approved), via cli.
- Raised in session `1b39645b-6b96-5cb8-a65b-033e802edde0`.
- Verbatim: "How should temper keep reviewer verdicts stable when the same unchanged tree is reviewed again (#136)?"
