---
id: "kb-decision-wi-641f99"
title: "Dedicated branch"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"The proxy refuses hidden refs, so may temper's Stop hook push each session's telemetry to a dedicated `temper-telemetry` branch (opt-in, with [skip ci]) in weigh-in and redkiln-rs?\", facing cloud containers discard .git/temper, so pilot telemetry is lost unless a hook pushes it somewhere durable, we decided for Dedicated branch and neglected CI evidence only; Commit with the work, on the premise that a visible branch per pilot repo is acceptable, accepting that if wrong: one stray branch per repo, remove the config key."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-09"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-641f99"
question: "The proxy refuses hidden refs, so may temper's Stop hook push each session's telemetry to a dedicated `temper-telemetry` branch (opt-in, with [skip ci]) in weigh-in and redkiln-rs?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-09T03:25:03Z"
tree_hash: "b5bea60e6ad2029cc69a69595c09b8afc9cd3354"
recommended: "A"
flip_condition: "a hook network push being unacceptable"
---

# Dedicated branch

## Context and problem statement

The proxy refuses hidden refs, so may temper's Stop hook push each session's telemetry to a dedicated `temper-telemetry` branch (opt-in, with [skip ci]) in weigh-in and redkiln-rs?

Cloud containers discard .git/temper, so pilot telemetry is lost unless a hook pushes it somewhere durable.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-641f99`. Anchor: `scripts/lib/project.mjs:383`.

## Decision drivers

- Evidence captured
- Network from hooks

## Considered options

### A · Dedicated branch (chosen)

Opt-in config; Stop pushes sessions/<date>-<session>.jsonl to temper-telemetry with [skip ci]

- Holds if a visible branch per pilot repo is acceptable.
- If wrong: one stray branch per repo, remove the config key.

### B · CI evidence only

No hook change; rebuild latency and review rounds from CI logs and PRs at pilot end

- Holds if denials and skill-gate data are not needed.
- If wrong: pilot ends without denial/block evidence.

### C · Commit with the work

Hook writes a tracked file the agent commits in each PR

- Holds if PR noise is acceptable.
- If wrong: noisy PRs and lost rows when the file is not added.

## Evidence

- `scripts/lib/project.mjs:383`: telemetry appends only to proj.stateDir (.git/temper/)
- `README.md:190`: temper's own code makes no network calls
- `probe in this session`: push to refs/temper/telemetry/probe -> HTTP 403; push to refs/heads/temper-telemetry/probe -> new branch

## Decision outcome

Chosen option: **Dedicated branch**, the recommended option.

Decider's note: Dedicated temper-telemetry branch, opt-in per repo, [skip ci]

### Consequences

- Good, because it holds if a visible branch per pilot repo is acceptable.
- Bad, because if wrong: one stray branch per repo, remove the config key.

### Confirmation

Revisit when: a hook network push being unacceptable

## Why this might be wrong

pushes from the Stop hook could fail silently under a stricter proxy

## Provenance

- Decided 2026-10-09T03:25:03Z by human:ryan (user-approved), via chat.
- Raised in session `df29e08b-a26d-58b2-b21f-3423fa97c5ad`.
- Verbatim: "kind: question"
