# Changelog

## Unreleased

**`temper stats` counts review rounds per change (#139).** The pilot had to tally by hand how many review rounds each change took to reach approval. `stats` now prints a `review rounds per change` line, and `review.changes` in `stats --json` holds the same: the number of changes, how many were approved, how many on the first round, how many were never approved, and the median and maximum rounds to approval. A change is a session's run of reviewer verdicts up to and including the first approval of the current tree; the next verdict starts another change. A stale approval does not close a change, and approving the same tree again is not a new one. Rounds left open when the telemetry ends count as never approved, which includes a session still under way. The temper lens's Stats tab shows this line in place of rounds per session. The `stats --json` keys already there are unchanged, and no hook decision changes.

**Repeat reviews of one tree converge (#136).** A pilot's reviewer approved one unchanged tree, then requested changes twice, then approved again, and whichever verdict came last decided Stop. Three changes make repeat reviews of one fingerprint converge:
- When `temper:rust-reviewer` starts on a tree that already has a non-stale verdict, its `SubagentStart` context gives that verdict, when it was recorded, its blocker and major findings (up to 10, each on one line), its scores below 3 and its gate entries that are not `pass`. The reviewer re-checks each one, and overturns an earlier approval of the unchanged tree only with a blocker or major finding, a score below 3 or a gate entry that is not `pass`, quoting the evidence. `agents/rust-reviewer.md` says the same.
- A `changes-requested` verdict with no blocker or major finding, no score below 3 and every gate entry `pass` or `n/a` no longer replaces a non-stale approval of the same fingerprint. The approval stays the verdict Stop reads, and the later verdict is appended to `verdicts/<fingerprint>.history.jsonl`, which SessionStart prunes together with the verdict. Its telemetry row still counts, with `"kept": "approved"`. A verdict with any of those replaces the approval as before, and an approval still replaces a `changes-requested` verdict.
- In a cloud session, `SubagentStop` no longer records again the verdict that the reviewer's `SubagentHandback` already recorded for the same fingerprint, so a kept verdict gets one history row and one telemetry row.

Upgrading from 1.4: `scripts/temper.mjs`, `scripts/lib/hooks.mjs` and `scripts/lib/verdict.mjs` changed, so `temper setup --dry-run` reports the vendored copy as drift. Refresh it with `temper setup --vendor-only`. Nothing in `templates/workspace/` changed.

## 1.4.0 (2026-10-19)

**CI type-checks the temper lens (wi-78d987).** `tests/lens-check.sh`, and so CI's `temper lens` step, now runs a strict `tsc` pass (`strict`, `noUncheckedIndexedAccess`) over the lens and its tests, so a type error fails CI rather than showing only when Claude Code loads the lens. CI fetches TypeScript 7.0.2 through npx, pinned like the Claude Code CLI. The plugin API types it checks against are vendored in `tests/lens-types/`, because Claude Code writes them only when an interactive session loads a mod.

**temper lens: live gate progress at `Stop` (#111).** While temper's gate runs at `Stop` or `SubagentStop`, the lens shows its progress, read from the gate's progress file through `status --json`:
- The spinner gains the step the gate is on, its place among the steps and the time since the gate started (`temper: gate 4 clippy · 1m 12s`).
- The band shows the same, with the steps already done and their times.
- When the gate ends, one toast gives the outcome (`temper: gate green, review approved`), or the first line of the block reason when temper's own telemetry recorded that block. Parallel subagents stopping at once share one poll.

The lens polls only while a Stop runs, and stops on `session.end` and on reload. Its Stop hooks return the settings hooks' result unchanged, and a lens failure replays that result, so the lens can never decide a Stop.

The README now says that Claude Code is still rolling out plugin modules. Until they are on for an account, the lens does not load and `/temper-lens` is an unknown command, while the hooks run as usual.

**temper lens: Activity and Stats tabs (#113).** The `/temper-lens` pane gains two tabs:
- **Activity** lists this session's denials and asks, newest first, from `temper stats --json --session <id>`. Each row has its event, rule IDs, file and agent, and pressing a row shows the rule. A legend points each kind of event to its section of the troubleshooting guide.
- **Stats** shows the clone's gate latency at `Stop`, gates cut short, review rounds, first-verdict approvals and the rules blocked most, with a sparkline of recent gate times in the terminal. A button fetches the telemetry branch (`stats --json --branch`), and only when pressed.

The pane remembers its last tab across sessions (`$.store`), and the tabs refresh only while it is open. `stats --json` gains `gate.recentMs`: the wall times of the last 30 uncached gate runs, oldest first.

**temper lens: the `/temper-lens` review pane (#112).** `/temper-lens` opens a pane with the reviewer's verdict for the current tree, so it can be read without opening `.git/temper/verdicts/`. The pane shows:
- the verdict, and whether it is stale;
- the 12 rubric scores, any below 3 highlighted;
- the findings by severity, each with its rule ID, `file:line` and fix.

Pressing a finding shows the rule's text from the plugin's own `rules.md`, and a **Draft fix** button. The button fills the prompt box with a request to address the finding (`$.prompt.fill`) and never submits it. With no verdict, the pane says why: no Rust changes, a red gate, or no review yet. The read-only allow-list still holds: the lens reads `rules.md`, and fills but never submits the prompt.

**temper lens: temper's state above the prompt (#110).** The plugin gains a read-only mod, temper lens. In a Rust project, in the terminal or the desktop app, it draws one row above the prompt with the Rust files changed, the gate (green, red with its failing step, running with its step, or not run), the reviewer's verdict and the baseline total. It reads `temper status --json --session <id>` from the plugin's own CLI when the session starts, when the desktop app attaches, after a turn ends and shortly after an edit or shell call. It hooks nothing that decides: every tool call's result passes on unchanged, and a lens failure is skipped. It draws and runs nothing in cloud sessions, VS Code's chat panel and `claude -p`, under `TEMPER_DISABLE=1`, or outside a Rust project, and `TEMPER_LENS=0` turns off the lens alone. `hooks/hooks.json` gains `"modules": ["./lens/register.tsx"]` and `plugin.json` gains `"types"`. CI runs the lens's plugin tests and checks it against an allow-list of the events and calls #107 permits (`tests/lens-check.sh`). It needs Claude Code 2.1.286 or later on the desktop and 2.1.287 or later in the terminal; CI's pin, 2.1.286, validates and tests it. The README has a "temper lens" section, and the troubleshooting guide covers "The lens shows nothing".

**The gate reports its progress while it runs (#109).** The gate keeps `gate-progress-<fingerprint>.json` in the state dir while it runs. From the end of formatting, once the fingerprint is known, and as each step starts, it records the steps done (name, result, time) and the step it is on. It removes the file when it ends, unless another gate on the same fingerprint has written it since. `temper status --json` reports it as `gate.running` for the current tree (`null` when no gate runs), so a reader such as the temper lens can show which step a slow gate at `Stop` is on. A file whose process is gone, or whose step is older than `gate.stepTimeoutSec` plus five minutes, is ignored, and SessionStart's pruning removes it. Gate results, caching and exit codes are unchanged. The shell guard on the state dir covers the new file under a `$VAR/temper/` prefix too.

**Troubleshooting: why a fix on `main` has not reached you.** `docs/troubleshooting.md` now explains that an installed plugin only moves when `plugin.json`'s version changes, so marketplace installs keep their release until the next one, and how to check which release you run. A pilot hit this: the desktop app reported temper as up to date while `main` had moved on.

**`temper stats` counts each review once.** In a cloud session the reviewer's verdict is recorded twice, once by the `SubagentHandback` hook and again by `SubagentStop`, so the review-loop line (and `review` in `stats --json`) counted every review as two rounds: the weigh-in pilot's telemetry read "rounds per session p50 28" for 14 reviews. A `subagent-stop` verdict row that repeats the `handback` row just before it (same agent, tree and verdict) is no longer counted as a round. The `verdict` event count still shows every row. No hook decision changes.

**`temper status --json` and `temper stats --json` (#108).** Both commands gain `--json`, which prints one versioned JSON document, so scripts, CI and the planned temper lens (#107) can read temper's state without parsing `.git/temper/`, whose layout stays private. `status --json` (`temper_status: 1`) holds the plugin version, the root, state dir, base, fingerprint and changed Rust files, the gate result for the tree (`null` when not run; steps with name, result, time and summary), the reviewer's verdict (`null` when none; scores, and findings without their `evidence` text), the baseline (`null`, its total and toolchain, or `{ error }`), the config, and `disabled: true` under `TEMPER_DISABLE=1`. Outside a Rust project it prints `{ "temper_status": 1, "version": ..., "project": null }` and exits 0. `stats --json` (`temper_stats: 1`) holds the counts `stats` prints: events, rules blocked, gate latency at `Stop` in milliseconds, gates cut short and review rounds. `--session ID` makes `status` use that session's recorded base, as its hooks do, and adds that session's telemetry rows to `stats --json`. The text output of both commands is unchanged.

**CI flags a changed vendored CLI (#66, #67).** CI runs `.claude/temper/` from the pull request itself, so an edit there could change what the gate and the report say without anyone noticing, for example an edited raise check in `baseline.mjs` (1.2 review item 27). The template's `rust` workflow now has a `Vendored temper CLI` step. It runs before any vendored code. When `.claude/temper/` differs from the ratchet base, it adds a warning annotation with the file count and the `VERSION` change, and the step summary and pull-request comment show `VENDORED CLI CHANGED`. The `report-comment` job, which runs no repository code, puts that line at the top of the comment, and posts it even when the gate stopped before writing a report. It flags and does not fail, like a raised baseline (wi-d23383), because the vendored copy of an unreleased `main` keeps the last release's `VERSION`, so "same version, different bytes" is not proof of a hand edit. For a hard sign-off, the README's CODEOWNERS recipe already covers `/.claude/temper/`. The fallback `cargo clippy` steps and the MSRV `cargo check` also pass `--keep-going` now, so one run reports every crate's failures (MOD-13).

**The Status block gains a `Declined:` line (#48).** The block that ends every turn that changed Rust (PROC-10) now has a line for requests the agent declined or carried out differently, each with a one-sentence reason, or `none`. In the 2026-10-05 eval run the agent declined to add `#[ignore]` and said so mid-turn, but its final message was only the status block, so a reader of that message could not tell. The line is in the standard's §5, PROC-10 and the `temper:rust-implementer` report. The `Stop` hook reads only the `Status:` line, so no hook decision changes.

**Telemetry from cloud sessions is kept, opt-in (wi-641f99).** A Claude Code cloud session's container is discarded when the session ends, and `.git/temper/telemetry.jsonl` with it, so the pilots' telemetry never reached `temper stats`. With `"telemetry": { "branch": "temper-telemetry" }` in `.claude/temper.json`, each `Stop` now starts a detached `temper telemetry push` that pushes the session's lines to that branch on `origin` as `sessions/<date>-<session>.jsonl`, in a `[skip ci]` commit built with a private index, so the clone's working tree, index, `HEAD` and branches are untouched. It is off by default, and it is temper's only network call. `temper stats --branch [NAME]` summarizes every session on the branch.
- **New telemetry fields.** `Stop` outcomes carry `ms` (the gate's wall time) and `cached`. A new `gate-start` event comes before the gate runs at `Stop`, so a hook killed by its timeout shows as a `gate-start` with no outcome. In a cloud session every line carries `ccr`, the cloud session ID that matches the `Claude-Session` link in the session's commits.
- **`temper stats`** also reports the gate's latency at `Stop` (p50, p90, max), the gates cut short, and the review rounds per session.

**Reviewer verdicts are recorded in cloud sessions (#97).** In a Claude Code cloud session a subagent delivers its report only through a `SubagentHandback({ message })` tool call, and `SubagentStop` receives no `last_assistant_message`. With the `PreToolUse` `SubagentHandback` hook removed (wi-103aa0, on `main` since 2026-10-08 and never released), no verdict was recorded, so every cloud session that changed Rust ended `BLOCKED` on PROC-08. The hook is back, so wi-103aa0 is reversed. It checks the reviewer's verdict before delivery, denies an invalid one with the problem list, and records a valid one.
- **`SubagentStop` reads the hand-back too.** Without `last_assistant_message`, it takes the report from the subagent's transcript (`agent_transcript_path`): the last `SubagentHandback` message, else the last assistant text. This applies to the reviewer's verdict and to the implementer's `Status:` line.
- **Subagent telemetry.** `SubagentStart` and `SubagentStop` now run for every agent type (the matchers are gone; the handlers still act only for temper's agents). Each records a `subagent-start` or `subagent-stop` row with the raw `type` before any filtering. `SubagentHandback` records `handback`, a rejected verdict records `verdict-invalid`, and `verdict` rows carry `via` (`handback` or `subagent-stop`).
- The agent definitions and the rust-craft skill say the report is the final message, or the `SubagentHandback` message where the subagent hands back that way.

Shell writes to protected configuration ask. The Bash hook's decision for a command that writes a file matching the `protected` globs in `.claude/temper.json` changes from pass to ask: such a command used to pass with no decision, and now asks the human, as an Edit or Write to the same file already did.
- **What counts as a write.** Redirects (`>`, `>>`, `&>`), `tee`, `sponge` and `truncate`, `sed -i` and `perl -i`, the destination of `cp`, `mv`, `install`, `ln`, `rsync`, `git mv`, `Copy-Item` and `Move-Item`, `curl -o`, `wget -O` and `dd of=` (also attached to an option cluster, as in `curl -sSLo<file>`), `git checkout <rev> -- <path>` and `git restore --source=<rev>` (HEAD and the index pass; for a wildcard pathspec such as `'*.toml'`, the files `git diff` says it would change), `git apply`, `git am` and `patch` (the files the patch names, its target and `-o`; a patch temper cannot read, from a pipe, a variable, a missing file or `git am --continue`, asks too, though in a Rust project the Rust-source check denies such a patch first, while `--check`, `--stat`, `--numstat` and `--summary` without `--apply`, `--dry-run`, `--help`, `--version`, `git am --abort` and `--show-current-patch` pass), `Set-Content`, `Add-Content` and `Out-File`, and python, node or ruby scripts (`-c`/`-e` or a here-doc) that open a file for writing by name or through a variable. The same forms count inside `bash -c`, `sh -c`, `eval`, `find -exec`, `$(...)`, and `if`/`for`/`while`/`until` bodies, brace groups and `!`. The check reads the command text; it is a guard, not a sandbox, so a write it cannot see (a script file, or a copy of `{}` or `$VAR` into a protected directory, where the landing name is unknown) still passes.
- **Where the target is.** It resolves against the command's working directory, following `cd` (also inside an `if` or loop body), `pushd`, `popd`, `cd "$(git rev-parse --show-toplevel)"`, `git -C`, `$PWD` and `VAR=value` assignments on their own, so `./clippy.toml`, an absolute path inside the project and a write from a crate directory all count. A `cd` inside `( ... )` or a pipeline ends with it. After a `cd` to a place the hook cannot know (`cd "$DIR"`), the target counts from both the starting directory and the project root.
- **Which files.** The default list is `.claude/**` (where `review` and `skillGate` live), `**/clippy.toml`, `**/rustfmt.toml`, `**/deny.toml`, `.clippy.toml` and `.rustfmt.toml` at any depth, `.cargo/**`, `.github/**`, `.githooks/**`, and `rust-toolchain`, `rust-toolchain.toml`, `CLAUDE.md` and `AGENTS.md` at the project root. A project's own `protected` list replaces it, as for edits. A file inside a nested git checkout, such as a Claude Code worktree under `.claude/worktrees/`, is judged by that checkout's own list, as an Edit to it is.
- **Whole directories.** A command that writes a whole directory asks when protected files live under it: `git checkout <rev> -- .github`, `git restore --source=<rev> .` (the project root holds them all), `cp -r <dir> .cargo`, `mv <dir> .github` when `.github` does not exist yet, `rsync -a <dir> .claude`. The reason names the globs it replaces, such as `.github/**`, or the path it lands at inside an existing protected directory, such as `.github/<dir>`. A recursive copy or move of a directory names each protected file it brings (`cp -r cfg docs/x` with a `cfg/sub/clippy.toml`), and `git checkout <rev> -- <dir>` or `git restore --source=<rev> <dir>` names each protected file `git diff` says it would change there (`git checkout main -- docs` over `docs/clippy.toml`). `git checkout -- .` and `git restore .` restore your own edits from the index and pass, as for a single file.
- **Denies still win.** The shell-write denies for `.claude/temper/` and the baseline file, and every other Bash deny, come first, so a command that also does something denied is still denied. A command that also adds a dependency (`cargo add`) or accepts snapshots shows both reasons in one ask. Reading, diffing and copying these files out of the project run as before. A copy to a protected name inside the project, such as `target/clippy.toml`, asks, as an Edit to it does. Text that only mentions a write (a commit message, a grep pattern, a here-doc fed to `cat`) does not count. The command is read by `scripts/lib/shell.mjs`, a new vendored file the other shell-write checks share.

Shell deletes of protected configuration ask too (wi-2000a3). The Bash hook's decision for a command that deletes a file matching the `protected` globs changes from pass to ask, with the reason `this shell command deletes <path>. <path> is verification/agent configuration. Deleting it can weaken the checks (H-05, H-12)`.
- **What counts as a delete.** `rm` and `unlink` (with or without `-f`), `Remove-Item` and its `ri`, `del`, `erase` and `rd` aliases (PowerShell's `rm` reads as `rm`), and `git rm`, `--cached` included, since the file leaves the repository. They count wherever writes count (behind `cd`, `git -C`, `sudo`, `bash -c`, compound commands) and resolve the same way. `git rm -n`, `--dry-run` and `-WhatIf` delete nothing and pass.
- **Directories.** A recursive delete (`rm -r`, `-Recurse`, `git rm -r`) names the protected globs under the directory, such as `.claude/**` for `rm -rf .claude`, and each protected file it holds (`rm -r cfg` over a `cfg/sub/clippy.toml`).
- **What still passes.** Deleting any other file (`rm src/main.rs`, `git rm --cached notes.txt`), a quoted mention (`git commit -m "rm deny.toml"`), and a glob operand, which is not expanded (`rm *.toml`), as for writes. `find -delete`, `xargs rm` and `rmdir` are not read. The state-dir rule is unchanged: a write into it is denied, and deleting state files such as `telemetry.jsonl` stays allowed.

One hook decision changes.
- **Shell writes into the state dir are denied.** `PreToolUse` on Bash used to allow a shell command that writes into temper's state dir, so a redirect, `cp`, `tee`, `sed -i` or a Python `open(..., 'w')` could forge a review verdict or a green gate record. It now denies it, as `Edit` and `Write` already were: verdicts come only from the `temper:rust-reviewer` subagent, and gate results only from the gate command. The check reads the command the way the shell does and looks only at where it writes: redirection targets, the destination of `cp`, `mv`, `install`, `ln`, `rsync`, `Copy-Item` and `Move-Item` (a trailing redirect, option or comment does not hide it), `tee`, `sponge`, `dd of=`, `curl -o`, `wget -O`, `tar -x -C`, `unzip -d`, the PowerShell writers, `bash -c`, and Python, Node, Ruby or Perl code that opens a state file for writing or copies onto it, by literal or through a variable. It covers `.git/temper/` under any prefix, the absolute state dir (in a worktree it is in the main repository's git dir), `target/temper/` without git, `"$(git rev-parse --git-common-dir)/temper/..."` (also with `--path-format=absolute`), a variable set to one of these earlier in the command, a `cd` or `pushd` into the state dir, and `$VAR/temper/` followed by a state entry (`verdicts/`, `gate/`, `logs/`, `sessions/`, `telemetry.jsonl`, `findings-*`) for a variable it cannot resolve, unless that variable is a home or temp directory such as `$HOME` or `$TMPDIR`. Reading the files, copying them elsewhere in any language, `temper status`, `gate` and `stats`, and deleting state files with `rm` (such as `telemetry*.jsonl`; a deleted verdict or gate result only means a fresh review or gate run) stay allowed. So does quoted text that only names the path (a commit message, a `grep` pattern, a heredoc printed with `cat`), and a name that only starts with `temper`, such as `target/temper-report.html`. The command is read by `scripts/lib/shell.mjs`, a new vendored file the other shell-write checks share, so writes inside `if`/`for`/`while` bodies, brace groups, `$(...)` and backquotes count too, as do `truncate` and a `mv` or `cp` that lands a `temper` directory in `.git/`. A backslash that bash removes (`.git/tem\per/...`, `c\p`) does not hide the path or the command, even behind a bash function named like a PowerShell command (`copy() { cat; }; ... | copy > .git/tem\per/...`), and neither does a Python mode held in a variable (`m = 'w'; open(path, m)`).

Documentation.
- **Cloud sessions.** `docs/troubleshooting.md` has a new section on why temper is missing from Claude Code cloud sessions, and how to install it from the environment's setup script. A repository's `.claude/settings.json` does not install the plugin there, and a `SessionStart` hook installs it too late. The section also covers what works in claude.ai chat: the skills load, but the hooks and agents do not.
- **The organization sync.** The same section says how claude.ai's organization sync keeps temper current: sync `Wet-Ink-Corporation/temper` with Sync automatically on, and remove any uploaded zip copy. It also corrects an earlier line that called the repository public; it is internal.

A false-positive fix (patch level). The Bash hook's `--no-verify` and lint-lowering decisions change.
- **`--no-verify` and lint lowering in prose.** The `PreToolUse` Bash hook used to deny any command whose text held `--no-verify`, `git commit ... -n`, `cargo clippy ... -A` or `RUSTFLAGS=...-A`, even inside a commit message: `git commit -m "docs: mention --no-verify flag"` was denied. The hook still matches the whole command text, so a command handed to `sh -c`, `git rebase -x`, `powershell -Command` or `watchexec` is still checked, but it first blanks three kinds of prose: the value after a message-like option on any command (a short-flag cluster ending in `m`, such as `-m`, `-am` or `-nm`, and `-b`, `-t`, `--message`, `--body`, `--title`, `--notes`, `--description`), a here-document body (`git commit -F- <<'EOF'`, and the `-m "$(cat <<'EOF' ... EOF)"` form), and the quoted pattern of `grep`, `rg`, `ag`, `ack` and `git grep`: deny -> allow. A here-document a shell or interpreter reads (`cat <<EOF | bash`, `bash <<EOF`, `python3 - <<EOF`, `node`, `perl`, `ruby`), or an unquoted one that runs `$(...)`, is not blanked. A search tool counts only in command position, so `cargo clippy -p grep -- -A x` stays denied, and an unquoted message value that starts with `--` (`git commit -m --no-verify`) is not blanked; a quoted one, or `--message=--...`, is. A value or pattern that holds a command substitution, `$(...)` or a backtick, is never blanked, because the shell runs it: `git commit -m "$(git commit --no-verify -m x)"` and `rg "$(...)"` stay denied, and so does a message that only mentions `$(` beside a flag. The `-m "$(cat <<'EOF' ... EOF)"` form is the exception, since its body is already blanked. An interpreter name in quotes (`"bash" <<'EOF'`) still reads its here-document. Backslash-newline continuations are joined first, so a flag on a continued line is still read. Other quoted text is still read, so `echo "--no-verify"` stays denied.
- **`--fix` flags.** `cargo clippy --fix --allow-dirty`, `--allow-staged` and `--allow-no-vcs` are allowed: deny -> allow. The `--allow` check used to catch them. So is `-A` inside a path in a `RUSTFLAGS` value, such as `RUSTFLAGS=-L/opt/My-App/lib`: only a flag word that starts with `-A` or `--cap-lints` counts.
- **New denials.** Combined short flags such as `git commit -nm msg` and `-anm`, a quoted `"-n"`, and `git -C dir commit -n` (also with a quoted value, `git -C "dir with space" commit -n`) are denied, since `-n` is `--no-verify` for `git commit`; `git push -n` is a dry run and stays allowed. So are an attached `-A` (`cargo clippy -- -Awarnings`), a quoted `"-A"`, cargo options before the subcommand (`cargo -q clippy -- -A x`), `cargo rustc -- -A x`, `RUSTDOCFLAGS="-A warnings"`, and a quoted `RUSTFLAGS` value with `-A` after another flag (`"-D warnings -A x"`): allow -> deny.

**Copies, moves and patches of Rust sources are checked.** The `PreToolUse` Bash hook used to allow `cp`, `mv`, `install`, `ln`, `rsync`, `scp`, `git mv`, and `Copy-Item`/`Move-Item` (and their `cpi`, `mi`, `copy` and `move` aliases) onto a `.rs` file, and `git apply`, `git am` and `patch`, without a check. Only redirects, `tee`, `sed -i` and interpreter writes were refused.
- **A copy or move onto a `.rs` file** is now scanned like a Write of the source's content to the destination. A new escape hatch is denied, and a new `#[expect]`, a removed test or a new `#[ignore]` asks. A copy that creates a new `mod.rs` outside test code is denied (ARC-06), and a copy onto a protected path asks, as a Write of it does. A move of a `.rs` file inside the repository compares with the source's own content, the way the gate treats a rename, so renaming a file with legacy findings is still allowed. That holds only while the file stays the same kind: moving test or example code into `src/` is scanned as new production code. A crate moved or copied whole with its `Cargo.toml` (`mv crates/core crates/engine`) keeps its layout, so its test files stay tests. `ln` and `ln -s` count as copies of what the link points at (a relative `ln -s` target is read from the link's directory, and with `-r` from the current one), and so does a `.rs` symlink inside a copied tree. A symbolic link to a directory is scanned like a recursive copy of it. Only `-WhatIf` alone or `-WhatIf:$true` makes `Copy-Item` a dry run; with any other value (`$false`, `0`, `$f`, an expression) it runs the copy and is scanned.
- **Duplicating a file with legacy findings is refused.** A copy is compared with the destination only, since it duplicates the findings, so `cp util.rs util_copy.rs` or `cp -r crates/core crates/core2` on brownfield code is denied, as a Write of the same content is. A copy moved on later in the same command still counts as a copy, and a file an earlier copy in the command overwrites is read as what that copy puts there, not as what it held before. A file that another write earlier in the command changes (a redirect, `tee`, `sed -i`, `curl -o`, interpreter code) cannot be read before it runs, so copying it onto a `.rs` file is denied as unreadable. Copies into `.temper/` and `target/`, which the gate skips, are not checked.
- **When temper cannot read what would land in a `.rs` file** (a variable, a glob, stdin, a missing source, a source copied into a Rust source directory such as `cp $DIR/* src/`, or a directory after a `cd` it cannot follow), the command is denied with the Edit or Write guidance. temper follows `cd`, `cd -`, `cd "$(git rev-parse --show-toplevel)"`, subshells, `pushd`/`popd`, variables set earlier in the command and `~/`, reads `$TMPDIR`, `$TMP`, `$TEMP`, `$env:TEMP` and `$(mktemp [-d])` as the temp directory, and looks inside `if`/`for`/`while` bodies, brace groups, `bash -c`, `eval`, `find -exec`, `xargs`, `nice` and `timeout`.
- **Backups to a variable destination pass.** A readable source copied or moved to a destination held in a variable (`cp src/money.rs "$BACKUP"`, `cp src/money.rs ${OUT_DIR}/`) passes when its content scans clean as production code and it does not land as a `mod.rs`; otherwise it is denied as above. Backups to the temp directory pass whatever they hold, as copies to a literal `/tmp/...` path did.
- **`git apply`, `git am` and `patch`** are denied when the patch touches a `.rs` file, or when temper cannot read the patch (stdin, a variable, a missing file, a file the same command writes first, or `git am --continue`). A heredoc patch is read from the command. Apply the change with the Edit tool instead. `git apply --check`, `--stat`, `--numstat` and `--summary` without `--apply`, `patch --dry-run`, `--help` and `--version`, `git am --abort`, and patches that touch no `.rs` file still run.
- **One reader for shell commands.** The command is read by `scripts/lib/shell.mjs`, a new vendored file that the shell-write checks share. If shell.mjs cannot read a command, the checks built on it (this one, and the state-dir and protected-config checks where they are present) are skipped for that command, and every other Bash check still runs. It reads a backslash outside quotes the way bash does (`c\p` runs `cp`; `<<\EOF` is a quoted here-doc) and keeps it as a path separator in PowerShell commands. A write into a `.rs` file in the project that the older text check misses is denied with the same message: a name spelled `x.r\s` or `x.r''s`, and writers such as `curl -o`, `dd of=` and `sponge`. When a copy's advisory (such as H-08) meets another check's ask in the same command, the ask carries the advisory too.

Clearer messages for new adopters. Only the text changes: every hook decision, exit code and gate result is the same.
- **The skill-gate denial** lists all sixteen Iron rules, H-01 to H-16. It used to stop at H-12, while `SessionStart` printed all sixteen.
- **Malformed hook input.** When a hook's stdin is not valid JSON, the hook still exits 0 with no decision, but it now prints a `systemMessage`: `temper hook <event>: could not parse the hook input as JSON; nothing was checked`. Before, it printed nothing, which read as a clean pass. With `TEMPER_DISABLE=1` it stays silent.
- **The gate's fmt summary when cargo cannot load the workspace.** `cargo fmt` fails before formatting anything when a `members` glob matches no crate (a fresh setup with an empty `crates/`) or a manifest does not parse, and the summary used to be the tail of rustfmt's help text. It now says that cargo cannot load the workspace, quotes cargo's `error:` and `-->` lines with paths relative to the workspace, and, for a glob that matches nothing, suggests a `cargo new --lib` path inside it (`crates/<name>` for `crates/*`). The fmt step and the gate stay RED.
- **Vendored-CLI drift.** `temper setup`'s drift line ends with `rerun with --vendor-only to replace only the vendored CLI (or --force to also overwrite the other template files; an existing Cargo.toml is never overwritten)`, in place of `rerun with --force to replace it`.

**Config files count as Rust changes.** `.cargo/config.toml` or the legacy `.cargo/config` in any directory, and the keys of `.claude/temper.json` at the project root that the gate reads (`gate`, `scanner`, `manifests`), are now part of the tree fingerprint and of the session's Rust changes. `Stop` after a session that changed only one of them (`rustflags`, `gate.allFeatures`, `gate.steps`, `gate.scan`, a scanner exemption) used to allow the stop with no gate, and a gate run reused a cached result from before the edit. It now runs the gate and, where `review` is `required`, blocks until a fresh review approves the new tree (allow -> gate, then block). A cached result or review for the old tree no longer applies. `.claude/temper.json` counts by its effective gate settings, not its bytes: a change to a key that only steers hooks (`review`, `skillGate`, `protected`, the dependency prompt), a property the gate does not read under `gate`, `scanner` or `manifests`, a reformat or a default spelled out is still not a Rust change. The file counts even when git ignores it; a git-ignored `.cargo/config.toml` still does not. Other files under `.claude/` and `.cargo/` (`settings.json`, `credentials.toml`) still do not count. On upgrade, the fingerprint changes for every git tree where a `.cargo/config(.toml)` or the gate settings in `.claude/temper.json` differ from the session base (uncommitted, untracked or committed since), and for every workspace outside git that has a `.cargo/config(.toml)` or a `.claude/temper.json` whose gate settings differ from the defaults. In those trees, gate results cached and review verdicts recorded before the upgrade no longer match, so the next Stop re-runs the gate and asks for a new review; a session resumed across the upgrade over such an uncommitted edit does so even if it changed nothing. In a git repository whose config files are committed and unchanged, the fingerprint stays the same, so cached results still apply. Until the vendored copy is refreshed, a gate run from `.claude/temper/` computes the old fingerprint for those trees, so the results it caches are not reused by the plugin's Stop hook. `scripts/lib/project.mjs` also changed; `temper setup --vendor-only` refreshes it with the rest.

Stricter review verdicts. `SubagentStop` blocks, and the `PreToolUse` `SubagentHandback` hook denies, two kinds of verdict they used to accept.
- **Files read.** An `evidence.files_read` entry covers a changed file only when it is the repo-relative path that the reviewer's context block lists, or an absolute path that ends in `/` followed by that whole path (`/home/me/repo/crates/core/src/lib.rs`), even one outside the repository. A bare suffix no longer counts: `lib.rs`, `src/lib.rs` or `s` used to cover `crates/core/src/lib.rs`, and now that file is reported as not reviewed. One entry covers one file, and an absolute entry covers only the longest changed path it ends with, so `crates/core/src/lib.rs` no longer also covers a root package's changed `src/lib.rs`. When the Cargo workspace sits in a subdirectory of the repository, the paths are still relative to the repository root (`rust/crates/core/src/lib.rs`), as the context block lists them. Either separator and a leading `./` still work.
- **Scores.** Every verdict, approved or not, rates `correctness`, `escape_hatches` and `craft` 0-4 (`ALWAYS_SCORED`), because they apply to any Rust change; `"n/a"` for any of them is refused, whatever else is rated. An approval with every score `"n/a"` used to pass. The reviewer agent's instructions now say so.

`scripts/lib/verdict.mjs` changed too; `temper setup --vendor-only` refreshes it.

`temper setup` adds `/target/` to `.gitignore` when no line ignores `target` already (`target`, `target/`, `/target` and `/target/` all count), next to the `.temper/` line it already added. `--dry-run` reports it without writing, `--vendor-only` leaves `.gitignore` alone, and a second run adds nothing. `Cargo.lock` is not added: it is committed.

**`SessionStart` reports a Node older than the documented minimum.** Before, nothing checked the README's Node 18 minimum. Now `SessionStart` in a Rust workspace adds a warning line, shown to the human (`systemMessage`) and to the model (`additionalContext`), on Node 15 to 17. Every hook decision and exit code is the same: it is a message only.
- **Node 17.** `SessionStart` runs as before and appends the warning.
- **Node 15 and 16.** `hook session-start` in a Rust workspace (judged from the hook input's `cwd`, as `SessionStart` judges it) prints only the warning, before the rest of temper loads; it used to print `temper hook session-start failed: structuredClone is not defined`. Outside a Rust workspace it still prints nothing. Other hooks run as before, and CLI commands print the warning on stderr and keep their own output and exit code. `TEMPER_DISABLE=1` stays silent.
- **Node 14 and older.** Nothing changes: temper's syntax needs Node 15, so these fail with a syntax error before any check runs, and print no warning.
- **Claude Code is not checked.** Hook input carries no version, and Claude Code passes hooks no variable that names its version or executable, so the 2.1.286 minimum stays documented only.

The minimums live in `MINIMUM_VERSIONS` in the new `scripts/lib/requirements.mjs`; `temper setup --vendor-only` copies it with the rest of the vendored CLI.

Bounded local state. No hook rule changes, but what temper keeps under `.git/temper/` goes from unbounded to bounded, so Stop on a tree whose records were pruned now runs the gate again or asks for a fresh review, where it used to reuse them.
- **Pruning at startup.** `SessionStart` on startup in a Rust project already pruned session state after 14 days. It now also prunes review verdicts (after 60 days, newest 300 kept) and gate runs (after 30 days, newest 300 kept, 64 MiB in all, though the newest run is always kept). A gate run is the gate result with its step logs, pruned together, so a kept result never points at a deleted log. The records of the tree the session starts on are always kept and count toward no limit. A gate run or verdict written in the last hour is never pruned either, so a gate still running in another worktree, which shares the state directory, keeps its logs; it counts toward the limits all the same. A tree whose verdict was pruned needs a fresh review at Stop, and one whose gate result was pruned runs the gate again. A `temper gate` run from the command line prunes nothing.
- **Telemetry rotation.** `telemetry.jsonl` was only ever appended. At 1 MiB it is now renamed to `telemetry.1.jsonl`, replacing the previous one, and `temper stats` reads both files.

This also changes `scripts/lib/project.mjs`, so refresh the vendored copy with `temper setup --vendor-only`. Until then the old vendored `temper stats` reads only `telemetry.jsonl`, not the rotated `telemetry.1.jsonl`.

Upgrading from 1.3.x: files under `scripts/` changed, so `temper setup --dry-run` reports the vendored copy as drift. Refresh it with `temper setup --vendor-only`. In `templates/workspace/`, only `.github/workflows/rust.yml` changed: it gains the `Vendored temper CLI` step and `--keep-going`. `temper setup` lists the file as `skip … (exists; compare with …)` and does not check whether it differs. Diff your copy with `templates/workspace/.github/workflows/rust.yml` and merge the changes by hand, or copy the template's file over yours if you never edited it. The pull request that refreshes the vendored copy shows `VENDORED CLI CHANGED`, as expected. A full `temper setup` also appends `/target/` to `.gitignore` if no `target` line is there.

## 1.3.0 (2026-10-05)

Fixes from the pilot adoptions in weigh-in and redkiln-rs (#32).
- **`rust.yml`'s `msrv` job** reads the MSRV from `rust-version` in the root `Cargo.toml`. Before, it was hard-coded to 1.96, so it failed in any repository whose `rust-version` is newer, because cargo refuses a toolchain older than `rust-version`. It also gains the `Baseline mode` step that the other jobs have, so with a baseline it no longer fails on legacy warnings once the MSRV is 1.97 or later.
- **`temper setup` reports what an adopting repository has to change by hand.** It adds:
  - a `manual` line for each workflow or contributor doc (`CLAUDE.md`, `AGENTS.md`, `CONTRIBUTING.md`, the PR template) that uses `-D warnings`. Those steps fail on every legacy finding once the lint tables are merged.
  - a `manual` line listing the `allow-*-in-tests` keys that an existing `clippy.toml` lacks. Without them, the restriction lints fire in test code and the baseline records it as debt. In redkiln-rs, that was 5,541 findings.
  - a reminder to run `cargo machete` before the first push.
- **The workspace template's `AGENTS.md`** lists `temper baseline --check` as the lint command, in place of `cargo clippy -- -D warnings` and `RUSTDOCFLAGS="-D warnings"`.
- **The README's brownfield rollout** has a new step 3 for the `manual` lines. It also says what moving lint to the ratchet costs: the baseline records one platform, so other platforms no longer lint.

`temper setup --vendor-only` refreshes only the vendored CLI (`.claude/temper/`) (#38). It replaces an out-of-date copy without `--force`, removing files the new release no longer ships. It leaves the templates, the lint tables and `.gitignore` alone, and `--dry-run` previews it. Use it when an upgrade's only drift is the vendored CLI, in place of `setup --force` followed by restoring every customised template file. The README's Upgrading section has the step.

Fixes found by the runner probe on macOS and Windows runners.
- On macOS, paths under a symlinked directory were reported relative to the wrong root. TMPDIR is under `/var`, a link to `/private/var`, and cargo reports the real path, so `crates/legacy/Cargo.toml` came out as `../../../../private/var/.../crates/legacy/Cargo.toml`. The 1.2.3 fix for Windows 8.3 names now applies on every platform: when the lexical path leaves the root, both sides are compared in their real form.
- The example workspace's JSONL store could not repair a torn tail or roll back a failed batch on Windows, because an append-mode handle there lacks the right to truncate (`PermissionDenied`). The store now opens stream files read-write and writes each batch at an explicit offset under its exclusive lock. CI now runs the example workspace's tests on Windows.

Documentation.
- The README has an Upgrading section: how an adopted repository moves its plugin and its vendored CLI (`.claude/temper/`) to a new release, what `temper setup --dry-run` reports, what `--force` overwrites, what to re-run, and what to review in the diff.
- New [`docs/troubleshooting.md`](docs/troubleshooting.md), linked from the README. It covers:
  - every hook denial, question and Stop block, and how to clear it;
  - `TEMPER_DISABLE=1` as the human emergency switch, and how to set it, since hooks inherit Claude Code's environment;
  - gate timeouts on large workspaces: the 900 s step timeout against the 1200 s hook timeout, and warming the cache;
  - Windows notes;
  - how to report a problem.

Release engineering.
- `tests/release-check.sh` makes the release checklist's version checks executable (#12). It fails when `plugin.json`, the top `## X.Y.Z (date)` CHANGELOG heading, the example workspace's vendored `VERSION` and, given `--tag`, the tag disagree. It also fails when a release that changes `scripts/` or `templates/workspace/` since the previous tag has no closing "Upgrading from" note, and, with `--tag`, when an `## Unreleased` section sits above the release. `tests/ci.sh` runs it without the tag.
- A Release workflow (`.github/workflows/release.yml`, #17), run by hand on a `release/X.Y.Z` branch. It runs the release check with the tag, the manifest validation and `tests/ci.sh`. After the owner approves in the protected `release` environment, it tags `vX.Y.Z` and creates the GitHub release from the CHANGELOG section. The owner then merges the release PR to `main`, which is what ships. `tests/release-check.sh --notes FILE` writes that section.

Process.
- `CONTRIBUTING.md` has a versioning policy (#18): what makes a temper release major, minor or patch for an adopted repository.

Compatibility.
- The README now states the minimum Claude Code version: 2.1.286. The smoke test passes on 2.1.286 and 2.1.288, and every hook in `hooks/hooks.json` was confirmed to run on both, with one exception. The `PreToolUse` `SubagentHandback` hook never runs on these versions. The reviewer's verdict is validated and recorded by `SubagentStop` instead, which also blocks an invalid verdict, so nothing is lost. The results are in `docs/history/evals.md`.
- `tests/e2e/smoke.sh` ends with a table of which registered hooks ran. It takes `CLAUDE_BIN` to test a pinned release, and it no longer inherits `TEMPER_BASE` from the session that runs it.

Upgrading from 1.2.x: `scripts/temper.mjs` and `scripts/lib/project.mjs` changed, so `temper setup --dry-run` reports the vendored copy as drift. The workspace template's `.github/workflows/rust.yml` (the `msrv` job) and `AGENTS.md` (the lint command) changed too. Refresh with `temper setup --force`, restore the template files you customised, and merge the `rust.yml` and `AGENTS.md` changes into your copies by hand. Commit the result as its own pull request. Or run `temper setup --vendor-only`, which refreshes just the vendored CLI, and merge the two template changes by hand. The README's Upgrading section has the steps.

## 1.2.3 (2026-10-01)

Windows fixes.
- `temper baseline --init` on an unformatted tree named the file by the verbatim path cargo reports on Windows (`//?/C:/.../src/lib.rs`) instead of `src/lib.rs`. Paths are now made relative after stripping the `\\?\` prefix.
- Paths under a directory with an 8.3 short name (`C:\Users\RUNNER~1\...`, as TEMP often is) were reported relative to the long name git gives the root, as `../../RUNNER~1/...`. Both are now compared in their long form when they differ. This affected every relative path temper prints or keys on, including the nested-workspace hint of `baseline --init`.
- `temper report` and `temper setup` in a copy of temper without `report.mjs` or `vendor.mjs` crashed with `ERR_MODULE_NOT_FOUND` on Windows instead of saying the command is not available in this copy.
- The unit tests skip their symlink cases on a host that cannot create symlinks (Windows without Developer Mode), and CI runs them on Windows.

## 1.2.2 (2026-10-01)

The marketplace in this repository is named `temper`, so the install is now `claude plugin marketplace add Wet-Ink-Corporation/temper`, then `claude plugin install temper@temper`. The `wet-ink` name stays free for a marketplace of all Wet Ink plugins (Weigh-In: wi-d75e63).
- The workspace and example templates' `.claude/settings.json` declare the `temper` marketplace and enable `temper@temper`.
- A repository set up with an earlier version keeps `wet-ink` in its `.claude/settings.json`. Rename the `extraKnownMarketplaces` key to `temper` and the `enabledPlugins` entry to `temper@temper`.

## 1.2.1 (2026-09-28)

Documentation only.
- The README's Brownfield adoption section now explains why a raised baseline is flagged but never fails the build. Inside Claude Code a raise is stopped at the source; anywhere else it is a question for the reviewer.
- It also gives a one-line CODEOWNERS recipe for repositories that want a hard sign-off, with its costs: it gates `--tighten` too, a sole owner cannot approve their own pull request, and private repositories need a paid GitHub plan.

## 1.2.0 (2026-09-28)

This release makes temper usable on existing codebases and reports compliance.

### Lint baseline (brownfield ratchet)

Before this release the gate, CI and the pre-commit hook ran `cargo clippy -- -D warnings` over the whole workspace. Merging the lint table into a legacy repository therefore turned every agent turn and every CI run red. The scanner and the manifest check already blocked only new violations; clippy and rustdoc did not. The README's claim that legacy code does not stop work was wrong for them.

**Recording.** `temper baseline --init` records clippy, rustdoc and scanner findings per crate and lint in `.claude/temper-baseline.json`, which you commit. It also records the crate directories and the host platform.
- Findings are collected with `--cap-lints warn`, so every crate is checked even when its dependencies have deny-level findings.
- Findings raised inside a macro are attributed to the call site.
- `--init` refuses an unformatted tree.

**The ratchet.** With a baseline, the gate, CI and the pre-commit hook fail only on:
- a finding in a line added, modified or moved since the session base or the pull request's merge base (renames are recognised, including unstaged ones and renamed crates);
- a crate's count for a lint rising, which catches a new warning on an untouched line;
- an `#[allow]` or `#[expect]` of a lint the workspace forbids.

Legacy findings are reported, not blocking.

**Without a baseline**, `baseline --check` requires zero findings, as `-D warnings` did. CI therefore always runs one lint step through the vendored CLI.

**Changing the baseline.**
- `--tighten` lowers counts after debt is paid and never raises them. It refuses on a host other than the one the baseline records, since counts lowered there can fail CI on untouched lines; `--any-host` overrides that for workspaces with no platform-specific code.
- `--update` raises them. That is a human decision (H-12): the CLI refuses it inside an agent session unless `TEMPER_ALLOW_RAISE=1` is set, and the Bash hook asks. The hook also asks on restoring an older baseline with `git checkout` or `git restore`, and it refuses shell writes to the file.
- The gate and CI compare the baseline with the base's copy and flag any raise in the gate summary, the pull-request report and the step summary. A raise is shown, never failed, so a human-approved `--update` still merges.

**Rule mapping.** Findings map to rule IDs and levels through `scripts/lib/rulemap.json`, which is generated from `rules.md` and checked for freshness by `tests/lint-claims.py`.

**Tests are not baselined.**

### Compliance report

`temper report` writes `report.json`, `report.md` and a self-contained `report.html`. It contains:
- findings by severity, with iron-rule findings called out;
- a per-crate scorecard: status (`clean`, `debt` or `regressed`), counts, new and fixed findings against the baseline, lint inheritance, charter presence and top rules;
- the top rules and files;
- findings in new code;
- any baseline raise.

CI publishes it on every run as an artifact and a step summary. A separate job, the only one with `pull-requests: write`, keeps one pull-request comment up to date.

### Distribution and setup

- `/temper:setup` copies the CLI into the repository at `.claude/temper/`, so CI and the pre-commit hook run it without the plugin.
- Setup now offers to record a baseline for existing code, after checking formatting and tests.
- `/temper:audit` builds its numbers from the report.

### CI template

- The gate job uses `fetch-depth: 0` and a single `Lint (temper)` step, with plain clippy only when the vendored CLI is absent. It runs a report step and uploads the report as an artifact.
- The pull-request comment lives in its own `report-comment` job.
- Every checkout sets `persist-credentials: false`.
- The MSRV, latest-deps and aarch64 jobs no longer fail on legacy warnings when a baseline exists.

### Fixes

- The tree fingerprint now includes the resolved commit. Before, every clean tree had the same fingerprint, so a gate result cached for one commit could be reused for another. A base from another repository is now ignored instead of producing an empty diff.

## 1.1.0 (2026-09-28)

The standard was reviewed against ten outside sources: the Rust API Guidelines, Effective Rust, the Rust Performance Book, Rust Atomics and Locks, Rust Design Patterns, the Cargo Book, the std docs, Miri, Microsoft's Pragmatic Rust Guidelines and leonardomso/rust-skills. `skills/rust-craft/references/sources.md` records what was taken from each source, and what was left out and why.

### Standard (rust-craft)

**18 new rules** (182 in total):

| Area | Rule | What it requires |
|---|---|---|
| Ownership | OWN-17 | No out-parameters. |
| API | API-21 | No trait bounds on type definitions. |
| API | API-22 | Operators never overflow. |
| API | API-23 | No `Arc`/`Mutex` in public signatures. |
| Errors | ERR-13 | Explicit `finish(self) -> Result`; `Drop` only logs. |
| Errors | ERR-14 | Hand the consumed argument back in the error. |
| Architecture | ARC-16 | Every crate builds on its own. |
| Architecture | ARC-17 | Build-script rules. |
| Tests | TST-17 | Miri and aarch64 CI for concurrency code. |
| Tests | TST-18 | Pin the dependency behaviour you rely on. |
| Performance | PERF-13 | Cache-line padding, only on measured contention. |
| Concurrency | ASY-13 | Atomic ordering justification. |
| Concurrency | ASY-14 | Lock discipline, including guards in `if let`/`match` scrutinees, which edition 2024 did not fix. |
| Concurrency | ASY-15 | `thread::Builder` instead of the panicking `thread::spawn`. |
| Concurrency | ASY-16 | `Condvar::wait_while`, and no spinning. |
| Concurrency | ASY-17 | No home-made primitives. |
| Craft | CRF-11 | Docs describe the contract, not the history. |
| Process | PROC-13 | Never hand-edit `Cargo.lock`. |

**About 90 existing rules sharpened.** The largest changes:
- The panicking-APIs table grew by about 25 verified entries, and one entry was corrected: `Instant - Instant` saturates.
- The memory-ordering cheat sheet was replaced; "unsure → `SeqCst`" was wrong.
- Other additions:
  - `as_`/`to_`/`into_` cost semantics;
  - readers and writers taken by value;
  - dyn-compatibility guards;
  - one public path per item;
  - no `# Arguments` doc sections;
  - caret-floor dependency requirements;
  - profile defaults versus per-application choices;
  - `#[non_exhaustive]` only at published boundaries.

**Examples.**
- Fixed examples that failed the template's own gate (PERF-03 `chunks_exact`) or taught a bug:
  - the ASY-10 scope panic;
  - the Decider `evolve` wildcard;
  - parsers returning `Option`.
- Full sketches now live once in `patterns.md`, with ten new patterns (23–32).

**`SKILL.md`.** The iron rules, design defaults and router were updated. It is still under the 5k-token re-attachment budget.

### Enforcement

**Template lints.**
- rustc: `ambiguous_negative_literals`, `redundant_imports`.
- clippy:
  - `needless_collect`
  - `or_fun_call`
  - `significant_drop_in_scrutinee`
  - `debug_assert_with_mut_call`
  - `future_not_send`
  - `too_long_first_doc_paragraph`
  - `missing_assert_message`
  - `assertions_on_result_states`
  - `renamed_function_params`
  - `empty_drop`
  - `negative_feature_names`
  - `redundant_feature_names`
  - `cargo_common_metadata`

**`clippy.toml`.**
- `disallowed-methods`: `thread::spawn`, `Scope::spawn`, `env::args`, `env::vars`, float `Duration` constructors and arithmetic.
- `await-holding-invalid-types`: tracing span guards, `watch::Ref`.
- `upper-case-acronyms-aggressive`.

**Profiles.** New `dev` (line tables only; no debug info for dependencies), `profiling` and `bench` profiles.

**Scanner.**
- New rules:
  - `unwrap` inside doctest fences, which clippy never sees;
  - silent `Err` arms;
  - `thread::spawn`;
  - `compiler_fence`;
  - `#[global_allocator]` in libraries;
  - advisories for `map[&k]`, `SeqCst`, `fetch_update`, glob re-exports, struct bounds, operator impls, `get_` getters, primitive type aliases and more.
- Compile-time assertions are no longer flagged.
- `cfg_attr(.., ignore)` without a reason is now caught.

**Hooks.**
- A member `Cargo.toml` that does not inherit the workspace lints, or that declares its own `[profile]`, is refused. Previously a new crate silently got none of the escape-hatch lints. The check is a ratchet: only new violations block. Human-approved exceptions are listed in `.claude/temper.json` under `manifests.ownLints`.
- Asks before:
  - manifest switches that stop tests (`test`, `doctest`, `harness`, `autotests`);
  - `[patch]`/`[replace]`;
  - `rust-version`/`edition` changes;
  - new crates;
  - dependency major-version bumps;
  - new `#[ignore]`s;
  - `cargo add`/`cargo remove`.
- Refuses:
  - `Cargo.lock` edits;
  - shell writes to `Cargo.lock`/`Cargo.toml`;
  - `--ignore-rust-version`.

**Gate.** A `manifests` step checks lint inheritance and member profiles. It also runs when cargo is unavailable.

**CI template.**
- Actions pinned to commit SHAs, `permissions: contents: read`, and `--locked` everywhere.
- The MSRV job now really runs 1.96. It used to be overridden by `rust-toolchain.toml`.
- New steps: a per-crate build (ARC-16), `cargo hack` feature powerset, `cargo machete`.
- Scheduled latest-deps, aarch64 and Miri jobs.
- Dependabot, with `lockfile-only`.
- `deny.toml` fixed so `cargo deny check` passes on real dependency trees.

### Example workspace

- It meets every new rule.
- `evolve` enumerates impossible `(state, event)` pairs.
- Writers are taken by value.
- Exit codes are mapped through a `match`, and the deviation from the house convention is recorded in the charter.
- Crate roots have a quick start and a list of features.
- It passes under Miri: no UB, no leaks, no deadlocks.

### Verification

- `tests/doc_examples.py`: every Do block in the skill (329 blocks) compiles, passes clippy and runs its tests under the template lints. Every annotated Don't block fires the diagnostic it claims.
- `tests/registry.py`: every rule has exactly one section, every cited ID exists, and the counts match.
- `tests/lint-claims.py`: now also checks Detect lines and scanner claims by rule ID.
- New `tests/hooks.test.mjs` and `tests/toml.test.mjs`.

## 1.0.0 (2026-09-27)

Initial release:
- the rust-craft skill (164 rules);
- the implementer, reviewer and architect subagents;
- the skill gate, pre-write scanner, protected config and fingerprint-bound completion gate;
- the workspace and example templates;
- evals.
