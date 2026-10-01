---
id: "kb-decision-wi-d75e63"
title: "Rename this marketplace to temper"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should the temper repository itself be the wet-ink marketplace (root marketplace.json, plugin source \"./\"), rather than a separate Wet Ink marketplace repo that lists temper and weigh-in?\", facing the marketplace name becomes everyone's install command on the first push, and weigh-in is already a second Wet Ink plugin in its own repo, we decided for Rename this marketplace to temper and neglected Keep: this repo is the wet-ink marketplace; Central repo now, on the premise that a central Wet Ink marketplace is plausible, accepting that if wrong: an odd temper@temper name for nothing."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-01"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-d75e63"
question: "Should the temper repository itself be the wet-ink marketplace (root marketplace.json, plugin source \"./\"), rather than a separate Wet Ink marketplace repo that lists temper and weigh-in?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-01T19:21:58Z"
recommended: "B"
flip_condition: "you create the central marketplace repo now (then C), or temper stays the only plugin (then A)"
---

# Rename this marketplace to temper

## Context and problem statement

Should the temper repository itself be the wet-ink marketplace (root marketplace.json, plugin source "./"), rather than a separate Wet Ink marketplace repo that lists temper and weigh-in?

The marketplace name becomes everyone's install command on the first push, and weigh-in is already a second Wet Ink plugin in its own repo.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-d75e63`. Anchor: `.claude-plugin/marketplace.json:2`.

## Decision drivers

- Install command
- Collides with a later central marketplace
- Work now

## Considered options

### A · Keep: this repo is the wet-ink marketplace

As built and as the README says: install with temper@wet-ink.

- Holds if temper stays the only plugin distributed this way.
- If wrong: a later central wet-ink marketplace collides by name; users re-add from the new source.

### B · Rename this marketplace to temper (chosen)

Install becomes temper@temper; wet-ink stays free for a central marketplace later. README lines change.

- Holds if a central Wet Ink marketplace is plausible.
- If wrong: an odd temper@temper name for nothing.

### C · Central repo now

You create e.g. Wet-Ink-Corporation/claude-plugins as the wet-ink marketplace listing temper and weigh-in by GitHub source; this repo drops its marketplace.json.

- Holds if you want one add command for all Wet Ink plugins now.
- If wrong: one more repo to keep in step with releases.

## Evidence

- `.claude-plugin/marketplace.json:2`: "name": "wet-ink", plugins: [{ name: temper, source: "./" }]
- `README.md:29`: claude plugin marketplace add Wet-Ink-Corporation/temper / claude plugin install temper@wet-ink
- `weigh-in plugin.json`: homepage https://github.com/Wet-Ink-Corporation/weigh-in: a separate plugin repo with no marketplace manifest

## Decision outcome

Chosen option: **Rename this marketplace to temper**, the recommended option.

Decider's note: Rename to temper (Recommended)

### Consequences

- Good, because it holds if a central Wet Ink marketplace is plausible.
- Bad, because if wrong: an odd temper@temper name for nothing.

### Confirmation

Revisit when: you create the central marketplace repo now (then C), or temper stays the only plugin (then A)

## Why this might be wrong

If weigh-in and future plugins never share a marketplace, the rename buys nothing.

## Provenance

- Decided 2026-10-01T19:21:58Z by human:ryan (user-approved), via chat.
- Raised in session `a1b8d21a-0b93-5c31-8a7f-242ae801112f`.
- Verbatim: "Should the temper repository itself be the wet-ink marketplace (root marketplace.json, plugin source \"./\"), rather than a separate Wet Ink marketplace repo that lists temper and weigh-in?"
