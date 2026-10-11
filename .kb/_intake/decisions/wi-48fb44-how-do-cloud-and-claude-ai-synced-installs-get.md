---
id: "kb-decision-wi-48fb44"
title: "Gate 0b on verify; setup-script install as fallback"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"How do cloud and claude.ai-synced installs get the 2.0 engine, and does install verification on both paths gate 2.0.0?\", facing cloud sessions install temper via org sync or a setup script; neither is verified to run npm ci, which 2.0's binary relies on, we decided for Gate 0b on verify; setup-script install as fallback and neglected Prebuilt binaries in the plugin; Fallback build with Rust in the image, on the premise that the setup script can run npm, accepting that if wrong: synced-only users still lack an engine."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-07"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-48fb44"
question: "How do cloud and claude.ai-synced installs get the 2.0 engine, and does install verification on both paths gate 2.0.0?"
door: two-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-07T05:35:50Z"
tree_hash: "8070945d1487c39512abdb7d2016b04b3d45b01b"
recommended: "A"
flip_condition: "verify-install showing the setup script cannot run npm"
---

# Gate 0b on verify; setup-script install as fallback

## Context and problem statement

How do cloud and claude.ai-synced installs get the 2.0 engine, and does install verification on both paths gate 2.0.0?

Cloud sessions install temper via org sync or a setup script; neither is verified to run npm ci, which 2.0's binary relies on.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-48fb44`. Anchor: `docs/plans/temper-2-rust-port.md:867`.

## Decision drivers

- Plugin size
- Works without npm

## Considered options

### A · Gate 0b on verify; setup-script install as fallback (chosen)

verify-install on both paths is a Phase 0b exit criterion; a path that skips npm ci installs the npm package from the setup script.

- Holds if the setup script can run npm.
- If wrong: synced-only users still lack an engine.

### B · Prebuilt binaries in the plugin

Ship six binaries in the plugin directory.

- Holds if plugin size (~20-35 MB) is acceptable.
- If wrong: heavy plugin for every user.

### C · Fallback build with Rust in the image

Rely on D6 cargo build; image must have Rust.

- Holds if cloud images carry Rust.
- If wrong: 1-3 min first SessionStart, or degraded mode.

## Evidence

- `docs/plans/temper-2-rust-port.md:867`: Install paths that may skip npm ... The Phase 0b manual verify-install step settles both.
- `PR #79 thread`: Greptile's 'Cowork probe' claim has no source in the repo; Greptile agreed

## Decision outcome

Chosen option: **Gate 0b on verify; setup-script install as fallback**, the recommended option.

Decider's note: Gate 0b on verify-install; setup-script install as fallback

### Consequences

- Good, because it holds if the setup script can run npm.
- Bad, because if wrong: synced-only users still lack an engine.

### Confirmation

Revisit when: verify-install showing the setup script cannot run npm

## Why this might be wrong

if synced installs never run any setup step

## Provenance

- Decided 2026-10-07T05:35:50Z by human:ryan (user-approved), via chat.
- Raised in session `bd5579cc-5d21-5bdf-9bc6-e4bca576d85f`.
- Verbatim: "kind: question"
