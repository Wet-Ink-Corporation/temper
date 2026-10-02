---
id: "kb-decision-wi-37c1df"
title: "install-action"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D13 · have user CI install temper via taiki-e/install-action (cargo-binstall from the GitHub release), rather than cargo install or curl plus sha256?\", facing every user repo's CI must install a matching temper binary before running the gate, we decided for install-action and neglected cargo install; curl + sha256, on the premise that users' CI can reach GitHub release assets, accepting that if wrong: falls back to compiling, about a minute."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-37c1df"
question: "D13 · have user CI install temper via taiki-e/install-action (cargo-binstall from the GitHub release), rather than cargo install or curl plus sha256?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T05:09:17Z"
tree_hash: "091b7182045afdf12f8b45bb325c6d558838eaa0"
recommended: "A"
flip_condition: "If users' CI policy forbids third-party actions, B"
---

# install-action

## Context and problem statement

D13 · have user CI install temper via taiki-e/install-action (cargo-binstall from the GitHub release), rather than cargo install or curl plus sha256?

Every user repo's CI must install a matching temper binary before running the gate.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-37c1df`. Anchor: `docs/plans/temper-2-rust-port.md:121`.

## Decision drivers

- Cold install time
- Third-party action

## Considered options

### A · install-action (chosen)

taiki-e/install-action fetches the prebuilt release binary (binstall), SHA-pinned

- Holds if users' CI can reach GitHub release assets.
- If wrong: falls back to compiling, about a minute.

### B · cargo install

Compile from crates.io with --locked, cached

- Holds if you want no third-party action in users' CI.
- If wrong: about a minute per cold run.

### C · curl + sha256

Download the release archive and verify the checksum

- Holds if you want no action and no compile.
- If wrong: 8 lines of YAML per job to maintain.

## Evidence

- `docs/plans/temper-2-rust-port.md:122`: A) taiki-e/install-action with tool: temper-cli@${{ steps.temper.outputs.version }}, SHA-pinned. Recommended. Through its cargo-binstall fallback ...
- `docs/plans/temper-2-rust-port.md:123`: B) cargo install temper-cli --locked --version X: about a minute per cold run, cached by Swatinem/rust-cache.
- `docs/plans/temper-2-rust-port.md:124`: C) curl the release asset plus sha256sum -c: no crates.io needed, but 8 lines of YAML per job.

## Decision outcome

Chosen option: **install-action**, the recommended option.

### Consequences

- Good, because it holds if users' CI can reach GitHub release assets.
- Bad, because if wrong: falls back to compiling, about a minute.

### Confirmation

Revisit when: If users' CI policy forbids third-party actions, B

## Why this might be wrong

A user whose CI blocks GitHub downloads pays the compile anyway

## Provenance

- Decided 2026-10-02T05:09:17Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "D13 · have user CI install temper via taiki-e/install-action (cargo-binstall from the GitHub release), rather than cargo install or curl plus sha256?"
