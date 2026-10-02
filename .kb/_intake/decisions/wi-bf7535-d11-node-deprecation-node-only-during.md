---
id: "kb-decision-wi-bf7535"
title: "Rust only in 2.0"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D11 · Node deprecation: Node only during development as an opt-in delegate, with 2.0.0 shipping Rust only; or keep Node behind TEMPER_ENGINE=node until 2.1?\", facing shipping two engines in 2.0 gives users an escape hatch but doubles what must stay correct, we decided for Rust only in 2.0 and neglected Node escape hatch to 2.1, on the premise that the parity harness gives enough confidence before release, accepting that if wrong: a 2.0 regression has no in-release fallback; users pin 1.2.3."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-bf7535"
question: "D11 · Node deprecation: Node only during development as an opt-in delegate, with 2.0.0 shipping Rust only; or keep Node behind TEMPER_ENGINE=node until 2.1?"
door: one-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:36:06Z"
tree_hash: "925ac261f5ed075cdc39044cc51fb53f62f097c3"
recommended: "A"
flip_condition: "If parity testing leaves known gaps at release time, B"
---

# Rust only in 2.0

## Context and problem statement

D11 · Node deprecation: Node only during development as an opt-in delegate, with 2.0.0 shipping Rust only; or keep Node behind TEMPER_ENGINE=node until 2.1?

Shipping two engines in 2.0 gives users an escape hatch but doubles what must stay correct.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-bf7535`. Anchor: `.temper/plans/temper-2-rust-port.md:112`.

## Decision drivers

- Engines in 2.0
- Fallback for users

## Considered options

### A · Rust only in 2.0 (chosen)

Node delegates per subcommand only on main during the port; 2.0.0 deletes it except the launcher

- Holds if the parity harness gives enough confidence before release.
- If wrong: a 2.0 regression has no in-release fallback; users pin 1.2.3.

### B · Node escape hatch to 2.1

2.0 ships both engines; TEMPER_ENGINE=node falls back; deleted in 2.1

- Holds if you expect 2.0 regressions users must route around.
- If wrong: two engines to keep in parity through 2.0.x.

## Evidence

- `.temper/plans/temper-2-rust-port.md:113`: A) Strangler on main during development only ... delegates per subcommand to Rust when TEMPER_ENGINE=rust
- `.temper/plans/temper-2-rust-port.md:114`: B) Ship 2.0 with the Node engine kept behind TEMPER_ENGINE=node and delete it in 2.1.
- `.temper/plans/temper-2-rust-port.md:135`: a 1.x repository keeps working untouched: its vendored copy is self-contained

## Decision outcome

Chosen option: **Rust only in 2.0**, the recommended option.

### Consequences

- Good, because it holds if the parity harness gives enough confidence before release.
- Bad, because if wrong: a 2.0 regression has no in-release fallback; users pin 1.2.3.

### Confirmation

Revisit when: If parity testing leaves known gaps at release time, B

## Why this might be wrong

Real-world repos may hit paths the parity harness never exercised

## Provenance

- Decided 2026-10-02T04:36:06Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "kind: question"
