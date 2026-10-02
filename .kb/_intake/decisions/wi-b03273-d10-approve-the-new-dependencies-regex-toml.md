---
id: "kb-decision-wi-b03273"
title: "Approve the four"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D10 · approve the new dependencies regex, toml (preserve_order), sha2 and wait-timeout, and exclude clap, walkdir, globset, jiff/time and dunce?\", facing rust-craft makes new dependencies a human decision; these four are the only non-pre-approved crates the engine needs, we decided for Approve the four and neglected Four plus clap; Fewer, on the premise that hand-rolling args, walking, globs and time is acceptable, accepting that if wrong: more hand-written code to test in place of clap and friends."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-b03273"
question: "D10 · approve the new dependencies regex, toml (preserve_order), sha2 and wait-timeout, and exclude clap, walkdir, globset, jiff/time and dunce?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:56:34Z"
tree_hash: "091b7182045afdf12f8b45bb325c6d558838eaa0"
recommended: "A"
flip_condition: "If argument parsing grows past the 1.x CLI, add clap"
---

# Approve the four

## Context and problem statement

D10 · approve the new dependencies regex, toml (preserve_order), sha2 and wait-timeout, and exclude clap, walkdir, globset, jiff/time and dunce?

rust-craft makes new dependencies a human decision; these four are the only non-pre-approved crates the engine needs.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-b03273`. Anchor: `docs/plans/temper-2-rust-port.md:100`.

## Decision drivers

- New crates
- Hand-rolled risk

## Considered options

### A · Approve the four (chosen)

regex, toml (preserve_order), sha2, wait-timeout; no clap, walkdir, globset, jiff/time, dunce

- Holds if hand-rolling args, walking, globs and time is acceptable.
- If wrong: more hand-written code to test in place of clap and friends.

### B · Four plus clap

Also take clap for argument parsing

- Holds if you want derived help and errors.
- If wrong: bigger binary and compile time; CLI messages tested byte-for-byte must be reproduced under clap.

### C · Fewer

Drop sha2 or wait-timeout and hand-roll them

- Holds if minimal dependencies matter most.
- If wrong: hand-rolled crypto hashing and process timeouts are risky.

## Evidence

- `docs/plans/temper-2-rust-port.md:213`: temper-domain: regex, toml (std, parse, preserve_order), sha2, serde (derive), serde_json, thiserror
- `docs/plans/temper-2-rust-port.md:215`: temper-host: wait-timeout, thiserror
- `docs/plans/temper-2-rust-port.md:110`: dunce: verbatim-path stripping is ported.
- `README.md:58`: preapprovedDependencies: thiserror, anyhow, serde, serde_json, tracing, ... proptest, insta, ... tempfile

## Decision outcome

Chosen option: **Approve the four**, the recommended option.

### Consequences

- Good, because it holds if hand-rolling args, walking, globs and time is acceptable.
- Bad, because if wrong: more hand-written code to test in place of clap and friends.

### Confirmation

Revisit when: If argument parsing grows past the 1.x CLI, add clap

## Why this might be wrong

Hand-rolled arg parsing must reproduce 1.x usage errors exactly, which clap would not

## Provenance

- Decided 2026-10-02T04:56:34Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "D10 · approve the new dependencies regex, toml (preserve_order), sha2 and wait-timeout, and exclude clap, walkdir, globset, jiff/time and dunce?"
