---
id: "kb-decision-wi-cdcecc"
title: "Hash-guarded migrate"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D15 · migrate 1.x repos with temper setup --migrate, rewriting only files whose hash matches a shipped 1.x template?\", facing moving a 1.x repo to 2.0 means changing its CI workflow, pre-commit hook and settings, which users may have customised, we decided for Hash-guarded migrate and neglected Manual only; Rewrite regardless, on the premise that many users kept the templates as shipped, accepting that if wrong: customised repos get only printed steps."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-cdcecc"
question: "D15 · migrate 1.x repos with temper setup --migrate, rewriting only files whose hash matches a shipped 1.x template?"
door: one-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T05:09:17Z"
tree_hash: "091b7182045afdf12f8b45bb325c6d558838eaa0"
recommended: "A"
flip_condition: "If you want temper never to rewrite user files, B"
---

# Hash-guarded migrate

## Context and problem statement

D15 · migrate 1.x repos with temper setup --migrate, rewriting only files whose hash matches a shipped 1.x template?

Moving a 1.x repo to 2.0 means changing its CI workflow, pre-commit hook and settings, which users may have customised.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-cdcecc`. Anchor: `.temper/plans/temper-2-rust-port.md:130`.

## Decision drivers

- Customised files
- Unmodified files

## Considered options

### A · Hash-guarded migrate (chosen)

Rewrite only files byte-identical to a shipped 1.x template; print manual edits for the rest

- Holds if many users kept the templates as shipped.
- If wrong: customised repos get only printed steps.

### B · Manual only

Setup prints the migration steps; nothing is rewritten

- Holds if you want temper never to touch users' CI.
- If wrong: every user does the edits by hand.

### C · Rewrite regardless

Overwrite the files with the 2.0 templates

- Holds if no one customised their files.
- If wrong: clobbers customised CI and hooks.

## Evidence

- `.temper/plans/temper-2-rust-port.md:132`: A) temper setup --migrate ... replaces rust.yml, pre-commit, settings.json allow-list entries and AGENTS.md command rows only when the file's SHA-256 equals one of the shipped 1.x template versions. Otherwise it prints the exact manual edits.
- `.temper/plans/temper-2-rust-port.md:134`: C) Rewrite the files regardless: no, it would clobber customised CI.
- `.temper/plans/temper-2-rust-port.md:135`: Either way, a 1.x repository keeps working untouched: its vendored copy is self-contained and the baseline format does not change.

## Decision outcome

Chosen option: **Hash-guarded migrate**, the recommended option.

### Consequences

- Good, because it holds if many users kept the templates as shipped.
- Bad, because if wrong: customised repos get only printed steps.

### Confirmation

Revisit when: If you want temper never to rewrite user files, B

## Why this might be wrong

Line-ending changes on Windows checkouts make hashes miss, so more repos get only printed steps

## Provenance

- Decided 2026-10-02T05:09:17Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "D15 · migrate 1.x repos with temper setup --migrate, rewriting only files whose hash matches a shipped 1.x template?"
