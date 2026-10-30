---
id: "kb-decision-wi-428ed1"
title: "Placeholder packages by hand"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Does the first npm publish go out as six 0.0.0 placeholder packages, published by hand with a short-lived token, so trusted publishing can be set up before phase 0b?\", facing phase 0b publishes 2.0.0-alpha.1 to npm through trusted publishing, we decided for Placeholder packages by hand and neglected Token for alpha.1 only, on the premise that npm needs the package to exist, accepting that if wrong: harmless extra step."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-30"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-428ed1"
question: "Does the first npm publish go out as six 0.0.0 placeholder packages, published by hand with a short-lived token, so trusted publishing can be set up before phase 0b?"
door: one-way
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-30T21:27:43Z"
tree_hash: "824334a35451b00e92b941186ad364410249b08a"
recommended: "A"
flip_condition: "npm allows pending trusted publishers"
---

# Placeholder packages by hand

## Context and problem statement

Does the first npm publish go out as six 0.0.0 placeholder packages, published by hand with a short-lived token, so trusted publishing can be set up before phase 0b?

Phase 0b publishes 2.0.0-alpha.1 to npm through trusted publishing.

Raised by an agent (sweep) as a question and captured by Weigh-In as `wi-428ed1`. Anchor: `docs/plans/temper-2-rust-port.md:592`.

## Considered options

### A · Placeholder packages by hand (chosen)

Publish six 0.0.0 placeholders with a short-lived token, revoke it, then configure trusted publishing.

- Holds if npm needs the package to exist.
- If wrong: harmless extra step.

### B · Token for alpha.1 only

release.yml uses a token for the first publish.

- Holds if a stored token is acceptable.
- If wrong: a long-lived secret in CI.

## Evidence

- `https://github.com/Wet-Ink-Corporation/temper/issues/23`: npm can only be configured once the packages exist

## Decision outcome

Chosen option: **Placeholder packages by hand**, the recommended option.

Decider's note: Ryan approved all five plan recommendations on the decision card, 2026-10-30; verify npm's rule before acting

### Consequences

- Good, because it holds if npm needs the package to exist.
- Bad, because if wrong: harmless extra step.

### Confirmation

Revisit when: npm allows pending trusted publishers

## Why this might be wrong

npm may now allow pending publishers

## Provenance

- Decided 2026-10-30T21:27:43Z by human:ryan (user-approved), via chat.
- Raised in session `24a0b143-5a79-55d6-9f31-d6bd30ba7048`.
- Verbatim: "Does the first npm publish go out as six 0.0.0 placeholder packages, published by hand with a short-lived token, so trusted publishing can be set up before phase 0b?"
