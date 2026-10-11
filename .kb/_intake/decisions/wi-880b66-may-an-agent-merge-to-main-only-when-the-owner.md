---
id: "kb-decision-wi-880b66"
title: "Standing grant"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"May an agent merge to main only when the owner asked for it in that session, rather than holding a standing grant or never merging?\", facing #114 writes into AGENTS.md who may merge to main, which only the owner can do through the ruleset bypass, we decided for Standing grant and neglected Per-session ask; Owner merges, on the premise that you trust the bar and want throughput, accepting that if wrong: an agent merges something you would have stopped; main ships to users."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-11"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-880b66"
question: "May an agent merge to main only when the owner asked for it in that session, rather than holding a standing grant or never merging?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-directed
decided_at: "2026-10-11T15:31:18Z"
tree_hash: "a30791bcf452d8954dd65e211fae462b1126097a"
recommended: "A"
flip_condition: "you prefer every merge by hand (C) or no per-session ask (B)"
---

# Standing grant

## Context and problem statement

May an agent merge to main only when the owner asked for it in that session, rather than holding a standing grant or never merging?

#114 writes into AGENTS.md who may merge to main, which only the owner can do through the ruleset bypass.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-880b66`. Anchor: `AGENTS.md:67`.

## Decision drivers

- Control
- AFK throughput

## Considered options

### A · Per-session ask

Agents merge only when you ask in that session, with every check green and a cited fresh-context approval.

- Holds if you want AFK sessions to finish work but keep control per session.
- If wrong: sessions you forget to authorise leave PRs waiting.

### B · Standing grant (chosen)

Any agent session may merge PRs that meet the bar, without a per-session ask.

- Holds if you trust the bar and want throughput.
- If wrong: an agent merges something you would have stopped; main ships to users.

### C · Owner merges

Agents never merge; they leave PRs merge-ready.

- Holds if every merge to main should be your hand.
- If wrong: AFK sessions stall on merges, as today.

## Evidence

- `.github/CODEOWNERS:1`: Only the owner can merge to main, through the ruleset's bypass
- `AGENTS.md:67`: An agent merges only when the owner asked for it in that session. Otherwise it leaves the PR merge-ready for the owner.

## Decision outcome

Chosen option: **Standing grant**, overriding the recommendation (A).

Decider's note: Owner chose 'Any agent' on the card: any agent session may merge PRs that meet the bar, without a per-session ask.

### Consequences

- Good, because it holds if you trust the bar and want throughput.
- Bad, because if wrong: an agent merges something you would have stopped; main ships to users.

### Confirmation

Revisit when: you prefer every merge by hand (C) or no per-session ask (B)

## Why this might be wrong

the classifier refused my merge today even when asked, so A may behave like C until a permission rule is added

## Provenance

- Decided 2026-10-11T15:31:18Z by human:ryan (user-directed), via cli.
- Raised in session `5b936e0c-c28f-556a-95d5-be0bf43a1b22`.
- Verbatim: "May an agent merge to main only when the owner asked for it in that session, rather than holding a standing grant or never merging?"
