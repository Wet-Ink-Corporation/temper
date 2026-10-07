---
id: "kb-decision-wi-2d6d35"
title: "Add both required checks"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Add the required checks Checks and Unit tests (Windows) to the Protect main ruleset, as AGENTS.md says they already are?\", facing AGENTS.md says Checks and Unit tests (Windows) are required on main; the audit's ruleset read found no required-status-checks rule, we decided for Add both required checks and neglected Correct AGENTS.md, on the premise that job names stay Checks and Unit tests (Windows), accepting that if wrong: a renamed job blocks merges until updated."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-07"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-2d6d35"
question: "Add the required checks Checks and Unit tests (Windows) to the Protect main ruleset, as AGENTS.md says they already are?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-07T05:35:50Z"
tree_hash: "8070945d1487c39512abdb7d2016b04b3d45b01b"
recommended: "A"
flip_condition: "a fresh read showing the rule already exists"
---

# Add both required checks

## Context and problem statement

Add the required checks Checks and Unit tests (Windows) to the Protect main ruleset, as AGENTS.md says they already are?

AGENTS.md says Checks and Unit tests (Windows) are required on main; the audit's ruleset read found no required-status-checks rule.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-2d6d35`. Anchor: `docs/plans/v1-readiness.md:86`.

## Decision drivers

- Go-live item 1

## Considered options

### A · Add both required checks (chosen)

Edit the Protect main ruleset (about 5 minutes).

- Holds if job names stay Checks and Unit tests (Windows).
- If wrong: a renamed job blocks merges until updated.

### B · Correct AGENTS.md

Say checks are not required.

- Holds if you prefer merging without CI gates.
- If wrong: go-live item 1 stays unmet.

## Evidence

- `AGENTS.md:6`: A ruleset protects main: the Checks and Unit tests (Windows) checks are required
- `docs/history/2026-10-audit/evidence-github.md:54`: rules deletion, non_fast_forward, pull_request ... No required_status_checks rule.

## Decision outcome

Chosen option: **Add both required checks**, the recommended option.

Decider's note: Add both required checks

### Consequences

- Good, because it holds if job names stay Checks and Unit tests (Windows).
- Bad, because if wrong: a renamed job blocks merges until updated.

### Confirmation

Revisit when: a fresh read showing the rule already exists

## Why this might be wrong

if the audit's read was of a stale ruleset

## Provenance

- Decided 2026-10-07T05:35:50Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
