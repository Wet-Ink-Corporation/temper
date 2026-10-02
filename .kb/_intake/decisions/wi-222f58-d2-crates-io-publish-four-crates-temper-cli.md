---
id: "kb-decision-wi-222f58"
title: "Publish four crates"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D2 · crates.io: publish four crates (temper-cli with binary `temper`, temper-domain, temper-app, temper-host) and reserve the names now? Alternatives: publish one combined crate, or nothing (users run cargo install --git). `temper` and `temper-core` are taken.\", facing crates.io names are permanent and anyone can register the free ones today, we decided for Publish four crates and neglected Publish nothing; One crate, on the premise that a public crates.io footprint is fine, accepting that if wrong: four crates to maintain semver on; yanking only hides versions."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-222f58"
question: "D2 · crates.io: publish four crates (temper-cli with binary `temper`, temper-domain, temper-app, temper-host) and reserve the names now? Alternatives: publish one combined crate, or nothing (users run cargo install --git). `temper` and `temper-core` are taken."
door: one-way
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:05:57Z"
tree_hash: "0b0ea4ed6ed8fe369cd131c0d19421b7996a1786"
recommended: "A"
flip_condition: "If you want no public registry footprint, B"
---

# Publish four crates

## Context and problem statement

D2 · crates.io: publish four crates (temper-cli with binary `temper`, temper-domain, temper-app, temper-host) and reserve the names now? Alternatives: publish one combined crate, or nothing (users run cargo install --git). `temper` and `temper-core` are taken.

crates.io names are permanent and anyone can register the free ones today.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-222f58`. Anchor: `docs/plans/temper-2-rust-port.md:42`.

## Decision drivers

- User install
- Layer enforcement

## Considered options

### A · Publish four crates (chosen)

temper-cli (bin temper), temper-domain, temper-app, temper-host, lockstep versions

- Holds if a public crates.io footprint is fine.
- If wrong: four crates to maintain semver on; yanking only hides versions.

### B · Publish nothing

cargo install --git with a tag; binaries from GitHub releases

- Holds if you want no registry presence.
- If wrong: cargo-binstall and install-action lose crates.io metadata; slower installs.

### C · One crate

A single temper-cli crate with the layers as modules

- Holds if simplest publishing matters most.
- If wrong: layer boundaries no longer enforced by cargo.

## Evidence

- `docs/plans/temper-2-rust-port.md:43`: temper and temper-core are taken (an unrelated game engine, and Temper-lang).
- `crates.io API`: temper-cli, temper-domain, temper-app, temper-host return 404 (free) on 2026-10-02
- `docs/plans/temper-2-rust-port.md:46`: C) Publish one crate holding all the layers as modules ... loses the cargo tree enforcement of the layers

## Decision outcome

Chosen option: **Publish four crates**, the recommended option.

### Consequences

- Good, because it holds if a public crates.io footprint is fine.
- Bad, because if wrong: four crates to maintain semver on; yanking only hides versions.

### Confirmation

Revisit when: If you want no public registry footprint, B

## Why this might be wrong

Publishing library crates invites outside users of temper-domain's API

## Provenance

- Decided 2026-10-02T04:05:57Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "kind: question"
