# temper's facts and telemetry as `rust.*` events

This page maps what temper 1.x records to the `rust.*` events it will write in redkiln's shared log once it moves there as Redkiln Rust (`rk-rust` 2.0). The event rules are redkiln's SPEC: types are `<namespace>.<name>` with names in `[a-z][a-z0-9_]*` (EV-03), and each type is either **durable** or **telemetry** (EV-14). A durable event is a fact that a decision, gate or record depends on. It is written under the busy timeout, exported to journals and never pruned. A telemetry event is dropped when the store is busy, never exported or committed, kept for 30 days, and leaves the machine only as aggregates. Telemetry lives in Core's side store (ST-09, `events.sqlite3.telemetry`).

The plugin document `references/docs/rust.md` in redkiln (frozen 2026-10-19) sets the split: gate runs and verdicts are durable, and the rest is telemetry. This page fills in the names and fields. The redkiln side defines the types in Redkiln Rust's crates (EV-04), so treat this page as their starting list, not as a contract that is already in force.

Nothing here changes 1.x. temper 1.x keeps writing `<state dir>/telemetry.jsonl` and pushing the `temper-telemetry` branch until the 2.0 cutover. The SPEC's journals replace that branch (`spec/journals.md`), and the branch is not imported.

## Durable facts

These come from state files, not from `telemetry.jsonl`.

| 1.x source | Event | Payload, from the 1.x fields | Notes |
|---|---|---|---|
| `<state dir>/gate/<fingerprint>.json` and the gate log | `rust.gate_run` | `fp`, `ok`, `at`, `steps` (each step's name, result and time) | Binds to the change fingerprint, which Core reads as `rust-fingerprint` (redkiln ID-10) and Redkiln Rust computes bit for bit as 1.x does (port Invariant 2). The per-fingerprint gate cache becomes a projection of these events |
| `<state dir>/verdicts/<fingerprint>.json` (`temper_verdict: 1`) | `rust.verdict` | decider agent id, fingerprint, verdict, findings (review-contract's schema), rubric scores, `kept` | Trust `asserted`: it comes from a subagent. Cites the `rust.gate_run` it was given (EV-15). The 1.x telemetry row `verdict` folds into this event and gets no telemetry type of its own |

## Telemetry

The 1.x code calls `telemetry(proj, input, '<name>', detail)` (`scripts/lib/project.mjs`) under 25 names. redkiln's briefing (`references/docs/rust.md`) counts "22 telemetry names"; that count leaves out the scan's own `deny-scan` and `ask` and the manifest scan's `deny-manifest`, and this table is the full list. Every row carries `t`, `sid`, `ccr` (cloud session), `agent` and `event`. In the envelope, those become `ts`, the `session` tag, a `ccr` field and an `agent` field, and the type itself.

Each name maps one to one, with a `rust.` prefix, kebab case turned to snake case, and verbs in the past tense where the 1.x name is a noun phrase. This keeps `temper stats` and its 2.0 successor counting the same things. Collapsing the six `stop-*` names into one `rust.stop` type with an `outcome` field would read better. It would also change every stats query, so it is left to the redkiln side to decide when it defines the types.

| 1.x name | Event | 1.x payload fields |
|---|---|---|
| `session-start` | `rust.session_started` | `source` |
| `skill-loaded` | `rust.skill_loaded` | `via` |
| `subagent-start` | `rust.subagent_started` | `type` |
| `subagent-stop` | `rust.subagent_stopped` | `type`, `msg`, `transcript` |
| `handback` | `rust.handback` | `type`, `reviewer` |
| `verdict` | folded into durable `rust.verdict` | `via`, `agent_id`, `verdict`, `fp`, `stale`, `findings`, `kept` |
| `verdict-invalid` | `rust.verdict_invalid` | `via`, `problems`, `kinds`, `agent_id` |
| `verdict-invalid-final` | `rust.verdict_invalid_final` | `problems`, `kinds`, `agent_id` (as `verdict-invalid` without `via`), on the retry that ends the turn |
| `gate-start` | `rust.gate_started` | `pid`, `agent_id`, `timeoutMs`, `afterMs?`, `tree?` |
| `gate-lost` | `rust.gate_lost` | `pid`, `reason` (`timeout`, `interrupted`, `unknown`), `ranMs`, `foundMs`, `timeoutMs`, `heartbeat`, `progress`, `step`, `steps` (`lostGateRow`; gate-lost detection reads `gate-start`'s `pid`, `timeoutMs` and `afterMs`) |
| `gate-unavailable` | `rust.gate_unavailable` | timing: `ms`, `cached`, `pid`, step times, `tree?` |
| `stop-green` | `rust.stop_green` | `fp`, timing |
| `stop-blocked` | `rust.stop_blocked` | `reason` (`gate`, `pending`, `changes-requested`, `no-review`), `fp`, timing or `tree` |
| `stop-declared` | `rust.stop_declared` | `status` |
| `stop-allowed-red` | `rust.stop_allowed_red` | `fp`, timing |
| `stop-allowed-unreviewed` | `rust.stop_allowed_unreviewed` | `fp`, timing |
| `stop-advisory-unreviewed` | `rust.stop_advisory_unreviewed` | `fp`, timing |
| `deny-scan` | `rust.scan_denied` | `file` or `via`, `rules`, `cmd` (redacted) |
| `deny-manifest` | `rust.manifest_denied` | `file`, `rules` (the MOD-* manifest checks) |
| `ask` | `rust.scan_asked` | `file`, `count`, or `via` (`shell-copy`, `cargo-deps`, `TEMPER_ALLOW_RAISE`, `git-restore-claude`, `shell-patch-unread`, `shell-write-protected`, `shell-delete-protected`) |
| `deny-shell-write` | `rust.shell_write_denied` | `via` or `file`, `files`, `cmd` (redacted) |
| `deny-lockfile` | `rust.lockfile_denied` | `file` or `via`, `cmd` (redacted) |
| `deny-skill-gate` | `rust.skill_gate_denied` | `file` |
| `deny-baseline-raise` | `rust.baseline_raise_denied` | `via` |
| `deny-ignore-rust-version` | `rust.ignore_rust_version_denied` | `cmd` (redacted) |

Two things to carry over unchanged. Commands are redacted before they are written (`redactCommand`), even though telemetry never leaves the machine. And the per-rule fired and overridden counts that `rust.scan_denied` and `rust.scan_asked` give are the evidence for promoting a review-only rule to a mechanical one, as wi-3fa7cf did.

The shell-guard events (`rust.shell_write_denied`, `rust.lockfile_denied`) come from the shell reader, which moves to `redkiln-guard` (wi-59cb6e). The guard runs Rust's rule pack, so the events stay in the `rust` namespace: the pack that decided writes them.
