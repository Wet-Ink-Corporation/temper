# Design history

The working record behind each temper release: the specs the changes were built from, the independent reviews that checked them, and the source findings behind the 1.1 standard. Read these to see why something is the way it is. The README, CHANGELOG, `skills/` and the code are authoritative where they differ, because every review finding listed here was fixed or explicitly accepted before the release shipped.

These files were written for the agents doing the work, and they are kept as written, apart from one change: absolute paths in the build sandbox were rewritten. `/home/claude/temper/` became repository-relative, and the session scratch directory became `scratchpad/`. Scratch crates and probes that the notes mention under `scratchpad/` were not kept.

## 1.1: review against ten external Rust resources

[`1.1-resource-review/`](1.1-resource-review/)

| File | What it is |
|---|---|
| `findings/*.md` | One file per source: what rust-craft already covered, what to adopt, what to reject, and how each claim was verified on the pinned toolchain. [`skills/rust-craft/references/sources.md`](../../skills/rust-craft/references/sources.md) is the summary. |
| `adoption-spec.md` | The spec the implementing agents worked from, after the adoption decisions. |
| `review-content.md` | Independent review of the standard's content changes (rules, references, examples). |
| `review-machinery.md` | Independent review of the hooks, scanner, templates, CI and example workspace. |

The adoption spec names the findings files by the short names they had in the scratch directory. They map to `findings/` as: `ms.md` is `microsoft-pragmatic-rust.md`, `apiguide.md` is `api-guidelines.md`, `effective.md` is `effective-rust.md`, `perf.md` is `performance-book.md`, `patterns.md` is `design-patterns.md`, `atomics.md` is `atomics-and-locks.md`, `tooling.md` is `cargo-std-miri.md`, and `rustskills.md` is `rust-skills.md`. The probe directories the reviews cite (`scratchpad/review-content/`, `scratchpad/review-machinery/`, and `review-baseline/` in the 1.2 review) were not kept, so their repros are described but cannot be re-run from this repository.

## 1.2: lint baseline and compliance report

[`1.2-baseline-and-report/`](1.2-baseline-and-report/)

| File | What it is |
|---|---|
| `baseline-spec.md` | The spec for `temper baseline` and `temper report`, written after decision wi-cc1518. The design changed during implementation and review. The ratchet now keys on changed lines and per-crate counts, not on (file, lint) pairs. The README's Brownfield adoption section describes what shipped. Other differences from the spec, recorded in the 1.2.0 CHANGELOG entry, in `review.md`, or both: the pull-request comment runs in its own `report-comment` job, not as a gate-job step, and every checkout sets `persist-credentials: false` (review item 17); the baseline's `files` table was dropped (review item 23); the baseline also records the host platform and each crate's directory (review items 14 and 5); `baseline --init` and `--update` are refused inside a Claude Code session unless `TEMPER_ALLOW_RAISE=1` is set, where the spec had the hook only ask (review item 18); and the gate and CI flag a raised baseline rather than failing it (decision wi-d23383). |
| `review.md` | The independent review of 1.2.0 and its re-check: each finding, the evidence, and the fix. |

## 2026-10: status audit against the vision and plans

[`2026-10-audit/`](2026-10-audit/)

An independent audit of `main` at `d87d887` (1.3.0) against the README, [`v1-readiness.md`](../plans/v1-readiness.md) and [`temper-2-rust-port.md`](../plans/temper-2-rust-port.md), made on 2026-10-06. Fourteen audit areas produced evidence-cited findings, and a separate verifier re-checked each one: 199 were confirmed and 2 refuted. The pilot repositories were read but not changed.

| File | What it is |
|---|---|
| `executive-summary.html` | The non-technical summary: progress by milestone, what holds v1 back, owner actions, the realistic path and risks. |
| `report.md` | The full technical report: milestone tables, quality scorecard, every finding by area, effort roll-up, stale plan text. |
| `findings.html` | The report and a filterable ledger of every finding (open it in a browser). |
| `findings.json` | The verified and refuted findings as data. |
| `issues.md` | Where each actionable finding is tracked: the per-area issues #57 to #70, or an existing issue. |
| `ci.log` | The full `tests/ci.sh` output at `d87d887`, which the report cites. |
| `evidence-github.md` | The snapshot of GitHub state (issues, PRs, CI runs, rulesets) that the audit worked from. |

## Decisions

[`decisions.md`](decisions.md) lists every question raised through Weigh-In, from 1.1 through the setup of this repository: the owner's decisions, the defaults taken without the owner, and the dismissed items. The full records for the decisions are in [`.kb/_intake/decisions/`](../../.kb/_intake/decisions/).

## Evals

[`evals.md`](evals.md) records each behavioural eval run: scores with and without the plugin, run counts and cost.
