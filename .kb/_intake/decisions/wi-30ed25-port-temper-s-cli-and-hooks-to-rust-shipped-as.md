---
id: "kb-decision-wi-30ed25"
title: "Native binary via per-platform npm packages + cargo fallback"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Port temper's CLI and hooks to Rust, shipped as per-platform npm packages with a cargo-build fallback (temper 2.0)?\", facing temper's CLI and hooks are 279 KB of Node (plus a vendored copy); you want Rust, and the distribution method decides what users and CI must have, we decided for Native binary via per-platform npm packages + cargo fallback and neglected Build from source on first use only; Rust compiled to WebAssembly + Node I/O layer; Keep Node; record why, on the premise that a tiny Node launcher and npm publishing are acceptable, accepting that if wrong: release pipeline and npm org to maintain; one broken target fails a release, not users."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-30ed25"
question: "Port temper's CLI and hooks to Rust, shipped as per-platform npm packages with a cargo-build fallback (temper 2.0)?"
door: one-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T02:53:11Z"
tree_hash: "f9b7ef221dfd98fda59bb3be19728bb83eb956a9"
recommended: "A"
flip_condition: "If npm publishing or a Node launcher is unacceptable, choose B"
---

# Native binary via per-platform npm packages + cargo fallback

## Context and problem statement

Port temper's CLI and hooks to Rust, shipped as per-platform npm packages with a cargo-build fallback (temper 2.0)?

temper's CLI and hooks are 279 KB of Node (plus a vendored copy); you want Rust, and the distribution method decides what users and CI must have.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-30ed25`. Anchor: `hooks/hooks.json:1`.

## Decision drivers

- Build matrix
- First-session experience
- Rust share of the engine
- User repo CI

## Considered options

### A · Native binary via per-platform npm packages + cargo fallback (chosen)

Rust CLI; release CI builds 5 targets into @scope/temper-<os>-<cpu> packages; Claude Code's own npm install picks one; ~20-line Node launcher; cargo build into CLAUDE_PLUGIN_DATA when no prebuilt

- Holds if a tiny Node launcher and npm publishing are acceptable.
- If wrong: release pipeline and npm org to maintain; one broken target fails a release, not users.

### B · Build from source on first use only

Plugin ships the Rust source; SessionStart runs cargo build --release into CLAUDE_PLUGIN_DATA once per version

- Holds if a ~1 minute first-session compile is acceptable.
- If wrong: hooks cannot enforce until the build finishes; eval sandboxes that hide ~/.rustup break it.

### C · Rust compiled to WebAssembly + Node I/O layer

Scanner, baseline and report in Rust as one .wasm; Node keeps running cargo and git

- Holds if one artifact with no matrix matters more than a fully Rust codebase.
- If wrong: gate and hooks orchestration stay in JS.

### D · Keep Node; record why

Status quo; mark the vendored copy linguist-vendored so the language bar reads ~18% Rust

- Holds if the language mix is cosmetic.
- If wrong: contradicts your stated Rust preference.

## Evidence

- `hooks/hooks.json:8`: "command": "node", "args": ["${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs", ...]
- `README.md:70`: It has no dependencies, needs Node 18 or later ... /temper:setup also copies it into the repository at .claude/temper/
- `https://code.claude.com/docs/en/plugins/loading`: installs the plugin's Node.js package dependencies ... lockfile ... --ignore-scripts ... 60-second timeout
- `https://code.claude.com/docs/en/hooks`: check for the dependency on first use and install on miss ... ${CLAUDE_PLUGIN_DATA}

## Decision outcome

Chosen option: **Native binary via per-platform npm packages + cargo fallback**, the recommended option.

Decider's note: per the recommendation - A with B as the fallback

### Consequences

- Good, because it holds if a tiny Node launcher and npm publishing are acceptable.
- Bad, because if wrong: release pipeline and npm org to maintain; one broken target fails a release, not users.

### Confirmation

Revisit when: If npm publishing or a Node launcher is unacceptable, choose B

## Why this might be wrong

npm's platform-optional lockfile bug (npm/cli#4828) and per-target build breakage are new failure modes

## Provenance

- Decided 2026-10-02T02:53:11Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "kind: question"
