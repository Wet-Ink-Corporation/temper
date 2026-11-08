---
id: "kb-decision-wi-b1bfb5"
title: "Retire for new adoptions"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Is the temper-telemetry branch retired in favour of SPEC journals (stop recommending it for new adoptions now, keep the code until cutover)?\", facing new adoptions are still pointed at the temper-telemetry branch, while redkiln's SPEC journals carry the rust.* events after the move, we decided for Retire for new adoptions and neglected Keep recommending it, on the premise that no pilot needs new telemetry before SPEC journals ship, accepting that if wrong: a pilot runs without telemetry until journals land."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-b1bfb5"
question: "Is the temper-telemetry branch retired in favour of SPEC journals (stop recommending it for new adoptions now, keep the code until cutover)?"
door: one-way
blast_radius: module
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T07:25:47Z"
tree_hash: "e882f655c1fb952eefe7f94465aea1b429ef7766"
recommended: "A"
flip_condition: "SPEC journals are not ready before the next pilot needs telemetry"
---

# Retire for new adoptions

## Context and problem statement

Is the temper-telemetry branch retired in favour of SPEC journals (stop recommending it for new adoptions now, keep the code until cutover)?

New adoptions are still pointed at the temper-telemetry branch, while redkiln's SPEC journals carry the rust.* events after the move.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-b1bfb5`. Anchor: `docs/plans/v1-readiness.md:116`.

## Decision drivers

- Migration work later

## Considered options

### A · Retire for new adoptions (chosen)

Stop recommending the branch now; keep the code until cutover.

- Holds if no pilot needs new telemetry before SPEC journals ship.
- If wrong: a pilot runs without telemetry until journals land.

### B · Keep recommending it

Branch stays the advice until SPEC journals ship.

- Holds if another pilot starts before redkiln Core 1.0.
- If wrong: more adoptions to migrate at the move.

## Evidence

- `docs/plans/v1-readiness.md:116`: 2.3 Collect findings: temper stats telemetry
- `docs/contracts/rust-events.md`: 25 rust.* names mapped to SPEC journal events

## Decision outcome

Chosen option: **Retire for new adoptions**, the recommended option.

### Consequences

- Good, because it holds if no pilot needs new telemetry before SPEC journals ship.
- Bad, because if wrong: a pilot runs without telemetry until journals land.

### Confirmation

Revisit when: SPEC journals are not ready before the next pilot needs telemetry

## Why this might be wrong

a pilot could start before Core 1.0

## Provenance

- Decided 2026-11-08T07:25:47Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Is the temper-telemetry branch retired in favour of SPEC journals (stop recommending it for new adoptions now, keep the code until cutover)?"
