---
id: "kb-decision-wi-dc7fe0"
title: "Build on branches, hold merges"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should the next AFK session build the\", facing the next AFK batch is the #16 engine fixes, which change hook decisions while the pilot freeze holds, we decided for Build on branches, hold merges and neglected Wait for pilot evidence; Build and merge now, on the premise that pilot doesn't produce conflicting changes to hooks.mjs, accepting that if wrong: merge conflicts with later pilot fixes."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-dc7fe0"
question: "Should the next AFK session build the"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-08T05:00:08Z"
tree_hash: "7c9af35670d61832500c52b3a192f373fa67bf9d"
recommended: "A"
flip_condition: "pilot findings expected to rewrite hooks.mjs"
---

# Build on branches, hold merges

## Context and problem statement

Should the next AFK session build the

The next AFK batch is the #16 engine fixes, which change hook decisions while the pilot freeze holds.

Raised by an agent (marker) as a call and captured by Weigh-In as `wi-dc7fe0`. Anchor: `docs/plans/v1-readiness.md:117`.

## Decision drivers

- Pilot data
- Time to 1.4.0

## Considered options

### A · Build on branches, hold merges (chosen)

Session builds and tests the fixes, opens PRs marked Hold; you merge when the pilot window ends.

- Holds if pilot doesn't produce conflicting changes to hooks.mjs.
- If wrong: merge conflicts with later pilot fixes.

### B · Wait for pilot evidence

Start #16 only after #15 yields findings.

- Holds if the bypasses aren't urgent.
- If wrong: 1.4.0 slips by the build time.

### C · Build and merge now

Lift the freeze for these fixes.

- Holds if pilot data needn't stay comparable.
- If wrong: pilot sessions see decision changes mid-window.

## Evidence

- `docs/plans/v1-readiness.md:117`: Fix the pilot findings and release them as 1.4.0, the v1 release (wi-891617).
- `issue #59`: engine-1-6: verdicts and gate cache can be forged with a shell write

## Decision outcome

Chosen option: **Build on branches, hold merges**, the recommended option.

Decider's note: Build on branches, hold merges

### Consequences

- Good, because it holds if pilot doesn't produce conflicting changes to hooks.mjs.
- Bad, because if wrong: merge conflicts with later pilot fixes.

### Confirmation

Revisit when: pilot findings expected to rewrite hooks.mjs

## Why this might be wrong

if the pilot surfaces something that reshapes these same checks

## Provenance

- Decided 2026-10-08T05:00:08Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: call"
