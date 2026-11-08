---
id: "kb-decision-wi-02195d"
title: "Retire npm for 2.0"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Does D1 (npm packages @wet-ink/temper-<platform>-<arch>) survive the move, given the suite runs ${CLAUDE_PLUGIN_ROOT}/libexec/<binary>?\", facing d1 ships the engine as six @wet-ink npm packages; suite plugins ship builds in libexec/<target>/ behind the shared launcher, we decided for Retire npm for 2.0 and neglected Keep npm packages, on the premise that the libexec-in-Cowork spike passes, accepting that if wrong: npm route rebuilt from history."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-02195d"
question: "Does D1 (npm packages @wet-ink/temper-<platform>-<arch>) survive the move, given the suite runs ${CLAUDE_PLUGIN_ROOT}/libexec/<binary>?"
door: one-way
blast_radius: external
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T05:52:22Z"
tree_hash: "c5cbac08db38f8afc906a77e411b3ca8fb91691d"
recommended: "A"
flip_condition: "libexec fails in Cowork"
---

# Retire npm for 2.0

## Context and problem statement

Does D1 (npm packages @wet-ink/temper-<platform>-<arch>) survive the move, given the suite runs ${CLAUDE_PLUGIN_ROOT}/libexec/<binary>?

D1 ships the engine as six @wet-ink npm packages; suite plugins ship builds in libexec/<target>/ behind the shared launcher.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-02195d`. Anchor: `docs/plans/temper-2-rust-port.md:17`.

## Decision drivers

- Routes to maintain

## Considered options

### A · Retire npm for 2.0 (chosen)

Ship libexec builds; keep the published alphas, deprecate them at cutover.

- Holds if the libexec-in-Cowork spike passes.
- If wrong: npm route rebuilt from history.

### B · Keep npm packages

Carry D1's packages into rk-rust alongside libexec.

- Holds if some install path cannot use libexec.
- If wrong: two delivery routes to maintain.

## Evidence

- `docs/plans/temper-2-rust-port.md:17`: D1 | npm scope and package names | @wet-ink/temper-<platform>-<arch>
- `redkiln references/docs/rust.md`: Prebuilt binaries in libexec/<target>/ behind the suite launcher ... No npm org

## Decision outcome

Chosen option: **Retire npm for 2.0**, the recommended option.

Decider's note: Ryan chose A on the card, 2026-11-08 05:52Z

### Consequences

- Good, because it holds if the libexec-in-Cowork spike passes.
- Bad, because if wrong: npm route rebuilt from history.

### Confirmation

Revisit when: libexec fails in Cowork

## Why this might be wrong

the Cowork libexec spike is still open

## Provenance

- Decided 2026-11-08T05:52:22Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Does D1 (npm packages @wet-ink/temper-<platform>-<arch>) survive the move, given the suite runs ${CLAUDE_PLUGIN_ROOT}/libexec/<binary>?"
