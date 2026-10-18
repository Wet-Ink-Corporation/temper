---
id: "kb-decision-wi-5fbb56"
title: "Adapt, no rewrite, reword macOS"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Amend D16(i) to adapt tests/release-check.sh into the port's xtask, keep D15 without a line-level rust.yml rewrite, and reword the port plan's macOS parity line?\", facing three port-plan points question settled decisions: release-check.sh under D16(i), a line-level rust.yml rewrite under D15, and macOS parity wording vs parked macOS CI, we decided for Adapt, no rewrite, reword macOS and neglected Leave pending until 0a, on the premise that pilots' rust.yml edits are varied, accepting that if wrong: manual migrate steps for 2 repos."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-5fbb56"
question: "Amend D16(i) to adapt tests/release-check.sh into the port's xtask, keep D15 without a line-level rust.yml rewrite, and reword the port plan's macOS parity line?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-08T05:04:47Z"
tree_hash: "92eff1b8ace909b89a350c8c67fbf1c176a442fe"
recommended: "A"
flip_condition: "pilot rust.yml edits turn out mechanical"
---

# Adapt, no rewrite, reword macOS

## Context and problem statement

Amend D16(i) to adapt tests/release-check.sh into the port's xtask, keep D15 without a line-level rust.yml rewrite, and reword the port plan's macOS parity line?

Three port-plan points question settled decisions: release-check.sh under D16(i), a line-level rust.yml rewrite under D15, and macOS parity wording vs parked macOS CI.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-5fbb56`. Anchor: `docs/plans/temper-2-rust-port.md:716`.

## Decision drivers

- Plan ambiguity

## Considered options

### A · Adapt, no rewrite, reword macOS (chosen)

release-check folds into xtask versions --check keeping its 3 jobs; D15 stays hash-guarded with exact manual lines; plan says macOS on demand via runner-probe.

- Holds if pilots' rust.yml edits are varied.
- If wrong: manual migrate steps for 2 repos.

### B · Leave pending until 0a

Keep all three pending in the plan.

- Holds if pilot data could change them.
- If wrong: same questions at 0a, plan stays ambiguous.

## Evidence

- `docs/plans/temper-2-rust-port.md:716`: Whether the script is adapted ... or retired: pending owner decision.
- `docs/history/decisions.md`: 0.9: macOS CI parked until the repository goes public.

## Decision outcome

Chosen option: **Adapt, no rewrite, reword macOS**, the recommended option.

Decider's note: All three

### Consequences

- Good, because it holds if pilots' rust.yml edits are varied.
- Bad, because if wrong: manual migrate steps for 2 repos.

### Confirmation

Revisit when: pilot rust.yml edits turn out mechanical

## Why this might be wrong

if the pilots show rust.yml rewrites are easy to automate

## Provenance

- Decided 2026-10-08T05:04:47Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
