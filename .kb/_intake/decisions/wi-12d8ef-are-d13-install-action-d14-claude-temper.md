---
id: "kb-decision-wi-12d8ef"
title: "Supersede with libexec"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Are D13 (install-action), D14 (.claude/temper-version) and D17 (binary on PATH) superseded by the suite's libexec launcher and install story?\", facing d13, D14 and D17 assume a separately installed temper binary; suite plugins run libexec/<binary>, we decided for Supersede with libexec and neglected Keep all three, on the premise that the libexec spike passes in Cowork, accepting that if wrong: a fallback install route is added later."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-12d8ef"
question: "Are D13 (install-action), D14 (.claude/temper-version) and D17 (binary on PATH) superseded by the suite's libexec launcher and install story?"
door: one-way
blast_radius: external
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T05:19:14Z"
tree_hash: "4ee4ff9aa08e8c3ca9c1a6b84bb5b805a20fa637"
recommended: "A"
flip_condition: "libexec spike fails in Cowork"
---

# Supersede with libexec

## Context and problem statement

Are D13 (install-action), D14 (.claude/temper-version) and D17 (binary on PATH) superseded by the suite's libexec launcher and install story?

D13, D14 and D17 assume a separately installed temper binary; suite plugins run libexec/<binary>.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-12d8ef`. Anchor: `docs/plans/temper-2-rust-port.md:29`.

## Decision drivers

- Fits suite rules

## Considered options

### A · Supersede with libexec (chosen)

Drop the three; adopt the suite's launcher and install story; keep TEMPER_BIN only if the lens needs it.

- Holds if the libexec spike passes in Cowork.
- If wrong: a fallback install route is added later.

### B · Keep all three

Carry the install-action, version pin file and PATH export into rk-rust.

- Holds if user CI outside Claude needs a standalone binary.
- If wrong: two install stories to maintain.

## Evidence

- `docs/plans/temper-2-rust-port.md:29`: D13 | How user CI installs temper | taiki-e/install-action
- `briefing temper.md`: hooks.json entries that run ${CLAUDE_PLUGIN_ROOT}/libexec/<binary> hook ...

## Decision outcome

Chosen option: **Supersede with libexec**, the recommended option.

Decider's note: Chosen on the decision card in the project thread, 2026-11-08 05:19Z

### Consequences

- Good, because it holds if the libexec spike passes in Cowork.
- Bad, because if wrong: a fallback install route is added later.

### Confirmation

Revisit when: libexec spike fails in Cowork

## Why this might be wrong

the libexec-in-Cowork spike is still open

## Provenance

- Decided 2026-11-08T05:19:14Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Are D13 (install-action), D14 (.claude/temper-version) and D17 (binary on PATH) superseded by the suite's libexec launcher and install story?"
