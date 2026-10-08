---
id: "kb-decision-wi-2000a3"
title: "Should the Bash hook also ask when a command deletes a protected config file (rm deny.toml, rm .claude/temper.json): A"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should the Bash hook also ask when a command deletes a protected config file (rm deny.toml, rm .claude/temper.json)?\", facing the question an agent raised, we decided for A."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-2000a3"
question: "Should the Bash hook also ask when a command deletes a protected config file (rm deny.toml, rm .claude/temper.json)?"
door: two-way
blast_radius: module
decider: "human:ryan"
provenance: user-directed
decided_at: "2026-10-08T04:24:26Z"
tree_hash: "7c9af35670d61832500c52b3a192f373fa67bf9d"
---

# Should the Bash hook also ask when a command deletes a protected config file (rm deny.toml, rm .claude/temper.json): A

## Context and problem statement

Should the Bash hook also ask when a command deletes a protected config file (rm deny.toml, rm .claude/temper.json)?

Raised by an agent (sweep) as a deferral and captured by Weigh-In as `wi-2000a3`. Anchor: `scripts/lib/hooks.mjs:1`.

## Decision outcome

Chosen option: **A**.

Decider's note: Yes, small follow-up: ask on rm of protected config files.

## Provenance

- Decided 2026-10-08T04:24:26Z by human:ryan (user-directed), via chat.
- Raised in session `653bb21b-0efd-5fa8-921b-5cf0f70e804a`.
- Verbatim: "Should the Bash hook also ask when a command deletes a protected config file (rm deny.toml, rm .claude/temper.json)?"
