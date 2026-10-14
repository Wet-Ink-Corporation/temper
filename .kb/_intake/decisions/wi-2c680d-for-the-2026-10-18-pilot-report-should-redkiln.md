---
id: "kb-decision-wi-2c680d"
title: "Recover local rows"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"For the 2026-10-18 pilot report, should redkiln-rs's local telemetry be recovered, or should the report cover weigh-in only, or should the window be extended?\", facing redkiln-rs, the brownfield pilot, has pushed no telemetry, so the report would rest on weigh-in alone, we decided for Recover local rows and neglected Weigh-in only; Extend window, on the premise that the desktop sessions ran with temper active, accepting that if wrong: the file is empty and the report is weigh-in only anyway."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-13"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-2c680d"
question: "For the 2026-10-18 pilot report, should redkiln-rs's local telemetry be recovered, or should the report cover weigh-in only, or should the window be extended?"
door: two-way
blast_radius: module
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-13T05:00:07Z"
recommended: "A"
flip_condition: "the local file is empty, then choose between B and C"
---

# Recover local rows

## Context and problem statement

For the 2026-10-18 pilot report, should redkiln-rs's local telemetry be recovered, or should the report cover weigh-in only, or should the window be extended?

redkiln-rs, the brownfield pilot, has pushed no telemetry, so the report would rest on weigh-in alone.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-2c680d`. Anchor: `docs/plans/v1-readiness.md:116`.

## Decision drivers

- Delay
- Brownfield evidence

## Considered options

### A · Recover local rows (chosen)

Ryan runs temper stats (or telemetry push --session) in his redkiln-rs checkout before 10-18, or connects the folder for me to read

- Holds if the desktop sessions ran with temper active.
- If wrong: the file is empty and the report is weigh-in only anyway.

### B · Weigh-in only

Report on weigh-in; note redkiln-rs as uncovered

- Holds if the brownfield half was covered enough by adoption and the audit.
- If wrong: brownfield false positives reach 1.4.0 unseen.

### C · Extend window

Move the report a week so redkiln-rs gets real sessions

- Holds if redkiln-rs work is planned this week.
- If wrong: 1.4.0 and go-live slip a week.

## Evidence

- `/mnt/project-files/reports/2026-10-13/pilot-interim.md`: redkiln-rs: 0 sessions, the temper-telemetry branch does not exist
- `scripts/lib/hooks.mjs:1254`: shipTelemetry runs at every Stop when telemetry.branch is set; local rows stay in .git/temper/telemetry.jsonl

## Decision outcome

Chosen option: **Recover local rows**, the recommended option.

Decider's note: Ryan chose Recover and uploaded redkiln-rs's local telemetry.jsonl, 2026-10-13

### Consequences

- Good, because it holds if the desktop sessions ran with temper active.
- Bad, because if wrong: the file is empty and the report is weigh-in only anyway.

### Confirmation

Revisit when: the local file is empty, then choose between B and C

## Why this might be wrong

if the desktop plugin was too old to log, there is little to recover

## Provenance

- Decided 2026-10-13T05:00:07Z by human:ryan (user-approved), via chat.
- Raised in session `e4cd5db3-f650-5d15-a833-43ba0b307ba6`.
- Verbatim: "kind: question"
