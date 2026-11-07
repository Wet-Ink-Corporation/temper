---
id: "kb-decision-wi-1bd09f"
title: "Yes, list it in D16"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Phase 2f: should D16 gain an item for pre-bash's 256 MiB per-rule directory-text budget, under which a single 2^27-character directory falls back (no answer) where 1.x answers in about 4 s?\", facing after #202's review fixes, each pre-bash rule that resolves shell paths stops after building 2 GiB of directory text for one command line and gives 1.x's fallback, we decided for Yes, list it in D16 and neglected No budget, on the premise that No real line builds gigabytes of directory text, accepting that if wrong: Raise or remove the budget later."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-06"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-1bd09f"
question: "Phase 2f: should D16 gain an item for pre-bash's 256 MiB per-rule directory-text budget, under which a single 2^27-character directory falls back (no answer) where 1.x answers in about 4 s?"
door: one-way
blast_radius: module
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-06T20:33:20Z"
tree_hash: "fc2d0b2abf57bd5950956bca40149b3d9f83776e"
recommended: "A"
flip_condition: "A real command reaching 2 GiB"
---

# Yes, list it in D16

## Context and problem statement

Phase 2f: should D16 gain an item for pre-bash's 256 MiB per-rule directory-text budget, under which a single 2^27-character directory falls back (no answer) where 1.x answers in about 4 s?

After #202's review fixes, each pre-bash rule that resolves shell paths stops after building 2 GiB of directory text for one command line and gives 1.x's fallback.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-1bd09f`. Anchor: `crates/temper-app/src/hooks/shell/dir.rs:1`.

## Considered options

### A · Yes, list it in D16 (chosen)

Keep the 2 GiB budget and record D16(s)

- Holds if No real line builds gigabytes of directory text.
- If wrong: Raise or remove the budget later.

### B · No budget

Drop the budget; such lines run until the hook timeout, as in 1.x

- Holds if Zero listed divergences matters more than bounded time.
- If wrong: Add it back.

## Evidence

- `2f implementer measurements`: 1.x has no budget but times out first: its state-dir rule took 20 s on 960 MiB of directory text and its protected rule 69 s on 1 GiB, past the 15 s hook timeout. No line 1.x answers within its timeout was found to diverge. Proposed D16(s) text is in docs/plans/temper-2-rust-port.md.

## Decision outcome

Chosen option: **Yes, list it in D16**, the recommended option.

Decider's note: Ryan chose 'Yes, list it in D16' on 2026-11-06: keep the 2 GiB per-rule budget and record D16(s).

### Consequences

- Good, because it holds if No real line builds gigabytes of directory text.
- Bad, because if wrong: Raise or remove the budget later.

### Confirmation

Revisit when: A real command reaching 2 GiB

## Why this might be wrong

The owner may prefer no divergence entries

## Provenance

- Decided 2026-11-06T20:33:20Z by human:claude (user-approved), via card.
- Raised in session `24a0b143-5a79-55d6-9f31-d6bd30ba7048`.
- Verbatim: "Phase 2f: should D16 gain an item for pre-bash's 256 MiB per-rule directory-text budget, under which a single 2^27-character directory falls back (no answer) where 1.x answers in about 4 s?"
