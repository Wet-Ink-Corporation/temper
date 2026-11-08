---
id: "kb-decision-wi-59cb6e"
title: "Split to kits"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Does D3's four temper-* crates give way to the suite split, with the language-neutral half (gate, verdict validation, fingerprint, baseline ratchet, gate cache; shell reader, protected globs) moving to redkiln-craft and redkiln-guard?\", facing d3 put everything in four temper-* crates; the suite puts the language-neutral half in kits, we decided for Split to kits and neglected Keep four crates, on the premise that the kits exist at Core 1.0, accepting that if wrong: units move back into rk-rust crates."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-11-08"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-59cb6e"
question: "Does D3's four temper-* crates give way to the suite split, with the language-neutral half (gate, verdict validation, fingerprint, baseline ratchet, gate cache; shell reader, protected globs) moving to redkiln-craft and redkiln-guard?"
door: one-way
blast_radius: system
decider: "human:claude"
provenance: user-approved
decided_at: "2026-11-08T05:18:52Z"
tree_hash: "399982f750973183e425fbd74a37d0c4550b3336"
recommended: "A"
flip_condition: "the kit list changes"
---

# Split to kits

## Context and problem statement

Does D3's four temper-* crates give way to the suite split, with the language-neutral half (gate, verdict validation, fingerprint, baseline ratchet, gate cache; shell reader, protected globs) moving to redkiln-craft and redkiln-guard?

D3 put everything in four temper-* crates; the suite puts the language-neutral half in kits.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-59cb6e`. Anchor: `docs/plans/temper-2-rust-port.md:19`.

## Decision drivers

- Duplication
- Work before import

## Considered options

### A · Split to kits (chosen)

Gate, verdict, fingerprint, ratchet, cache to redkiln-craft; shell reader, globs to redkiln-guard; Rust-only stays.

- Holds if the kits exist at Core 1.0.
- If wrong: units move back into rk-rust crates.

### B · Keep four crates

Import the four crates intact; kits extract later if a second plugin needs them.

- Holds if no second plugin needs these units soon.
- If wrong: duplicate gate/guard code across plugins, which the suite exists to remove.

## Evidence

- `docs/plans/temper-2-rust-port.md:19`: D3 | Crate split | 4 crates (domain / app / host / cli) plus an unpublished xtask
- `briefing temper.md`: About 3,500 language-neutral lines go to kits: redkiln-craft ... redkiln-guard

## Decision outcome

Chosen option: **Split to kits**, the recommended option.

Decider's note: Chosen on the decision card in the project thread, 2026-11-08 05:18Z

### Consequences

- Good, because it holds if the kits exist at Core 1.0.
- Bad, because if wrong: units move back into rk-rust crates.

### Confirmation

Revisit when: the kit list changes

## Why this might be wrong

if kit APIs are not frozen by Core 1.0

## Provenance

- Decided 2026-11-08T05:18:52Z by human:claude (user-approved), via cli.
- Raised in session `12c49b6a-375f-54ce-8d4a-968dee482d3c`.
- Verbatim: "Does D3's four temper-* crates give way to the suite split, with the language-neutral half (gate, verdict validation, fingerprint, baseline ratchet, gate cache; shell reader, protected globs) moving to redkiln-craft and redkiln-guard?"
