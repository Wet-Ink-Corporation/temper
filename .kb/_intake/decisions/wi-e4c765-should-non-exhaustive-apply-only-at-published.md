---
id: "kb-decision-wi-e4c765"
title: "Published boundaries only (current)"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should #[non_exhaustive] apply only at published crate boundaries, keeping workspace-internal enums (errors, Decider events) exhaustive?\", facing during the review fixes I narrowed API-05: #[non_exhaustive] only at published boundaries, with workspace-internal enums (errors, Decider events) exhaustive so a new variant breaks every match (CRF-03); 1.0 recommended it for all public errors, config and events, we decided for Published boundaries only (current) and neglected Back to 1.0 default; Errors and config open, events closed, on the premise that Your workspaces are mostly unpublished apps where cross-crate exhaustiveness is wanted, accepting that if wrong: Publishing a crate later needs a pass to add the attribute before its first release."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-09-28"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-e4c765"
question: "Should #[non_exhaustive] apply only at published crate boundaries, keeping workspace-internal enums (errors, Decider events) exhaustive?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-09-28T19:19:40Z"
recommended: "A"
flip_condition: "If you plan to publish workspace crates to crates.io regularly"
---

# Published boundaries only (current)

## Context and problem statement

Should #[non_exhaustive] apply only at published crate boundaries, keeping workspace-internal enums (errors, Decider events) exhaustive?

During the review fixes I narrowed API-05: #[non_exhaustive] only at published boundaries, with workspace-internal enums (errors, Decider events) exhaustive so a new variant breaks every match (CRF-03); 1.0 recommended it for all public errors, config and events.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-e4c765`. Anchor: `skills/rust-craft/references/rules.md:64`.

## Decision drivers

- New variant, sibling crate
- Publishing a crate

## Considered options

### A · Published boundaries only (current) (chosen)

Workspace enums stay exhaustive; the attribute goes on before a crate's first publication.

- Holds if Your workspaces are mostly unpublished apps where cross-crate exhaustiveness is wanted.
- If wrong: Publishing a crate later needs a pass to add the attribute before its first release.

### B · Back to 1.0 default

#[non_exhaustive] on all public errors, config and events, including inside the workspace.

- Holds if You publish crates, or want sibling crates insulated from new variants.
- If wrong: Sibling crates carry _ arms, and a new event or error variant compiles silently everywhere.

### C · Errors and config open, events closed

Errors and config get the attribute everywhere; Decider events stay exhaustive.

- Holds if You want replay safety for events while error enums grow freely.
- If wrong: Sibling-crate _ arms silently absorb new failure kinds.

## Evidence

- `skills/rust-craft/references/rules.md:64`: In published crates, mark public enums and structs that may grow (errors, config, events) `#[non_exhaustive]` ... Inside a workspace, enums stay exhaustive so matches break (CRF-03); Decider events are exhaustive.
- `skills/rust-craft/SKILL.md:84`: typed context fields, `#[non_exhaustive]` only in published crates.

## Decision outcome

Chosen option: **Published boundaries only (current)**, the recommended option.

Decider's note: Published only (Recommended)

### Consequences

- Good, because it holds if Your workspaces are mostly unpublished apps where cross-crate exhaustiveness is wanted.
- Bad, because if wrong: Publishing a crate later needs a pass to add the attribute before its first release.

### Confirmation

Revisit when: If you plan to publish workspace crates to crates.io regularly

## Why this might be wrong

In a workspace with many crates, adding a variant can touch many files

## Provenance

- Decided 2026-09-28T19:19:40Z by human:ryan (user-approved), via chat.
- Raised in session `a1b8d21a-0b93-5c31-8a7f-242ae801112f`.
- Verbatim: "Should #[non_exhaustive] apply only at published crate boundaries, keeping workspace-internal enums (errors, Decider events) exhaustive?"
