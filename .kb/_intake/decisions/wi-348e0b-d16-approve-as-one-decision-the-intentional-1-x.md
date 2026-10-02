---
id: "kb-decision-wi-348e0b"
title: "Approve the list"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D16 · approve as one decision the intentional 1.x divergences in plan §2.4 (JSON parse-error wording, broken-pipe exit code, stricter TOML, invalid temper.json reported, emoji-safe truncation, 120 s git timeout, --version, anti-bypass for temper.exe and .claude/temper-version, telemetry key order, SessionStart timeout 900 s) and retiring 7 tests (6 vendor tests plus \"lacks report.mjs\")?\", facing these are the only places 2.0 behaves differently from 1.x; everything else is held byte-identical by the parity harness, we decided for Approve the list and neglected Review §2.4 item by item first; No divergences, on the premise that none of these is behaviour a user scripts against, accepting that if wrong: a user script parsing an old message or exit code breaks."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-348e0b"
question: "D16 · approve as one decision the intentional 1.x divergences in plan §2.4 (JSON parse-error wording, broken-pipe exit code, stricter TOML, invalid temper.json reported, emoji-safe truncation, 120 s git timeout, --version, anti-bypass for temper.exe and .claude/temper-version, telemetry key order, SessionStart timeout 900 s) and retiring 7 tests (6 vendor tests plus \"lacks report.mjs\")?"
door: one-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:36:06Z"
tree_hash: "925ac261f5ed075cdc39044cc51fb53f62f097c3"
recommended: "A"
flip_condition: "If any item is behaviour you know users script against, B"
---

# Approve the list

## Context and problem statement

D16 · approve as one decision the intentional 1.x divergences in plan §2.4 (JSON parse-error wording, broken-pipe exit code, stricter TOML, invalid temper.json reported, emoji-safe truncation, 120 s git timeout, --version, anti-bypass for temper.exe and .claude/temper-version, telemetry key order, SessionStart timeout 900 s) and retiring 7 tests (6 vendor tests plus "lacks report.mjs")?

These are the only places 2.0 behaves differently from 1.x; everything else is held byte-identical by the parity harness.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-348e0b`. Anchor: `docs/plans/temper-2-rust-port.md:137`.

## Decision drivers

- Compatibility
- Known quirks kept

## Considered options

### A · Approve the list (chosen)

JSON parse-error wording, broken-pipe exit code, stricter TOML, invalid temper.json reported, emoji-safe truncation, 120 s git timeout, --version, anti-bypass for temper.exe and .claude/temper-version, telemetry key order, SessionStart timeout 900 s; retire 7 tests

- Holds if none of these is behaviour a user scripts against.
- If wrong: a user script parsing an old message or exit code breaks.

### B · Review §2.4 item by item first

Read the plan's §2.4 and approve or reject each divergence

- Holds if some item looks risky to you.
- If wrong: delays the test port by a sitting.

### C · No divergences

Keep every 1.x behaviour byte-identical, including the quirks

- Holds if strict compatibility matters most.
- If wrong: keeps known quirks: invalid temper.json silently ignored, no git timeout.

## Evidence

- `docs/plans/temper-2-rust-port.md:138`: Approve the list in §2.4 as a whole. Every other observable behaviour stays byte-identical, and the parity harness enforces that.
- `docs/plans/temper-2-rust-port.md:32`: D16 | Intentional divergences from 1.x behaviour, and retired tests (H-12)

## Decision outcome

Chosen option: **Approve the list**, the recommended option.

### Consequences

- Good, because it holds if none of these is behaviour a user scripts against.
- Bad, because if wrong: a user script parsing an old message or exit code breaks.

### Confirmation

Revisit when: If any item is behaviour you know users script against, B

## Why this might be wrong

Reporting an invalid temper.json could newly block repos that had a broken config

## Provenance

- Decided 2026-10-02T04:36:06Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "kind: question"
