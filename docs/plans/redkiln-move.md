# Moving temper into redkiln: the import manifest

temper moves into the `Wet-Ink-Corporation/redkiln` monorepo as Redkiln Rust (install id `rk-rust`, 2.0). The redkiln agent does the move, following a runbook it writes from redkiln's `docs/plan/move-1-weigh-in.md` template. This page gives that agent what only temper knows: where each path goes, what stays behind, what the redkiln root must provide, and the state imported once. It was written on 2026-11-08 at temper `main` after #211, when the port paused after Phase 3 (wi-14cf95).

## Decided

| Call | Decision | Record |
|---|---|---|
| Where the port lands | The history-preserving import of temper `main`: one merge commit whose second parent is temper's tip, unchanged, with files placed by `git read-tree --prefix`. The port's crates come as they are after Phase 3; the remaining phases are built in redkiln on Core 1.0, after Move 2 | wi-187aa7, wi-14cf95 |
| Crate split | The language-neutral half moves to the kits: the completion gate, verdict validation, fingerprint, baseline ratchet and gate cache to `redkiln-craft`; the shell reader and protected globs to `redkiln-guard`. The scanner, lexer, manifest rules and cargo steps stay in Redkiln Rust's crates | wi-59cb6e (amends D3) |
| Install story | The suite's `libexec/` launcher replaces D13 (install-action), D14 (`.claude/temper-version`) and D17 (binary on PATH). `TEMPER_BIN` stays only if the lens plugin still needs it | wi-12d8ef |
| Old state | Invariant 3 is narrowed: 2.0 reads 1.x state until it imports that state once into the shared log; after that the Core store is the source of truth | wi-f4c3ac |

The other port-plan clashes (D1, D2, D4, D10, the telemetry branch, #193 and whether #172/#173 are built in 1.x) are open Weigh-In questions (wi-02195d, wi-0eea27, wi-a2240a, wi-fa59d1, wi-b1bfb5, wi-39f19a, wi-0924e6). `docs/history/decisions.md` lists each once it is answered.

## Placement

`<pkg>` below is `rust`, following redkiln's `references/docs/rust.md` and the install id `rk-rust`. The redkiln runbook may choose another folder name. The plugin root is temper's git root today, so the plugin's files are listed one by one.

| From (temper) | To (redkiln) | Notes |
|---|---|---|
| `crates/temper-domain/`, `crates/temper-app/`, `crates/temper-host/`, `crates/temper-cli/` | `crates/rust/<crate>/` | As they are (wi-187aa7). The kit-bound modules move to `redkiln-craft` and `redkiln-guard` later (wi-59cb6e); each crate's `AGENTS.md` names them. Each crate keeps its licence copies (`cargo xtask check-licenses`) |
| `xtask/` | Not placed as a crate | Its tasks (`check-licenses`, `versions`, `check-npm-lock`, `npm-pack`) become redkiln `cargo xtask` tasks or are dropped with npm (D1, wi-02195d). Reachable through the second parent |
| `skills/`, `agents/`, `hooks/`, `.claude-plugin/plugin.json` | `plugins/rust/` | The plugin. `hooks/hooks.json` runs `node scripts/temper.mjs hook <event>` until the 2.0 cutover, then `${CLAUDE_PLUGIN_ROOT}/libexec/temper hook <event>` |
| `skills/rust-craft/references/rules.md` and its sources | `plugins/rust/skills/rust-craft/references/` | Unchanged. Rule IDs are permanent and never renumbered |
| `templates/workspace/`, `templates/crate/` | `plugins/rust/templates/` | What `/temper:setup` installs |
| `templates/example-workspace/` | `plugins/rust/templates/example-workspace/` without its `.claude/temper/` | The vendored copy is not imported (below). The example's gate runs the root's engine instead |
| `scripts/temper.mjs`, `scripts/lib/`, `scripts/launch.mjs`, `scripts/fallback.mjs` | `tools/rust/engine-1x/` | The 1.x Node engine stays as the parity oracle until the cutover deletes it (D11). It is a script, so it cannot sit under `plugins/` (redkiln's "CLI first" rule). The 1.x plugin keeps shipping from temper until then (below) |
| `plugins/temper-lens/` | `plugins/rust-lens/` | Its own plugin, because a mod switches off the whole plugin in desktop chats. It gets a launcher copy as `rk-weigh-in-mod` did (Move 1, M1-C). Its vendored `scripts/` and `references/rules.md` copies are not imported |
| `tests/*.test.mjs`, `tests/fixtures/` | `tools/rust/engine-1x/tests/` | The Node tests, the oracle's own |
| `tests/parity/` | `tools/rust/parity/` | The differential harness (1.x against Rust). Deleted at the cutover (`tests/parity/README.md`) |
| `tests/registry.py`, `lint-claims.py`, `gen_rulemap.py`, `doc_examples.py`, `lens_allowlist.py` | `tools/rust/` | Checks over the standard. They join redkiln's `ci.yml` under `ok` |
| `tests/e2e/` | `tools/rust/e2e/` | `smoke.sh` costs money: it runs only with Ryan's go-ahead |
| `tests/ci.sh`, `tests/release-check.sh`, `tests/lens-check.sh` | Not placed | Their steps are rewritten into redkiln's `ci.yml` and release tasks |
| `evals/` | `evals/rust/` | Not shipped. Paid to run |
| `docs/` | `docs/rust/` | `docs/plans/`, `docs/history/` (specs, reviews, findings, `evals.md`) and `docs/research/` keep their paths below it. `docs/troubleshooting.md` ships with the plugin's README links, so the runbook decides whether it goes to `plugins/rust/` instead |
| `docs/history/decisions.md` | `docs/rust/decisions.md` | The decisions log. wi- and D- records keep their ids |
| `.kb/_intake/decisions/` (81 atoms) | `.kb/_intake/decisions/` | Atoms move once and keep their ids |
| `AGENTS.md` | `docs/rust/charter.md` | Its repository-only parts (the vendored-copy steps, the ruleset, the release checklist, the CI commands) are superseded by the root charter, as its opening note says |
| `.claude/CLAUDE.md` | `docs/rust/working-rules.md` | Today it holds only `@../AGENTS.md`, so at its new path it says nothing; the runbook can drop it or keep it as a pointer to the charter |
| `CHANGELOG.md`, `README.md`, `SECURITY.md`, `CONTRIBUTING.md` | `plugins/rust/` | Per-package changelog; 2.0.0's section goes on top |
| `LICENSE` (MIT), `LICENSE-APACHE` | `plugins/rust/` for the plugin's MIT `LICENSE` | The crates keep their own copies. The root's licences govern the rest |

## Not imported

All of it stays reachable through the import's second parent, and in the archived temper repository.

| Path | Why |
|---|---|
| Tags (`v1.1.0` to `v1.6.0`, `v2.0.0-alpha.2`) | Tags are not imported; redkiln tags `rk-rust--v<version>` |
| `.github/` (`ci.yml`, `changelog.yml`, `corpus.yml`, `release.yml`, `release-2.yml`, `runner-probe.yml`, the PR template, Dependabot) | Rewritten into redkiln's `ci.yml` under `ok` and its release workflow |
| `templates/example-workspace/.claude/temper/`, `plugins/temper-lens/scripts/`, `plugins/temper-lens/references/rules.md` | Vendored copies. redkiln's root runs temper vendored once; legacy findings are re-baselined at the new paths |
| `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `clippy.toml`, `rustfmt.toml`, `deny.toml`, `.cargo/` | The root's copies govern. Anything below that the root lacks is a decision for Ryan |
| `.claude-plugin/marketplace.json` | The suite's release repository assembles the marketplace |
| `.claude/settings.json`, `.claude/hooks/session-start.sh` | Cloud-session setup for this repository. redkiln has its own |
| `docs/plans/reserve-temper-crates.sh` | A packaging script; crates are not published from redkiln (D2, wi-0eea27) |
| `.gitattributes`, `.gitignore`, `.editorconfig` | Diffed against the root's. temper's LF rule matches redkiln's |
| The `temper-telemetry` branch, `refs/temper/telemetry-tip` | Replaced by the SPEC's journals (wi-b1bfb5) |

## What the redkiln root must provide

The port's crates build today under temper's root. Diff each item against redkiln's root in the import PR; whatever is missing is an H-16 or H-05 call, not a silent merge.

| temper root has | Used by | redkiln |
|---|---|---|
| `[workspace.dependencies]`: `regex` 1.13 (`std`, `perf`, `unicode`), `toml` 1.1 (`std`, `parse`, `preserve_order`), `sha2` 0.11, `wait-timeout` 0.2.1, `serde` 1.0 (`std`, no `derive`), `serde_json` 1.0 (`std`, `float_roundtrip`) | The port's crates (D10, wi-b03273, and the 2026-11-01 and 2026-11-02 rows of `AGENTS.md`'s decisions log) | At redkiln `6bfe6c5`: `toml` **0.9.12** (`std`, `parse`, `serde`), `sha2` **0.10**, `serde` with `derive`, `serde_json` without `float_roundtrip`. Missing: `regex`, `wait-timeout`. Two version splits (`toml` 1.1 against 0.9, `sha2` 0.11 against 0.10) mean either the port steps down or the root steps up, which is an H-16 call. `float_roundtrip` matters, because the fingerprint and verdict parity cases depend on it, and Cargo unifies features, so turning it on changes it for every crate that uses `serde_json` |
| `[workspace.lints]` from the template | Every crate | Same template (temper 1.4.0's), so expect no gap; check for lints added in 1.5.0 and 1.6.0 |
| `rust-version = "1.96"` | D4 | 1.98.1, the root's single pin (wi-a2240a) |
| `clippy.toml`, `deny.toml` | The gate | redkiln's are temper 1.4.0's; diff against temper 1.6.0's template |
| `exclude` of `templates/*` | Keeps the templates' own workspaces out | Needs the same `exclude` for `plugins/rust/templates/` |
| CI on Linux, Windows and macOS for every code-touching change (wi-e85ef8) | The engine's path and process code | redkiln's `ci.yml` jobs |

## State imported once

Read by 2.0 until imported, then never written again and never deleted (wi-f4c3ac, redkiln ST-15 and ST-16).

| 1.x state | Becomes |
|---|---|
| `<git-common-dir>/temper/gate/<fp>.json` | `rust.gate_run` events (`docs/contracts/rust-events.md`) |
| `<git-common-dir>/temper/verdicts/<fp>.json` | `rust.verdict` events |
| `<git-common-dir>/temper/telemetry.jsonl` and `.old` | `rust.*` telemetry, within the 30-day window |
| `${CLAUDE_PLUGIN_DATA}/sessions/`, skill markers | Nothing: per-session projections are rebuilt |
| `.claude/temper.json`, `.claude/temper-baseline.json` | Committed config, moved to `.redkiln/rust/` at the cutover (rust.md call E). Read from the old path as a fallback through 2.x |

## While the move waits

The move waits for Core 1.0 (after Move 2). Until then:
- temper keeps shipping 1.x from this repository. The last 1.x release prints a SessionStart notice naming `rk-rust`, and Ryan schedules it.
- redkiln vendors temper 1.4.0 at its root. Bumping that copy to a later 1.x release is the redkiln side's change.
- Changes go to `main` through PRs with green CI, as `AGENTS.md` says. Nothing on `main` is squashed, rebased or force-pushed, because the import cites its hashes.
- Open issues are triaged (keep, close) before the move, and the keepers transfer to redkiln when temper is archived. Ryan does the transfer and the archive.
