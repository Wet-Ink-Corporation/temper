---
id: "kb-decision-wi-103aa0"
title: "Remove in #16"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Remove the never-firing PreToolUse SubagentHandback hook in the\", facing the PreToolUse SubagentHandback hook never fires on 2.1.286-2.1.289; SubagentStop does the same check, we decided for Remove in #16 and neglected Keep with a note, on the premise that no CC release starts calling it, accepting that if wrong: re-add one registration."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-103aa0"
question: "Remove the never-firing PreToolUse SubagentHandback hook in the"
door: two-way
blast_radius: module
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-08T05:00:08Z"
tree_hash: "7c9af35670d61832500c52b3a192f373fa67bf9d"
recommended: "A"
flip_condition: "a CC changelog announcing SubagentHandback"
---

# Remove in #16

## Context and problem statement

Remove the never-firing PreToolUse SubagentHandback hook in the

The PreToolUse SubagentHandback hook never fires on 2.1.286-2.1.289; SubagentStop does the same check.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-103aa0`. Anchor: `hooks/hooks.json:33`.

## Decision drivers

- Port work

## Considered options

### A · Remove in #16 (chosen)

Drop the registration and preHandback; drop its port parity tests (D16 amendment).

- Holds if no CC release starts calling it.
- If wrong: re-add one registration.

### B · Keep with a note

Leave it; README already documents the exception.

- Holds if you expect CC to start firing it.
- If wrong: dead code carried into the 2.0 port.

## Evidence

- `hooks/hooks.json:33`: "matcher": "SubagentHandback"
- `docs/history/evals.md`: PreToolUse SubagentHandback | no

## Decision outcome

Chosen option: **Remove in #16**, the recommended option.

Decider's note: Remove in #16

### Consequences

- Good, because it holds if no CC release starts calling it.
- Bad, because if wrong: re-add one registration.

### Confirmation

Revisit when: a CC changelog announcing SubagentHandback

## Why this might be wrong

if CC starts firing it soon

## Provenance

- Decided 2026-10-08T05:00:08Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
