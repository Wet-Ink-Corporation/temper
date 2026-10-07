# Troubleshooting

This page covers what each temper message means and how to clear it. Every refusal names the rule it enforces (`H-02`, `PROC-13`, …). [`rules.md`](../skills/rust-craft/references/rules.md) has the full rule, and [`escape-hatches.md`](../skills/rust-craft/references/escape-hatches.md) has the right shape for each escape hatch.

Most refusals are aimed at the agent, which reads the message and changes its approach. You only need this page when the agent is stuck, when a message asks you something, or when temper seems to be doing nothing.

## First checks

| Command | What it tells you |
|---|---|
| `node .claude/temper/temper.mjs status` | The root, the session base, the tree fingerprint, the changed files, the gate and review state for this fingerprint, the baseline, and the effective config |
| `/temper:gate`, or `node .claude/temper/temper.mjs gate` | Runs the gate and names each failing step |
| `node .claude/temper/temper.mjs stats` | How often each denial, block and verdict fired, and which rules blocked the most escape hatches |
| `.git/temper/logs/<fingerprint>-<step>.log` | The full output of each gate step. The gate shows only a summary. |

temper keeps its state under `.git/temper/`: gate results and review verdicts per fingerprint, the logs, and telemetry. Never edit these files by hand. The hooks refuse `Write`, `Edit` and `MultiEdit` into `.git/`, but shell commands that write there are not checked, so the state is not a security boundary. `telemetry.jsonl` is never pruned and only `temper stats` reads it, so it is safe to delete; the README's [Local data and telemetry](../README.md#local-data-and-telemetry) says what it records.

**`temper hook <event> failed: …`.** A hook threw an error, and temper let the action through rather than block your work. This is a temper bug. [Report it](#reporting-a-problem) with the message.

**`temper needs the Cargo workspace at the git root`**, from `temper baseline`. The workspace's `Cargo.toml` is in a subdirectory of the repository. Nested workspaces are not supported yet.

**temper does nothing at all.** There is no session-start context, and `.rs` edits go through without the skill loaded. Check these, in order:
- The plugin is installed and enabled: `claude plugin list` shows `temper@temper`, and the repository's `.claude/settings.json` enables it. Repositories set up before 1.2.2 still name the old `wet-ink` marketplace there (see the CHANGELOG).
- `node` is on the `PATH` that Claude Code sees. When `node` is missing, Claude Code reports a non-blocking hook error and the action proceeds, so nothing is enforced. See [Windows](#windows) for the desktop app.
- `TEMPER_DISABLE` is not set in the environment Claude Code was launched from.
- The repository has a `Cargo.toml`. Outside a Rust project the hooks stand down.
- In a Claude Code cloud session, see [Cloud sessions and claude.ai](#cloud-sessions-and-claudeai).

## What fails open

The in-session layers are built not to wedge a session, so in these cases the action goes ahead:
- A hook throws an error (the `temper hook <event> failed` message above).
- A hook cannot parse its input, or `node` cannot be started.
- A hook runs past its timeout in `hooks/hooks.json`, and Claude Code stops it.
- The Stop hook's loop breaker: after one block on an unchanged tree, the turn may end (see [Finishing a turn](#finishing-a-turn-the-stop-hook)).
- A reviewer's verdict is still invalid on its second try. The reviewer's turn ends and no verdict is recorded, so the Stop hook still finds no approving review.

By design, every hook stands down with `TEMPER_DISABLE=1`, and the Stop hook does not enforce in plan mode or when the agent ends with `Status: WIP` or `Status: BLOCKED`. CI runs the same gate and fails closed, so it is the layer to rely on. The pre-commit hook adds a faster check (fmt, then clippy, rustdoc and the scanner through `baseline --check`) on commits made where it is enabled.

## Cloud sessions and claude.ai

**temper is missing from a Claude Code cloud session** (claude.ai/code, or a session started from the app or a GitHub integration). `ListPlugins` is empty, no `temper:*` skill is listed, and no temper hook runs. These are the causes, with what was seen in a cloud session on Claude Code 2.1.289:
- **The repository's `.claude/settings.json` is not enough.** A cloud session clones the repository fresh into a new container, and nothing in `~/.claude` survives between sessions. A headless session does register the `temper` marketplace from `extraKnownMarketplaces`, but only in a trusted workspace. It never installs `temper@temper` from a repository's `enabledPlugins` alone. The debug log shows `plugin-cache-miss` for `temper@temper` on every start.
- **The plugin on your claude.ai account may not reach the session.** Cloud sessions sync your account's plugins into `~/.claude/plugins/synced/` at startup. If that directory is empty, the account copy was not synced, for example because it is enabled in a different organization from the one the session runs under. A plugin uploaded as a zip under Customize > Plugins is a separate copy that never updates. Only the organization sync tracks this repository: under Organization settings > Plugins & skills, sync from GitHub `Wet-Ink-Corporation/temper` with **Sync automatically** on, and set `temper` to installed by default. The sync reads the default branch, and `main` is the release tree, so each merge to `main` reaches the organization with no release repository. Weigh-In needs one only because its `main` is not a built plugin. Then remove any uploaded copy, so temper is loaded once.
- **Installing from a `SessionStart` hook is too late.** The install succeeds, but Claude Code has already loaded its plugins, so temper appears only the next time Claude Code starts.

The fix is to install the plugin in the cloud environment's **setup script**, which runs before Claude Code starts. Open the environment menu in the session's title bar, choose Edit, and add these lines to the Setup script:

```bash
claude plugin marketplace add Wet-Ink-Corporation/temper
claude plugin install temper@temper
```

The repository is internal to the Wet-Ink-Corporation enterprise, so the clone works only where git has credentials for it. In a cloud session, the session's GitHub connection provides them. The setup script also needs `node` on the `PATH`, and the Rust toolchain if the gate is going to run. New sessions in that environment start with temper loaded. To check, run `claude plugin list` in a session; it shows `temper@temper` as enabled.

**temper in claude.ai chat.** Chat loads a plugin's skills, and ignores its hooks and agents. In chat, `rust-craft` and the `/temper:*` instructions are available as reading, but nothing is enforced: there is no skill gate, no escape-hatch scan and no completion gate. temper's enforcement needs Claude Code. The Anthropic Directory also lists an unrelated plugin named "Temper", by another author, so check that the one enabled is this one (by Ryan Britton, with `rust-craft` among its skills).

## The emergency switch: `TEMPER_DISABLE=1`

`TEMPER_DISABLE=1` turns every temper hook off. Each one exits without a decision. It is for a human who needs to get work done while temper is broken. It is not a way to get past a rule you disagree with: for that, change the configuration below, or open a false-positive issue.

Hooks inherit the environment Claude Code was started with, so set the variable when you launch it:

```bash
TEMPER_DISABLE=1 claude              # macOS, Linux, Git Bash
```
```powershell
$env:TEMPER_DISABLE = "1"; claude    # PowerShell; Remove-Item Env:TEMPER_DISABLE afterwards
```

An `export` inside a running session reaches only that shell, not the hooks. Restart Claude Code without the variable to turn temper back on.

The switch covers the hooks only. CI and the pre-commit hook still run the vendored CLI. Do not put `TEMPER_DISABLE` in a committed `.claude/settings.json`, because it would turn temper off for everyone.

Narrower switches, in `.claude/temper.json` (an edit to it asks you first):

| Setting | Effect |
|---|---|
| `"skillGate": false` | Rust edits no longer wait for the `temper:rust-craft` skill to load |
| `"review": "advisory"` or `"off"` | The Stop hook no longer requires an approving review; `advisory` still mentions a missing one |
| `"askOnNewDependency": false`, or `preapprovedDependencies` | New dependencies no longer ask, or the listed ones never ask |
| `"scanner": { "exempt": [{ "rule": "H-01", "paths": ["crates/ffi-sys/**"] }] }` | Turns one scanner rule off for some paths, in the hook and the gate alike |
| `"manifests": { "ownLints": ["crate-name"] }` | Lets an approved member keep its own `[lints]` table |

## Edits (`Write`, `Edit`, `MultiEdit`)

| Message starts with | Meaning | What to do |
|---|---|---|
| `files under .git (including temper gate state and review verdicts) are not edited directly` | Gate results and verdicts are evidence, so only the gate and the reviewer write them | Nothing; this is by design |
| `Cargo.lock is maintained by Cargo and is never edited by hand (PROC-13)` | Lockfile edits are always refused | Use `cargo update -p <crate> --precise <version>`, or for a merge conflict, check out one side's `Cargo.lock` and re-run cargo |
| `the rust-craft standard is not loaded in this agent context, so this Rust edit was not applied` | The skill gate: `.rs` and `Cargo.toml` edits wait until the agent has loaded `temper:rust-craft` | The agent invokes the skill and retries. Each subagent loads it for itself, except `rust-implementer`, `rust-reviewer` and `rust-architect`, which have it preloaded. |
| `<path> is verification/agent configuration. Changing it can weaken the checks` (asks) | The file matches `protected` (lint, fmt and deny config, the toolchain pin, `.cargo/`, `.claude/`, `.github/`, `.githooks/`, `CLAUDE.md`, `AGENTS.md`) | Approve only a change you asked for |
| `this manifest edit breaks N workspace rule(s)`, then `MOD-09` | The member's `Cargo.toml` would not inherit the workspace lints | Keep `[lints]` with only `workspace = true`. A crate that needs its own lints goes in `manifests.ownLints`, by a human. |
| … then `PERF-09` | A `[profile]` table in a member manifest, which Cargo ignores | Move it to the root manifest, as a decision recorded in the root `AGENTS.md` |
| `new crate`, `moves to a new major version`, `new dependenc…`, `the [lints]/[profile] sections change`, or a watched key such as `test`, `doctest`, `harness`, `autotests`, `rust-version`, `edition` or `[patch]` (all ask) | Decisions the standard leaves to a human (H-05, H-12, H-16) | Approve, or tell the agent why not |
| `this edit introduces N escape hatch(es), so it was not applied`, then lines like `H-02 src/lib.rs:12: unwrap/expect in production code` | The pre-write scanner found an escape hatch that the edit adds. Existing ones in the file do not count. | The agent rewrites without it; each line carries a `fix:`. For a genuine exception it adds `#[expect(<lint>, reason = "…")]` on the item, which turns the refusal into a question for you. An `#[expect]` without a `reason` is always refused. |
| `Lint exception requested at …` (asks) | That `#[expect(…, reason)]` route | Approve only if the stated reason holds |
| `ARC-06 … new mod.rs file` | New modules use `foo.rs` plus `foo/bar.rs` | Rename the file |
| `this edit removes N test(s)`, or `adds #[ignore] to N existing test(s)` (asks) | Tests may not disappear quietly (H-12) | Approve a test that is moved or intentionally retired. A bare `#[ignore]` with no `= "reason"` is always refused. |

Advisories such as `H-08 … new .clone() call(s)` and `ARC-12` caret-requirement notes never block the edit. A reviewer will ask about them.

## Shell commands (`Bash`)

| Message starts with | Meaning | What to do |
|---|---|---|
| `write Rust sources with the Edit or Write tools, not the shell` | A redirect, `tee`, `sed -i`, `perl -i`, a script or a PowerShell cmdlet that writes a `.rs` file | Use the Edit tool, so the scanner sees the change |
| `edit Cargo.toml with the Edit or Write tools`, or the `Cargo.lock` message | The same rule for manifests (where shell copies and moves are refused too) and for the lockfile | Edit tool for `Cargo.toml`; cargo commands for `Cargo.lock` |
| `.claude/temper-baseline.json records the accepted lint debt` | Only `temper baseline` writes the baseline | `baseline --tighten` after fixing findings; raising it is yours to decide |
| `.claude/temper/ is the vendored temper CLI` | Only `temper setup` writes the vendored copy | See the README's Upgrading section |
| `` `temper baseline --init` `` / `` `--update` `` `records every current finding as accepted legacy debt` | Recording or raising the baseline is a human decision (H-12) | The agent re-runs the command prefixed with `TEMPER_ALLOW_RAISE=1`, and you are asked to approve it. Or run it yourself in a terminal. |
| `this command sets TEMPER_ALLOW_RAISE` (asks) | That request | Approve only if you want the current findings accepted as debt |
| `… puts older verification configuration back into .claude/` (asks) | A `git checkout`, `restore`, `stash` or `show` would bring back an older baseline, settings or vendored CLI | Approve only if you asked for it |
| `--ignore-rust-version bypasses the MSRV check` | Green would not mean the declared `rust-version` builds | Raising `rust-version` is a human decision; the agent should ask |
| `skipping git hooks (--no-verify) is not allowed` | `--no-verify` or `git commit -n` | Fix what the pre-commit hook reports |
| `lowering lint levels on the command line is not allowed` | `-A`, `--allow` or `--cap-lints` on clippy, or in `RUSTFLAGS` | Fix the code the lint points at |
| `` `cargo add …` changes the dependency graph `` (asks) | Adding or removing a dependency that is not preapproved | Approve if justified |
| `accepting all snapshot changes at once can hide regressions` (asks) | `cargo insta accept`, or `test --accept` | Review with `cargo insta review` first |

## Review verdicts (the `rust-reviewer` subagent)

`the review verdict is invalid, so the report was not delivered`, or `your review verdict is invalid`: the reviewer's closing JSON block failed validation. The listed problem says why. For example, it may say the fingerprint is not the one in the review ticket, or that changed files were not read. It may also say the reviewer approved despite blocker or major findings, despite a red gate, or with a dimension below 3. The reviewer fixes the block and hands back again. You do not need to act.

A verdict whose fingerprint no longer matches the tree is recorded as stale. Any change after the review needs a fresh one.

## Finishing a turn (the `Stop` hook)

| Message | Meaning | What to do |
|---|---|---|
| `temper gate: RED …` and `The work is not done while the gate is red` | A gate step failed | Fix the failure. Never weaken tests or lints to do it (H-12). |
| `The gate is green, but the current tree (fingerprint …) has no approving independent review (PROC-08)` | Review is required, and no approval is bound to this exact tree | Delegate to `temper:rust-reviewer` (`/temper:review`) |
| `The reviewer requested changes for this tree` | The latest verdict is `changes-requested` | Address or rebut each finding, then ask for a fresh review |
| `stopped with a RED gate (no progress after the previous block)`, or `stopped without an approving review` | The loop breaker: the hook blocked once, and the tree did not change, so it lets the turn end rather than loop | Read the reported failures yourself |
| `Rust changes left in progress/blocked; the gate and review were not enforced` | The agent ended with `Status: WIP` or `Status: BLOCKED` | This is the honest exit for unfinished work. A `BLOCKED` status carries a question for you. |
| `temper gate: UNAVAILABLE (cargo is not usable in this environment …)` | `cargo --version` failed | Fix the toolchain, as `rust-toolchain.toml` pins it. Only the TOML manifest check ran. |

The Stop hook does not run in plan mode, when no Rust files changed, or when nothing changed since the session started.

## Gate timeouts on large workspaces

The gate runs each cargo step in turn: clippy, the tests (nextest when installed, plus doctests), and rustdoc. Each step may take up to `gate.stepTimeoutSec` seconds, 900 by default. The Stop and SubagentStop hooks themselves allow **1200 s**, so a large cold workspace can run out of time in two ways:
- **A step times out.** It fails with `could not run: spawnSync cargo ETIMEDOUT`. That red result is cached for the fingerprint, so re-running the gate on an unchanged tree shows it again. Use `gate --no-cache` to retry.
- **The hook times out.** Claude Code stops the hook before the gate finishes, and nothing is cached.

What helps, starting with the cheapest:
1. **Warm the cache from a terminal.** Run `node .claude/temper/temper.mjs gate` while the agent works or before it finishes. Results are cached per tree fingerprint, so the Stop hook reuses a result for a tree that has already been gated; only `cargo fmt` runs again. The fingerprint covers the session base too, so this works when the session has made no commits. Otherwise, set `TEMPER_BASE` to the base that `temper status` shows inside the session.
2. **Install `cargo-nextest`.** The gate uses it automatically when `cargo nextest --version` succeeds.
3. **Make builds incremental**: keep one `target/` directory across sessions, or use `sccache`.
4. **Trim the gate in `.claude/temper.json`:**
   - `"allFeatures": false` checks default features only. CI still runs the feature combinations.
   - A lower `stepTimeoutSec` makes a hung step fail fast instead of eating the hook's budget.
   - `"steps": [{ "name": "…", "cmd": ["cargo", …] }]` replaces the default steps entirely. It also turns off the baseline ratchet, so use it only when you have no baseline.

## Windows

- **`node` must be a real `node.exe` on `PATH`.** Hooks start `node` directly, with no shell, and `.cmd` or `.bat` shims from version managers cannot be started that way. The desktop app reads the user and system environment, but not PowerShell profiles. Set `PATH` there, not in `$PROFILE`, then restart the app.
- **Environment variables** use PowerShell syntax: `$env:TEMPER_DISABLE = "1"`, or for a baseline raise, `$env:TEMPER_ALLOW_RAISE = "1"; node .claude/temper/temper.mjs baseline --update`.
- **Line endings.** temper reads CRLF sources and manifests, and does not report a CRLF checkout of the vendored CLI as drift. A `* text=auto eol=lf` line in `.gitattributes` still avoids churn in diffs and the baseline.
- **Paths.** Verbatim paths (`\\?\C:\…`) and 8.3 short names (`RUNNER~1`) were reported wrongly before 1.2.3. If paths in temper's output look like `../../RUNNER~1/…`, upgrade (see the README's Upgrading section).
- **The baseline is per platform.** Findings depend on the host's `cfg`s. Record and tighten the baseline on the platform CI runs. `--tighten` refuses on a different host unless you pass `--any-host`, and `--check` warns.
- **Speed.** Windows builds are the slowest of the platforms CI runs, so the [timeout advice](#gate-timeouts-on-large-workspaces) applies there first.
- **Developing temper itself:** the unit tests skip their symlink cases without Developer Mode, and evals that grant `Bash` need WSL2.

## Reporting a problem

Open an issue with the matching template:
- **Bug**: a crash, a `temper hook … failed` message, or wrong output.
- **False positive or negative**: the scanner or a hook refused good code, or let a real escape hatch through. Include the message line with its rule ID, and the code it pointed at.
- **Rule proposal**: a change to the standard itself.

For a security problem, follow [`SECURITY.md`](../SECURITY.md) instead.
