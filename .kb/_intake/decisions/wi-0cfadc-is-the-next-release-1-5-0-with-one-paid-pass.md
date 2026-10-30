---
id: "kb-decision-wi-0cfadc"
title: "1.5.0 with one paid pass"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Is the next release 1.5.0, with one paid pass approved for the smoke test, the RC evals and the #101 cases?\", facing main has new scanner rules and per-host baselines since 1.4.0, we decided for 1.5.0 with one paid pass and neglected 1.4.1 patch, on the premise that new rules are features, accepting that if wrong: a patch number misleads adopters."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-30"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-0cfadc"
question: "Is the next release 1.5.0, with one paid pass approved for the smoke test, the RC evals and the #101 cases?"
door: one-way
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-30T21:27:43Z"
tree_hash: "824334a35451b00e92b941186ad364410249b08a"
recommended: "A"
flip_condition: "Ryan wants no paid runs now"
---

# 1.5.0 with one paid pass

## Context and problem statement

Is the next release 1.5.0, with one paid pass approved for the smoke test, the RC evals and the #101 cases?

main has new scanner rules and per-host baselines since 1.4.0.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-0cfadc`. Anchor: `CHANGELOG.md:3`.

## Considered options

### A · 1.5.0 with one paid pass (chosen)

Minor release; one paid pass covers smoke, RC evals and the #101 cases (about $11 plus $1 per new case).

- Holds if new rules are features.
- If wrong: a patch number misleads adopters.

### B · 1.4.1 patch

Ship as a patch without the paid checks.

- Holds if the changes are fixes only.
- If wrong: adopters miss that new checks run.

## Evidence

- `CHANGELOG.md:3`: ## Unreleased with new rules

## Decision outcome

Chosen option: **1.5.0 with one paid pass**, the recommended option.

Decider's note: Ryan approved all five plan recommendations on the decision card, 2026-10-30

### Consequences

- Good, because it holds if new rules are features.
- Bad, because if wrong: a patch number misleads adopters.

### Confirmation

Revisit when: Ryan wants no paid runs now

## Why this might be wrong

none known

## Provenance

- Decided 2026-10-30T21:27:43Z by human:ryan (user-approved), via chat.
- Raised in session `24a0b143-5a79-55d6-9f31-d6bd30ba7048`.
- Verbatim: "Is the next release 1.5.0, with one paid pass approved for the smoke test, the RC evals and the #101 cases?"
