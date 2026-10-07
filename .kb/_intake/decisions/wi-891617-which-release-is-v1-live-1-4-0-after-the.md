---
id: "kb-decision-wi-891617"
title: "1.4.0 after #16"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Which release is \"v1 live\": 1.4.0 after the\", facing the go-live checklist needs a named v1 release published by the release workflow, we decided for 1.4.0 after #16 and neglected 1.3.0 is v1; 1.3.1 now, 1.4.0 later, on the premise that #16 lands within the pilot window, accepting that if wrong: v1 slips while fixes wait."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-07"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-891617"
question: "Which release is \"v1 live\": 1.4.0 after the"
door: one-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-07T05:35:50Z"
tree_hash: "8070945d1487c39512abdb7d2016b04b3d45b01b"
recommended: "A"
flip_condition: "#16 not landing within about two weeks"
---

# 1.4.0 after #16

## Context and problem statement

Which release is "v1 live": 1.4.0 after the

The go-live checklist needs a named v1 release published by the release workflow.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-891617`. Anchor: `docs/plans/v1-readiness.md:138`.

## Decision drivers

- Paid runs
- Known bypasses in v1

## Considered options

### A · 1.4.0 after #16 (chosen)

Fold #72–#79 and the #16 pilot fixes into 1.4.0; smoke + evals (~$8) on the RC.

- Holds if #16 lands within the pilot window.
- If wrong: v1 slips while fixes wait.

### B · 1.3.0 is v1

Declare the shipped 1.3.0 v1; patch the rest as 1.3.x.

- Holds if the pilot finds nothing that changes hook decisions.
- If wrong: v1 ships with known shell-write bypasses.

### C · 1.3.1 now, 1.4.0 later

Cut 1.3.1 from tonight's PRs, then 1.4.0 after #16.

- Holds if adopters want the message fixes soon.
- If wrong: two release cycles and two eval spends.

## Evidence

- `docs/plans/v1-readiness.md:138`: The release workflow published the release (1.3.0, or 1.4.0 if the pilot fixes warrant it).
- `PR #74`: user-visible hook messages changed; CHANGELOG Unreleased has an Upgrading from 1.3.x note

## Decision outcome

Chosen option: **1.4.0 after #16**, the recommended option.

Decider's note: 1.4.0 after #16

### Consequences

- Good, because it holds if #16 lands within the pilot window.
- Bad, because if wrong: v1 slips while fixes wait.

### Confirmation

Revisit when: #16 not landing within about two weeks

## Why this might be wrong

if adopters need tonight's message fixes before #16

## Provenance

- Decided 2026-10-07T05:35:50Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
