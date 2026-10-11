---
id: "kb-decision-wi-78d987"
title: "Add tsc via npx to the CI lens step"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should CI type-check the lens with tsc, which adds a TypeScript (npx) dependency to the CI lens step?\", facing a strict tsc pass caught real type errors in the lens three times this session, but CI cannot run it without adding TypeScript, we decided for Add tsc via npx to the CI lens step and neglected Keep tsc out of CI, on the premise that you accept a pinned typescript fetched by npx in CI, accepting that if wrong: one more pinned version to bump by hand."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-11"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-78d987"
question: "Should CI type-check the lens with tsc, which adds a TypeScript (npx) dependency to the CI lens step?"
door: one-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-11T18:49:38Z"
tree_hash: "3fd8c4079e18ed2c9f64697c94b3e3fc0e7690d5"
recommended: "A"
flip_condition: "you want CI free of anything npm beyond the pinned Claude CLI"
---

# Add tsc via npx to the CI lens step

## Context and problem statement

Should CI type-check the lens with tsc, which adds a TypeScript (npx) dependency to the CI lens step?

A strict tsc pass caught real type errors in the lens three times this session, but CI cannot run it without adding TypeScript.

Raised by an agent (sweep) as a deferral and captured by Weigh-In as `wi-78d987`. Anchor: `tests/lens-check.sh:1`.

## Decision drivers

- new dependency
- catches type errors in CI

## Considered options

### A · Add tsc via npx to the CI lens step (chosen)

pin a typescript version with npx, check hooks/lens, types and tests under the engine's strict tsconfig

- Holds if you accept a pinned typescript fetched by npx in CI.
- If wrong: one more pinned version to bump by hand.

### B · Keep tsc out of CI

type-check by hand when the lens changes

- Holds if lens changes stay rare and reviewed.
- If wrong: a type error lands that only shows at load in the terminal or desktop.

## Evidence

- `AGENTS.md:63`: A one-way or high-impact call waits for the owner. Such calls include ... a new dependency
- `tests/lens-check.sh:1`: runs the allow-list and claude plugin test; no tsc step

## Decision outcome

Chosen option: **Add tsc via npx to the CI lens step**, the recommended option.

Decider's note: Ryan: A

### Consequences

- Good, because it holds if you accept a pinned typescript fetched by npx in CI.
- Bad, because if wrong: one more pinned version to bump by hand.

### Confirmation

Revisit when: you want CI free of anything npm beyond the pinned Claude CLI

## Why this might be wrong

claude plugin validate may already refuse the errors that matter at load

## Provenance

- Decided 2026-10-11T18:49:38Z by human:ryan (user-approved), via chat.
- Raised in session `0248a1ca-8aa4-51ee-bac9-77ac19608f46`.
- Verbatim: "Should CI type-check the lens with tsc, which adds a TypeScript (npx) dependency to the CI lens step?"
