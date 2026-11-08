---
id: "kb-decision-wi-39f19a"
title: "Close as superseded"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Is #193 (publish the vendored plugin layout to GHCR for assay's pinned gates) closed as superseded, now that assay becomes Review in the same monorepo?\", facing #193 publishes temper's plugin layout to GHCR so assay's gates can pin it by digest; after Move 3 Review and Rust live in one monorepo, we decided for Close as superseded and neglected Keep it open, on the premise that Review stops pinning temper by OCI digest after Move 3, accepting that if wrong: reopen it in redkiln as a packaging task."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-39f19a"
question: "Is #193 (publish the vendored plugin layout to GHCR for assay's pinned gates) closed as superseded, now that assay becomes Review in the same monorepo?"
door: one-way
blast_radius: external
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T07:25:53Z"
tree_hash: "e882f655c1fb952eefe7f94465aea1b429ef7766"
recommended: "A"
flip_condition: "Review keeps pinning temper by OCI digest after Move 3"
---

# Close as superseded

## Context and problem statement

Is #193 (publish the vendored plugin layout to GHCR for assay's pinned gates) closed as superseded, now that assay becomes Review in the same monorepo?

#193 publishes temper's plugin layout to GHCR so assay's gates can pin it by digest; after Move 3 Review and Rust live in one monorepo.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-39f19a`. Anchor: `github:Wet-Ink-Corporation/temper#193`.

## Decision drivers

- Work now

## Considered options

### A · Close as superseded (chosen)

Close #193; Review gets temper from the monorepo.

- Holds if Review stops pinning temper by OCI digest after Move 3.
- If wrong: reopen it in redkiln as a packaging task.

### B · Keep it open

Transfer #193 to redkiln with the other issues.

- Holds if Review keeps an OCI-pinned temper.
- If wrong: a dead issue in redkiln's tracker.

## Evidence

- `github:Wet-Ink-Corporation/temper#193`: publish the vendored plugin layout to GHCR for assay's pinned gates

## Decision outcome

Chosen option: **Close as superseded**, the recommended option.

### Consequences

- Good, because it holds if Review stops pinning temper by OCI digest after Move 3.
- Bad, because if wrong: reopen it in redkiln as a packaging task.

### Confirmation

Revisit when: Review keeps pinning temper by OCI digest after Move 3

## Why this might be wrong

Review may still want a pinned, trusted temper outside the PR's tree

## Provenance

- Decided 2026-11-08T07:25:53Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Is #193 (publish the vendored plugin layout to GHCR for assay's pinned gates) closed as superseded, now that assay becomes Review in the same monorepo?"
