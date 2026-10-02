---
id: "kb-decision-wi-4a9bfe"
title: "Six targets"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"D5 · release targets: six (linux x64/arm64 musl static, macOS x64/arm64, Windows x64 and arm64), or five without Windows arm64?\", facing each target is a platform you promise prebuilt binaries for and must keep building every release, we decided for Six targets and neglected Five targets, on the premise that windows-11-arm runners are available to this repo, accepting that if wrong: one more job that can fail a release."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-10-02"
reversibility: low
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-4a9bfe"
question: "D5 · release targets: six (linux x64/arm64 musl static, macOS x64/arm64, Windows x64 and arm64), or five without Windows arm64?"
door: one-way
decider: "human:ryan"
provenance: user-approved
decided_at: "2026-10-02T04:05:57Z"
tree_hash: "0b0ea4ed6ed8fe369cd131c0d19421b7996a1786"
recommended: "A"
flip_condition: "If windows-11-arm runners are unavailable to this repo, B"
---

# Six targets

## Context and problem statement

D5 · release targets: six (linux x64/arm64 musl static, macOS x64/arm64, Windows x64 and arm64), or five without Windows arm64?

Each target is a platform you promise prebuilt binaries for and must keep building every release.

Raised by an agent (marker) as a question and captured by Weigh-In as `wi-4a9bfe`. Anchor: `docs/plans/temper-2-rust-port.md:59`.

## Decision drivers

- Release jobs
- Windows ARM first session

## Considered options

### A · Six targets (chosen)

Adds win32-arm64 on a windows-11-arm runner

- Holds if windows-11-arm runners are available to this repo.
- If wrong: one more job that can fail a release.

### B · Five targets

Windows arm64 users get the cargo fallback build

- Holds if few users are on Windows-on-ARM.
- If wrong: those users wait ~1 minute on first session.

## Evidence

- `docs/plans/temper-2-rust-port.md:60`: A) Six packages, including win32-arm64 ... built natively on a windows-11-arm runner.
- `docs/plans/temper-2-rust-port.md:61`: B) The five required targets: win32-arm64 users get the fallback build.
- `docs/plans/temper-2-rust-port.md:62`: Linux binaries are x86_64/aarch64-unknown-linux-musl, fully static

## Decision outcome

Chosen option: **Six targets**, the recommended option.

### Consequences

- Good, because it holds if windows-11-arm runners are available to this repo.
- Bad, because if wrong: one more job that can fail a release.

### Confirmation

Revisit when: If windows-11-arm runners are unavailable to this repo, B

## Why this might be wrong

Runner availability is unverified

## Provenance

- Decided 2026-10-02T04:05:57Z by human:ryan (user-approved), via chat.
- Raised in session `53e5cb21-eb65-4224-b36b-cc1add4e9121`.
- Verbatim: "kind: question"
