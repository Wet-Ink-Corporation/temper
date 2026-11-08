---
id: "kb-decision-wi-0eea27"
title: "Do not publish"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Does D2 (publish temper-cli, -domain, -app, -host to crates.io) still hold in the monorepo?\", facing d2 publishes four temper-* crates; in the monorepo the crates link into one binary and CI fetches it by digest, we decided for Do not publish and neglected Publish as planned, on the premise that no outside consumer needs the crates, accepting that if wrong: publish later; names are still reserved."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-0eea27"
question: "Does D2 (publish temper-cli, -domain, -app, -host to crates.io) still hold in the monorepo?"
door: one-way
blast_radius: external
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T05:52:22Z"
tree_hash: "c5cbac08db38f8afc906a77e411b3ca8fb91691d"
recommended: "A"
flip_condition: "an outside consumer appears"
---

# Do not publish

## Context and problem statement

Does D2 (publish temper-cli, -domain, -app, -host to crates.io) still hold in the monorepo?

D2 publishes four temper-* crates; in the monorepo the crates link into one binary and CI fetches it by digest.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-0eea27`. Anchor: `docs/plans/temper-2-rust-port.md:18`.

## Decision drivers

- Reversible

## Considered options

### A · Do not publish (chosen)

Leave the reserved 0.0.0 names idle; link crates into the binary only.

- Holds if no outside consumer needs the crates.
- If wrong: publish later; names are still reserved.

### B · Publish as planned

Publish the four crates from redkiln, overriding its publish=false rule.

- Holds if outside users want the scanner as a library.
- If wrong: permanent public API to support.

## Evidence

- `docs/plans/temper-2-rust-port.md:18`: D2 | crates.io | Publish 4 crates
- `redkiln AGENTS.md`: no crate is publishable (publish = false; turning it on is H-16)

## Decision outcome

Chosen option: **Do not publish**, the recommended option.

Decider's note: Ryan chose A on the card, 2026-11-08 05:52Z

### Consequences

- Good, because it holds if no outside consumer needs the crates.
- Bad, because if wrong: publish later; names are still reserved.

### Confirmation

Revisit when: an outside consumer appears

## Why this might be wrong

Rust adopters outside the suite might want the scanner as a crate

## Provenance

- Decided 2026-11-08T05:52:22Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Does D2 (publish temper-cli, -domain, -app, -host to crates.io) still hold in the monorepo?"
