# Decision log

This page lists every question raised through [Weigh-In](https://github.com/Wet-Ink-Corporation/weigh-in) while temper 1.1 and 1.2 were built and while this repository was set up. That covers questions an agent raised, assumptions it made, deferrals and its own calls. Each one was decided by the owner, defaulted as a reversible call, or dismissed. Anchors were rewritten to be relative to this repository. Line numbers are as they were when the item was raised.

## Decided by the owner

Each decision has a full record (context, options, criteria, recommendation) in [`.kb/_intake/decisions/`](../../.kb/_intake/decisions/).

| Item | Question | Decision |
|---|---|---|
| [wi-e4c765](../../.kb/_intake/decisions/wi-e4c765-should-non-exhaustive-apply-only-at-published.md) | Should `#[non_exhaustive]` apply only at published crate boundaries, keeping workspace-internal enums (errors, Decider events) exhaustive? | Published crates only (API-05). |
| [wi-cc1518](../../.kb/_intake/decisions/wi-cc1518-should-i-build-temper-baseline-a-committed.md) | Should temper build a committed lint baseline used by the gate and CI, plus a per-crate compliance report, and correct the README's brownfield claim? | Build both (1.2.0). |
| [wi-d23383](../../.kb/_intake/decisions/wi-d23383-should-ci-fail-rather-than-only-flag-when-a.md) | Should CI fail, rather than only flag, when a pull request raises the committed lint baseline? | Flag only. The README gives a CODEOWNERS recipe for repositories that want a hard sign-off (1.2.1). B (flag plus CODEOWNERS) was chosen first, then reversed after a comparison: CODEOWNERS also gates `--tighten`, and it deadlocks repositories with a single owner. |
| [wi-d75e63](../../.kb/_intake/decisions/wi-d75e63-should-the-temper-repository-itself-be-the-wet.md) | Should this repository be the `wet-ink` marketplace, or should that name be kept for a marketplace of all Wet Ink plugins? | Rename this repository's marketplace to `temper` (`temper@temper`, 1.2.2), so `wet-ink` stays free. |
| [wi-96caeb](../../.kb/_intake/decisions/wi-96caeb-should-docs-history-the-agents-working-notes.md) | Should `docs/history/`, which includes a few verbatim sentences quoted from the source books, stay in the repository pushed to GitHub? | Keep all of it. The repository was private when this was decided. |

## Defaults taken without the owner

Each of these is a two-way door. An agent took the default shown, recorded why, and left it for review. Reopen any of them by changing the anchor and noting it in the CHANGELOG.

| Item | Question | Default | Why | Anchor |
|---|---|---|---|---|
| wi-7a61c4 | Apply the adopted guidance to the plugin (as 1.1.0) rather than only listing it for review? | Applied as 1.1.0 | 1.0.0 stays recoverable from git, and building the complete model first is the owner's stated preference. | `CHANGELOG.md:3` |
| wi-2909ae | Keep `clippy::empty_enum_variants_with_brackets` out of the template lints? | Kept out | The lint forbids braces on unit variants, which would silently disable `deny_unknown_fields`. | `templates/example-workspace/crates/ledger-store-jsonl/src/record.rs:69` |
| wi-a9eb36 | Should Dependabot use `versioning-strategy: lockfile-only` for cargo? | `lockfile-only` | It is safe whatever the result; check Insights > Dependabot in the first repository that adopts the template. | `templates/workspace/.github/dependabot.yml:12` |
| wi-efdd4d | Keep the example CLI's exit codes 1, 4 and 5 as a deviation from the house convention, recorded in the charter? | Kept, with the charter record | Code 4 tells a script it is safe to retry; changing it would break a tested contract. | `templates/example-workspace/crates/ledger-cli/src/main.rs:7` |
| wi-24dd25 | Should ERR-13's fallback `Drop` only log that `finish` was skipped, doing no flush or other blocking work? | `Drop` only logs | It follows "Drop never blocks" (H-14, C-DTOR-BLOCK); `finish(self)` is the only place a failure is visible. | `skills/rust-craft/references/rules.md:100` |
| wi-600bb1 | Should CRF-11 allow present-tense `//` why-comments, while keeping history and rule tables out of docs? | Allowed | Why-comments are already CRF-04; history and rule tables stay in plans. | `skills/rust-craft/references/rules.md:235` |
| wi-5a2b0a | Should the template enable the new nursery and restriction lints at warn, which the gate denies? | Enabled | Probed on the example workspace and the 329 reference blocks with no false positives. | `templates/workspace/Cargo.toml:60` |
| wi-20fd28 | Should `clippy.toml` disallow `thread::spawn` and `Scope::spawn` in tests too? | Disallowed everywhere | `disallowed-methods` has no test exemption, and the scanner enforces the same rule. | `templates/workspace/clippy.toml:18` |
| wi-39b3a1 | Are the doc-example harness's lint relaxations acceptable? | Kept as documented | They are narrower than the spec allowed; every other template lint applies. | `tests/doc_examples.py:52` |
| wi-cbded1 | When should the behavioural evals be re-run for 1.1.0? | Before the first push to GitHub | The smoke test covers the main loop; the evals confirm the gap between running with and without the plugin. Done: run on 1.2.1 on 2026-10-01, before the first push ([evals.md](evals.md)). | `evals/adds-function-under-standard/case.yaml:1` |
| wi-fbd494 | Should zero-findings mode stop scanning `.rs` files that no crate compiles? | Keep scanning every `.rs` file under a crate directory | Reachability through the module tree misses `#[path]`, `include!` and macro-generated `mod` items. A stray file fails with its path, and the fix is obvious. | `scripts/lib/findings.mjs:215` |
| wi-cd9821 | Should `baseline --tighten` refuse, rather than warn, on a host other than the one the baseline records? | Refuse unless `--any-host` | Counts lowered on another platform can fail CI on lines nobody touched. | `scripts/temper.mjs:303` |
| wi-06d675 | Should code moved from one place to another count as new code for the ratchet? | Moved lines count as new | Exact-match move detection would let a deleted `unwrap` line reappear elsewhere unchecked. File and crate renames are recognised. | `CHANGELOG.md:17` |
| wi-d26284 | Should 1.2.0 ship without re-running the evals? | Shipped without them | 1.2 changed three passages of skill text, and the smoke session ran the changed gate path green. The evals have since run on 1.2.1 ([evals.md](evals.md)). | `.claude-plugin/plugin.json:4` |
| wi-12f51b | Should the commits that seed this repository be authored as ryan.britton@wet-ink.net rather than the GitHub noreply address? | ryan.britton@wet-ink.net | Every earlier temper commit and `plugin.json` use it. If GitHub shows the commits as unlinked, add the address to the account rather than rewriting history. | `.claude-plugin/plugin.json:6` |

## Dismissed

| Item | Question | Why |
|---|---|---|
| wi-fd7154 | Is the new plugin and marketplace description wording acceptable? | Not useful: descriptive wording, and no behaviour depends on it. |
| wi-a61c52 | Should the LICENSE name Wet Ink Corporation rather than "Ryan Britton / Wet Ink Corporation"? | Already answered: the owner wrote this LICENSE when creating the repository (initial commit 7429197). |
