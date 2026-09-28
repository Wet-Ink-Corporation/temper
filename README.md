# temper

The **rust-craft** engineering standard for Rust, plus the machinery that makes coding agents follow it, whether or not they "decide" to.

- **The standard** (`skills/rust-craft`) is a portable Agent Skill. It covers ownership and lifetimes, public API design, crate and module architecture, typed errors, TDD, performance and resources, async and concurrency, serde and structured output, modern edition-2024 idioms, and a catalogue of escape hatches with the correct shape for each. Every rule has an ID (`OWN-08`, `ERR-03`, `H-02`), which implementers cite in plans and reviewers cite in findings. There are 164 rules; `references/rules.md` is the registry. `templates/example-workspace/` is a complete event-sourced, hexagonal reference workspace that passes the gate.
- **The enforcement** (hooks, subagents, workflow commands, repository templates) runs in Claude Code.

## Why a plugin and not only a skill

A skill is advice that the model chooses to load. Rules the model can't skip have to live somewhere else: in the compiler, in hooks, or in a separate agent whose output is machine-checked. temper places each rule at the lowest layer that can enforce it:

| Layer | Mechanism | Deterministic? | What it covers |
|---|---|---|---|
| 1. Compiler | `[workspace.lints]`, `clippy.toml`, `rustfmt.toml`, `deny.toml`, toolchain pin | Yes | Every escape hatch a lint can see: unwrap, panic, indexing, `as` casts, `#[allow]`, print macros, leaks, `&String` params... |
| 2. Just-in-time context | `SessionStart` injection (after startup, resume, clear and compaction), `.claude/rules/*.md` scoped by `paths:`, a charter per crate (`AGENTS.md`, imported by `CLAUDE.md`) | Loading is deterministic; adherence is not | The iron rules and each crate's local invariants arrive in context when the files are touched |
| 3. Skill gate | `PreToolUse` denies `.rs` and `Cargo.toml` edits until `temper:rust-craft` is loaded in this agent context | Yes | "The agent chose not to load the skill" |
| 4. Pre-write scanner | `PreToolUse` lexes the file as it would look after the edit and refuses newly introduced escape hatches, citing rule and fix | Yes | Catches hatches before they reach disk, with actionable feedback |
| 5. Human gates | `PreToolUse` → `ask` for lint and CI config edits, new dependencies, `#[expect]` exceptions, removed tests, blind snapshot acceptance | Yes | H-05, H-12, H-16: the decisions agents must not make alone |
| 6. Anti-bypass | `PreToolUse:Bash` refuses shell writes to `.rs` files, `--no-verify`, and `-A`/`--cap-lints`; the gate re-scans every changed file | Yes (heuristic for shell) | Routing around layers 3 and 4 |
| 7. Completion gate | `Stop` runs fmt, clippy `-D warnings`, tests, rustdoc and the scan (cached per tree fingerprint), then requires an **approving review bound to that exact fingerprint** | Yes | "Done" means green and independently reviewed. `Status: WIP` or `Status: BLOCKED` is the honest exit. |
| 8. Independent review | `temper:rust-reviewer` runs in a fresh context with the skill preloaded and read-only tools (`gate --check`). Its verdict JSON is validated by `SubagentStop`: an approval with majors, a red gate, or unread changed files is refused. | The validation is deterministic | Judgment calls tools can't make: clone justification, API shape, architecture |
| 9. CI | The same gate in `.github/workflows/rust.yml` | Yes | Everything that happened outside Claude Code |
| 10. Measurement | `temper stats` (telemetry of denials, blocks and verdicts) and `claude plugin eval` suites in `evals/` | n/a | Whether the enforcement actually changes outcomes |

## Install

```bash
# as a marketplace (this repository)
claude plugin marketplace add Wet-Ink-Corporation/temper
claude plugin install temper@wet-ink

# or for one session while developing
claude --plugin-dir /path/to/temper
```

Then, in a Rust repository, run `/temper:setup`. It installs the workspace lints, clippy, rustfmt and deny config, the toolchain pin, the `CLAUDE.md`/`AGENTS.md` hierarchy, the path-scoped rules, CI, a pre-commit hook and `.claude/temper.json`, and it writes a charter for every crate. The committed `.claude/settings.json` enables the plugin for everyone who opens the repository.

## Commands and agents

| | Purpose |
|---|---|
| `/temper:implement <task>` | Orchestrates the whole loop: frame, architect plan, human decisions up front, `rust-implementer` test-first, gate, `rust-reviewer`, fix loop (up to 3 rounds), Status report |
| `/temper:review [base]` | Independent review of the current diff in a forked reviewer context |
| `/temper:gate` | Run and summarize the gate |
| `/temper:setup` | Install templates and write crate charters |
| `/temper:audit [crate]` | Brownfield assessment and remediation plan (`.temper/audit.md`) |
| `temper:rust-architect` | Explore, then write a plan (placement, signatures, errors, test list, HITL decisions) |
| `temper:rust-implementer` | Red, green, refactor, self-review, gate, and a Status block. Returns `BLOCKED` on human decisions. |
| `temper:rust-reviewer` | Adversarial, read-only, fresh-context review with a validated, fingerprint-bound verdict |

## Configuration (`.claude/temper.json`)

```json
{
  "skillGate": true,
  "review": "required",
  "askOnNewDependency": true,
  "preapprovedDependencies": ["thiserror", "anyhow", "serde", "serde_json", "tracing", "tracing-subscriber", "proptest", "insta", "trybuild", "divan", "criterion", "tempfile"],
  "gate": { "allFeatures": true, "doc": true, "scan": true, "stepTimeoutSec": 900, "steps": null },
  "protected": ["**/clippy.toml", ".claude/**", "..."],
  "scanner": { "exempt": [{ "rule": "H-01", "paths": ["crates/ffi-sys/**"] }] }
}
```

`review` can be `required`, `advisory` or `off`. The `TEMPER_DISABLE=1` environment variable turns every hook off; that is an emergency switch for a human, not something for agents.

## CLI

`node scripts/temper.mjs <gate [--check]|scan|status|setup|stats>`. It has no dependencies, needs Node 18 or later, and runs on Windows, macOS and Linux. Hooks call it in exec form (`node <script>`), so no shell quoting is involved.

## Brownfield adoption

Every check compares against the session's base revision, so only *new* violations block. That makes the scanner a ratchet: legacy code doesn't stop work, and `/temper:audit` plans the paydown crate by crate.

## Using the standard with other agents

`skills/rust-craft/` is a plain Agent Skill: its frontmatter uses spec fields only and its paths are relative. Copy it to wherever Codex, Gemini or Junie read skills, and use the root `AGENTS.md` template, which carries the iron rules in tool-neutral form. Enforcement layers 1 and 9 (lints and CI) apply to every agent and every human. Layers 3 to 8 are specific to Claude Code.

## Development

```bash
node --test tests/*.test.mjs        # scanner unit tests
python3 tests/lint-claims.py        # every lint rules.md cites is enabled by the template
claude plugin validate --strict .   # manifest + frontmatter
tests/e2e/smoke.sh                  # a real headless session with the plugin (a few model calls)
claude plugin eval . --allow-tools Write Edit --scaffold --trust-plugin --runs 3   # behavioral evals vs no-plugin baseline
```

Eval runs that grant `Bash` need an OS sandbox (WSL2 on Windows) and the sandbox hides `~/.rustup`, so the suite grades static outcomes (skill loaded, no escape hatches in the produced file, honest reply, review verdict) rather than running cargo. `tests/e2e/smoke.sh` covers the gate end to end.
