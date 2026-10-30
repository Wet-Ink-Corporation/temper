---
id: "kb-decision-wi-779016"
title: "Core, rest accepted"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"How much of #155 goes in before 1.5.0: the core (three slow regexes plus cheap every-agent gaps) with the rest accepted, or all of it?\", facing #155 lists about 80 bypass shapes; three slow regexes let commands through for every agent, we decided for Core, rest accepted and neglected All of it, on the premise that the Stop gate and CI backstop the rest, accepting that if wrong: a later issue reopens specific shapes."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-30"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-779016"
question: "How much of #155 goes in before 1.5.0: the core (three slow regexes plus cheap every-agent gaps) with the rest accepted, or all of it?"
door: two-way
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-30T21:27:43Z"
tree_hash: "824334a35451b00e92b941186ad364410249b08a"
recommended: "A"
flip_condition: "evidence that agents use the remaining shapes"
---

# Core, rest accepted

## Context and problem statement

How much of #155 goes in before 1.5.0: the core (three slow regexes plus cheap every-agent gaps) with the rest accepted, or all of it?

#155 lists about 80 bypass shapes; three slow regexes let commands through for every agent.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-779016`. Anchor: `scripts/lib/shell.mjs:1`.

## Considered options

### A · Core, rest accepted (chosen)

Fix the three slow regexes plus $"...", eval --, trap, awk system(; record the rest as accepted; close #155.

- Holds if the Stop gate and CI backstop the rest.
- If wrong: a later issue reopens specific shapes.

### B · All of it

Fix every listed shape before 1.5.0.

- Holds if the shapes are exploited in practice.
- If wrong: weeks spent for little gain.

## Evidence

- `https://github.com/Wet-Ink-Corporation/temper/issues/155`: slow regexes run past the 15 s hook timeout

## Decision outcome

Chosen option: **Core, rest accepted**, the recommended option.

Decider's note: Ryan approved all five plan recommendations on the decision card, 2026-10-30

### Consequences

- Good, because it holds if the Stop gate and CI backstop the rest.
- Bad, because if wrong: a later issue reopens specific shapes.

### Confirmation

Revisit when: evidence that agents use the remaining shapes

## Why this might be wrong

a remaining shape may matter for the reviewer

## Provenance

- Decided 2026-10-30T21:27:43Z by human:ryan (user-approved), via chat.
- Raised in session `24a0b143-5a79-55d6-9f31-d6bd30ba7048`.
- Verbatim: "How much of #155 goes in before 1.5.0: the core (three slow regexes plus cheap every-agent gaps) with the rest accepted, or all of it?"
