---
id: "kb-decision-wi-4aa7eb"
title: "Read-only, CI-held"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should temper lens be strictly read-only (never deny, allow or rewrite a tool call, never submit a prompt or call a model), enforced by a CI allow-list on its validate report?\", facing a mod can approve or rewrite tool calls; the design rules that out for the lens and makes CI hold it, we decided for Read-only, CI-held and neglected Read-only by convention, on the premise that command hooks keep enforcing, accepting that if wrong: a later enforcement idea needs a second module."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-11"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-4aa7eb"
question: "Should temper lens be strictly read-only (never deny, allow or rewrite a tool call, never submit a prompt or call a model), enforced by a CI allow-list on its validate report?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-11T21:33:12Z"
tree_hash: "0679046ed2525710353f1f9b4413d6d8f67a1be6"
recommended: "A"
flip_condition: "a concrete enforcement need only a mod can meet"
---

# Read-only, CI-held

## Context and problem statement

Should temper lens be strictly read-only (never deny, allow or rewrite a tool call, never submit a prompt or call a model), enforced by a CI allow-list on its validate report?

A mod can approve or rewrite tool calls; the design rules that out for the lens and makes CI hold it.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-4aa7eb`. Anchor: `hooks/hooks.json:1`.

## Decision drivers

- Guard

## Considered options

### A · Read-only, CI-held (chosen)

The lens only observes and draws; CI fails on any new hook or call.

- Holds if command hooks keep enforcing.
- If wrong: a later enforcement idea needs a second module.

### B · Read-only by convention

Same design, no CI allow-list.

- Holds if reviewers catch drift.
- If wrong: a lens change could quietly gain approve powers.

## Evidence

- `https://github.com/Wet-Ink-Corporation/temper/issues/110`: fails if the lens hooks any event or makes any call outside the allow-list
- `https://code.claude.com/docs/en/plugins/mods/overview`: Act without asking you: approve a tool call before you're asked

## Decision outcome

Chosen option: **Read-only, CI-held**, the recommended option.

Decider's note: Chosen on the decision card: CI-held

### Consequences

- Good, because it holds if command hooks keep enforcing.
- Bad, because if wrong: a later enforcement idea needs a second module.

### Confirmation

Revisit when: a concrete enforcement need only a mod can meet

## Why this might be wrong

If temper later wants in-process guards, the allow-list must be widened deliberately

## Provenance

- Decided 2026-10-11T21:33:12Z by human:ryan (user-approved), via cli.
- Raised in session `b650fa60-608c-5624-8b6f-9d5546d278c1`.
- Verbatim: "Should temper lens be strictly read-only (never deny, allow or rewrite a tool call, never submit a prompt or call a model), enforced by a CI allow-list on its validate report?"
