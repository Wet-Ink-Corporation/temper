---
id: "kb-decision-wi-0924e6"
title: "Contracts only"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Are the #172/#173 check subcommands implemented in 1.x Node now (a 1.7.0 release), or are only the JSON contracts written now?\", facing review's gates want temper's check subcommands (#172/#173); the JSON contracts are already in docs/contracts/ (#212), we decided for Contracts only and neglected Build in 1.x as 1.7.0, on the premise that Review can wait for Redkiln Rust 2.0, accepting that if wrong: Review keeps its own workaround longer."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-0924e6"
question: "Are the #172/#173 check subcommands implemented in 1.x Node now (a 1.7.0 release), or are only the JSON contracts written now?"
door: one-way
blast_radius: external
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T07:25:53Z"
tree_hash: "e882f655c1fb952eefe7f94465aea1b429ef7766"
recommended: "A"
flip_condition: "Review must switch before the port lands and cannot wait for it"
---

# Contracts only

## Context and problem statement

Are the #172/#173 check subcommands implemented in 1.x Node now (a 1.7.0 release), or are only the JSON contracts written now?

Review's gates want temper's check subcommands (#172/#173); the JSON contracts are already in docs/contracts/ (#212).

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-0924e6`. Anchor: `github:Wet-Ink-Corporation/temper#172`.

## Decision drivers

- Work now
- Parity burden

## Considered options

### A · Contracts only (chosen)

Keep the written contracts; the Rust engine implements them in redkiln.

- Holds if Review can wait for Redkiln Rust 2.0.
- If wrong: Review keeps its own workaround longer.

### B · Build in 1.x as 1.7.0

Implement in Node now and release 1.7.0.

- Holds if Review must switch before the port lands.
- If wrong: two implementations to keep in parity.

## Evidence

- `docs/contracts/README.md`: the check subcommands' JSON
- `AGENTS.md`: The 2.0 port is paused after Phase 3 (wi-14cf95)

## Decision outcome

Chosen option: **Contracts only**, the recommended option.

### Consequences

- Good, because it holds if Review can wait for Redkiln Rust 2.0.
- Bad, because if wrong: Review keeps its own workaround longer.

### Confirmation

Revisit when: Review must switch before the port lands and cannot wait for it

## Why this might be wrong

Move 3 may land well before rk-rust 2.0

## Provenance

- Decided 2026-11-08T07:25:53Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Are the #172/#173 check subcommands implemented in 1.x Node now (a 1.7.0 release), or are only the JSON contracts written now?"
