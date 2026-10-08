---
id: "kb-decision-wi-6e0cc2"
title: "Parity line"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Now that scripts/lib/shell.mjs is on main, should the 2.0 port plan add a parity line covering its decisions, or list the shell-write guards as a D16-style divergence the port may re-implement?\", facing shell.mjs is now on main, and the 2.0 parity harness holds both engines byte-identical, we decided for Parity line and neglected Listed divergence, on the premise that the parser's decisions are worth keeping exactly, accepting that if wrong: a large port item for marginal gain."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-6e0cc2"
question: "Now that scripts/lib/shell.mjs is on main, should the 2.0 port plan add a parity line covering its decisions, or list the shell-write guards as a D16-style divergence the port may re-implement?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-08T04:24:26Z"
tree_hash: "7c9af35670d61832500c52b3a192f373fa67bf9d"
recommended: "A"
flip_condition: "Port cost judged too high"
---

# Parity line

## Context and problem statement

Now that scripts/lib/shell.mjs is on main, should the 2.0 port plan add a parity line covering its decisions, or list the shell-write guards as a D16-style divergence the port may re-implement?

shell.mjs is now on main, and the 2.0 parity harness holds both engines byte-identical.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-6e0cc2`. Anchor: `docs/plans/temper-2-rust-port.md:1`.

## Decision drivers

- Port cost
- Bypass safety

## Considered options

### A · Parity line (chosen)

Port must reproduce shell.mjs decisions; add a parity line.

- Holds if the parser's decisions are worth keeping exactly.
- If wrong: a large port item for marginal gain.

### B · Listed divergence

List shell-write guards as a D16-style divergence the port may re-implement.

- Holds if 2.0 should use a cleaner Rust tokenizer.
- If wrong: 2.0 may regress bypasses 1.4 closed.

## Evidence

- `scripts/lib/shell.mjs:1`: shared shell parser, about 1,000 lines, used by three Bash rules

## Decision outcome

Chosen option: **Parity line**, the recommended option.

Decider's note: Parity line: the 2.0 port must reproduce shell.mjs decisions.

### Consequences

- Good, because it holds if the parser's decisions are worth keeping exactly.
- Bad, because if wrong: a large port item for marginal gain.

### Confirmation

Revisit when: Port cost judged too high

## Why this might be wrong

A purpose-built Rust tokenizer could be cleaner than a faithful port

## Provenance

- Decided 2026-10-08T04:24:26Z by human:ryan (user-approved), via chat.
- Raised in session `653bb21b-0efd-5fa8-921b-5cf0f70e804a`.
- Verbatim: "Now that scripts/lib/shell.mjs is on main, should the 2.0 port plan add a parity line covering its decisions, or list the shell-write guards as a D16-style divergence the port may re-implement?"
