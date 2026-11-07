---
id: "kb-decision-wi-91c5c1"
title: "Phase 3 OC-2 (3-5): is a pid's liveness read from /proc where it exists, else from one ps -A -o pid= or tasklist per hook call, with a probe that cannot answer reading every pid as alive (new D16(u)), rather than adding sysinfo or never logging lost gates: A (recommended)"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Phase 3 OC-2 (3-5): is a pid's liveness read from /proc where it exists, else from one ps -A -o pid= or tasklist per hook call, with a probe that cannot answer reading every pid as alive (new D16(u)), rather than adding sysinfo or never logging lost gates?\", facing the question an agent raised, we decided for A (recommended)."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-07"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-91c5c1"
question: "Phase 3 OC-2 (3-5): is a pid's liveness read from /proc where it exists, else from one ps -A -o pid= or tasklist per hook call, with a probe that cannot answer reading every pid as alive (new D16(u)), rather than adding sysinfo or never logging lost gates?"
door: one-way
decider: "human:claude"
provenance: user-directed
decided_at: "2026-11-07T19:00:16Z"
tree_hash: "0d158bb6cd3df97a3996460328924aaac049b2cb"
---

# Phase 3 OC-2 (3-5): is a pid's liveness read from /proc where it exists, else from one ps -A -o pid= or tasklist per hook call, with a probe that cannot answer reading every pid as alive (new D16(u)), rather than adding sysinfo or never logging lost gates: A (recommended)

## Context and problem statement

Phase 3 OC-2 (3-5): is a pid's liveness read from /proc where it exists, else from one ps -A -o pid= or tasklist per hook call, with a probe that cannot answer reading every pid as alive (new D16(u)), rather than adding sysinfo or never logging lost gates?

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-91c5c1`. Anchor: `docs/plans/temper-2-rust-port.md:589`.

## Decision outcome

Chosen option: **A (recommended)**.

Decider's note: Decided by Ryan on the decision card 'Accept all six recommended Phase 3 calls' 2026-11-07T18:59Z: option A as in /mnt/project-files/port/phase-3-owner-calls.md.

## Provenance

- Decided 2026-11-07T19:00:16Z by human:claude (user-directed), via cli.
- Raised in session `24a0b143-5a79-55d6-9f31-d6bd30ba7048`.
- Verbatim: "Phase 3 OC-2 (3-5): is a pid's liveness read from /proc where it exists, else from one ps -A -o pid= or tasklist per hook call, with a probe that cannot answer reading every pid as alive (new D16(u)), rather than adding sysinfo or never logging lost gates?"
