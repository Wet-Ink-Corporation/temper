---
id: "kb-decision-wi-2c887f"
title: "Dismiss as moot, drop from #91"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should wi-dc7fe0 (\"next AFK session builds the\", facing wi-dc7fe0 decided 'build on branches, hold merges' for a batch (#82-#89) that had already run and been merged, we decided for Dismiss as moot, drop from #91 and neglected Keep as standing policy, on the premise that no future batch needs this policy written down, accepting that if wrong: re-raise it for the next decision-changing batch."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-08"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-2c887f"
question: "Should wi-dc7fe0 (\"next AFK session builds the"
door: two-way
blast_radius: file
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-08T05:04:47Z"
tree_hash: "92eff1b8ace909b89a350c8c67fbf1c176a442fe"
recommended: "A"
flip_condition: "wanting a written rule for future freeze periods"
---

# Dismiss as moot, drop from #91

## Context and problem statement

Should wi-dc7fe0 ("next AFK session builds the

wi-dc7fe0 decided 'build on branches, hold merges' for a batch (#82-#89) that had already run and been merged.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-2c887f`. Anchor: `.kb/_intake/decisions/wi-dc7fe0-should-the-next-afk-session-build-the.md:1`.

## Decision drivers

- Record matches history

## Considered options

### A · Dismiss as moot, drop from #91 (chosen)

Reopen wi-dc7fe0, dismiss it moot, remove its record and row from #91.

- Holds if no future batch needs this policy written down.
- If wrong: re-raise it for the next decision-changing batch.

### B · Keep as standing policy

Keep the record: future batches that change hook decisions are built on branches and held.

- Holds if you want that as a rule going forward.
- If wrong: a record that doesn't match the merged #82-#89.

## Evidence

- `.kb/_intake/decisions/wi-dc7fe0-should-the-next-afk-session-build-the.md:3`: title: Build on branches, hold merges
- `git log origin/main`: Merge pull request #82..#89 (hopeful-allen), #90 follow-up decisions

## Decision outcome

Chosen option: **Dismiss as moot, drop from #91**, the recommended option.

Decider's note: Dismiss wi-dc7fe0 as moot and drop it from #91

### Consequences

- Good, because it holds if no future batch needs this policy written down.
- Bad, because if wrong: re-raise it for the next decision-changing batch.

### Confirmation

Revisit when: wanting a written rule for future freeze periods

## Why this might be wrong

if you meant it as policy for the next freeze

## Provenance

- Decided 2026-10-08T05:04:47Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
