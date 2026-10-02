---
id: "kb-decision-wi-4679b3"
title: "MIT OR Apache-2.0"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"License the new crates MIT (matching plugin.json and LICENSE) rather than the template's MIT OR Apache-2.0?\", facing a published crate version's licence can't be changed after the fact, we decided for MIT OR Apache-2.0 and neglected MIT, on the premise that you want the library crates usable wherever Rust's own crates are, accepting that if wrong: two licence files and a split between plugin and crates."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-4679b3"
question: "License the new crates MIT (matching plugin.json and LICENSE) rather than the template's MIT OR Apache-2.0?"
door: one-way
blast_radius: external
decider: "human:ryan"
provenance: user-directed
decided_at: "2026-10-02T04:36:06Z"
tree_hash: "925ac261f5ed075cdc39044cc51fb53f62f097c3"
recommended: "A"
flip_condition: "If you expect others to depend on temper-domain as a library, B"
---

# MIT OR Apache-2.0

## Context and problem statement

License the new crates MIT (matching plugin.json and LICENSE) rather than the template's MIT OR Apache-2.0?

A published crate version's licence can't be changed after the fact.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-4679b3`. Anchor: `docs/plans/temper-2-rust-port.md:1`.

## Decision drivers

- Files
- Ecosystem convention

## Considered options

### A · MIT

Crates use license = "MIT", matching LICENSE and plugin.json

- Holds if one licence across the repo matters most.
- If wrong: Rust projects needing Apache-2.0's patent grant may avoid the library crates.

### B · MIT OR Apache-2.0 (chosen)

Rust-ecosystem convention; add LICENSE-APACHE; plugin stays MIT

- Holds if you want the library crates usable wherever Rust's own crates are.
- If wrong: two licence files and a split between plugin and crates.

## Evidence

- `LICENSE:1`: MIT License
- `.claude-plugin/plugin.json:9`: "license": "MIT"
- `templates/workspace/Cargo.toml`: the workspace template uses license = "MIT OR Apache-2.0"

## Decision outcome

Chosen option: **MIT OR Apache-2.0**, overriding the recommendation (A).

### Consequences

- Good, because it holds if you want the library crates usable wherever Rust's own crates are.
- Bad, because if wrong: two licence files and a split between plugin and crates.

### Confirmation

Revisit when: If you expect others to depend on temper-domain as a library, B

## Why this might be wrong

Publishing four library crates makes outside use more likely

## Provenance

- Decided 2026-10-02T04:36:06Z by human:ryan (user-directed), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "License the new crates MIT (matching plugin.json and LICENSE) rather than the template's MIT OR Apache-2.0?"
