---
id: "kb-decision-wi-187aa7"
title: "Read-tree import of main"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Where does the 2.0 port's code land in redkiln, and how is it imported: the history-preserving read-tree import of temper main (crates as they stand after Phase 3), or a restart of the port's Rust inside the monorepo as the redkiln migration plan says?\", facing the briefing flags that the migration plan starts the port's Rust in the monorepo, but Phases 0-3 were built in temper's own workspace, we decided for Read-tree import of main and neglected Restart port in redkiln, on the premise that Core 1.0 does not reshape the crates beyond a seam move, accepting that if wrong: some crates are rewritten after import; history still resolves."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-187aa7"
question: "Where does the 2.0 port's code land in redkiln, and how is it imported: the history-preserving read-tree import of temper main (crates as they stand after Phase 3), or a restart of the port's Rust inside the monorepo as the redkiln migration plan says?"
door: one-way
blast_radius: system
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T05:18:18Z"
tree_hash: "950c85e56f6f7a17131ba7caf35448983500f3e3"
recommended: "A"
flip_condition: "Core 1.0 forces a different engine shape"
---

# Read-tree import of main

## Context and problem statement

Where does the 2.0 port's code land in redkiln, and how is it imported: the history-preserving read-tree import of temper main (crates as they stand after Phase 3), or a restart of the port's Rust inside the monorepo as the redkiln migration plan says?

The briefing flags that the migration plan starts the port's Rust in the monorepo, but Phases 0-3 were built in temper's own workspace.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-187aa7`. Anchor: `docs/plans/temper-2-rust-port.md:1`.

## Decision drivers

- Rework
- History

## Considered options

### A · Read-tree import of main (chosen)

Import temper main with history; port crates land in crates/ as they are after Phase 3.

- Holds if Core 1.0 does not reshape the crates beyond a seam move.
- If wrong: some crates are rewritten after import; history still resolves.

### B · Restart port in redkiln

Import only the 1.x plugin; rebuild the Rust in the monorepo, using temper's crates as reference.

- Holds if Core 1.0 makes the current layout throwaway.
- If wrong: Phases 0-3 (about 6 weeks of reviewed work) are redone.

## Evidence

- `briefing temper.md`: Decide with Ryan where it lands and how it gets imported, before the crate layout hardens.
- `AGENTS.md`: Cargo.toml, crates/, xtask/: the 2.0 port's Cargo workspace

## Decision outcome

Chosen option: **Read-tree import of main**, the recommended option.

Decider's note: Chosen on the decision card in the project thread, 2026-11-08 05:18Z

### Consequences

- Good, because it holds if Core 1.0 does not reshape the crates beyond a seam move.
- Bad, because if wrong: some crates are rewritten after import; history still resolves.

### Confirmation

Revisit when: Core 1.0 forces a different engine shape

## Why this might be wrong

if redkiln's layer rules reject temper-app's ports as-is

## Provenance

- Decided 2026-11-08T05:18:18Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Where does the 2.0 port's code land in redkiln, and how is it imported: the history-preserving read-tree import of temper main (crates as they stand after Phase 3), or a restart of the port's Rust inside the monorepo as the redkiln migration plan says?"
