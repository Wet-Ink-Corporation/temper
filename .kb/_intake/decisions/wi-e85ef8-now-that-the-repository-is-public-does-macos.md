---
id: "kb-decision-wi-e85ef8"
title: "macOS in CI on every PR"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Now that the repository is public, does macOS run in CI on every PR (changing wi-5fbb56 and #24)?\", facing wi-5fbb56 parked macOS until the repository is public, which it now is, we decided for macOS in CI on every PR and neglected Keep on demand, on the premise that minutes are free for public repos, accepting that if wrong: slower PR checks."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-30"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-e85ef8"
question: "Now that the repository is public, does macOS run in CI on every PR (changing wi-5fbb56 and #24)?"
door: two-way
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-30T21:27:43Z"
tree_hash: "824334a35451b00e92b941186ad364410249b08a"
recommended: "A"
flip_condition: "CI time becomes a problem"
---

# macOS in CI on every PR

## Context and problem statement

Now that the repository is public, does macOS run in CI on every PR (changing wi-5fbb56 and #24)?

wi-5fbb56 parked macOS until the repository is public, which it now is.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-e85ef8`. Anchor: `docs/plans/temper-2-rust-port.md:567`.

## Considered options

### A · macOS in CI on every PR (chosen)

Port Rust jobs and graduation run on Linux, Windows and macOS; update #24 and plan section 3.

- Holds if minutes are free for public repos.
- If wrong: slower PR checks.

### B · Keep on demand

macOS via runner-probe only.

- Holds if CI time matters more than coverage.
- If wrong: macOS regressions found late.

## Evidence

- `docs/plans/temper-2-rust-port.md:567`: macOS through runner-probe.yml on demand until the repository is public

## Decision outcome

Chosen option: **macOS in CI on every PR**, the recommended option.

Decider's note: Ryan approved all five plan recommendations on the decision card, 2026-10-30

### Consequences

- Good, because it holds if minutes are free for public repos.
- Bad, because if wrong: slower PR checks.

### Confirmation

Revisit when: CI time becomes a problem

## Why this might be wrong

macOS runners queue slowly

## Provenance

- Decided 2026-10-30T21:27:43Z by human:ryan (user-approved), via chat.
- Raised in session `24a0b143-5a79-55d6-9f31-d6bd30ba7048`.
- Verbatim: "Now that the repository is public, does macOS run in CI on every PR (changing wi-5fbb56 and #24)?"
