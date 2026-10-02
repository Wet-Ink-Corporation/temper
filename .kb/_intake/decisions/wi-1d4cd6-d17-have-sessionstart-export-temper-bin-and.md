---
id: "kb-decision-wi-1d4cd6"
title: "Export PATH"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D17 · have SessionStart export TEMPER_BIN and prepend the binary's directory to PATH via CLAUDE_ENV_FILE?\", facing the 2.0 pre-commit hook calls temper; inside Claude sessions it only finds the plugin's binary if SessionStart puts it on PATH, we decided for Export PATH and neglected No PATH change, on the premise that a session-scoped PATH entry is acceptable, accepting that if wrong: a user's own temper install is shadowed in Claude sessions."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-1d4cd6"
question: "D17 · have SessionStart export TEMPER_BIN and prepend the binary's directory to PATH via CLAUDE_ENV_FILE?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:56:34Z"
tree_hash: "091b7182045afdf12f8b45bb325c6d558838eaa0"
recommended: "A"
flip_condition: "If a user's pinned temper must win over the plugin's, B"
---

# Export PATH

## Context and problem statement

D17 · have SessionStart export TEMPER_BIN and prepend the binary's directory to PATH via CLAUDE_ENV_FILE?

The 2.0 pre-commit hook calls temper; inside Claude sessions it only finds the plugin's binary if SessionStart puts it on PATH.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-1d4cd6`. Anchor: `.temper/plans/temper-2-rust-port.md:140`.

## Decision drivers

- Pre-commit in Claude sessions
- Environment change

## Considered options

### A · Export PATH (chosen)

Prepend a directory holding only the temper binary, inside Claude sessions only

- Holds if a session-scoped PATH entry is acceptable.
- If wrong: a user's own temper install is shadowed in Claude sessions.

### B · No PATH change

Users install temper themselves for the pre-commit hook

- Holds if you want the plugin not to touch the environment.
- If wrong: commits from Claude sessions fail until users install temper.

## Evidence

- `.temper/plans/temper-2-rust-port.md:141`: A) SessionStart appends export TEMPER_BIN and export PATH to CLAUDE_ENV_FILE. Without it, the 2.0 pre-commit hook fails inside Claude sessions for anyone who has not also run cargo binstall temper-cli. The directory contains only the temper binary.
- `.temper/plans/temper-2-rust-port.md:142`: B) No PATH change. The pre-commit hook then needs temper installed separately (it prints how).

## Decision outcome

Chosen option: **Export PATH**, the recommended option.

### Consequences

- Good, because it holds if a session-scoped PATH entry is acceptable.
- Bad, because if wrong: a user's own temper install is shadowed in Claude sessions.

### Confirmation

Revisit when: If a user's pinned temper must win over the plugin's, B

## Why this might be wrong

The plugin's binary version may differ from the repo's pin; the hook warns but runs it

## Provenance

- Decided 2026-10-02T04:56:34Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "D17 · have SessionStart export TEMPER_BIN and prepend the binary's directory to PATH via CLAUDE_ENV_FILE?"
