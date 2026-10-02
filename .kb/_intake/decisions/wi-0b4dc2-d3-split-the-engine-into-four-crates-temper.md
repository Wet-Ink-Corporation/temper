---
id: "kb-decision-wi-0b4dc2"
title: "Four crates + xtask"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D3 · split the engine into four crates (temper-domain, temper-app, temper-host, temper-cli) plus an unpublished xtask?\", facing with four crates publishing (D2), the split becomes four public crates whose boundaries cargo enforces, we decided for Four crates + xtask and neglected Two crates; One crate, on the premise that cargo-enforced layering is worth four manifests, accepting that if wrong: four crates to version and publish in lockstep."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-0b4dc2"
question: "D3 · split the engine into four crates (temper-domain, temper-app, temper-host, temper-cli) plus an unpublished xtask?"
door: one-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:56:34Z"
tree_hash: "091b7182045afdf12f8b45bb325c6d558838eaa0"
recommended: "A"
flip_condition: "If four lockstep crates feel heavy, B and revisit D2"
---

# Four crates + xtask

## Context and problem statement

D3 · split the engine into four crates (temper-domain, temper-app, temper-host, temper-cli) plus an unpublished xtask?

With four crates publishing (D2), the split becomes four public crates whose boundaries cargo enforces.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-0b4dc2`. Anchor: `.temper/plans/temper-2-rust-port.md:48`.

## Decision drivers

- Layering enforced by cargo
- Fits D2 (publish four)

## Considered options

### A · Four crates + xtask (chosen)

domain (pure), app (use cases, ports), host (std adapters), cli (binary)

- Holds if cargo-enforced layering is worth four manifests.
- If wrong: four crates to version and publish in lockstep.

### B · Two crates

temper-domain and temper-cli; app and host are modules of the binary

- Holds if only the pure/impure line needs enforcing.
- If wrong: app code can call std directly; D2 then publishes 2, not 4.

### C · One crate

Everything in temper-cli as modules

- Holds if least ceremony matters most.
- If wrong: no layer enforcement; contradicts D2's four-crate publish.

## Evidence

- `.temper/plans/temper-2-rust-port.md:49`: A) Four crates. temper-domain is pure (no I/O). temper-app holds the use cases and the ports. temper-host holds the std adapters.
- `.temper/plans/temper-2-rust-port.md:50`: B) Two crates: temper-domain and temper-cli, with app and host as modules of the binary. Fewer manifests, but the app layer could reach std directly.
- `.temper/plans/temper-2-rust-port.md:51`: C) One crate: the least ceremony and the weakest enforcement.
- `.kb/_intake/decisions/wi-222f58-d2-crates-io-publish-four-crates-temper-cli.md`: D2 decided: publish temper-cli, temper-domain, temper-app, temper-host

## Decision outcome

Chosen option: **Four crates + xtask**, the recommended option.

### Consequences

- Good, because it holds if cargo-enforced layering is worth four manifests.
- Bad, because if wrong: four crates to version and publish in lockstep.

### Confirmation

Revisit when: If four lockstep crates feel heavy, B and revisit D2

## Why this might be wrong

temper-app and temper-host may have no use outside temper-cli

## Provenance

- Decided 2026-10-02T04:56:34Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "D3 · split the engine into four crates (temper-domain, temper-app, temper-host, temper-cli) plus an unpublished xtask?"
