---
id: "kb-decision-wi-cc1518"
title: "Build baseline + report, fix docs"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should I build temper baseline (a committed clippy/rustdoc/scanner ratchet used by the gate and CI) and temper report (a per-crate compliance scorecard from CI), and correct the README/setup brownfield claim?\", facing the gate runs workspace-wide clippy with -D warnings and no ratchet, so a brownfield repo that adopts the lint table gets a red gate on every turn that changes Rust, while the README says legacy code does not stop work, we decided for Build baseline + report, fix docs and neglected Baseline ratchet only, fix docs; Docs fix + per-crate opt-in; Status quo, on the premise that You will adopt temper on existing repos, accepting that if wrong: About half a day of agent build time, and one more file format to maintain."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-09-28"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-cc1518"
question: "Should I build temper baseline (a committed clippy/rustdoc/scanner ratchet used by the gate and CI) and temper report (a per-crate compliance scorecard from CI), and correct the README/setup brownfield claim?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-09-28T19:19:40Z"
recommended: "A"
flip_condition: "If your brownfield targets are already clean under clippy defaults, C is enough"
---

# Build baseline + report, fix docs

## Context and problem statement

Should I build temper baseline (a committed clippy/rustdoc/scanner ratchet used by the gate and CI) and temper report (a per-crate compliance scorecard from CI), and correct the README/setup brownfield claim?

The gate runs workspace-wide clippy with -D warnings and no ratchet, so a brownfield repo that adopts the lint table gets a red gate on every turn that changes Rust, while the README says legacy code does not stop work.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-cc1518`. Anchor: `scripts/lib/gate.mjs:19`.

## Decision drivers

- Day-one gate on a legacy repo
- Compliance visibility
- Build cost

## Considered options

### A · Build baseline + report, fix docs (chosen)

temper baseline: committed .temper/baseline.json of clippy, rustdoc and scanner counts per crate, file and rule; the gate and CI fail only on increases. temper report: a per-crate scorecard as a CI artifact plus a PR delta. Fix the README and setup text.

- Holds if You will adopt temper on existing repos.
- If wrong: About half a day of agent build time, and one more file format to maintain.

### B · Baseline ratchet only, fix docs

The same ratchet, with no scorecard; compliance stays visible only through /temper:audit and CI pass/fail.

- Holds if Nobody outside Claude Code needs compliance visibility.
- If wrong: Aggregation is rebuilt later when a team asks.

### C · Docs fix + per-crate opt-in

Correct the README and setup; document per-crate [lints] workspace = true with -p lists in gate.steps and CI.

- Holds if Your brownfield targets are few, or already clean under clippy defaults.
- If wrong: Every adoption hand-maintains -p lists; debt in crates not yet opted in goes unmeasured.

### D · Status quo

No change.

- Holds if No brownfield adoption is planned.
- If wrong: A false README claim, and a red gate on the first legacy repo.

## Evidence

- `scripts/lib/gate.mjs:19`: { name: 'clippy', cmd: ['cargo', 'clippy', '--workspace', '--all-targets', ...feat, '--message-format=short', '--', '-D', 'warnings'] }
- `README.md:73`: Every check compares against the session's base revision, so only *new* violations block.
- `skills/setup/SKILL.md:38`: the hooks only block *new* violations (a ratchet), so legacy code does not stop anyone working.

## Decision outcome

Chosen option: **Build baseline + report, fix docs**, the recommended option.

Decider's note: Baseline + report (Recommended)

### Consequences

- Good, because it holds if You will adopt temper on existing repos.
- Bad, because if wrong: About half a day of agent build time, and one more file format to maintain.

### Confirmation

Revisit when: If your brownfield targets are already clean under clippy defaults, C is enough

## Why this might be wrong

A per-file count baseline can hide a swap (one fixed, one new) unless it counts per lint per file

## Provenance

- Decided 2026-09-28T19:19:40Z by human:ryan (user-approved), via chat.
- Raised in session `a1b8d21a-0b93-5c31-8a7f-242ae801112f`.
- Verbatim: "Should I build temper baseline (a committed clippy/rustdoc/scanner ratchet used by the gate and CI) and temper report (a per-crate compliance scorecard from CI), and correct the README/setup brownfield claim?"
