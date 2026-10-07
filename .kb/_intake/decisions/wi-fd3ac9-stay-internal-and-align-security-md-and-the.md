---
id: "kb-decision-wi-fd3ac9"
title: "Stay internal, align docs"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Stay internal and align SECURITY.md and the README to that, or plan a public flip (notices, LICENSE-APACHE, fuller SECURITY.md)?\", facing decision 0.7 says internal, but SECURITY.md and marketplace text read as public, we decided for Stay internal, align docs and neglected Plan a public flip; Leave as is, on the premise that no public release is planned this quarter, accepting that if wrong: redo the wording at flip time."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-07"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-fd3ac9"
question: "Stay internal and align SECURITY.md and the README to that, or plan a public flip (notices, LICENSE-APACHE, fuller SECURITY.md)?"
door: one-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-07T05:35:50Z"
tree_hash: "8070945d1487c39512abdb7d2016b04b3d45b01b"
recommended: "A"
flip_condition: "a decision to publish"
---

# Stay internal, align docs

## Context and problem statement

Stay internal and align SECURITY.md and the README to that, or plan a public flip (notices, LICENSE-APACHE, fuller SECURITY.md)?

Decision 0.7 says internal, but SECURITY.md and marketplace text read as public.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-fd3ac9`. Anchor: `SECURITY.md:1`.

## Decision drivers

- Reversible
- Effort

## Considered options

### A · Stay internal, align docs (chosen)

Small docs PR: SECURITY.md and README say internal and name the channel.

- Holds if no public release is planned this quarter.
- If wrong: redo the wording at flip time.

### B · Plan a public flip

Notices for quoted material, LICENSE-APACHE, fuller SECURITY.md, history review.

- Holds if publishing outside the org is wanted.
- If wrong: history with quoted text becomes public.

### C · Leave as is

No change.

- Holds if nobody outside reads them.
- If wrong: docs keep contradicting 0.7.

## Evidence

- `docs/history/decisions.md`: 0.7: Internal for now: Wet Ink org members. Revisit before any public release.
- `SECURITY.md:3`: Please report a vulnerability privately, not in an issue

## Decision outcome

Chosen option: **Stay internal, align docs**, the recommended option.

Decider's note: Internal, align docs

### Consequences

- Good, because it holds if no public release is planned this quarter.
- Bad, because if wrong: redo the wording at flip time.

### Confirmation

Revisit when: a decision to publish

## Why this might be wrong

if a public release is already planned

## Provenance

- Decided 2026-10-07T05:35:50Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
