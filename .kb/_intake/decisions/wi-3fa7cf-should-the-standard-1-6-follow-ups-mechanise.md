---
id: "kb-decision-wi-3fa7cf"
title: "Staged, after pilot"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should the standard-1-6 follow-ups (mechanise ARC-02, ARC-08, TST-09, OUT-04 and ASY-17; mark 16 rules as review-only by design and add them to the rubric) start after the pilot report?\", facing 30 MUST rules block merge only through the reviewer; the analysis found 5 that a tool can check with low false-positive risk, we decided for Staged, after pilot and neglected Accept all as review; Mechanise all 14, on the premise that the five checks stay low false-positive in the pilots, accepting that if wrong: a noisy check gets reverted to advisory."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-13"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-3fa7cf"
question: "Should the standard-1-6 follow-ups (mechanise ARC-02, ARC-08, TST-09, OUT-04 and ASY-17; mark 16 rules as review-only by design and add them to the rubric) start after the pilot report?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-13T05:00:07Z"
recommended: "A"
flip_condition: "the pilot shows review already catches these, which favours B"
---

# Staged, after pilot

## Context and problem statement

Should the standard-1-6 follow-ups (mechanise ARC-02, ARC-08, TST-09, OUT-04 and ASY-17; mark 16 rules as review-only by design and add them to the rubric) start after the pilot report?

30 MUST rules block merge only through the reviewer; the analysis found 5 that a tool can check with low false-positive risk.

Raised by an agent (marker) as a deferral and captured by Weigh-In as `wi-3fa7cf`. Anchor: `skills/rust-craft/references/rules.md:7`.

## Decision drivers

- Work
- 2.0 port cost

## Considered options

### A · Staged, after pilot (chosen)

After 2026-10-18: mechanise ARC-02, ARC-08, TST-09, OUT-04, ASY-17; mark 16 rules review-only by design and name them in the rubric

- Holds if the five checks stay low false-positive in the pilots.
- If wrong: a noisy check gets reverted to advisory.

### B · Accept all as review

Only document all 30 as review-only by design and add rubric lines; no new checks

- Holds if the reviewer reliably catches these.
- If wrong: MUST violations merge when review misses them.

### C · Mechanise all 14

Also add the 9 softer scanner advisories (ARC-05, CRF-11, ERR-03, ERR-08, ASY-08, OUT-02 ...)

- Holds if advisory noise is acceptable to adopters.
- If wrong: advisory fatigue; more 2.0 parity surface.

## Evidence

- `/mnt/project-files/reports/2026-10-13/must-rules-review-only.md`: A or A-capable 3 (ARC-02, API-17, ARC-08) plus ARC-16 already done; B 12; C 16.
- `templates/workspace/deny.toml:26`: [bans] wrappers already used; the same mechanism enforces ARC-02
- `skills/rust-craft/references/review-rubric.md:5`: A finding may cite any rule (9 of the C rules are not named in the rubric)

## Decision outcome

Chosen option: **Staged, after pilot**, the recommended option.

Decider's note: Ryan chose Staged on the decision card, 2026-10-13

### Consequences

- Good, because it holds if the five checks stay low false-positive in the pilots.
- Bad, because if wrong: a noisy check gets reverted to advisory.

### Confirmation

Revisit when: the pilot shows review already catches these, which favours B

## Why this might be wrong

if the pilot shows near-zero violations of these rules, the checks add cost for little catch

## Provenance

- Decided 2026-10-13T05:00:07Z by human:ryan (user-approved), via chat.
- Raised in session `e4cd5db3-f650-5d15-a833-43ba0b307ba6`.
- Verbatim: "kind: deferral"
