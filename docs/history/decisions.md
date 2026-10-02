# Decision log

This page lists every question raised through [Weigh-In](https://github.com/Wet-Ink-Corporation/weigh-in) while temper 1.1 and 1.2 were built, while this repository was set up, and while temper 2.0 (the Rust port, [`docs/plans/temper-2-rust-port.md`](../plans/temper-2-rust-port.md)) was planned. That covers questions an agent raised, assumptions it made, deferrals and its own calls. Each one was decided by the owner, defaulted as a reversible call, or dismissed. Anchors were rewritten to be relative to this repository. Line numbers are as they were when the item was raised.

## Decided by the owner

Each decision has a full record (context, options, criteria, recommendation) in [`.kb/_intake/decisions/`](../../.kb/_intake/decisions/).

| Item | Question | Decision |
|---|---|---|
| [wi-e4c765](../../.kb/_intake/decisions/wi-e4c765-should-non-exhaustive-apply-only-at-published.md) | Should `#[non_exhaustive]` apply only at published crate boundaries, keeping workspace-internal enums (errors, Decider events) exhaustive? | Published crates only (API-05). |
| [wi-cc1518](../../.kb/_intake/decisions/wi-cc1518-should-i-build-temper-baseline-a-committed.md) | Should temper build a committed lint baseline used by the gate and CI, plus a per-crate compliance report, and correct the README's brownfield claim? | Build both (1.2.0). |
| [wi-d23383](../../.kb/_intake/decisions/wi-d23383-should-ci-fail-rather-than-only-flag-when-a.md) | Should CI fail, rather than only flag, when a pull request raises the committed lint baseline? | Flag only. The README gives a CODEOWNERS recipe for repositories that want a hard sign-off (1.2.1). B (flag plus CODEOWNERS) was chosen first, then reversed after a comparison: CODEOWNERS also gates `--tighten`, and it deadlocks repositories with a single owner. |
| [wi-d75e63](../../.kb/_intake/decisions/wi-d75e63-should-the-temper-repository-itself-be-the-wet.md) | Should this repository be the `wet-ink` marketplace, or should that name be kept for a marketplace of all Wet Ink plugins? | Rename this repository's marketplace to `temper` (`temper@temper`, 1.2.2), so `wet-ink` stays free. |
| [wi-96caeb](../../.kb/_intake/decisions/wi-96caeb-should-docs-history-the-agents-working-notes.md) | Should `docs/history/`, which includes a few verbatim sentences quoted from the source books, stay in the repository pushed to GitHub? | Keep all of it. The repository was private when this was decided. |
| [wi-30ed25](../../.kb/_intake/decisions/wi-30ed25-port-temper-s-cli-and-hooks-to-rust-shipped-as.md) | Should temper's CLI and hooks be ported to Rust (temper 2.0), and how should it be shipped? | A Rust engine shipped as per-platform npm packages, with a `cargo build` fallback and a small Node launcher. |
| [wi-836680](../../.kb/_intake/decisions/wi-836680-d1-npm-scope-and-names-publish-the-binaries-as.md) | D1: what npm scope and package names should the binaries use? | `@wet-ink/temper-<platform>-<arch>`, six packages. The plugin-root `package.json` stays private. |
| [wi-222f58](../../.kb/_intake/decisions/wi-222f58-d2-crates-io-publish-four-crates-temper-cli.md) | D2: should the crates be published to crates.io, and should the names be reserved now? | Publish four crates, versioned in lockstep. The names were reserved as `0.0.0` placeholders on 2026-10-02. |
| [wi-0b4dc2](../../.kb/_intake/decisions/wi-0b4dc2-d3-split-the-engine-into-four-crates-temper.md) | D3: how should the engine be split into crates? | Four crates (`temper-domain`, `temper-app`, `temper-host`, `temper-cli`) plus an unpublished `xtask`. |
| [wi-5fc813](../../.kb/_intake/decisions/wi-5fc813-d4-msrv-1-96-matching-the-template-or-1-88-1-85.md) | D4: what MSRV should the crates have? | 1.96, matching the workspace template. |
| [wi-4a9bfe](../../.kb/_intake/decisions/wi-4a9bfe-d5-release-targets-six-linux-x64-arm64-musl.md) | D5: which release targets? | Six: Linux x64 and arm64 (musl), macOS x64 and arm64, Windows x64 and arm64. |
| [wi-4abeef](../../.kb/_intake/decisions/wi-4abeef-d6-while-the-cargo-fallback-build-is-pending-or.md) | D6: how should hooks behave while the fallback build is pending or has failed? | The hybrid: SessionStart builds synchronously; Rust edits are denied with a retry message; other hooks pass; the gate reports UNAVAILABLE. |
| [wi-b03273](../../.kb/_intake/decisions/wi-b03273-d10-approve-the-new-dependencies-regex-toml.md) | D10: which new dependencies are approved? | `regex`, `toml` (with `preserve_order`), `sha2` and `wait-timeout`. Not clap, walkdir, globset, jiff/time or dunce. |
| [wi-bf7535](../../.kb/_intake/decisions/wi-bf7535-d11-node-deprecation-node-only-during.md) | D11: how long does the Node engine stay? | Only on `main` during the port, as an opt-in delegate. 2.0.0 ships Rust only. |
| [wi-6aac03](../../.kb/_intake/decisions/wi-6aac03-d12-publishing-credentials-oidc-trusted.md) | D12: what credentials does publishing use? | OIDC trusted publishing to npm and crates.io, from a protected `release` environment that needs the owner's approval. |
| [wi-37c1df](../../.kb/_intake/decisions/wi-37c1df-d13-have-user-ci-install-temper-via-taiki-e.md) | D13: how does user CI install temper? | `taiki-e/install-action` (cargo-binstall from the GitHub release). |
| [wi-3b5428](../../.kb/_intake/decisions/wi-3b5428-d14-pin-the-temper-version-in-user-repos-with-a.md) | D14: where does a user repository pin its temper version? | A one-line file, `.claude/temper-version`. |
| [wi-cdcecc](../../.kb/_intake/decisions/wi-cdcecc-d15-migrate-1-x-repos-with-temper-setup-migrate.md) | D15: how are 1.x repositories migrated? | `temper setup --migrate`, which rewrites only files whose hash matches a shipped 1.x template, and prints manual steps for the rest. |
| [wi-348e0b](../../.kb/_intake/decisions/wi-348e0b-d16-approve-as-one-decision-the-intentional-1-x.md) | D16: approve, as one decision, the intentional 1.x behaviour changes and the retired tests? | Approved: the ten divergences in the port plan's §2.4, and seven retired tests. |
| [wi-1d4cd6](../../.kb/_intake/decisions/wi-1d4cd6-d17-have-sessionstart-export-temper-bin-and.md) | D17: should SessionStart put the temper binary on `PATH`? | Yes: export `TEMPER_BIN` and prepend its directory through `CLAUDE_ENV_FILE`. |
| [wi-4679b3](../../.kb/_intake/decisions/wi-4679b3-license-the-new-crates-mit-matching-plugin-json.md) | What licence do the new crates use: MIT, or the template's MIT OR Apache-2.0? | `MIT OR Apache-2.0`, overriding the MIT recommendation. The plugin itself stays MIT. |

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
| wi-f22cef | D7: cargo-dist, or a hand-written release workflow? | A hand-written `release.yml` plus `cargo xtask` | cargo-dist's npm installer downloads at run time, so the npm packaging and checks would be ours anyway. A hand-written workflow keeps the supply chain small. | `docs/plans/temper-2-rust-port.md:79` |
| wi-2f31ee | D8: should the scanner use syn, rustc's lexer, or a port of the masking lexer? | Port `lexer.mjs` and the 56 rules faithfully | The scanner must handle code that does not parse, and a faithful port can be compared byte for byte with 1.x. `ra_ap_rustc_lexer` could follow in 2.x. | `docs/plans/temper-2-rust-port.md:84` |
| wi-e9386f | D9: which regex engine? | `regex`, with explicit predicates for the ~10 look-around patterns | Linear time, so agent-written input cannot cause ReDoS (H-13). `fancy-regex` is the fallback. | `docs/plans/temper-2-rust-port.md:91` |

## Dismissed

| Item | Question | Why |
|---|---|---|
| wi-fd7154 | Is the new plugin and marketplace description wording acceptable? | Not useful: descriptive wording, and no behaviour depends on it. |
| wi-a61c52 | Should the LICENSE name Wet Ink Corporation rather than "Ryan Britton / Wet Ink Corporation"? | Already answered: the owner wrote this LICENSE when creating the repository (initial commit 7429197). |

## Decided outside Weigh-In

Decisions the owner made directly while working through [`docs/plans/v1-readiness.md`](../plans/v1-readiness.md). That plan records the context and options for each.

| Step | Question | Decision |
|---|---|---|
| 0.7 | Who is v1 for, and what visibility does the repository have? | Internal for now: Wet Ink org members. Revisit before any public release. |
| 0.9 | Which runners build the arm64 targets? | GitHub's standard arm64 runners, confirmed for this repository with `runner-probe.yml`. |
| 0.9 | How much CI does the port run? | Tiered: Linux on every PR; Windows and macOS on PRs that touch code and on `main`; the corpus scan weekly and on `release/*`; the release matrix on `release/*` only. Plus Actions budget alerts. |
