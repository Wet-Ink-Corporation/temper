---
id: "kb-decision-wi-46d668"
title: "Should the Stop gate block, rather than pass with a notice, when cargo is unusable after a toolchain change (engine-1-7, second half): A"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should the Stop gate block, rather than pass with a notice, when cargo is unusable after a toolchain change (engine-1-7, second half)?\", facing the question an agent raised, we decided for A."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-46d668"
question: "Should the Stop gate block, rather than pass with a notice, when cargo is unusable after a toolchain change (engine-1-7, second half)?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-directed
decided_at: "2026-10-08T04:24:26Z"
tree_hash: "7c9af35670d61832500c52b3a192f373fa67bf9d"
---

# Should the Stop gate block, rather than pass with a notice, when cargo is unusable after a toolchain change (engine-1-7, second half): A

## Context and problem statement

Should the Stop gate block, rather than pass with a notice, when cargo is unusable after a toolchain change (engine-1-7, second half)?

Raised by an agent (marker) as a deferral and captured by Weigh-In as `wi-46d668`. Anchor: `scripts/lib/hooks.mjs:561`.

## Decision outcome

Chosen option: **A**.

Decider's note: Keep as is: cargo unusable stays UNAVAILABLE with a notice; the toolchain-write ask covers the direct route.

## Provenance

- Decided 2026-10-08T04:24:26Z by human:ryan (user-directed), via chat.
- Raised in session `653bb21b-0efd-5fa8-921b-5cf0f70e804a`.
- Verbatim: "kind: deferral"
