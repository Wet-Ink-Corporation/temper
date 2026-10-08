---
id: "kb-decision-wi-7c3212"
title: "Create P0/P1/P2"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Create P0/P1/P2 labels so go-live item 4 (\"no P0/P1 open\") means something?\", facing go-live item 4 says no P0/P1 open, but no priority labels exist, we decided for Create P0/P1/P2 and neglected Severity in issue text, on the premise that someone applies them, accepting that if wrong: unused labels."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-7c3212"
question: "Create P0/P1/P2 labels so go-live item 4 (\"no P0/P1 open\") means something?"
door: two-way
blast_radius: module
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-08T05:00:08Z"
tree_hash: "7c9af35670d61832500c52b3a192f373fa67bf9d"
recommended: "A"
flip_condition: "preferring issue types over labels"
---

# Create P0/P1/P2

## Context and problem statement

Create P0/P1/P2 labels so go-live item 4 ("no P0/P1 open") means something?

Go-live item 4 says no P0/P1 open, but no priority labels exist.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-7c3212`. Anchor: `docs/plans/v1-readiness.md:135`.

## Decision drivers

- Item 4 checkable

## Considered options

### A · Create P0/P1/P2 (chosen)

Labels plus one-line definitions in CONTRIBUTING; triage pilot issues with them.

- Holds if someone applies them.
- If wrong: unused labels.

### B · Severity in issue text

No labels; judge go-live item 4 by reading issues.

- Holds if few issues.
- If wrong: item 4 is unverifiable.

## Evidence

- `docs/plans/v1-readiness.md:135`: No P0 or P1 issue is open among the v1-live issues

## Decision outcome

Chosen option: **Create P0/P1/P2**, the recommended option.

Decider's note: Create P0/P1/P2

### Consequences

- Good, because it holds if someone applies them.
- Bad, because if wrong: unused labels.

### Confirmation

Revisit when: preferring issue types over labels

## Why this might be wrong

if nobody labels pilot issues

## Provenance

- Decided 2026-10-08T05:00:08Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
