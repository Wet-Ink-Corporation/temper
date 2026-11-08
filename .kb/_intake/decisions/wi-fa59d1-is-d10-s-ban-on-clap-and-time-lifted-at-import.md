---
id: "kb-decision-wi-fa59d1"
title: "Take the suite's set"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Is D10's ban on clap and time lifted at import so the temper binary matches its siblings?\", facing d10 allows only regex, toml, sha2 and wait-timeout and bans clap and time; the suite's binaries use clap and time, and Core brings tokio hidden behind a sync API, we decided for Take the suite's set and neglected Keep D10, on the premise that Rust writes to the shared log, accepting that if wrong: Rust stays outside the log."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-fa59d1"
question: "Is D10's ban on clap and time lifted at import so the temper binary matches its siblings?"
door: one-way
blast_radius: module
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T05:52:37Z"
tree_hash: "c5cbac08db38f8afc906a77e411b3ca8fb91691d"
recommended: "A"
flip_condition: "the suite drops the shared log for Rust"
---

# Take the suite's set

## Context and problem statement

Is D10's ban on clap and time lifted at import so the temper binary matches its siblings?

D10 allows only regex, toml, sha2 and wait-timeout and bans clap and time; the suite's binaries use clap and time, and Core brings tokio hidden behind a sync API.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-fa59d1`. Anchor: `docs/plans/temper-2-rust-port.md:26`.

## Decision drivers

- In the shared log

## Considered options

### A · Take the suite's set (chosen)

Allow redkiln-core and its deps, clap and time; the toml 1.1/0.9 and sha2 0.11/0.10 splits are settled in the import PR.

- Holds if Rust writes to the shared log.
- If wrong: Rust stays outside the log.

### B · Keep D10

No Core link; Rust keeps its own state files.

- Holds if Rust does not need the shared log.
- If wrong: the one plugin outside the log.

## Evidence

- `docs/plans/temper-2-rust-port.md:26`: D10 | Approve regex, toml, sha2, wait-timeout. Do not add clap, walkdir, globset, jiff/time or dunce.
- `redkiln references/docs/rust.md call C`: amend D10 ... so the port links Core (tokio hidden behind a synchronous API)

## Decision outcome

Chosen option: **Take the suite's set**, the recommended option.

Decider's note: Ryan chose A on the card, 2026-11-08 05:52Z

### Consequences

- Good, because it holds if Rust writes to the shared log.
- Bad, because if wrong: Rust stays outside the log.

### Confirmation

Revisit when: the suite drops the shared log for Rust

## Why this might be wrong

tokio in the tree, even hidden, adds build time and audit surface

## Provenance

- Decided 2026-11-08T05:52:37Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Is D10's ban on clap and time lifted at import so the temper binary matches its siblings?"
