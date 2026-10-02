---
id: "kb-decision-wi-6aac03"
title: "OIDC + approval"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D12 · publishing credentials: OIDC trusted publishing to npm and crates.io from a protected `release` environment that requires your approval, or long-lived token secrets?\", facing whoever holds the publishing credential can ship code that runs in every user's hooks, we decided for OIDC + approval and neglected Long-lived tokens, on the premise that trusted publishing is configurable for the org on both registries, accepting that if wrong: first publish of a new crate may still need a token once."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-6aac03"
question: "D12 · publishing credentials: OIDC trusted publishing to npm and crates.io from a protected `release` environment that requires your approval, or long-lived token secrets?"
door: one-way
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:05:57Z"
tree_hash: "0b0ea4ed6ed8fe369cd131c0d19421b7996a1786"
recommended: "A"
flip_condition: "If trusted publishing is unavailable on either registry, B with an approval-gated environment"
---

# OIDC + approval

## Context and problem statement

D12 · publishing credentials: OIDC trusted publishing to npm and crates.io from a protected `release` environment that requires your approval, or long-lived token secrets?

Whoever holds the publishing credential can ship code that runs in every user's hooks.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-6aac03`. Anchor: `.temper/plans/temper-2-rust-port.md:117`.

## Decision drivers

- Leaked-credential window
- Setup

## Considered options

### A · OIDC + approval (chosen)

Short-lived tokens minted per run, only from a protected release environment you approve

- Holds if trusted publishing is configurable for the org on both registries.
- If wrong: first publish of a new crate may still need a token once.

### B · Long-lived tokens

NPM_TOKEN and CARGO_REGISTRY_TOKEN repository secrets

- Holds if simplicity beats credential risk.
- If wrong: a leaked token lets anyone publish until revoked.

## Evidence

- `.temper/plans/temper-2-rust-port.md:118`: A) npm trusted publishing (OIDC, --provenance) and crates.io trusted publishing (rust-lang/crates-io-auth-action), both only from a protected release environment
- `.temper/plans/temper-2-rust-port.md:119`: B) NPM_TOKEN and CARGO_REGISTRY_TOKEN repository secrets: simpler, but long-lived credentials.

## Decision outcome

Chosen option: **OIDC + approval**, the recommended option.

### Consequences

- Good, because it holds if trusted publishing is configurable for the org on both registries.
- Bad, because if wrong: first publish of a new crate may still need a token once.

### Confirmation

Revisit when: If trusted publishing is unavailable on either registry, B with an approval-gated environment

## Why this might be wrong

crates.io may require a token for a crate's very first publish

## Provenance

- Decided 2026-10-02T04:05:57Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "kind: question"
