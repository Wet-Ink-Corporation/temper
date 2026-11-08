---
id: "kb-decision-wi-a2240a"
title: "Adopt 1.98.1 at import"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Does D4 (MSRV 1.96) give way to the suite's 1.98.1 at import?\", facing the port's crates declare MSRV 1.96; redkiln pins MSRV equal to its toolchain, 1.98.1, we decided for Adopt 1.98.1 at import and neglected Keep 1.96, on the premise that rk-rust users run current stable, accepting that if wrong: lower it later, which is easy."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-a2240a"
question: "Does D4 (MSRV 1.96) give way to the suite's 1.98.1 at import?"
door: one-way
blast_radius: system
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T05:52:27Z"
tree_hash: "c5cbac08db38f8afc906a77e411b3ca8fb91691d"
recommended: "A"
flip_condition: "an adopter needs < 1.98.1"
---

# Adopt 1.98.1 at import

## Context and problem statement

Does D4 (MSRV 1.96) give way to the suite's 1.98.1 at import?

The port's crates declare MSRV 1.96; redkiln pins MSRV equal to its toolchain, 1.98.1.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-a2240a`. Anchor: `docs/plans/temper-2-rust-port.md:20`.

## Decision drivers

- Fits the root

## Considered options

### A · Adopt 1.98.1 at import (chosen)

The crates inherit the root's MSRV; the Rust MSRV 1.96 job is dropped.

- Holds if rk-rust users run current stable.
- If wrong: lower it later, which is easy.

### B · Keep 1.96

Ask redkiln for a per-crate rust-version exception.

- Holds if adopters pin older toolchains.
- If wrong: an exception to maintain.

## Evidence

- `docs/plans/temper-2-rust-port.md:20`: D4 | MSRV | rust-version = 1.96
- `redkiln root manifest`: rust-version = 1.98.1 # MSRV equals the toolchain pin

## Decision outcome

Chosen option: **Adopt 1.98.1 at import**, the recommended option.

Decider's note: Ryan chose A on the card, 2026-11-08 05:52Z

### Consequences

- Good, because it holds if rk-rust users run current stable.
- Bad, because if wrong: lower it later, which is easy.

### Confirmation

Revisit when: an adopter needs < 1.98.1

## Why this might be wrong

the binary ships prebuilt, so MSRV mostly matters to the cargo fallback

## Provenance

- Decided 2026-11-08T05:52:27Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Does D4 (MSRV 1.96) give way to the suite's 1.98.1 at import?"
