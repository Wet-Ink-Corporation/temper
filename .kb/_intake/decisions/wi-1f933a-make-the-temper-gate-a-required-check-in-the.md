---
id: "kb-decision-wi-1f933a"
title: "Require it in both"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Make the temper gate a required check in the weigh-in and redkiln-rs rulesets?\", facing go-live item 3 needs the temper gate required in both pilots; the audit found it required in neither, we decided for Require it in both and neglected Advisory until v1, on the premise that the gates are stable green, accepting that if wrong: a flaky gate blocks pilot merges."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-1f933a"
question: "Make the temper gate a required check in the weigh-in and redkiln-rs rulesets?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-08T05:00:08Z"
tree_hash: "7c9af35670d61832500c52b3a192f373fa67bf9d"
recommended: "A"
flip_condition: "a gate that is often red for reasons unrelated to code"
---

# Require it in both

## Context and problem statement

Make the temper gate a required check in the weigh-in and redkiln-rs rulesets?

Go-live item 3 needs the temper gate required in both pilots; the audit found it required in neither.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-1f933a`. Anchor: `docs/plans/v1-readiness.md:115`.

## Decision drivers

- Go-live item 3

## Considered options

### A · Require it in both (chosen)

gate on weigh-in main; temper on redkiln-rs build branch, then its default branch.

- Holds if the gates are stable green.
- If wrong: a flaky gate blocks pilot merges.

### B · Advisory until v1

Leave as is; require at go-live.

- Holds if pilots shouldn't be blocked.
- If wrong: go-live item 3 stays unmet; red gates can land.

## Evidence

- `docs/history/2026-10-audit/evidence-github.md:56`: weigh-in required_status_checks ... gate is not among them
- `docs/history/2026-10-audit/evidence-github.md:57`: redkiln-rs build/phase-0-to-1: []

## Decision outcome

Chosen option: **Require it in both**, the recommended option.

Decider's note: Require in both

### Consequences

- Good, because it holds if the gates are stable green.
- Bad, because if wrong: a flaky gate blocks pilot merges.

### Confirmation

Revisit when: a gate that is often red for reasons unrelated to code

## Why this might be wrong

if pilot devs route around a blocking gate

## Provenance

- Decided 2026-10-08T05:00:08Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
