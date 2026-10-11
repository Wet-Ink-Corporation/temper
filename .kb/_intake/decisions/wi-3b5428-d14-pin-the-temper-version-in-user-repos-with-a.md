---
id: "kb-decision-wi-3b5428"
title: ".claude/temper-version"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D14 · pin the temper version in user repos with a new one-line file, .claude/temper-version?\", facing CI, the pre-commit hook and status checks all need to know which temper version the repo expects, we decided for .claude/temper-version and neglected cliVersion in temper.json; Only in rust.yml, on the premise that one more small file is acceptable, accepting that if wrong: another file under .claude/ to protect and migrate."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-3b5428"
question: "D14 · pin the temper version in user repos with a new one-line file, .claude/temper-version?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T05:09:17Z"
tree_hash: "091b7182045afdf12f8b45bb325c6d558838eaa0"
recommended: "A"
flip_condition: "If you want a single config file and accept a JSON parse in the hook, B"
---

# .claude/temper-version

## Context and problem statement

D14 · pin the temper version in user repos with a new one-line file, .claude/temper-version?

CI, the pre-commit hook and status checks all need to know which temper version the repo expects.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-3b5428`. Anchor: `docs/plans/temper-2-rust-port.md:126`.

## Decision drivers

- Readable from a shell hook
- Files

## Considered options

### A · .claude/temper-version (chosen)

A one-line file read by CI, the pre-commit hook and temper status

- Holds if one more small file is acceptable.
- If wrong: another file under .claude/ to protect and migrate.

### B · cliVersion in temper.json

Pin inside the existing config file

- Holds if you want all temper settings in one file.
- If wrong: the shell pre-commit hook needs a JSON parser.

### C · Only in rust.yml

The workflow holds the version literal

- Holds if only CI needs the pin.
- If wrong: pre-commit and status cannot see it.

## Evidence

- `docs/plans/temper-2-rust-port.md:128`: B) A cliVersion field in .claude/temper.json: one file fewer, but the pre-commit hook would need a JSON parser.
- `docs/plans/temper-2-rust-port.md:129`: C) A literal in rust.yml only: the pre-commit hook cannot see it.
- `docs/plans/temper-2-rust-port.md:685`: .githooks/pre-commit: if .claude/temper-version exists, it requires temper on PATH ... warns when temper --version differs from the pin

## Decision outcome

Chosen option: **.claude/temper-version**, the recommended option.

### Consequences

- Good, because it holds if one more small file is acceptable.
- Bad, because if wrong: another file under .claude/ to protect and migrate.

### Confirmation

Revisit when: If you want a single config file and accept a JSON parse in the hook, B

## Why this might be wrong

Two sources of temper config can drift

## Provenance

- Decided 2026-10-02T05:09:17Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "D14 · pin the temper version in user repos with a new one-line file, .claude/temper-version?"
