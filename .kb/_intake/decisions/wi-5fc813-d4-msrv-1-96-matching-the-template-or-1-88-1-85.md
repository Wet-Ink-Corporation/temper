---
id: "kb-decision-wi-5fc813"
title: "1.96"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D4 · MSRV 1.96 matching the template, or 1.88/1.85 for a wider cargo-fallback reach?\", facing the MSRV is the oldest compiler the cargo fallback can build with on a user's machine, we decided for 1.96 and neglected 1.88; 1.85, on the premise that fallback users run a recent stable toolchain, accepting that if wrong: a fallback user on an older toolchain gets a build error and must update rustup."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-5fc813"
question: "D4 · MSRV 1.96 matching the template, or 1.88/1.85 for a wider cargo-fallback reach?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:56:34Z"
tree_hash: "091b7182045afdf12f8b45bb325c6d558838eaa0"
recommended: "A"
flip_condition: "If fallback users report old toolchains, lower it in a minor release"
---

# 1.96

## Context and problem statement

D4 · MSRV 1.96 matching the template, or 1.88/1.85 for a wider cargo-fallback reach?

The MSRV is the oldest compiler the cargo fallback can build with on a user's machine.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-5fc813`. Anchor: `docs/plans/temper-2-rust-port.md:54`.

## Decision drivers

- Who hits a build error
- Matches the template

## Considered options

### A · 1.96 (chosen)

Same as the workspace template; full use of the standard's idioms

- Holds if fallback users run a recent stable toolchain.
- If wrong: a fallback user on an older toolchain gets a build error and must update rustup.

### B · 1.88

Wider fallback reach; tests use matches! instead of assert_matches!

- Holds if you expect fallback users on older toolchains.
- If wrong: the port diverges from the template's MSRV.

### C · 1.85

Edition-2024 floor; no let-chains

- Holds if maximum reach matters most.
- If wrong: more workarounds than the standard recommends.

## Evidence

- `docs/plans/temper-2-rust-port.md:54`: The fallback build compiles on the user's default toolchain, so the MSRV is the oldest compiler a fallback user can have.
- `docs/plans/temper-2-rust-port.md:55`: A) 1.96, as in templates/workspace/Cargo.toml. Consistent with the standard (assert_matches! in tests, ...)
- `docs/plans/temper-2-rust-port.md:56`: B) 1.88: let-chains, a wider fallback reach, and no assert_matches! (tests use matches!).

## Decision outcome

Chosen option: **1.96**, the recommended option.

### Consequences

- Good, because it holds if fallback users run a recent stable toolchain.
- Bad, because if wrong: a fallback user on an older toolchain gets a build error and must update rustup.

### Confirmation

Revisit when: If fallback users report old toolchains, lower it in a minor release

## Why this might be wrong

The fallback exists for unusual setups, which skew older

## Provenance

- Decided 2026-10-02T04:56:34Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "D4 · MSRV 1.96 matching the template, or 1.88/1.85 for a wider cargo-fallback reach?"
