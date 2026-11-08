---
id: "kb-decision-wi-f4c3ac"
title: "Narrow to one direction"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Is Invariant 3 (both engines read each other's state in both directions) narrowed to 2.0-reads-1.x until the one-time import into the shared log?\", facing invariant 3 makes both engines read each other's state both ways; the suite imports old state once into the shared log, we decided for Narrow to one direction and neglected Keep both directions, on the premise that no repo runs 1.x and 2.0 side by side after cutover, accepting that if wrong: a 1.x session misreads 2.0 state in a mixed repo."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-f4c3ac"
question: "Is Invariant 3 (both engines read each other's state in both directions) narrowed to 2.0-reads-1.x until the one-time import into the shared log?"
door: one-way
blast_radius: system
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T05:19:24Z"
tree_hash: "638a6e753f5abe297a4ce50115278530a383ed9d"
recommended: "A"
flip_condition: "mixed-version adopters exist"
---

# Narrow to one direction

## Context and problem statement

Is Invariant 3 (both engines read each other's state in both directions) narrowed to 2.0-reads-1.x until the one-time import into the shared log?

Invariant 3 makes both engines read each other's state both ways; the suite imports old state once into the shared log.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-f4c3ac`. Anchor: `docs/plans/temper-2-rust-port.md:165`.

## Decision drivers

- Code kept

## Considered options

### A · Narrow to one direction (chosen)

2.0 reads 1.x state until the one-time import; then the Core store is the source of truth.

- Holds if no repo runs 1.x and 2.0 side by side after cutover.
- If wrong: a 1.x session misreads 2.0 state in a mixed repo.

### B · Keep both directions

Keep bidirectional compatibility through 2.x.

- Holds if mixed-version repos must be supported.
- If wrong: extra compat code with no consumer.

## Evidence

- `docs/plans/temper-2-rust-port.md:165`: 3. Every file temper writes under .git/temper/ ... is readable by both engines in both directions.
- `briefing temper.md`: Old state is imported once and never deleted.

## Decision outcome

Chosen option: **Narrow to one direction**, the recommended option.

Decider's note: Chosen on the decision card in the project thread, 2026-11-08 05:19Z

### Consequences

- Good, because it holds if no repo runs 1.x and 2.0 side by side after cutover.
- Bad, because if wrong: a 1.x session misreads 2.0 state in a mixed repo.

### Confirmation

Revisit when: mixed-version adopters exist

## Why this might be wrong

pilot repos may upgrade unevenly

## Provenance

- Decided 2026-11-08T05:19:24Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Is Invariant 3 (both engines read each other's state in both directions) narrowed to 2.0-reads-1.x until the one-time import into the shared log?"
