---
id: "kb-decision-wi-b63bd6"
title: "Open now, hold merge"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should the behaviour-changing PRs (wi-2000a3 rm-of-protected-config ask, #48 Declined: status line) be opened now but held unmerged until the pilot window closes (report due 2026-10-18), rather than merged as soon as green?\", facing skill and hook text on main can change pilot behaviour mid-window, which muddies the pilot data, we decided for Open now, hold merge and neglected Merge when green; Don't start them, on the premise that main changes reach pilot sessions, accepting that if wrong: a one-week delay on two small fixes."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-10"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-b63bd6"
question: "Should the behaviour-changing PRs (wi-2000a3 rm-of-protected-config ask, #48 Declined: status line) be opened now but held unmerged until the pilot window closes (report due 2026-10-18), rather than merged as soon as green?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-10T08:02:06Z"
tree_hash: "eb1d2fcb0ccd6bd59330fb3ab58adb8de9a1df70"
recommended: "A"
flip_condition: "pilots run a pinned plugin version"
---

# Open now, hold merge

## Context and problem statement

Should the behaviour-changing PRs (wi-2000a3 rm-of-protected-config ask, #48 Declined: status line) be opened now but held unmerged until the pilot window closes (report due 2026-10-18), rather than merged as soon as green?

Skill and hook text on main can change pilot behaviour mid-window, which muddies the pilot data.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-b63bd6`. Anchor: `docs/plans/v1-readiness.md:116`.

## Decision drivers

- Pilot data
- Delay

## Considered options

### A · Open now, hold merge (chosen)

PRs are prepared and CI-green, merged after 2026-10-18.

- Holds if main changes reach pilot sessions.
- If wrong: a one-week delay on two small fixes.

### B · Merge when green

Treat them like any other fix.

- Holds if pilots pin a version or the change is too small to matter.
- If wrong: pilot telemetry mixes two behaviours.

### C · Don't start them

Wait for the pilot report before writing them.

- Holds if the pilot may change their shape.
- If wrong: a week of idle AFK time.

## Evidence

- `docs/plans/v1-readiness.md:116`: 2.3 Collect findings ... (ongoing)
- `issue #48`: merge after the pilot window, because skill text reaches pilots via main

## Decision outcome

Chosen option: **Open now, hold merge**, the recommended option.

Decider's note: Open now, hold merge

### Consequences

- Good, because it holds if main changes reach pilot sessions.
- Bad, because if wrong: a one-week delay on two small fixes.

### Confirmation

Revisit when: pilots run a pinned plugin version

## Why this might be wrong

If the pilots don't pick up main mid-window, the hold buys nothing.

## Provenance

- Decided 2026-10-10T08:02:06Z by human:ryan (user-approved), via chat.
- Raised in session `5ce92570-ff20-5019-a993-a571d4cc77c1`.
- Verbatim: "Should the behaviour-changing PRs (wi-2000a3 rm-of-protected-config ask, #48 Declined: status line) be opened now but held unmerged until the pilot window closes (report due 2026-10-18), rather than merged as soon as green?"
