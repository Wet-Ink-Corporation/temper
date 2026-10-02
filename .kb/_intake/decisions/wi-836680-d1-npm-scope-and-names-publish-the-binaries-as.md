---
id: "kb-decision-wi-836680"
title: "@wet-ink scope"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D1 · npm scope and names: publish the binaries as @wet-ink/temper-<platform>-<arch> (6 packages), with the plugin-root package.json private? Alternatives: @wetink, or another scope you own. Unscoped `temper` is taken.\", facing the six npm package names are permanent once published and every plugin install resolves them, we decided for @wet-ink scope and neglected @wetink scope; Unscoped names, on the premise that you can create the wet-ink org on npm, accepting that if wrong: fall back to B before the first publish."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-836680"
question: "D1 · npm scope and names: publish the binaries as @wet-ink/temper-<platform>-<arch> (6 packages), with the plugin-root package.json private? Alternatives: @wetink, or another scope you own. Unscoped `temper` is taken."
door: one-way
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:05:57Z"
tree_hash: "0b0ea4ed6ed8fe369cd131c0d19421b7996a1786"
recommended: "A"
flip_condition: "If the wet-ink org is unavailable, B"
---

# @wet-ink scope

## Context and problem statement

D1 · npm scope and names: publish the binaries as @wet-ink/temper-<platform>-<arch> (6 packages), with the plugin-root package.json private? Alternatives: @wetink, or another scope you own. Unscoped `temper` is taken.

The six npm package names are permanent once published and every plugin install resolves them.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-836680`. Anchor: `.temper/plans/temper-2-rust-port.md:35`.

## Decision drivers

- Squatting risk
- Setup

## Considered options

### A · @wet-ink scope (chosen)

Create the wet-ink npm org; six @wet-ink/temper-<platform>-<arch> packages

- Holds if you can create the wet-ink org on npm.
- If wrong: fall back to B before the first publish.

### B · @wetink scope

Same layout under @wetink

- Holds if wet-ink is taken on npm.
- If wrong: scope no longer matches the GitHub org name.

### C · Unscoped names

temper-cli-linux-x64 and five siblings

- Holds if you do not want an npm org.
- If wrong: each of six names can be squatted separately.

## Evidence

- `.temper/plans/temper-2-rust-port.md:36`: A) @wet-ink/temper-linux-x64 and so on, scoped under a new npm org wet-ink. Recommended.
- `.temper/plans/temper-2-rust-port.md:40`: The plugin root's package.json ("private": true) is the main package and is never published.
- `registry.npmjs.org/temper`: unscoped temper is taken (HTTP 200, checked 2026-10-02)
- `registry.npmjs.org/@wet-ink%2ftemper-linux-x64`: free (HTTP 404); whether the wet-ink org exists is unconfirmed

## Decision outcome

Chosen option: **@wet-ink scope**, the recommended option.

### Consequences

- Good, because it holds if you can create the wet-ink org on npm.
- Bad, because if wrong: fall back to B before the first publish.

### Confirmation

Revisit when: If the wet-ink org is unavailable, B

## Why this might be wrong

The org may already be registered by someone else

## Provenance

- Decided 2026-10-02T04:05:57Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "kind: question"
