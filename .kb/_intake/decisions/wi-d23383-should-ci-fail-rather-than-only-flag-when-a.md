---
id: "kb-decision-wi-d23383"
title: "Flag only (as built)"
kind: decision
status: accepted
authority_tier: decision
summary: "In the context of \"Should CI fail, rather than only flag, when a pull request raises the committed lint baseline?\", facing agents cannot raise the baseline, but a human or a non-Claude tool can, and CI then only shows a banner before the merge goes through, we decided for Flag only (as built) and neglected Flag + CODEOWNERS line; Fail unless labelled, on the premise that the PR reviewer reads the comment, or the owner is the only reviewer, accepting that if wrong: a raise from a hurried human or a non-Claude agent merges with only a banner."
depends_on: []
related: []
source_paths: []
last_reviewed: "2026-09-29"
reversibility: high
phase: null
supersedes: []
superseded_by: null
weighin_item: "wi-d23383"
question: "Should CI fail, rather than only flag, when a pull request raises the committed lint baseline?"
door: two-way
blast_radius: external
decider: "human:ryan"
provenance: user-directed
decided_at: "2026-09-29T01:07:02Z"
recommended: "B"
flip_condition: "your adopting repos have no reviewer besides the owner, or no branch protection; then A"
---

# Flag only (as built)

## Context and problem statement

Should CI fail, rather than only flag, when a pull request raises the committed lint baseline?

Agents cannot raise the baseline, but a human or a non-Claude tool can, and CI then only shows a banner before the merge goes through.

Raised by an agent (sweep) as a call and captured by Weigh-In as `wi-d23383`. Anchor: `scripts/lib/gate.mjs:279`.

## Decision drivers

- Who approves a raise
- New CI code
- Cost on tighten PRs
- Works on solo repos

## Considered options

### A · Flag only (as built) (chosen)

Banner in the gate, PR comment and step summary; the merge is allowed.

- Holds if the PR reviewer reads the comment, or the owner is the only reviewer.
- If wrong: a raise from a hurried human or a non-Claude agent merges with only a banner.

### B · Flag + CODEOWNERS line

Setup asks for an owner handle and adds `.claude/temper-baseline.json @owner` to .github/CODEOWNERS; with 'require code-owner review' on, any baseline change needs that person.

- Holds if repos use GitHub branch protection with code-owner review.
- If wrong: without that setting the line does nothing; tighten PRs also need owner approval.

### C · Fail unless labelled

The gate job fails on a raise unless the PR carries a `temper-baseline-raise` label; needs pull-requests: read.

- Holds if only maintainers can add labels.
- If wrong: anyone with triage or write rights labels it themselves; red CI on every legitimate raise until labelled.

## Evidence

- `scripts/lib/gate.mjs:279`: const raised = all.raised.length ? [`BASELINE RAISED since ...` (shown in the summary; the verdict is unchanged)
- `scripts/lib/baseline.mjs:438`: let raised = []; ... const r = raisedSince(proj, diff.base, baseline); raised = r.raised;
- `CHANGELOG.md:28`: A raise is shown, never failed, so a human-approved `--update` still merges.
- `scripts/temper.mjs:303`: CLAUDECODE sessions need TEMPER_ALLOW_RAISE=1 for --init/--update; other agents (no CLAUDECODE) are not blocked by the CLI.

## Decision outcome

Chosen option: **Flag only (as built)**, overriding the recommendation (B).

Decider's note: stick with A. Flag only; document a CODEOWNERS recipe in the README for client or team repos that want a hard sign-off.

### Consequences

- Good, because it holds if the PR reviewer reads the comment, or the owner is the only reviewer.
- Bad, because if wrong: a raise from a hurried human or a non-Claude agent merges with only a banner.

### Confirmation

Revisit when: your adopting repos have no reviewer besides the owner, or no branch protection; then A

## Why this might be wrong

On solo repos B adds nothing over A, and it only works where an admin turns on code-owner review.

## Provenance

- Decided 2026-09-29T01:07:02Z by human:ryan (user-directed), via chat.
- Raised in session `a1b8d21a-0b93-5c31-8a7f-242ae801112f`.
- Verbatim: "Should CI fail, rather than only flag, when a pull request raises the committed lint baseline?"
