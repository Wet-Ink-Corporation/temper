# Hook runtime and Node.js availability across Claude Code surfaces (as of 2026-10-01)

Source labels used in this file: **[DOC]** is official Anthropic documentation fetched 2026-10-01. **[CHANGELOG]** is an official changelog. **[ISSUE]** is a user report on anthropics/claude-code. **[3P]** is a third-party write-up. **[REPO]** is Anthropic's own plugin source code. **Inference** marks conclusions drawn from those sources.

## Q1. Do the official docs state anything about Node availability or the hook runtime?

### Takeaway
The docs never promise that `node` (or python, or jq) is present for hooks on a user's machine. They document only the shell that runs hooks (`sh -c` on macOS/Linux; Git Bash on Windows, or PowerShell when Git Bash is missing), the timeouts, and the env vars. Since npm v2.1.198 the Claude Code CLI is the same native binary however it is installed, and that binary "does not itself invoke Node". Node is no longer a prerequisite of Claude Code itself. The only environment that documents a preinstalled Node is the Anthropic-hosted cloud VM (Claude Code on the web): Ubuntu 24.04 with Node 22 on PATH.

### Cited Findings
**Shell and exec model [DOC]**
- Shell form: "The `command` string is passed to a shell: `sh -c` on macOS and Linux, Git Bash on Windows, or PowerShell when Git Bash isn't installed." — [Hooks reference](https://code.claude.com/docs/en/hooks)
- The `shell` field: "Accepts `"bash"` or `"powershell"`. Defaults to `"bash"`, or to `"powershell"` on Windows when Git Bash isn't installed… Ignored when `args` is set". — [Hooks reference](https://code.claude.com/docs/en/hooks)
- Exec form, used when `args` is present: "Claude Code resolves `command` as an executable on `PATH` and spawns it directly with `args` as the argument vector. There is no shell". — [Hooks reference](https://code.claude.com/docs/en/hooks)
- Windows exec-form note: ".cmd and .bat shims… can't be spawned without a shell… The `node` plus script-path pattern works on every platform because `node.exe` is a real binary." The docs use `node` in examples but never say it is installed. — [Hooks reference](https://code.claude.com/docs/en/hooks)
- The docs require jq for their own examples: "uses `jq`, so install `jq` and make sure it is on your `PATH` before trying them." — [Hooks reference](https://code.claude.com/docs/en/hooks)
- "On macOS and Linux, command hooks run in their own session without a controlling terminal… Windows has no `/dev/tty`." — [Hooks reference](https://code.claude.com/docs/en/hooks)
- A missing interpreter or script gives a non-blocking error, for example "`Failed with non-blocking status code: /bin/sh: /path/to/hook.sh: No such file or directory`. For most hook events, the action proceeds." — [Hooks reference](https://code.claude.com/docs/en/hooks)

**Timeouts [DOC]**
- "Defaults: 600 for `command`, `http`, and `mcp_tool`; 30 for `prompt`; 60 for `agent`." The command/http/mcp_tool default drops to 30 s on `UserPromptSubmit`, `PreModelSwitch` and `PostModelSwitch`, and to 10 s on `MessageDisplay`. The timeout is not enforced for `async: true` command hooks. — [Hooks reference](https://code.claude.com/docs/en/hooks)
- SessionEnd hooks share a 1.5 s budget, which rises to the per-hook timeout up to 60 s. Source: a WebFetch summary of the hooks page; this sentence was not re-checked verbatim. — [Hooks reference](https://code.claude.com/docs/en/hooks)

**Environment variables [DOC]**
- Hook processes inherit the parent environment, minus `OTEL_*` exporter variables, and minus credentials when `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1`. — [Hooks reference](https://code.claude.com/docs/en/hooks)
- Hooks receive `CLAUDE_PLUGIN_ROOT`, `CLAUDE_PLUGIN_DATA`, `CLAUDE_PROJECT_DIR` and `CLAUDE_PLUGIN_OPTION_<KEY>` in their environment. Monitor commands do not receive them. — [Plugin manifest reference](https://code.claude.com/docs/en/plugins-reference)
- Other hook variables:
  - `CLAUDE_CODE_REMOTE` is "`true` when Claude Code is running as a cloud session".
  - `CLAUDE_CODE_REMOTE_SESSION_ID` is set in cloud sessions.
  - `CLAUDE_ENV_FILE` is available to SessionStart, Setup, CwdChanged and FileChanged hooks.
  - `CLAUDE_EFFORT` and `CLAUDE_CODE_BRIDGE_SESSION_ID` (v2.1.199+).
  — [Env vars](https://code.claude.com/docs/en/env-vars); [Hooks reference](https://code.claude.com/docs/en/hooks)
- `CLAUDE_CODE_ENTRYPOINT` does not appear on the env-vars reference page. Only `OTEL_METRICS_INCLUDE_ENTRYPOINT` is documented. — [Env vars](https://code.claude.com/docs/en/env-vars)
- On Windows, "the substituted paths use forward slashes so a shell doesn't read backslashes as escapes." — [Plugin manifest reference](https://code.claude.com/docs/en/plugins-reference)

**CLAUDE_PLUGIN_ROOT and CLAUDE_PLUGIN_DATA [DOC]**
- `${CLAUDE_PLUGIN_ROOT}` is the "Absolute path of the plugin's installed version". It changes when the plugin updates ("don't write state there"). Old version directories get an `.orphaned_at` marker and are deleted 14 days later. — [Plugin manifest reference](https://code.claude.com/docs/en/plugins-reference); [Plugin loading](https://code.claude.com/docs/en/plugins/loading)
- `${CLAUDE_PLUGIN_DATA}` resolves to "`~/.claude/plugins/data/<id>/`, created on first reference and kept across plugin updates". It is meant for "Installed dependencies such as `node_modules`, generated code, and caches". It is deleted when the plugin is uninstalled from its last scope, unless `--keep-data` is used. — [Plugin manifest reference](https://code.claude.com/docs/en/plugins-reference)
- The plugins root can be moved with `CLAUDE_CODE_PLUGIN_CACHE_DIR`. — [Plugin loading](https://code.claude.com/docs/en/plugins/loading)

**Plugin `bin/` [DOC]**
- "Files here are on the Bash tool's `PATH` while the plugin is enabled, so Claude runs them as bare commands. claude.ai and Cowork don't install a plugin that has this directory". — [Plugin manifest reference](https://code.claude.com/docs/en/plugins-reference)
- The platform-support table rates "Executables in a top-level `bin/` directory" as "Can't be installed" in Chat and Cowork and "Loads" in Claude Code. "Can't be installed" means the surface refuses the whole plugin. — [Platform support](https://claude.com/docs/plugins/platform-support)
- Related CLI changelog entries [CHANGELOG]:
  - "Plugins can now ship executables under `bin/` and invoke them as bare commands from the Bash tool".
  - "Fixed Bash tool failing on Windows with many plugins enabled: plugin `bin/` directories that don't exist are no longer added to PATH".
  — [CHANGELOG.md](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md)

**Automatic Node dependency install [DOC]**
- When Claude Code copies a plugin into the cache, it installs the plugin's npm or Bun dependencies. This needs a `package.json` plus `package-lock.json`, `npm-shrinkwrap.json` or `bun.lock`.
- "Claude Code runs the matched lockfile's package manager from the user's PATH and doesn't try the other lockfile instead if that package manager is missing."
- Limits: `--ignore-scripts`, registry-only dependencies, 60 s timeout. "A failed or skipped install never blocks the plugin".
- The install does not run for plugins loaded in place, such as `--plugin-dir` or a local-directory marketplace.
— [Plugin loading](https://code.claude.com/docs/en/plugins/loading)

### Inferences
- Even the automatic dependency install assumes the user already has npm or bun on PATH. Anthropic does not supply Node for it. A plugin that relies on `node` must still handle Node being absent.
- Plugin `bin/` is documented only as being on the **Bash tool's** PATH. The docs do not say it is on the PATH of hook processes. Hooks should call the binary by its full path, `"${CLAUDE_PLUGIN_ROOT}/bin/x"`.

### Gaps
- No doc page states which binaries are guaranteed for hooks on a local machine: coreutils, curl, git, python3. These depend on the host OS.
- Whether hook processes get the plugin `bin/` directory on PATH is not documented either way.

## Q2. Native installer vs npm install: is Node bundled or exposed to hooks? Is Node still a prerequisite?

### Takeaway
No and no. The native install is a single compiled binary with Bun embedded inside it. That runtime is not exposed as `node` or `bun` on PATH. An undocumented Bun escape hatch (`BUN_BE_BUN=1`) stopped working in 2.1.35. Since v2.1.198 the npm package just installs the same native binary. Node 22+ is needed only to run `npm install` itself, and the docs list no Node prerequisite for the native, Homebrew, WinGet, apt, dnf or apk installs.

### Cited Findings
- System requirements list OS, hardware, network and shell ("Bash, Zsh, PowerShell, or CMD"). Node is not listed. The only extra dependency named is ripgrep, "usually included". — [Advanced setup](https://code.claude.com/docs/en/setup) [DOC]
- "As of v2.1.198, the npm package requires Node.js 22 or later… the package downloads a native binary that doesn't use your Node.js at runtime." and "The npm package installs the same native binary as the standalone installer… The installed `claude` binary does not itself invoke Node." — [Advanced setup](https://code.claude.com/docs/en/setup) [DOC]
- Native layout on macOS/Linux: launcher at `~/.local/bin/claude`, symlinked into `~/.local/share/claude/versions/`. On Windows: `%USERPROFILE%\.local\bin\claude.exe`. — [Advanced setup](https://code.claude.com/docs/en/setup) [DOC]
- Feature request #30465 (closed NOT_PLANNED by the inactivity bot, 2026-03-31): "Native install users… have Bun embedded inside the `claude` binary but no `bun` or `node` on their PATH… there is no **guaranteed runtime** available for executing those scripts." — [Issue #30465](https://github.com/anthropics/claude-code/issues/30465) [ISSUE]
- Issue #24575 (closed NOT_PLANNED, 2026-03-11): "`BUN_BE_BUN=1 claude`… until 2.1.34: this shows bun version; since 2.1.35: this shows claude version". — [Issue #24575](https://github.com/anthropics/claude-code/issues/24575) [ISSUE]
- Earlier (Feb 2026), a user reported that `BUN_BE_BUN=1 claude` "already works". This was superseded by #24575. — [Issue #25535](https://github.com/anthropics/claude-code/issues/25535) [ISSUE]
- Issue #90315 (open, 2026-08-28) confirms the Windows binary embeds Bun. Windows CLI 2.1.250 "launches the embedded Bun CLI": `claude --version` printed `1.4.1`, and the file metadata showed `InternalName: bun`. — [Issue #90315](https://github.com/anthropics/claude-code/issues/90315) [ISSUE]

### Inferences
- A hook command that starts with `node` works only where the user has installed Node themselves. That is likely for npm-install users and developers, and not guaranteed for anyone else, especially on the native or WinGet installs that Anthropic recommends.
- Reusing Claude Code's embedded Bun from a hook (`BUN_BE_BUN=1 "$(which claude)" …`) is unsupported and has been broken since 2.1.35. Do not rely on it.

### Gaps
- I found no GitHub issue titled literally "node: command not found" in hooks after switching to the native installer. The searches returned nothing direct, so this failure mode is inferred rather than reported.

## Q3. Claude Code on the web / cloud sessions: image, runtimes, network

### Takeaway
Anthropic-hosted cloud sessions are the one place where Node is documented as present. Each session is a fresh Ubuntu 24.04 x86_64 VM with Node 20, 21 and 22 installed (22 on PATH), plus Python 3, git, gh and jq. Network defaults to "Trusted", which allows github.com and registry.npmjs.org. Cloud sessions do not install plugins from the repo or user settings, so most plugin hooks never reach this surface. Hooks committed in the repo's `.claude/settings.json` do run.

### Cited Findings
**Image and tools [DOC]**
- "each session gets a fresh virtual machine (VM) running Ubuntu 24.04 on x86_64, regardless of your own operating system and CPU architecture". — [Cloud environments](https://code.claude.com/docs/en/cloud-environments)
- Installed tools:
  - Node.js: "20, 21, and 22, with npm, yarn, pnpm, bun¹, eslint, prettier, chromedriver".
  - "Node.js versions are installed at `/opt/node20`, `/opt/node21`, and `/opt/node22`, with 22 on `PATH` by default."
  - Python: "Python 3.x with pip, poetry, uv…".
  - Utilities: "git, gh, jq, yq, ripgrep, tmux, vim, nano".
  - Also present: Go, Rust, Java 21, Ruby, PHP, Docker, PostgreSQL 16 and Redis 7.
  — [Cloud environments](https://code.claude.com/docs/en/cloud-environments)
- "Scripts run as root on Ubuntu 24.04". This refers to setup scripts. — [Cloud environments](https://code.claude.com/docs/en/cloud-environments)

**Network [DOC]**
- The default **Trusted** access level reaches "package registries and other allowlisted domains". The allowlist includes github.com, api.github.com, codeload.github.com, registry.npmjs.org, pypi.org and nodejs.org.
- All outbound traffic passes through a security proxy, and "Bun is a known example" of a package manager that doesn't work with it.
- With **None** network access, installs and hooks that fetch fail.
— [Cloud environments](https://code.claude.com/docs/en/cloud-environments)

**Which hooks and plugins carry over [DOC]**
- Repo `.claude/settings.json` hooks: "Yes, in a session with one repository".
- "Plugins and marketplaces declared in your repo's `.claude/settings.json`: No — A cloud session doesn't install the plugins a repository turns on under `enabledPlugins`".
- "Plugins enabled only in your user settings: No".
- Hooks from server-managed settings also run, except in Claude Tag sessions.
— [Cloud environments](https://code.claude.com/docs/en/cloud-environments)
- Hook timeout in the cloud: "Claude Code cancels a `command` hook after 600 seconds unless you set `timeout`". The docs suggest gating cloud-only work with `if [ "$CLAUDE_CODE_REMOTE" != "true" ]`. — [Cloud environments](https://code.claude.com/docs/en/cloud-environments)
- Self-hosted environments use "the tools your runner image provides". — [Cloud environments](https://code.claude.com/docs/en/cloud-environments)

### Inferences
- A plugin hook could only run in a web cloud session if the plugin arrives some other way, for example synced from claude.ai or seeded. Whether synced plugins load in Claude Code on the web is not stated, though skills enabled on claude.ai are.

### Gaps
- The docs do not say whether claude.ai-synced plugins, and so their hooks, load in Claude Code on the web.

## Q4. Claude Cowork: where hooks run, image, Node, network

### Takeaway
Cowork has two execution modes, and from **2026-10-06** new tasks run only in the cloud.
- **Cloud (default):** an Anthropic-hosted container, reported as `CLAUDE_CODE_ENTRYPOINT=remote_cowork` and `CLAUDE_CODE_REMOTE_ENVIRONMENT_TYPE=cloud_default`.
- **Local:** an isolated Linux VM on the user's machine. The "Only on your computer" option is being removed.

In both modes Claude Code inside the VM is the **native ELF binary**, not npm. There is good but indirect evidence that Node 22 is preinstalled in the Cowork workspace:
- a January 2026 3P inspection of the local VM;
- a September 2026 Cowork changelog fix for "preinstalled Node.js packages… not being found by `require()`".

No official doc lists Cowork's tools or guarantees `node` on PATH for hooks. Egress is filtered and is not configurable the way Claude Code on the web is. Plugin hooks load in Cowork, and plugins with a top-level `bin/` are refused.

### Cited Findings
**Execution location and dates**
- "Cowork runs your tasks in the cloud (in beta). Claude's work runs on Anthropic's servers, in an isolated environment… Shell commands and code Claude writes run inside that environment." — [Get started with Cowork](https://support.claude.com/en/articles/13345190-get-started-with-claude-cowork) [DOC]
- "On October 6, 2026, new Cowork tasks run in the cloud and the Only on your computer option in Settings > General will be removed. Tasks you already started on your computer stay there." — [Get started with Cowork](https://support.claude.com/en/articles/13345190-get-started-with-claude-cowork) [DOC]
- Local mode, Claude for Government doc: "the Claude Desktop application runs shell commands and model-written code inside a dedicated local virtual machine… Code sessions run on the workstation itself rather than in the virtual machine". — [Gov security and data handling](https://claude.com/docs/government/security/security-and-data-handling) [DOC]

**Plugin components in Cowork [DOC]**
- Hooks: "Loads" in Cowork.
- Local (stdio) MCP servers: "Loads when the Cowork session runs on your computer". This implies they do not load in cloud Cowork.
- `bin/`: "Can't be installed".
- MCP servers that reference `${user_config.*}`: ignored when no default is set ("Cowork doesn't prompt for values").
— [Platform support](https://claude.com/docs/plugins/platform-support)
- "Cowork loads the ones enabled for your claude.ai account, synced at session start, and doesn't read the Claude Code CLI's `~/.claude` directory". — [Cowork overview](https://claude.com/docs/cowork/overview) [DOC]
- Synced plugins in Cowork: "Claude Code downloads them into the session's own environment when the session starts". — [Plugin loading](https://code.claude.com/docs/en/plugins/loading) [DOC]
- Desktop 3P doc: "Cowork sessions run hooks from marketplace plugins, from plugins in the `org-plugins/` directory, and from plugins users add themselves." — [Desktop 3P extensions](https://claude.com/docs/third-party/claude-desktop/extensions) [DOC]

**Node in the Cowork workspace**
- The Cowork changelog for v2.2553.0 (2026-09-17, bundled Claude Code 2.1.274) has, under the Cowork heading: "Fixed preinstalled Node.js packages such as `pptxgenjs`, `docx`, `pdf-lib` and `sharp` not being found by `require()`." — [Cowork changelog](https://claude.com/docs/cowork/changelog) [CHANGELOG]
- 3P inspection of the local Cowork VM, 2026-01-26 (dated, predates the cloud default):
  - OS: "Ubuntu 22.04.5 LTS" on aarch64, 4 vCPU, 3.8 GB RAM.
  - Claude Code as a native ELF binary at `/usr/local/bin/claude` (v2.1.15), not via npm.
  - Python 3.10.12 and Node.js 22.22.0.
  - Traffic goes through a local proxy allowlist that includes `api.anthropic.com`, `pypi.org` and `registry.npmjs.org`. Direct DNS is blocked.
  — [pvieito.com, Inside Claude Cowork](https://pvieito.com/2026/01/inside-claude-cowork) [3P]
- Issue #40495 (2026-03-29) also describes the local sandbox as "a Linux VM (Ubuntu 22.04 aarch64)", with `CLAUDE_CONFIG_DIR=/sessions/<name>/mnt/.claude` and `CLAUDE_CODE_HOST_PLATFORM=darwin`. — [Issue #40495](https://github.com/anthropics/claude-code/issues/40495) [ISSUE]

**Cloud Cowork environment [ISSUE]**
- Issue #88989 (2026-08-23) prints env flags from the Cowork container: `CLAUDE_CODE_ENTRYPOINT=remote_cowork`, `CLAUDE_CODE_REMOTE_ENVIRONMENT_TYPE=cloud_default`, `CCR_EGRESS_GATEWAY_ENABLED=1`, `CCR_UPSTREAM_PROXY_ENABLED=1`. It reports that the cloud_default egress blocks `*.frame.claudeusercontent.com`, and that the claude.ai/code "Network access → Custom → Allowed domains" setting "does not apply to the managed cloud_default environment Cowork actually runs in". — [Issue #88989](https://github.com/anthropics/claude-code/issues/88989)
- Issue #90483 (2026-08-29): "Claude Code 2.1.42 [sic], entrypoint `remote_cowork` (Cowork session in an Anthropic-hosted cloud container…)". It reports `nproc` 2 and about 8 GB RAM, versus the documented 4 vCPU and 16 GB for cloud sessions. — [Issue #90483](https://github.com/anthropics/claude-code/issues/90483)
- An open issue title (#84581) reports: "Cowork cloud sessions cannot access any GitHub repository, and the git proxy instructs the agent to call a nonexistent add_repo tool". I did not read the body. — [Issue #84581](https://github.com/anthropics/claude-code/issues/84581)

**Egress controls [DOC] [CHANGELOG]**
- "Cowork respects your current network egress permissions", except for web fetch, web search and MCPs. — [Get started with Cowork](https://support.claude.com/en/articles/13345190-get-started-with-claude-cowork) [DOC]
- On managed or 3P desktop, admins set `coworkEgressAllowedHosts` for "shell commands and package installs in Cowork sessions". — [Cowork changelog](https://claude.com/docs/cowork/changelog) [CHANGELOG]

**History of Cowork hook bugs (dated)**
- #27398 (Feb 2026, closed 2026-02-25): plugin hooks never fired because the VM spawned the CLI with `--setting-sources user`. — [Issue #27398](https://github.com/anthropics/claude-code/issues/27398) [ISSUE]
- #51281 (Apr 2026, closed 2026-05-27): hooks.json silently ignored on Windows MSIX. — [Issue #51281](https://github.com/anthropics/claude-code/issues/51281) [ISSUE]
- Cowork changelog, 2026-07-24: "Fixed plugin hooks silently doing nothing on Windows." — [Cowork changelog](https://claude.com/docs/cowork/changelog) [CHANGELOG]
- Cowork changelog, 2026-09-10: "Changed hooks from organization plugins to also run in Chat, matching Cowork and Code." — [Cowork changelog](https://claude.com/docs/cowork/changelog) [CHANGELOG]
- #40495 (open): user `~/.claude/settings.json` hooks and MDM managed settings are ignored in local Cowork. — [Issue #40495](https://github.com/anthropics/claude-code/issues/40495) [ISSUE]

### Inferences
- On Linux Cowork, both cloud and local VM, hooks run via `sh -c`. The Windows Git Bash and PowerShell question does not arise for Cowork hooks, because the workspace is Linux even on Windows hosts.
- `node` on PATH for Cowork hooks is **likely but undocumented**. Treat it as observed behaviour, not a contract.
- Fetching from github.com in cloud Cowork may fail or be proxied (#84581, #88989), so hooks should not depend on downloading release assets at runtime there.
- `CLAUDE_PLUGIN_DATA` in cloud Cowork lives inside an ephemeral per-session environment. It probably does not persist across sessions, since plugins are downloaded fresh at session start.

### Gaps
- No official list of the Cowork cloud (`cloud_default`) image's OS, architecture or tools. It is unknown whether it matches the documented Claude Code on the web image (Ubuntu 24.04 x86_64, Node 22).
- No official statement on whether `CLAUDE_PLUGIN_DATA` persists between Cowork sessions.
- The default Cowork egress allowlist (github.com, objects.githubusercontent.com, etc.) is not published.

## Q5. Claude Desktop Code tab

### Takeaway
The Code tab runs Claude Code bundled in the Desktop app, directly on the host OS. Its hook runtime is the same as the CLI's (`sh -c`, or Git Bash/PowerShell on Windows). `node` is available only if the user installed it and the app picked up the PATH. On macOS the app reads PATH from the shell profile. On Windows it inherits the user and system environment.

### Cited Findings
- "Code sessions use Claude Code built into the desktop application and run on the user's workstation itself, not in the virtual machine." — [Gov security and data handling](https://claude.com/docs/government/security/security-and-data-handling) [DOC]
- The Desktop release v2.2553.0 states "Bundled Claude Code version: 2.1.274." — [Cowork changelog](https://claude.com/docs/cowork/changelog) [CHANGELOG]
- "On macOS, when you launch the app from the Dock or Finder, it reads your shell profile, such as `~/.zshrc` or `~/.bashrc`, to extract `PATH`… On Windows, the app inherits user and system environment variables but does not read PowerShell profiles." — [Desktop docs](https://code.claude.com/docs/en/desktop) [DOC]
- Troubleshooting: "If Claude can't find tools like `npm`, `node`… check that your shell profile properly sets up PATH, and restart the desktop app". — [Desktop docs](https://code.claude.com/docs/en/desktop) [DOC]
- Code sessions run hooks from marketplace plugins (Desktop 1.32352.0+) and from user-installed Claude Code plugins. They do not run hooks from `org-plugins/`, and in remote SSH sessions they do not run hooks from plugins copied to the host. — [Desktop 3P extensions](https://claude.com/docs/third-party/claude-desktop/extensions) [DOC]
- The Desktop app embeds a Node runtime for its own extensions ("extensions that use the built-in Node.js runtime", 2026-09-17). That runtime is for the Electron host, not hooks. — [Cowork changelog](https://claude.com/docs/cowork/changelog) [CHANGELOG]

### Inferences
- PATH resolution through nvm, volta or fnm may differ from the user's terminal, especially on macOS GUI launches. A hook that needs `node` should probe for it and degrade gracefully.

### Gaps
- No doc states whether the Electron Node is ever exposed to Code-tab hooks. No evidence suggests it is.

## Q6. Windows: shell for hooks, and what it means for `sh` and `node` launchers

### Takeaway
On native Windows, shell-form hooks run under Git Bash when Git for Windows is installed, and under PowerShell otherwise. Git for Windows is optional, so a POSIX `sh` cannot be assumed. Exec form (`command` plus `args`) spawns the executable directly. It needs a real `.exe` on PATH, not `.cmd` or `.bat` shims. `node.exe` qualifies only if Node is installed.

### Cited Findings
- "Installing Git for Windows is optional. It enables the Bash tool by providing Git Bash." and "If Git for Windows is not installed, Claude Code uses PowerShell as the shell tool instead." — [Advanced setup](https://code.claude.com/docs/en/setup) [DOC]
- `CLAUDE_CODE_GIT_BASH_PATH` points to bash.exe. An invalid value is ignored and auto-detection is used; before v2.1.219 an invalid value made Claude Code exit at startup. — [Env vars](https://code.claude.com/docs/en/env-vars) [DOC]
- The hook `shell` field defaults to `"powershell"` "on Windows when Git Bash isn't installed". — [Hooks reference](https://code.claude.com/docs/en/hooks) [DOC]
- Historical CLI changelog entry, undated in the excerpt: "Fixed hooks (PreToolUse, PostToolUse) silently failing to execute on Windows by using Git Bash instead of cmd.exe (#25981)". — [CHANGELOG.md](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md) [CHANGELOG]
- Open issue #80039 (2026-07-22): "Windows: PreToolUse hook exit 2 doesn't block the tool call, PostToolUse exit-2 stderr doesn't reach the model". — [Issue #80039](https://github.com/anthropics/claude-code/issues/80039) [ISSUE]
- Anthropic's own `security-guidance` plugin ships a `sg-python.sh` shim. It exists because "On Windows + Git Bash, `python3` typically resolves to the Microsoft Store stub… which exits 49 silently", and because Git Bash passes `/c/Users/...` POSIX paths that Windows python.exe misreads. — [claude-plugins-official/security-guidance](https://github.com/anthropics/claude-plugins-official/tree/main/plugins/security-guidance/hooks) [REPO]

### Inferences
- A hook written as `sh "${CLAUDE_PLUGIN_ROOT}/x.sh"` fails on Windows machines without Git for Windows, because PowerShell has no `sh`.
- A launcher that must work everywhere has two options. It can use exec form pointing at a real native `.exe` shipped in the plugin, with one `.exe` per OS/arch. Or it can provide a `shell: "powershell"` variant.

### Gaps
- I did not find a doc that says exactly which PowerShell edition hooks use when they fall back (`powershell.exe` 5.1 vs `pwsh`).

## Q7. What runtimes do Anthropic's own plugins assume in hooks.json?

### Takeaway
None of Anthropic's official plugins uses `node` in a hook. They use `sh`, `bash` and `python3`, and they guard python with probes and user-facing warnings. This is strong indirect evidence that Anthropic does not treat Node as available to hooks, and treats only a POSIX shell as near-universal.

### Cited Findings
Survey of every `hooks.json` in anthropics/claude-plugins-official (HEAD 2026-09-30) [REPO]:
- `claude-security` and `code-modernization` use `sh "${CLAUDE_PLUGIN_ROOT}/…sh"`.
- `explanatory-output-style`, `learning-output-style` and `ralph-loop` use `bash "…sh"`.
- `security-guidance` uses `bash "…/sg-python.sh" "…py"`.
- `hookify` uses `python3 "…py"`.
- Zero `node` hook commands.
— [claude-plugins-official](https://github.com/anthropics/claude-plugins-official)

The `claude-security` `hooks.sh` probes python3 ≥ 3.9 and otherwise emits a systemMessage [REPO]:
> "Claude Security needs a working python3 (3.9 or newer) on PATH and could not run one. Install Python 3, then start a new session."

— [claude-security/hooks/hooks.sh](https://github.com/anthropics/claude-plugins-official/blob/main/plugins/claude-security/hooks/hooks.sh)

Other repos:
- anthropics/knowledge-work-plugins (the Cowork-oriented marketplace) contains no `hooks.json` files at HEAD. — [knowledge-work-plugins](https://github.com/anthropics/knowledge-work-plugins) [REPO]
- In the same repos, MCP server commands do use `bun` (4) and `npx` (3) in `.mcp.json`/`plugin.json`. Runtime assumptions are therefore looser for MCP servers than for hooks. — [claude-plugins-official](https://github.com/anthropics/claude-plugins-official) [REPO]
- The example plugins in anthropics/claude-code use `python3 ${CLAUDE_PLUGIN_ROOT}/…py` and direct `.sh` paths. — [anthropics/claude-code plugins](https://github.com/anthropics/claude-code/tree/main/plugins) [REPO]

### Inferences
- Anthropic's own pattern for a hook that needs a runtime is a thin POSIX `sh` entry point. It probes for the interpreter, degrades gracefully with a `systemMessage`, and always exits 0. Copying this pattern for a native-binary launcher matches first-party practice.

### Gaps
- No first-party plugin ships a native binary invoked from hooks, so there is no Anthropic precedent for that exact pattern.
