---
id: "kb-decision-wi-4abeef"
title: "Hybrid"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D6 · while the cargo fallback build is pending or has failed: hybrid (SessionStart builds synchronously with a 900 s timeout; Rust/Cargo.toml edits are denied with a retry message; other hooks pass with a notice; Stop reports `temper gate: UNAVAILABLE`), or pure fail-closed, or pure degrade?\", facing when no prebuilt binary resolves, temper either blocks agents until the cargo build finishes or lets them work unenforced, we decided for Hybrid and neglected Fail closed; Degrade, on the premise that a one-time wait of about a minute on rare platforms is acceptable, accepting that if wrong: the first session on such a platform starts slowly."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-4abeef"
question: "D6 · while the cargo fallback build is pending or has failed: hybrid (SessionStart builds synchronously with a 900 s timeout; Rust/Cargo.toml edits are denied with a retry message; other hooks pass with a notice; Stop reports `temper gate: UNAVAILABLE`), or pure fail-closed, or pure degrade?"
door: one-way
blast_radius: system
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:36:06Z"
tree_hash: "925ac261f5ed075cdc39044cc51fb53f62f097c3"
recommended: "C"
flip_condition: "If a first-session wait is unacceptable, B"
---

# Hybrid

## Context and problem statement

D6 · while the cargo fallback build is pending or has failed: hybrid (SessionStart builds synchronously with a 900 s timeout; Rust/Cargo.toml edits are denied with a retry message; other hooks pass with a notice; Stop reports `temper gate: UNAVAILABLE`), or pure fail-closed, or pure degrade?

When no prebuilt binary resolves, temper either blocks agents until the cargo build finishes or lets them work unenforced.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-4abeef`. Anchor: `docs/plans/temper-2-rust-port.md:64`.

## Decision drivers

- Rust edits while unbuilt
- Non-Rust work while unbuilt
- First session, no prebuilt

## Considered options

### A · Fail closed

Every enforcing hook blocks until the engine exists

- Holds if no unenforced edit is ever acceptable.
- If wrong: a failed build blocks all work, including non-Rust edits, until fixed.

### B · Degrade

One notice per session; everything allowed until the binary exists

- Holds if never blocking the user matters more than enforcement.
- If wrong: agents write Rust unchecked during the window.

### C · Hybrid (chosen)

Synchronous build at SessionStart; Rust edits denied with retry; other hooks pass; Stop reports UNAVAILABLE

- Holds if a one-time wait of about a minute on rare platforms is acceptable.
- If wrong: the first session on such a platform starts slowly.

## Evidence

- `docs/plans/temper-2-rust-port.md:65`: The fallback is needed only when no prebuilt binary resolves: an unsupported platform, a failed or skipped npm install, or --plugin-dir
- `docs/plans/temper-2-rust-port.md:66`: A) Fail closed. Every enforcing hook denies or blocks with "temper's engine is not available: <reason>".
- `docs/plans/temper-2-rust-port.md:67`: B) Degrade (fail open). Each session gets one notice: "temper is not enforcing". Everything is allowed.
- `docs/plans/temper-2-rust-port.md:69`: C) SessionStart builds synchronously when no binary exists (timeout 30 -> 900 s).

## Decision outcome

Chosen option: **Hybrid**, the recommended option.

### Consequences

- Good, because it holds if a one-time wait of about a minute on rare platforms is acceptable.
- Bad, because if wrong: the first session on such a platform starts slowly.

### Confirmation

Revisit when: If a first-session wait is unacceptable, B

## Why this might be wrong

A 900 s SessionStart may feel hung if the build is slow

## Provenance

- Decided 2026-10-02T04:36:06Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "kind: question"
