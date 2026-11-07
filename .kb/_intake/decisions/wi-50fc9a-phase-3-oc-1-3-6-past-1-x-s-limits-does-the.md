---
id: "kb-decision-wi-50fc9a"
title: "Phase 3 OC-1 (3-6): past 1.x's limits, does the fingerprint stream the whole diff into the hash and hash only the header of an entry that is not a regular file (new D16(t)), rather than reproducing 1.x's 256 MiB truncation or failing the handler: A (recommended)"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Phase 3 OC-1 (3-6): past 1.x's limits, does the fingerprint stream the whole diff into the hash and hash only the header of an entry that is not a regular file (new D16(t)), rather than reproducing 1.x's 256 MiB truncation or failing the handler?\", facing the question an agent raised, we decided for A (recommended)."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-07"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-50fc9a"
question: "Phase 3 OC-1 (3-6): past 1.x's limits, does the fingerprint stream the whole diff into the hash and hash only the header of an entry that is not a regular file (new D16(t)), rather than reproducing 1.x's 256 MiB truncation or failing the handler?"
door: one-way
decider: "human:claude"
provenance: user-directed
decided_at: "2026-11-07T19:00:16Z"
tree_hash: "0d158bb6cd3df97a3996460328924aaac049b2cb"
---

# Phase 3 OC-1 (3-6): past 1.x's limits, does the fingerprint stream the whole diff into the hash and hash only the header of an entry that is not a regular file (new D16(t)), rather than reproducing 1.x's 256 MiB truncation or failing the handler: A (recommended)

## Context and problem statement

Phase 3 OC-1 (3-6): past 1.x's limits, does the fingerprint stream the whole diff into the hash and hash only the header of an entry that is not a regular file (new D16(t)), rather than reproducing 1.x's 256 MiB truncation or failing the handler?

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-50fc9a`. Anchor: `docs/plans/temper-2-rust-port.md:589`.

## Decision outcome

Chosen option: **A (recommended)**.

Decider's note: Decided by Ryan on the decision card 'Accept all six recommended Phase 3 calls' 2026-11-07T18:59Z: option A as in /mnt/project-files/port/phase-3-owner-calls.md.

## Provenance

- Decided 2026-11-07T19:00:16Z by human:claude (user-directed), via cli.
- Raised in session `24a0b143-5a79-55d6-9f31-d6bd30ba7048`.
- Verbatim: "Phase 3 OC-1 (3-6): past 1.x's limits, does the fingerprint stream the whole diff into the hash and hash only the header of an entry that is not a regular file (new D16(t)), rather than reproducing 1.x's 256 MiB truncation or failing the handler?"
