# Contributing to temper

temper is developed by Wet Ink Corporation and is internal for now. [`AGENTS.md`](AGENTS.md) is the working guide for this repository: its layout, the commands, the rules, and the release checklist. Read it first. It applies to people and coding agents alike.

## How a change lands

1. Open an issue, or pick one from the `v1 live` or `2.0` milestone. Use the templates: bug, false positive or negative, or rule proposal.
2. Work on a branch. Run `bash tests/ci.sh` before pushing; it is what CI runs.
3. Open a pull request. The template's checklist covers the vendored copy, the rule map, the CHANGELOG and decisions.
4. CI must be green: `Checks` and `Unit tests (Windows)` are required on `main`.
5. Merging to `main` ships to marketplace users, so merge only what is ready for them.

## Changing the standard

A rule change is a change in behaviour for every repository that uses temper.
- A new or tightened MUST rule can make existing code fail. Raise it as an issue with the rule-proposal template before writing it.
- Rule IDs are permanent: never renumber or reuse one.
- Every example must compile and lint under the template lints (`tests/doc_examples.py`).

## Decisions

Questions that belong to the owner (new dependencies, crate boundaries, lint exceptions, anything H-16 lists) are raised through Weigh-In and recorded in `.kb/_intake/decisions/` and [`docs/history/decisions.md`](docs/history/decisions.md).

## Versioning

temper follows semantic versioning. Which changes count as major, minor or patch for a plugin that enforces a standard is defined in plan step 3.2 ([`docs/plans/v1-readiness.md`](docs/plans/v1-readiness.md)). Until then, ask the owner on the PR.
