# temper 2.0: port the CLI and hook engine from Node to Rust

## Goal

temper's enforcement engine (the `temper` CLI and every hook handler: about 5,650 lines of Node (1.3.0) in `scripts/`, plus the copy vendored into user repositories) becomes a Rust workspace in this repository that follows the rust-craft standard. It ships as per-platform npm packages that Claude Code installs itself (exact pins, `--ignore-scripts`), with a `cargo build --release --locked` fallback into `${CLAUDE_PLUGIN_DATA}`. Hooks, messages, exit codes and on-disk state behave exactly as in the last 1.x release before the port (1.3.0 today), except for a short list of divergences a human approves (D16). User repositories stop vendoring a Node copy and install a pinned `temper` binary instead.

Settled by Weigh-In wi-30ed25 (`.kb/_intake/decisions/wi-30ed25-port-temper-s-cli-and-hooks-to-rust-shipped-as.md`) and not reopened here: Rust engine, per-platform npm packages with a cargo fallback, a small Node launcher, hooks in exec form (`node <launcher> ...`).

---

## Decisions needed (H-16), up front

Nothing below may be built on until it is decided. The implementer returns `Status: BLOCKED` on any of these. Each one has its trade-offs and a recommendation. Summary:

| # | Decision | Recommendation |
|---|---|---|
| D1 | npm scope and package names | `@wet-ink/temper-<process.platform>-<process.arch>`; plugin-root `package.json` is private and not published |
| D2 | crates.io: publish, and the crate names | Publish 4 crates: `temper-cli` (bin `temper`), `temper-domain`, `temper-app`, `temper-host`. Reserve the names now. |
| D3 | Crate split | 4 crates (domain / app / host / cli) plus an unpublished `xtask` |
| D4 | MSRV | `rust-version = "1.96"` (the template's), toolchain pin 1.98.1 |
| D5 | Release targets | 6: linux x64/arm64 (musl, static), darwin x64/arm64, win32 x64 **and arm64** |
| D6 | Hooks while the fallback build is pending or failed | C, the hybrid: SessionStart builds synchronously; edits fail closed with a retry message; Stop waits, then reports UNAVAILABLE |
| D7 | cargo-dist or a hand-written release workflow | Hand-written workflow plus a Rust `xtask` |
| D8 | Scanner: syn or the token lexer | Port the masking lexer and regex rules faithfully; no syn |
| D9 | Regex engine | `regex` (linear time) plus explicit predicates for the ~10 look-around patterns; not `fancy-regex` |
| D10 | New dependencies | Approve `regex`, `toml` (with `preserve_order`), `sha2`, `wait-timeout`. Do **not** add clap, walkdir, globset, jiff/time or dunce. |
| D11 | Deprecation timeline for the Node path | Node lives on `main` only while the port is in progress, as an opt-in delegate. 2.0.0 ships Rust only. |
| D12 | Publishing credentials and release approval | npm and crates.io trusted publishing (OIDC) from a protected `release` environment with a required human reviewer |
| D13 | How user CI installs temper | `taiki-e/install-action` with `tool: temper-cli@<pin>` (cargo-binstall from GitHub release assets) |
| D14 | Where a user repository pins its temper version | New one-line file `.claude/temper-version` |
| D15 | Migrating 1.x repositories | `temper setup --migrate`: rewrites files only when they match a known 1.x template byte for byte; otherwise prints manual steps |
| D16 | Intentional divergences from 1.x behaviour, and retired tests (H-12) | Approve the list in §2.4 as one decision |
| D17 | Put the binary on PATH in Claude sessions through `CLAUDE_ENV_FILE` | Yes: export `TEMPER_BIN` and prepend its directory to `PATH` |

### D1 · npm scope and package names
- **A) `@wet-ink/temper-linux-x64` and so on, scoped under a new npm org `wet-ink`. Recommended.** It matches the GitHub org, keeps the namespace (an unscoped `temper` is taken on npm), and a scope cannot be squatted package by package. Cost: someone creates and owns the org. Its availability was not verified; the npm org page returned 403 to an anonymous request.
- B) `@wetink/...`: same benefits, if `wet-ink` is unavailable.
- C) Unscoped `temper-cli-linux-x64`, ...: no org needed, but each of the six names can be squatted separately.
- Suffixes are Node's `process.platform` and `process.arch` values, so the launcher computes the name with no lookup table: `linux-x64`, `linux-arm64`, `darwin-x64`, `darwin-arm64`, `win32-x64`, `win32-arm64`.
- The plugin root's `package.json` (`"private": true`, name `temper-claude-plugin`) is the "main package" and is **never published**. Nobody needs `npx temper`; user CI uses cargo (D13).

### D2 · crates.io publishing and names
Checked on 2026-10-01: `temper` and `temper-core` are taken (an unrelated game engine, and Temper-lang). `temper-cli`, `temper-domain`, `temper-app`, `temper-host` and `temper-engine` are free.
- **A) Publish all four, lockstep versioned, with `cargo publish --workspace --locked`. Recommended.** `cargo install temper-cli --locked --version X` then works, and cargo-binstall finds the crate metadata it needs (D13). `cargo install` from crates.io needs every library the binary uses on crates.io, which is why all four are published. Each crate needs `description`, `readme`, `keywords` and `categories` (ARC-01 publishing; `cargo_common_metadata` fires once `publish` is on).
- B) Publish nothing. Users run `cargo install --git https://github.com/Wet-Ink-Corporation/temper --tag vX temper-cli --locked` or download the release asset. Less release machinery, but binstall cannot resolve the crate without git support, and user CI is clumsier.
- C) Publish one crate holding all the layers as modules. Simplest to publish, but it loses the `cargo tree` enforcement of the layers (see D3).

### D3 · Crate split (ARC-07, H-16 crate boundaries)
- **A) Four crates. Recommended.** `temper-domain` is pure (no I/O). `temper-app` holds the use cases and the ports. `temper-host` holds the std::process and std::fs adapters. `temper-cli` is the binary. This is the standard's own reference layout (`templates/example-workspace`). The domain has a genuinely different dependency profile (regex, toml, sha2, serde; no process or file I/O), and it lets the scanner and policy logic be unit-tested and property-tested without fakes.
- B) Two crates: `temper-domain` and `temper-cli`, with app and host as modules of the binary. Fewer manifests, but the app layer could reach `std::process` without `cargo tree` noticing.
- C) One crate: the least ceremony and the weakest enforcement.

### D4 · MSRV (H-16 rust-version)
The fallback build compiles on the **user's** default toolchain (§5.4), so the MSRV is the oldest compiler a fallback user can have. Rust 1.99 is current on 2026-10-01. The candidate dependencies need ≤ 1.85.
- **A) 1.96, as in `templates/workspace/Cargo.toml`. Recommended.** It is consistent with the standard (`assert_matches!` in tests, if-let guards) and with clippy's `incompatible_msrv`. A user whose toolchain is older than about 4 months gets a clear fallback message: "rustup update".
- B) 1.88: let-chains, a wider fallback reach, and no `assert_matches!` (tests use `matches!`).
- C) 1.85, the edition-2024 floor: the widest reach and the most constraints.

### D5 · Release targets
- **A) Six packages, including `win32-arm64`. Recommended.** It is built natively on a `windows-11-arm` runner. Windows-on-ARM laptops are common, and the alternative for those users is a fallback build that needs the MSVC toolchain.
- B) The five required targets: win32-arm64 users get the fallback build.
- Linux binaries are `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, fully static, so the packages carry **no** `libc` field and install on glibc and musl systems alike. No musl detection is needed. Windows builds link the CRT statically (`-C target-feature=+crt-static`, set in the release workflow's environment, not in `.cargo/config.toml`) so no VC++ redistributable is needed.

### D6 · Hook behaviour while the fallback build is pending or has failed
The fallback is needed only when no prebuilt binary resolves: an unsupported platform, a failed or skipped npm install, or `--plugin-dir` and local-directory marketplaces. Hook timeouts are 15 to 30 s for PreToolUse and UserPromptSubmit, 60 to 120 s for some others, and 1200 s for Stop and SubagentStop. A cold `cargo build --release` takes about 1 to 3 minutes.
- A) **Fail closed.** Every enforcing hook denies or blocks with "temper's engine is not available: <reason>". This is the strongest guarantee, but a broken toolchain blocks all Rust work until a human sets `TEMPER_DISABLE=1`, and the Stop block re-fires every turn.
- B) **Degrade (fail open).** Each session gets one notice: "temper is not enforcing: engine building/failed (<log>)". Everything is allowed. This matches today's "a hook bug never wedges the session", but edits made during the window are unchecked until the gate runs.
- **C) Hybrid. Recommended.**
  1. SessionStart builds **synchronously** when no binary exists (its timeout rises from 30 to 900 s, with a status message). The window is normally zero: the session starts after the build.
  2. If the binary is still missing (the build timed out or failed, or another session is building):
     - PreToolUse `pre-edit` on `.rs` and `Cargo.toml` **denies** with a retry message. This is cheap and recoverable, since the agent waits and retries. Other edits pass.
     - `pre-bash` and `pre-handback` pass, with a once-per-session notice. (`pre-handback` was removed by wi-103aa0 and restored by #97: cloud sessions deliver a subagent's report through `SubagentHandback`. Its parity tests come back with it.)
     - `post-tool` and `user-prompt` pass silently. The skill-load marker is not written, so after the build the skill gate asks the agent to load the skill again, which heals itself.
     - Stop and SubagentStop wait up to 600 s for an in-flight build, then run the gate. Without a binary they emit `temper gate: UNAVAILABLE (...)`, the same semantics as today's "cargo is not usable here".
  3. A failed build records `${CLAUDE_PLUGIN_DATA}/engine/<key>/build.failed` with the log path. It is retried at the next SessionStart, never on every hook.

  `TEMPER_DISABLE=1` short-circuits in the launcher before any binary lookup, so the human's escape always works.

### D7 · cargo-dist or a hand-written workflow
- A) cargo-dist 0.33 generates the matrix, archives, checksums, GitHub release and binstall-compatible names. But its npm installer downloads at run time (not the optionalDependencies layout), so npm packaging, lockfile generation, the plugin-version sync and the post-publish verification are ours anyway. We would also maintain dist's generated workflow and its pinned actions.
- **B) Hand-written `release.yml` (about 350 lines, SHA-pinned like `ci.yml`) plus `cargo xtask` subcommands for packaging and checks. Recommended.** The supply chain stays small, every step is explicit, and the whole pipeline is in the language the repo standardises on.
- cargo-npm (8 stars, immature) is not considered.

### D8 · Scanner: syn or a token lexer
The scanner must handle text that may not parse: the pre-edit hook scans the file as it *would* look after an edit, often mid-refactor. It also reads doctest fences with `# ` hidden lines, `proptest! {}` and `macro_rules!` bodies, and `#[cfg(test)]` regions, it reports line text for `newHits` matching, and it needs offsets that map 1:1 to the source.
- **A) Port `lexer.mjs` (masking) and the 56 rules of `scan.mjs` faithfully. Recommended.** It is byte-for-byte comparable with 1.x through the differential corpus (§2.2), adds no dependency, and tolerates broken code.
- B) Replace the masking with `ra_ap_rustc_lexer` (rustc's own error-tolerant lexer). This is more exact for raw strings, nested comments and char/lifetime disambiguation, but the `ra_ap_*` crates release weekly as 0.x (churn), and every masking difference changes hits, so parity is lost.
- C) A syn AST: precise for well-formed code, but it fails on the code the pre-edit hook sees most, sees nothing inside macro invocations, and would be a rewrite of every rule with a fallback anyway.
- B could follow in 2.x as its own change, measured against the corpus.

### D9 · Regex engine
The rules use JS regex features: look-ahead in `ignore-no-reason`, `extern-crate`, `get-prefix`, the shell-write family, `baselineRaise`, `--ignore-rust-version`, `emptyIfLetErr` and `traitImplRegions`, and variable-length look-behind in `compiler-fence` and `unwrap-or-default`.
- **A) The `regex` crate (linear time, so no ReDoS on agent-written input, H-13), with each look-around rewritten as the plain pattern plus a predicate over the match and its context. Recommended.** Section 1.6 specifies the rewrite of each one. Risk: JS backtracking can find a shorter match at the same start position when a look-ahead fails. The rewrite resumes the search at `start + 1` character, and the corpus catches what remains.
- B) `fancy-regex` 0.19 with the `variable-lookbehinds` feature. It gives JS semantics almost exactly, but it is a backtracking engine (it has a backtrack limit, which turns into an error to handle) and adds a dependency on top of `regex`.

### D10 · New dependencies (H-16, ARC-12)
These are already pre-approved in `.claude/temper.json`: `thiserror`, `anyhow`, `serde`, `serde_json`, `proptest`, `insta`, `tempfile`, and `divan` if a benchmark is needed.

| Crate (locked version seen in the local registry) | Used for | Alternative if refused |
|---|---|---|
| `regex` 1.13.1, default features | scanner rules, hook command policies, glob compilation | D9-B, or hand-written matchers (no) |
| `toml` 1.1.6, `default-features = false`, features `std`, `parse`, `preserve_order` (pulls `indexmap`) | reading manifests; `Table: FromStr` needs only `parse`; `preserve_order` keeps document order, which the 1.x ask messages depend on (dependency lists, watched keys) | port `toml.mjs` (267 lines) by hand: exact parity, but ours to maintain |
| `sha2` 0.11.0 (MSRV 1.85) | the tree fingerprint, which must stay SHA-256 truncated to 16 hex characters for state compatibility (§2.3) | hand-rolled SHA-256 (no) |
| `wait-timeout` 0.2.1 | child-process timeouts (git, cargo, rustc) without polling | `try_wait` plus `thread::sleep` polling (flagged as advisory by our own scanner) |

Not added, with the reason:
- `clap`: the usage texts and error behaviour are exact contracts, and the argument surface is small; a hand-written typed parser keeps them identical.
- `walkdir`, `globset`: the 1.x glob semantics are custom and must match exactly.
- `jiff`/`time`: a ~40-line RFC 3339 UTC formatter with injected time.
- `dunce`: verbatim-path stripping is ported.

### D11 · Deprecation timeline for the Node path
- **A) Strangler on `main` during development only. Recommended.** `scripts/launch.mjs` delegates per subcommand to Rust when `TEMPER_ENGINE=rust` and to the unchanged Node CLI otherwise, and `hooks.json` keeps calling `scripts/temper.mjs` until the cutover. At the cutover PR (2.0.0) the launcher becomes `scripts/temper.mjs`, and `scripts/lib/`, the Node tests and the parity harness are deleted in the same change. Users never run two engines.
- B) Ship 2.0 with the Node engine kept behind `TEMPER_ENGINE=node` and delete it in 2.1. This gives an escape hatch, but two engines in production (two behaviours to support, two to keep in sync), while the cargo fallback already covers a missing binary.
- C) Keep Node through all of 2.x: no.

### D12 · Publishing credentials and release approval
- **A) npm trusted publishing (OIDC, `--provenance`) and crates.io trusted publishing (`rust-lang/crates-io-auth-action`), both only from `release.yml` in a GitHub environment `release` that requires the owner's approval. Recommended.** No long-lived tokens.
- B) `NPM_TOKEN` and `CARGO_REGISTRY_TOKEN` repository secrets: simpler, but long-lived credentials.

### D13 · How user CI installs temper
- **A) `taiki-e/install-action` with `tool: temper-cli@${{ steps.temper.outputs.version }}`, SHA-pinned. Recommended.** Through its cargo-binstall fallback it downloads the GitHub release asset named by `[package.metadata.binstall]` in `temper-cli`, in seconds. This requires D2-A.
- B) `cargo install temper-cli --locked --version X`: about a minute per cold run, cached by `Swatinem/rust-cache`.
- C) `curl` the release asset plus `sha256sum -c`: no crates.io needed, but 8 lines of YAML per job.

### D14 · Version pin in user repositories
- **A) `.claude/temper-version` containing `2.0.0\n`. Recommended.** It is readable from sh (`cat`) and YAML (`hashFiles`), protected by the existing `.claude/**` rule, and it replaces `hashFiles('.claude/temper/temper.mjs')` as the "temper is set up here" signal.
- B) A `cliVersion` field in `.claude/temper.json`: one file fewer, but the pre-commit hook would need a JSON parser.
- C) A literal in `rust.yml` only: the pre-commit hook cannot see it.

### D15 · Migrating 1.x repositories
- **A) `temper setup --migrate`, offered by `/temper:setup` when `.claude/temper/temper.mjs` exists. Recommended.** It removes `.claude/temper/`, writes `.claude/temper-version`, and replaces `.github/workflows/rust.yml`, `.githooks/pre-commit`, the `.claude/settings.json` allow-list entries and the `AGENTS.md` command rows **only when the file's SHA-256 equals one of the shipped 1.x template versions**. Otherwise it prints the exact manual edits. Setup never overwrites a customised CI file.
- B) Print manual instructions only: safe, but tedious for every repository.
- C) Rewrite the files regardless: no, it would clobber customised CI.
- Either way, a 1.x repository keeps working untouched: its vendored copy is self-contained and the baseline format does not change (§2.3).

### D16 · Intentional divergences and retired tests (H-12)
Approve the list in §2.4 as a whole. Every other observable behaviour stays byte-identical, and the parity harness enforces that.

### D17 · Binary on PATH in Claude sessions
- **A) SessionStart appends `export TEMPER_BIN="<binary>"` and `export PATH="<bin dir>:$PATH"` to `CLAUDE_ENV_FILE`. Recommended.** Without it, the 2.0 pre-commit hook (which calls `temper`) fails inside Claude sessions for anyone who has not also run `cargo binstall temper-cli`. The directory contains only the temper binary.
- B) No PATH change. The pre-commit hook then needs `temper` installed separately (it prints how).

---

## Constraints and non-goals

- The port follows rust-craft: the iron rules H-01..H-16, the template lints copied verbatim into this repository's root manifest, zero warnings, and `foo.rs` plus `foo/` modules (ARC-06).
- No change to the rust-craft standard text, the rule IDs, `rules.md`, the skills' behaviour, or the agents' prompts, beyond the CLI paths and install notes they mention.
- The Python checks (`registry.py`, `lint-claims.py`, `gen_rulemap.py`, `doc_examples.py`) stay Python. They are adapted to read Rust sources and to call the binary; they are not ported.
- `.claude/temper-baseline.json`, `.git/temper/**` and `${CLAUDE_PLUGIN_DATA}/sessions/**` keep their 1.x formats. There is no schema migration.
- The hook protocol is unchanged: `hooks/hooks.json` keeps exec form with `command: "node"`, and the launcher path stays `${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs`. So `node "$TEMPER_CLI" gate`, the skills and the agents keep working unchanged.
- No async runtime (ASY-01): the work is sequential and process-bound. Threads exist only to drain child pipes (scoped and joined).
- Non-goals:
  - New scanner rules or changed rule semantics.
  - A syn-based scanner (D8).
  - Signing macOS binaries. npm does not set the quarantine attribute, and arm64 binaries are ad-hoc signed by the linker.
  - Bounding the whole gate to the hook's time budget (noted as a 2.x follow-up in Risks).
  - Vendoring crates for an offline fallback build.

## Invariants (testable)

1. For every input in the parity corpus, `temper <args>` in Rust and `node scripts/temper.mjs <args>` in 1.x produce the same exit code, the same stdout and stderr after normalising temp paths, timestamps and durations, and the same files written, except for the D16 divergences. This holds until the Node code is deleted.
2. `fingerprint(project, base)` is bit-identical between 1.x and 2.0 on the same tree, including untracked files and non-UTF-8 diff output. So gate caches and verdicts keyed by fingerprint carry over (§2.3).
3. Every file temper writes under `.git/temper/`, `${CLAUDE_PLUGIN_DATA}/sessions/` and `.claude/temper-baseline.json` is readable by both engines in both directions.
4. A hook never exits with code 2. A handler error becomes `{"systemMessage":"temper hook <event> failed: <msg>"}` with exit 0, and malformed stdin JSON exits 0 silently, as in 1.x.
5. One version string X appears in `Cargo.toml` `[workspace.package]`, `.claude-plugin/plugin.json`, `plugins/temper-lens/.claude-plugin/plugin.json`, `package.json` (its version and all six exact optionalDependency pins), each npm platform package, `templates/*/.claude/temper-version`, the git tag `vX`, and `temper --version`. `cargo xtask versions --check` fails otherwise.
6. `package-lock.json` (lockfileVersion 3) lists all six platform packages at version X, with `https` `resolved` URLs, `sha512` `integrity` and matching `os`/`cpu`, and nothing else (the npm/cli#4828 guard). `package.json` has no `scripts`, `overrides` or non-registry dependencies.
7. On every supported OS/arch, a copy of the plugin directory installed with `npm ci --ignore-scripts` within 60 s runs `node scripts/temper.mjs --version` and prints `temper X`.
8. Production code has no `unsafe`, `unwrap`/`expect`, panics, `as` casts or print macros, and every spawned thread or process has an owner, a timeout and a capped buffer (H-01..H-13).

---

## Design

### 1. Workspace and crate layout

#### 1.1 Placement in the repository

```text
/                               plugin root = git root = Cargo workspace root
├── Cargo.toml                  virtual manifest: members crates/*, xtask; exclude ["templates", "tests", "docs", "evals"]
├── Cargo.lock                  committed
├── rust-toolchain.toml         1.98.1 (copy of templates/workspace/rust-toolchain.toml)
├── clippy.toml rustfmt.toml deny.toml .cargo/config.toml   copies of templates/workspace/*
├── AGENTS.md + CLAUDE.md       new workspace charter: crate map, layer rules, decisions log (D1..D17)
├── package.json package-lock.json   private plugin package with the six exact optionalDependencies (added at cutover)
├── LICENSE LICENSE-APACHE      MIT (existing, also the plugin's licence) + Apache-2.0 (new, Phase 0a); crates are `MIT OR Apache-2.0` (wi-4679b3)
├── crates/
│   ├── temper-domain/          pure: lexer, scanner, manifest policy, config, glob, verdict, fingerprint hasher,
│   │                           findings/baseline/ratchet/report models and renderers, hook policies and messages
│   ├── temper-app/             use cases + ports (Files, ProcessRunner, Clock); git/cargo command wrappers
│   ├── temper-host/            std adapters: process runner with timeouts, files, clock
│   └── temper-cli/             bin `temper`: args, stdin/stdout, composition root, exit codes
├── xtask/                      publish = false: versions, npm-pack, check-npm-lock, verify-install
├── scripts/temper.mjs          the launcher (after cutover); during the port: scripts/launch.mjs + the 1.x CLI untouched
└── hooks/ skills/ agents/ templates/ tests/ docs/ evals/   unchanged locations
```

Why the root:
- temper requires the Cargo workspace at the git root ("Nested workspaces are not supported yet"). A root workspace lets this repository dogfood its own gate, ratchet and hooks.
- Claude Code copies the whole plugin directory into its cache. With the workspace at the plugin root, `cargo build --release --locked --manifest-path ${CLAUDE_PLUGIN_ROOT}/Cargo.toml` works there without extra packaging (§5.4).

No collisions:
- `templates/workspace/Cargo.toml` and `templates/example-workspace/Cargo.toml` declare their own `[workspace]`. Cargo stops at the nearest workspace root, so their crates never join this workspace. The `exclude` entry makes that explicit.
- `skills/` holds no manifests.
- `tests/doc_examples.py` and `tests/e2e/*.sh` build in temporary directories.

#### 1.2 Crates, layers and dependencies (ARC-01, ARC-02, ARC-07, ARC-08)

| Crate | Layer | Depends on | Third-party (D10) | Must never contain |
|---|---|---|---|---|
| `temper-domain` | domain | nothing in-workspace | regex, toml (std, parse, preserve_order), sha2, serde (derive), serde_json, thiserror | `std::fs`, `std::process`, `std::env`, `SystemTime::now`, threads |
| `temper-app` | application | domain | serde_json, thiserror | `std::fs`, `std::process`, `std::env` (all through ports or `RunEnv`) |
| `temper-host` | adapter (OS: processes, files, clock) | app, domain | wait-timeout, thiserror | policy decisions, message texts |
| `temper-cli` | binary | app, domain, host | anyhow, serde_json | logic beyond parse, wire, run and map exit codes (ARC-14) |
| `xtask` | tooling, `publish = false` | none | anyhow, serde_json | anything the shipped binary needs |

- Dev-dependencies: `proptest`, `insta`, `tempfile` (all pre-approved).
- `temper-app` has a `test-util` feature exposing its fakes (TST-09). `temper-cli/tests/it` holds the integration tests that need real git, cargo and the file system: they run through the composition root, so no dev-dependency cycle arises.
- Each crate gets `AGENTS.md` (the ARC-08 charter, filled from this plan) and `CLAUDE.md` (`@AGENTS.md`).
- Members inherit `[lints] workspace = true`, `edition`, `version`, `rust-version`, `license = "MIT OR Apache-2.0"` (the Rust convention and the template's licence; decided by Weigh-In wi-4679b3, overriding the MIT recommendation), `repository` and `publish`. Each published crate must ship both licence texts, and `cargo package` only packages files inside the crate's own directory, so every crate gets committed copies `LICENSE-MIT` and `LICENSE-APACHE` (plain files, not symlinks, for Windows checkouts). `cargo xtask check-licenses` fails CI when a copy differs from the root files, and the release workflow asserts `cargo package --list` shows both. `plugin.json` stays `MIT`. Siblings are declared once in `[workspace.dependencies]` with `path` and `version` (ARC-01 publishing).

#### 1.3 Module map (Node → Rust) and public surfaces

| 1.x module | Rust home |
|---|---|
| `lexer.mjs` | `temper-domain::scan::lexer`, `scan::regions` (both `pub(crate)`) |
| `scan.mjs` | `temper-domain::scan::{rules, finders/*, hit, scanner}` |
| `toml.mjs` | replaced by `toml`; `temper-domain::manifest::parse` |
| `manifest.mjs` | `temper-domain::manifest::{problems, watched, deps, requirement, sections}`. `workspaceRootFor` and `membersFromToml` (fs walks) go to `temper-app::workspace`. |
| `verdict.mjs` | `temper-domain::verdict` (extract, validate, summarise); `temper-app::state::verdicts` (record, read) |
| `project.mjs` | `temper-domain::{glob, config, fingerprint, paths, time}`; `temper-app::{project, session, retention, telemetry, changes, output_path}`. `telemetry` appends and rotates `telemetry.jsonl`, pushes a session's rows to `telemetry.branch` (`push`, #96) and fetches that branch for `stats --branch`. |
| `gate.mjs` | `temper-domain::gate` (step model, summarize, tail, render); `temper-app::gate` (run, scan_changed, manifests step) |
| `findings.mjs` | `temper-domain::findings` (Finding, Collection, cargo-message parser, rule map, dedupe and sort, crate attribution, forbidden-lint table, mod-decl roots); `temper-app::findings` (collect, cache, scan source over files) |
| `baseline.mjs` | `temper-domain::baseline` (Baseline, render, from_collection, tighten, diff_totals, crate_aliases, parse_diff, ratchet core, render_ratchet, debt_line); `temper-app::baseline` (temporary-index diff, renamed_since, merge_base, raised_since, commands) |
| `report.mjs` | `temper-domain::report` (build, markdown, html); `temper-app::report` (argument handling, input selection, output dir, writes) |
| `vendor.mjs` | retired; replaced by `temper-app::setup::{pin, migrate}` |
| `hooks.mjs` | `temper-domain::hooks` (outcomes, all message texts, IRON_RULES, bash policy, edit policy, manifest checks, status parsing); `temper-app::hooks` (the nine handlers, `pre-handback` among them, and `report_from_transcript`, the `SubagentStop` fallback that reads `agent_transcript_path`) |
| `shell.mjs` | `temper-domain::hooks::shell` (the one shell reader behind the Bash write rules: lexer, command walk with `cd` and variables, write targets, patch commands; pure, no I/O) |
| `requirements.mjs` | stays in JS, in the launcher (§5.1): the Node minimum (`MINIMUM_VERSIONS`) is about the Node that runs the launcher. Its warning text is a message contract, and SessionStart's warning line must reach the Rust context (#89). |
| `temper.mjs` | `temper-cli::{args, main, io}`; `temper-app::commands::{scan, status, stats, setup, telemetry}` (`stats --branch [NAME]` and `telemetry push --session ID`, #96) |
| `rulemap.json` | moved to `crates/temper-domain/data/rulemap.json`, embedded with `include_str!`. `gen_rulemap.py` `OUT_REL` updated. |

**`temper-domain` public surface** (re-exported from `lib.rs`; everything else is `pub(crate)`):

```rust
// paths.rs: pure path text helpers (Windows rules are a parameter, not cfg!, so both are testable everywhere)
pub enum PathStyle { Windows, Posix }
pub struct RelPath(String);                  // forward slashes; may begin with ".." (outside the root)
impl RelPath { pub fn as_str(&self) -> &str; pub fn is_outside(&self) -> bool; }
pub fn unverbatim(path: &str, style: PathStyle) -> Cow<'_, str>;   // \\?\C:\x -> C:\x, \\?\UNC\h\s -> \\h\s
pub fn clean_rel(path: &str) -> String;                            // cleanRel: posix-normalise, strip ./ and trailing /
pub fn js_trim(text: &str) -> &str;                                // JS String.prototype.trim set (incl. U+FEFF, excl. U+0085)
// 1d.2a has PathStyle, unverbatim and js_trim; RelPath and clean_rel arrive with their first
// readers (Phases 2 and 3). Node's lexical path algebra (resolve, relative, isAbsolute,
// dirname, join) is pub(crate) in temper-app::paths, where every reader is (OC-1 B, wi-30e6a5).

// scan
pub enum CodeKind { Prod, Bin, Example, Test }
pub enum Severity { Hard, Ask, Soft }
pub enum LibTarget { Present, Absent }                        // API-10: not a bool (owner, wi-208563)
pub fn classify_path(rel_to_crate: &str, lib: LibTarget) -> CodeKind;
pub fn is_build_script(rel_to_crate: &str) -> bool;
pub fn new_hits(before: &[Hit], after: Vec<Hit>) -> Vec<Hit>;   // multiset on key + text; keeps after's order
pub struct Scanner { /* compiled rules */ }
impl Scanner {
    pub fn new() -> Result<Self, ScannerSetupError>;           // compiles every pattern once; built at the composition root
    pub fn scan(&self, src: &str, options: &ScanOptions<'_>) -> ScanReport;   // infallible, total
    pub fn rules(&self) -> impl Iterator<Item = &RuleInfo> + '_;               // key, id, lints, contexts (rule-table test)
}
// `rules()` and `RuleInfo` land with their first non-test reader, the gate's new-code-only
// set (Phase 4, wi-ca74b3); 1d ports the rule-table test against the in-crate table.
// `is_rust_fence` is private to the doc-rule finders: its only reader is `doc-unwrap` (wi-e9f84d).
pub struct ScanOptions<'a> { pub kind: CodeKind, pub file: FileRole, pub forbid: Option<&'a BTreeSet<String>> }
pub enum FileRole { Source, BuildScript }                     // API-10: not a bool
pub struct ScanReport { /* hits: Vec<Hit>, tests: usize, ignores: usize, clones: usize */ }  // getters
pub struct Hit { /* key: RuleKey, id: RuleId, severity, line: LineNo, text: String, msg: Cow<'static, str>, fix: &'static str, exception: Option<Exception> (private: Requested | Attribute) */ }
// Serialize (hand-written, 1d-12): key, id, severity, line, text, exception? (a covering #[expect]'s request), msg, exception? (the expect-attr finder's own), fix; HitAt { hit, file } appends file
pub struct HitAt<'a> { pub hit: &'a Hit, pub file: &'a str }   // a view: the caller owns hits and paths
pub fn new_hits(before: &[Hit], after: Vec<Hit>) -> Vec<Hit>;   // consumes `after` (OWN-02): multiset difference on (key, text)
pub fn format_hit(hit: &Hit, file: Option<&str>) -> String;
pub fn render_scan_text(hits: Vec<HitAt<'_>>) -> String;      // temper scan's text report, sorted (D16(l)); like render_gate (OC-2 A, wi-274f9a)
pub const NOT_A_RUST_PROJECT: &str;                            // "temper: not inside a Rust project (no Cargo.toml found)", no newline (OC-2 A)

// manifest
pub struct Manifest(toml::Table);
impl FromStr for Manifest { type Err = ManifestParseError; }
pub enum RootRelation { Itself, Member }
pub struct GoverningRoot<'m> { pub manifest: &'m Manifest, pub relation: RootRelation }
pub enum OwnLints { Listed, NotListed }
pub enum ManifestProblem { MissingLintInheritance { own_lints: OwnLints }, MemberProfile }   // id(), msg(), fix() reproduce 1.x text
pub fn member_problems(member: &Manifest, root: Option<&GoverningRoot<'_>>, own_lints: OwnLints) -> Vec<ManifestProblem>;
pub fn split_problems(before: &[ManifestProblem], after: Vec<ManifestProblem>) -> SplitProblems;   // { fresh, legacy }
pub fn watched_keys(m: &Manifest) -> Vec<(String, WatchedKey)>;      // insertion order, as the 1.x Map
pub fn dependency_names(m: &Manifest) -> Vec<DepName>;                // ordered set, document order
pub fn dep_names_by_line(text: &str) -> Vec<DepName>;                 // fallback for invalid TOML
pub fn dep_requirements(m: &Manifest) -> Vec<(String, String)>;
pub fn major_of(req: &str) -> Option<String>;
pub fn non_caret_requirement(req: &str) -> Option<&'static str>;
pub fn lint_sections(text: &str) -> String;
pub fn lint_config_changed(before: &str, after: &str) -> bool;
pub fn manifest_checks(facts: &ManifestEditFacts<'_>) -> ManifestFindings;   // { denies, asks, advice }: Vec<String>, exact 1.x text

// config (parse once at the composition root, ARC-11)
pub struct Config { /* private; getters */ }
pub enum ReviewMode { Required, Advisory, Off }
pub struct ConfigLoad { pub config: Config, pub problem: Option<ConfigProblem> }   // D16(d)
impl Config { pub fn load(text: Option<&str>) -> Result<ConfigLoad, GlobError>; pub fn defaults() -> Self; }
pub struct Glob(regex::Regex);
impl Glob { pub fn new(pattern: &str) -> Result<Self, GlobError>; pub fn is_match(&self, rel: &str) -> bool; }

// verdict
pub fn extract_verdict(text: &str) -> Option<ExtractedVerdict>;   // enum { Parsed(serde_json::Value), Unparseable { message: String } }
pub struct VerdictContext<'a> { pub changed_rust: &'a [RelPath], pub gate: GateState }   // GateState { Unknown, Green, Red }
pub fn validate_verdict(verdict: Option<&ExtractedVerdict>, ctx: &VerdictContext<'_>) -> Vec<VerdictProblem>;   // Display = exact 1.x text
pub struct ValidVerdict { value: serde_json::Value, fingerprint: Fingerprint, outcome: VerdictOutcome }
impl ValidVerdict { pub fn new(v: ExtractedVerdict, ctx: &VerdictContext<'_>) -> Result<Self, VerdictRejected>; }   // ERR-14: hands the value back
pub fn summarize_findings(verdict: &serde_json::Value, max: usize) -> String;

// fingerprint and time
pub struct Fingerprint([u8; 8]);              // Display = 16 lowercase hex; FromStr validates ^[0-9a-f]{16}$
pub struct FingerprintHasher(sha2::Sha256);
impl FingerprintHasher {
    pub fn new() -> Self;
    pub fn head_and_rev(&mut self, head: Option<&str>, rev: Option<&str>);   // "head:<h>\nrev:<r>\n"
    pub fn git_diff(&mut self, stdout: &[u8]);       // hashes String::from_utf8_lossy(stdout): matches 1.x string hashing
    pub fn untracked(&mut self, rel: &str, contents: Option<&[u8]>);   // "\n?<rel>\n" then raw bytes
    pub fn file(&mut self, rel: &str, contents: Option<&[u8]>);        // no-git mode: "\n<rel>\n" then raw bytes
    pub fn finish(self) -> Fingerprint;
}
pub struct Timestamp { unix_millis: i64 }   // Display = 1x toISOString(): YYYY-MM-DDTHH:MM:SS.mmmZ
impl Timestamp { pub fn from_unix_millis(ms: i64) -> Option<Self>; pub fn unix_millis(self) -> i64; }

// findings, baseline, report, gate: models are serde DTOs mirroring the 1.x JSON (field order = 1.x order)
pub struct Collection { .. }  pub struct Finding { .. }  pub struct CrateInfo { .. }  pub struct CollectError { .. }
pub struct RuleMap { .. }  impl RuleMap { pub fn embedded() -> Result<Self, RuleMapError>; pub fn rules_for(&self, lint: &str) -> &[RuleId]; .. }
pub struct CargoMessageParser<'a> { .. }  impl CargoMessageParser<'_> { pub fn new(ctx: ParseContext<'_>) -> Self; pub fn line(&mut self, line: &str) -> Option<&str> /* rendered text for the log */; pub fn finish(self) -> ParsedMessages; }
pub struct Baseline { .. }   // field order: temper_baseline, created, updated, commit, toolchain, host, #[serde(flatten)] extra (BTreeMap), crates (BTreeMap), totals (BTreeMap<String, BTreeMap<String, u64>>)
impl Baseline { pub fn parse(text: &str) -> Result<Self, BaselineParseError>; pub fn render(&self) -> String; pub fn from_collection(c: &Collection, created: Option<Timestamp>, now: Timestamp) -> Self; pub fn tighten(&self, c: &Collection, aliases: &CrateAliases) -> Self; }
pub struct ChangedLines { .. }   // file -> Lines::{Set(BTreeSet<u32>), All}; renames; base
pub fn parse_diff(text: &str) -> ChangedLines;
pub fn crate_aliases(baseline: &Baseline, crates: &[CrateInfo], renames: &dyn Fn() -> Renames) -> CrateAliases;   // lazily consulted, as in 1.x
pub fn ratchet(input: &RatchetInput<'_>) -> RatchetResult;   // pure: collection, baseline, changed, sources, raised (precomputed by app)
pub fn render_ratchet(r: &RatchetResult, max: usize) -> String;  pub fn debt_line(r: &RatchetResult) -> String;
pub struct Report { .. }  pub fn build_report(input: &ReportInput<'_>) -> Report;  pub fn render_markdown(r: &Report) -> String;  pub fn render_html(r: &Report) -> String;
pub struct GateResult { .. }  pub fn render_gate(r: &GateResult) -> String;  pub fn default_steps(cfg: &Config, nextest: Nextest) -> Vec<StepSpec>;  pub fn ratchet_steps(..) -> Vec<StepSpec>;

// hooks: pure policies and the output protocol
pub enum HookEvent { SessionStart, UserPrompt, PreEdit, PreBash, PreHandback, PostTool, Stop, SubagentStop, SubagentStart }   // FromStr from the 1.x CLI names
pub enum HookOutcome { Pass, Context { event: &'static str, text: String }, Deny { reason: String, context: Option<String> },
                       Ask { reason: String, context: Option<String> }, Block { reason: String }, Notice { message: String } }
impl HookOutcome { pub fn to_json(&self) -> Option<String>; }   // compact, key order as 1.x; Pass -> None (no output)
pub struct Verdict<T> { pub outcome: HookOutcome, pub telemetry: Option<TelemetryEvent> }   // what a policy decided + what to log
pub fn bash_policy(command: &str, deps: &DependencyPolicy<'_>) -> Verdict<..>;   // the whole of preBash after discovery: pure
pub fn edit_policy(facts: &EditFacts<'_>, scanner: &Scanner) -> Verdict<..>;     // preEdit after I/O gathering: pure
pub fn apply_edit(before: &str, edit: &EditOp<'_>) -> Option<String>;            // JS replace semantics incl. empty old_string
pub fn parse_status(last_message: &str) -> Option<DeclaredStatus>;               // Done | Wip | Blocked
pub const IRON_RULES: [&str; 16];
pub enum TelemetryEvent { .. }   // typed payloads for the 23 1.x event names (after 1.3.0: gate-start, #96; handback, verdict-invalid, subagent-start, subagent-stop, #98); serialised as {"t","sid"?,"ccr"?,"agent","event",..detail}
```

**`temper-app` (ports and use cases)**:

```rust
pub trait Files {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, FsError>;          // None = not found
    fn read_text(&self, path: &Path) -> Result<Option<String>, FsError>;      // lossy UTF-8, as readFileSync(.., 'utf8')
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError>; // mkdir -p, unique tmp, rename
    fn append_line(&self, path: &Path, line: &str) -> Result<(), FsError>;    // one write_all call (O_APPEND)
    fn metadata(&self, path: &Path) -> Result<Option<FileMeta>, FsError>;     // lstat: kind, modified
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FsError>;
    fn real_path(&self, path: &Path) -> Result<Option<PathBuf>, FsError>;     // canonicalize + unverbatim
    fn create_dir_all(&self, path: &Path) -> Result<(), FsError>;
    fn remove_file(&self, path: &Path) -> Result<(), FsError>;
    fn remove_dir_all(&self, path: &Path) -> Result<(), FsError>;
    fn copy_file(&self, from: &Path, to: &Path) -> Result<(), FsError>;
    fn temp_dir(&self, prefix: &str) -> Result<TempDir, FsError>;             // TempDir: finish(self) -> Result (ERR-13); Drop removes best-effort and logs
}
pub trait ProcessRunner { fn run(&self, request: &ProcessRequest<'_>) -> Result<ProcessOutput, ProcessError>; }
pub struct ProcessRequest<'a> { pub program: &'a str, pub args: &'a [OsString], pub cwd: &'a Path, pub env: &'a [(&'a str, OsString)],
                                pub stdout: StdoutSink<'a>, pub stderr_cap: ByteCap, pub timeout: Duration }
pub enum StdoutSink<'a> { Capture(ByteCap), File(&'a Path) }
pub struct ProcessOutput { pub status: ExitKind, pub stdout: Vec<u8>, pub stderr: Vec<u8> }   // ExitKind { Code(i32), Signal, TimedOut }
pub enum Stream { Stdout, Stderr }                                                              // names the stream in ProcessError::Overflow (OC-3 A)
pub trait Clock { fn now(&self) -> Timestamp; }
pub trait Ports { type Files: Files; type Process: ProcessRunner; type Clock: Clock;
                  fn files(&self) -> &Self::Files; fn process(&self) -> &Self::Process; fn clock(&self) -> &Self::Clock; }
// The ports grow with their readers. Files: 1d.2a has read_text, read_dir and real_path, and
// existsSync is real_path(..) is Ok(Some(_)) because metadata is lstat (OC-9 B, wi-bb98f7); the
// rest arrive from Phase 2. ProcessRequest: 1d.2b has stdout_cap: ByteCap and no env, and
// Phase 4 replaces stdout_cap with StdoutSink and adds env (OC-8 B, wi-b175fb). Ports: Files in
// 1d.2a, Process in 1d.2b, Clock in Phase 3.

/// Environment read once by the binary (ARC-11).
pub struct RunEnv { pub cwd: PathBuf, pub plugin_root: Option<PathBuf>, pub plugin_data: Option<PathBuf>, pub env_file: Option<PathBuf>,
                    pub temper_base: Option<String>, pub session: SessionKind /* ClaudeCode | Terminal */, pub raise: RaisePermission,
                    pub temp_dir: PathBuf, pub pid: u32, pub style: PathStyle, pub temper_bin: Option<PathBuf> }

pub struct Engine<P> { ports: P, env: RunEnv, scanner: Scanner, rules: RuleMap }   // no bounds on the struct (API-21)
impl<P: Ports> Engine<P> {
    pub fn new(ports: P, env: RunEnv, scanner: Scanner, rules: RuleMap) -> Self;   // 1d.2a: new(ports, env, scanner); rules arrives in Phase 5 (1d-15, wi-12a32e)
    pub fn hook(&self, event: HookEvent, input: &HookInput) -> Result<HookOutcome, HookError>;
    pub fn gate(&self, opts: &GateOptions) -> Result<CommandOutput, CommandError>;
    pub fn scan(&self, args: &ScanArgs) -> Result<CommandOutput, CommandError>;
    pub fn status(&self) -> Result<CommandOutput, CommandError>;
    pub fn stats(&self) -> Result<CommandOutput, CommandError>;
    pub fn baseline(&self, args: &BaselineArgs, progress: &mut dyn Progress) -> Result<CommandOutput, CommandError>;
    pub fn report(&self, args: &ReportArgs, progress: &mut dyn Progress) -> Result<CommandOutput, CommandError>;
    pub fn setup(&self, args: &SetupArgs) -> Result<CommandOutput, CommandError>;
}
pub struct CommandOutput { pub stdout: String, pub stderr: String, pub exit: Exit }   // Exit { Ok = 0, Fail = 1, Usage = 2 }
pub trait Progress { fn line(&mut self, text: &str); }   // stderr progress lines written before long cargo runs
pub struct HookInput { .. }   // serde DTO: unknown fields allowed (third-party, evolving protocol; deliberate exception to OUT-02's deny_unknown_fields, noted in the charter)
```

- Git and cargo are app-level typed wrappers over `ProcessRunner` (`app::git::{toplevel, common_dir, resolve_commit, head, diff, ls_untracked, show, merge_base, diff_working_tree}` and `app::cargo::{probe, fmt, metadata, clippy_json, doc_json, step}`). Their argument lists, environment (`CARGO_TERM_COLOR=never`, `CARGO_BUILD_WARNINGS=warn`, `RUSTDOCFLAGS`, `GIT_INDEX_FILE`, `GIT_LITERAL_PATHSPECS`) and `-c diff.*` overrides are copied verbatim from 1.x.
- `temper-host`: `HostPorts` implements `Ports` with `StdFiles`, `StdProcess` and `SystemClock`.
- `temper-cli`:
  - `main.rs` (≤ 100 lines): `fn main() -> ExitCode`.
  - `args.rs`: a typed hand-written parser per subcommand that reproduces `baselineArgs` and the report argument parser exactly, including `--flag=value`, plus a new `--version`.
  - `hook_io.rs`: bounded stdin read (cap 256 MiB, matching 1.x `maxBuffer`), TTY yields `{}`.
  - `output.rs`: `writeln!` on locked handles; a broken pipe stops output quietly (D16(b)).
  - `wire.rs`: composition root building `RunEnv` from `std::env`, then `Scanner::new()`, `RuleMap::embedded()` and `Engine::new(HostPorts, ..)`.

#### 1.4 Error design (ERR-01..ERR-14)

`temper-domain`:
- `ScannerSetupError { #[source] regex::Error, rule: &'static str }`
- `ManifestParseError(#[source] toml::de::Error)`
- `BaselineParseError::{NotJson { #[source] source: serde_json::Error }, NotABaseline }`. Display reproduces `".claude/temper-baseline.json is not valid JSON (<detail>)"`; the detail differs from JS (D16(a)).
- `FingerprintParseError { value: String }`
- `GlobError { pattern: String, #[source] source: regex::Error }`
- `RuleMapError(#[source] serde_json::Error)`
- `VerdictRejected { problems: Vec<VerdictProblem>, verdict: ExtractedVerdict }`, which hands the input back (ERR-14).
- `VerdictProblem` is data (an enum with one variant per 1.x message), not an error.

`temper-app`:
- `FsError { op: FsOp, path: PathBuf, #[source] source: io::Error }`
- `ProcessError::{Spawn { program, #[source] io::Error }, Io { program, #[source] io::Error }, Overflow { program, stream: Stream, cap: usize }}`. A timeout is an `ExitKind`, not an error, so callers reproduce 1.x behaviour, which treats a timeout as a failed run. A stream past its `ByteCap` is `Overflow`, as 1.x's `maxBuffer` fails the run with `ENOBUFS`; output is never silently truncated (owner, OC-3 A, wi-129daa).
- `GitError::{Process(#[source] ProcessError), Failed { args: String, code: Option<i32>, stderr_head: String }}`
- `StateError`, `HookError`, `GateError`, `CollectError`, `CommandError`, each per operation (ERR-02).
- `CommandError::Refused { exit: Exit, message: String }` carries the exact 1.x refusal text with exit 2. Real failures are typed variants.

Where 1.x swallows a failure (for example, `untracked()` returns `[]` when git fails, `telemetry` is best-effort, `pruneSessions` ignores errors), Rust handles it with an explicit `match` and a one-line comment saying why the fallback is the meaning (H-10), plus `tracing::debug!`. The binary installs no subscriber by default, so behaviour stays identical. Telemetry and pruning failures stay silent, as in 1.x, with the reason documented.

`temper-cli`: `anyhow` with `.context()` at each boundary. An unexpected error prints `temper: <error:#>` on stderr and exits 1 (1.x crashed with a stack trace and exit 1). A hook error becomes the `systemMessage` JSON with exit 0.

#### 1.5 Concurrency and resources (H-13, H-14, ASY)

- Synchronous throughout; no async runtime.
- The one network call is opt-in (#96, wi-641f99): with `telemetry.branch` set, Stop starts `temper telemetry push --session <id>` as a detached child (stdio ignored, never awaited), so the hook never waits on the network. The push builds its commit with a private `GIT_INDEX_FILE` and the private ref `refs/temper/telemetry-tip`, touches no working tree, index, `HEAD` or branch, gives each git call a 60 s timeout, and tries a rejected push up to 3 times.
- `StdProcess::run`:
  - `stdin(Stdio::null())`, as 1.x does: cargo's rustc probes read stdin.
  - Two named threads inside `thread::scope` drain stdout (to a capped buffer, or streamed to the file sink) and stderr (cap 64 MiB; 1.x used 256 MiB for `run` and 64 MiB for `runToFile`, kept per call site). Both are joined.
  - `wait_timeout(deadline)`. On timeout: `kill()`, then `wait()`, then `ExitKind::TimedOut`.
  - On Windows, `creation_flags(CREATE_NO_WINDOW)` (`windowsHide`), with no unsafe needed.
- Timeouts:
  - The 1.x values: fmt 120 s, metadata 120 s, `rustc -vV` 60 s, probe 60 s, `nextest --version` 20 s, steps `gate.stepTimeoutSec`.
  - git calls, which had **no** timeout in 1.x, get 120 s (D16(f)).
- State writes are `write_atomic` to a unique `<file>.<pid>.<nanos>.tmp` then `rename`, which replaces existing files on Windows too. Telemetry is a single `write_all` with `O_APPEND`.
- Retention, as in 1.x (`RETENTION` in `project.mjs`, #88). The findings cache keeps 20 entries. SessionStart on `startup` prunes markers at 7 days and sessions at 14, then verdicts at 60 days (newest 300 kept) and gate runs at 30 days (newest 300 kept, 64 MiB in all, the newest run always kept). A gate run is `gate/<fp>.json` with its `logs/<fp>-*` files, pruned as one group. The records of the start fingerprint are never pruned and count toward no limit, and a group written in the last 60 minutes is never pruned either (another worktree's gate may be running). Resume, clear and compaction prune nothing. `telemetry.jsonl` is renamed to `telemetry.1.jsonl` at 1 MiB, replacing the previous one, and `stats` reads both. Every delete ignores errors.

#### 1.6 Scanner engine port (D8, D9)

Lexer (`mask_rust`):
- Operates on the source's UTF-8 bytes. Masked spans replace every non-newline byte with `b' '`, so the masked text is valid UTF-8 and byte offsets map 1:1 to the source.
- The char-literal branch uses `char` length, equivalent to 1.x `codePointAt`.

Line text:
- `js_trim(line).chars()` truncated to at most 160 UTF-16 code units, never splitting a surrogate pair (D16(e)).

Translating JS regex classes:
- `\w` becomes `[0-9A-Za-z_]`, `\d` becomes `[0-9]`, and `\b` becomes `(?-u:\b)`. The implementer verifies that `regex` 1.13 accepts `(?-u:\b)` on `&str`; if not, `\b` becomes an explicit ASCII boundary predicate.
- `\s` becomes the constant `JS_SPACE = [\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]`.
- `[^...]` and `.` keep their meaning, since JS `.` excludes `\n`, `\r`, U+2028 and U+2029. Use `[^\n\r\x{2028}\x{2029}]` where `.` appears without the `s` flag.

Rewriting look-arounds (D9-A). Each becomes the pattern without the look-around plus a `look` predicate. When a look predicate rejects a match at start `s`, the search resumes at `s + 1 char`, which reproduces JS `lastIndex` semantics for patterns with no shorter alternative at the same start. A `keep` predicate (1.x's `keep` functions) that rejects a match resumes at its end, as 1.x does. Where JS backtracking at a look-around changes the result, the predicate reproduces it, and the oracle tests pin the cases (Phase 1b, wi-31f653).

| Rule | Look-around | Predicate |
|---|---|---|
| `ignore-no-reason` | `(?=[\],)])` | the pattern consumes `[\],)]` and the keep trims it; a JS match never starts with that char, so the result is the same. A plain next-byte test is not faithful: `#[cfg_attr(x, ignorefoo, ignore)]` differs |
| `extern-crate` | `(?!(?:alloc\|std)\s*;\|self\s+as\b)` | holds when the match ends in two or more JS spaces (JS gives one back and the look-ahead then sees a space), or when the rest after the match does not match `^(alloc\|std)JS_SPACE*;` or `^self JS_SPACE+as\b` (`extern crate  alloc;` and `extern crate\n\nstd;` hit in 1.x) |
| `get-prefix` | `get_(?!(mut\|ref\|pin_mut\|unchecked(_mut)?)\b)` | the identifier after `get_` is not one of those |
| `compiler-fence` | `(?<![.\w]\s*\|\bfn\s+)` | walk back over JS_SPACE from the match start: the previous char is not `.` or an ASCII word char. The `fn` alternative is redundant (`n` is a word char) |
| `unwrap-or-default` | `(?<!\bparse\s*(?:::\s*<[^>]*>)?\s*\(\s*\)\s*)` | apply the regex `\bparse\s*(::\s*<[^>]*>)?\s*\(\s*\)\s*$` to `code[..start]` (anchored at the end) |
| shell writes (`shellWrites`) | `(?![\w.])` after the target | the next char is not a word char or `.`; on reject, retry a shorter `[^\s"'\|;&<>]*` prefix by searching for the target name again inside the matched span (covers JS backtracking) |
| `baselineRaise` flag, `--ignore-rust-version` | `(?![\w-])` | the next char is not `[\w-]` |
| `emptyIfLetErr` | `=(?!=)` | the next byte is not `=` |
| `traitImplRegions` | `\bfor\b(?!\s*<)` | the next non-space char is not `<` |

JS flags and windows (Phase 1b, wi-afd1cd, wi-1636ef):
- `m`: `regex`'s multi-line `^` follows only `\n`, so `^` becomes `(?:(?m:^)|[\r\x{2028}\x{2029}])` and the match start moves past the consumed terminator. `$` under `m` is rejected by `translate`; no 1.x pattern uses it.
- `i` (without `u`): ASCII letters become two-case classes. `regex`'s `(?i)` would also fold `ſ` and the Kelvin sign with ASCII letters, which JS does not. A non-ASCII char, a `\u`/`\x` escape or a class range under `i` is rejected.
- `translate` returns `Result<String, UnsupportedJs>` and rejects constructs that `regex` would read with another meaning (`[]`, `[^]`, `--` in a class, letter and digit escapes outside the JS set, `\u`/`\x` without their digits); `Scanner::new` reports them through `ScannerSetupError` (wi-ef9d9d, wi-d31f39).
- The 40- and 300-unit back-windows (`compiler-fence`, `seqcst`) count UTF-16 code units over the source and then slice the masked code, since masked non-ASCII text is wider in bytes than in units.

- Every regex is compiled once in `Scanner::new()` (and once per process for the hook policies, in their own `Policies::new()`). Nothing is compiled at module scope with a `LazyLock` that would need an `unwrap`.
- The `RULES` table is a `const` slice of `RuleSpec { key: "unsafe", id: "H-01", contexts: Contexts { prod: Some(Hard), .. }, msg: "...", fix: "...", fix_build: Option<&str>, lints: &[...], matcher: Matcher::Regex { pattern, keep } | Matcher::Find(fn(&Env) -> Vec<Found>) }`. Key and id are written as string literals so that `lint-claims.py` and `gen_rulemap.py` can read them as text: `\bkey:\s*"([a-z0-9-]+)"` and `id:\s*"([A-Z]+-\d{2})"`.
- The custom finders (forbidOverrides, expectAttrs, erasedErrors, todoComments, commentedCode, emptyIfLetErr, otherArmBindsErr, cfgAttrAbove, structBounds, docBlocks, docUnwrap, docArgSections) are ported one file per concept under `scan/finders/`, each ≤ 400 lines with functions ≤ 50 lines (ARC-06, CRF-02). The `Env` lazy fields (consts, traitImpls, doc) become `OnceCell` fields on a per-scan `Env` struct, not a global.

---

### 2. Behavioural parity strategy

#### 2.1 Contract inventory (byte-identical unless listed in §2.4)

- **Hook protocol**, in `scripts/temper.mjs::hook` and `hooks.mjs`:
  - stdin is read fully; a TTY means `{}`. Invalid JSON exits 0 with no output. An unknown event prints `temper: unknown hook event X` on stderr and exits 0.
  - `TEMPER_DISABLE=1` exits 0 after stdin is read.
  - A handler error prints `{"systemMessage":"temper hook <event> failed: <msg>"}` and exits 0.
  - Output is compact JSON with no trailing newline, or nothing.
  - The output shapes are `ctx`, `deny`, `ask`, `block` and `notice`, with 1.x key order: `hookSpecificOutput{hookEventName, permissionDecision, permissionDecisionReason, additionalContext?}`, `{decision:"block",reason}`, `{systemMessage}`.
- **Messages:** every string in `hooks.mjs`, `temper.mjs`, `gate.mjs`, `baseline.mjs`, `report.mjs` and `verdict.mjs`: the deny, ask and block texts, `BASELINE_USAGE`, the usage line, the `status` lines, the `stats` layout, `renderGate`, `renderRatchet`, `debtLine`, the session-start and subagent-start context, `IRON_RULES` (kept in step with SKILL.md).
- **JSON documents** (#108): `status --json` is `{ temper_status: 1, version, root, stateDir, base, fingerprint, changed, gate: null | { ok, unavailable, at, steps: [{ name, ok, ms, summary }], running: null | { startedAt, steps: [{ name, ok, ms }], current: null | { name, startedAt } } }, review: null | { verdict, stale, fingerprint, recorded_at, scores, findings: [{ severity, rule, file, line, fix }] }, baseline: null | { path, total, toolchain } | { error }, config: { review, skillGate }, disabled? }`, or `{ temper_status: 1, version, project: null }` with exit 0 outside a Rust project. `stats --json` is `{ temper_stats: 1, sessions, from, to, events, rules, gate: { runs, cached, cutShort, p50Ms, p90Ms, maxMs, recentMs, lost: { timeout, interrupted, unknown }, lostSteps: { <step> | "fmt or before" | "after fmt, before any step" | "after <step>": n }, stepP50Ms: { <step>: ms } }, review: { sessions, roundsP50, roundsMax, firstApproved }, session?: { id, rows: [{ t, event, agent, file, rules, via }] } }`, or `{ temper_stats: 1, project: null }`. Key order is part of the contract; temper lens reads them. The gate's progress file `gate-progress-<fp>.json` (#109) is `{ fp, pid, startedAt, steps, current }`, with `startedAt` the gate's own start (before the toolchain probe and formatting), written atomically after formatting, at each step's start and with `current: null` before the cargo steps (the nextest probe), and removed at the end by the gate whose `pid` it holds, by the hook that logs `gate-lost` for that `pid`, or at a session's startup once it is a day old; a dead `pid` or a step older than `stepTimeoutSec` + 300 s makes it stale.
- **Exit codes:**
  - `gate`: 0 green, 1 red.
  - `scan`: 1 when there are hard hits.
  - `baseline`: 0, 1 or 2 per branch.
  - `report`: 2 for usage and refusals.
  - No command: usage and 0. An unknown command: usage and 2.
  - "Not inside a Rust project": 2.
- **Files:**
  - Under `.git/temper/`: `gate/<fp>.json`, `verdicts/<fp>.json`, `findings-<fp>.json`, `logs/<fp>-<step>.log`, `sessions/<sid>/{base, started, start-fp, verdict-<agentKey>, stop-<agentKey>.json, gate-alive-<agentKey>.json, gate-lost-<pid>-<t>, telemetry-pushed}`, `telemetry.jsonl`, `telemetry.1.jsonl`. With `telemetry.branch` set: the ref `refs/temper/telemetry-tip`, and `sessions/<date>-<session>.jsonl` on that branch of `origin`.
  - Under `${CLAUDE_PLUGIN_DATA}`: `sessions/<sid>/skill-<agentKey>`.
  - `.claude/temper-baseline.json`, written by `renderBaseline`.
  - The report files `report.{json,md,html}`.
- **Environment:** `TEMPER_DISABLE`, `TEMPER_BASE`, `TEMPER_ALLOW_RAISE`, `CLAUDECODE`, `CLAUDE_PLUGIN_ROOT`, `CLAUDE_PLUGIN_DATA`, `CLAUDE_CODE_REMOTE_SESSION_ID` (the `ccr` telemetry field), `CLAUDE_ENV_FILE` (exports `TEMPER_CLI` and `TEMPER_BASE`; D17 adds `TEMPER_BIN` and `PATH`).

#### 2.2 How parity is established

1. **Port each Node test as a Rust test** at the narrowest real seam (TST-02). Keep its inputs and expected values verbatim and its name in snake_case. There were 143 engine tests at 1.3.0 and 352 on `main` on 2026-10-14, counted as `node --test` reports them (recounted when Phase 0a starts); `tests/release-check.test.mjs` (13 tests) checks the bash script `tests/release-check.sh`, not the engine, and is covered in §7:

   | Node file | Tests | Rust destination |
   |---|---|---|
   | `scan.test.mjs` | 69 (35 written out, 34 generated from `KEY_CASES`) | `temper-domain` unit tests |
   | `toml.test.mjs` | 7 | `temper-domain` manifest tests; the template-manifest test goes to `temper-cli/tests/it` |
   | `findings.test.mjs` | 15 | parser and model tests in domain; collection, cache and the `rulemap` check in `cli/tests/it` |
   | `hooks.test.mjs` | 84 | bash and edit policy in domain; manifest-on-disk, gate-manifest and git cases in `cli/tests/it` |
   | `handlers.test.mjs` | 34 | the handlers (session-start, skill gate, change detection, stop, subagent-start, subagent-stop, pre-handback, `report_from_transcript`) in `temper-app` with fakes; the git and gate cases in `cli/tests/it` |
   | `shell.test.mjs` | 22 | `temper-domain::hooks::shell` unit tests (wi-6e0cc2), including the deletes of protected config that ask (#104, wi-2000a3) |
   | `verdict.test.mjs` | 17 | extract and validate in domain; record and read in `temper-app` |
   | `state-retention.test.mjs` | 13 | pruning and rotation in `temper-app` with a fake clock and files; the `stats` cases in `cli/tests/it` |
   | `messages.test.mjs` | 12 | message texts in domain or `cli/tests/it`; the Node-minimum cases go to the launcher tests (§5.1) |
   | `status-json.test.mjs` | 11 | `cli/tests/it`: the `status --json` and `stats --json` documents (#108), malformed gate and verdict files read as none, and a verdict recorded at hand-back and again at `SubagentStop` counted once (#125) |
   | `telemetry-push.test.mjs` | 9 | `cli/tests/it`, with real git and a bare `origin` |
   | `hooks-registry.test.mjs` | 3 | `cli/tests/it`: every registered hook names a `HookEvent`; the `SubagentHandback` matcher is registered; `SubagentStart` and `SubagentStop` have no matcher |
   | `baseline.test.mjs` | 31 | ratchet, parse_diff, tighten and render in domain; git, temp index, 8.3 names, symlinked ancestors, symlinks and CLI (including setup's strict-lint report) in `cli/tests/it`; "CLI: setup --vendor-only refreshes .claude/temper/ and touches nothing else" is retired (D16(i), added 2026-10-19) |
   | `report.test.mjs` | 19 | build and render in domain (insta snapshots seeded from 1.x output); CLI in `cli/tests/it` |
   | `vendor.test.mjs` | 6 | **retired** (D16(i)), replaced by pin and migrate tests |

   `tests/parity/test-map.json` maps every 1.x test name to its Rust `path::test_name`s, with a `retired` note citing its D16 item where an assertion was retired (`{ "node": …, "rust": […], "retired": "D16(k) …" }`). `tests/parity/check_test_map.py` (run in CI) fails if any 1.x test is unmapped or mapped twice, an entry names no 1.x test, any mapped Rust test does not exist, or a retirement cites no D16 item. Phase 1e maps `scan.test.mjs` (1e-9, wi-a9dd30); Phase 8 adds the rest. This is the H-12 evidence that no test was silently dropped.
2. **Differential harness** (`tests/parity/`, node:test; deleted at cutover with the Node code):
   - Every case runs in two fresh copies of its fixture, one per engine. It compares exit code, stdout, stderr and written files after normalisation: temp roots become `<TMP>`; `at`, `recorded_at`, `created`, `updated` and `ms` fields are masked; log paths are masked. Each normaliser that encodes a D16 divergence cites the decision in a comment.
   - **Hook cases** (`cases/hooks/*.json`, at least 120): every message branch of the nine handlers, built from the 1.x test inputs plus extra negative cases. They include a `SubagentStop` with `agent_transcript_path` and no `last_assistant_message`. The Bash write rules hold `shell.mjs`'s decisions exactly (wi-6e0cc2): every `shell.test.mjs` and Bash `hooks.test.mjs` command is a `pre-bash` case, so a bypass 1.x closes stays closed in 2.0.
   - **CLI cases**: `scan`, `gate --json`, `status`, `stats`, `stats --branch` and `telemetry push` (against a local bare `origin`), `baseline` (every branch of `BASELINE_USAGE`), `report` and `setup --dry-run`, on the fixture workspaces.
   - **Scanner corpus** (`tests/parity/corpus.mjs`, `.github/workflows/corpus.yml`; Phase 1e): `scan --all --json` from the root of each of (1) a fresh copy of `templates/example-workspace`, (2) a fresh copy of every code block that `doc_examples.py --extract` writes from the skill, (3) every registry package named in `Cargo.lock` and `templates/example-workspace/Cargo.lock` (`cargo fetch --locked`, then `cargo metadata`; 1e-2, wi-37a444), so the corpus is a function of the commit, never whatever the runner's registry cache holds, and (4) the generated "large file". Exit code, stdout and stderr are compared byte for byte; the only normaliser is D16(e)'s, on 1.x's side, and its uses are counted. A mismatch is explained by the first differing byte and a hit-level diff (1e-6, wi-9817a6). It runs on Linux, Windows and macOS (1e-4, wi-fc6a1c). Tests, ignores and clone counts are compared through `hook pre-edit` ask cases.
   - **Fingerprint corpus**: a git fixture with staged, unstaged and untracked files, a non-UTF-8 file, a binary file, a rename and the baseline file. `status` must print the same fingerprint from both engines.
   - **State cross-read**: Node writes (session-start, gate, a reviewer verdict through pre-handback and another through subagent-stop), then Rust reads (status, stop), and the reverse.
3. **Golden files:** `docs/examples/sample-report.{md,html,json}` are regenerated by the Rust renderer from `tests/fixtures/report-sample.mjs`'s data (moved to a JSON fixture). They must come out byte-identical; a diff means a renderer bug.

#### 2.3 State compatibility

- The fingerprint stays `sha256(...)[..16 hex]` over the same byte stream: `head:<sha>\nrev:<sha>\n`, then `git diff --no-color --binary --no-ext-diff <rev> -- <pathspecs> :(exclude).temper :(exclude)target`, hashed as lossy UTF-8 (1.x hashes the decoded string), then for each untracked file in the fingerprint, sorted, `\n?<path>\n` and its raw bytes. The pathspecs include `.cargo/config` and `.cargo/config.toml` in any directory (#87). Last, when the gate settings of `.claude/temper.json` (`gate`, `scanner`, `manifests`, merged with the defaults, cut to the properties the defaults list, as canonical JSON) differ from those at the base, `\n.claude/temper.json\n<that JSON>`. A git-ignored `.cargo/config.toml`, `include_str!` files and build-script inputs are not hashed.
- The gate cache, verdict records, findings cache, session files and telemetry use serde DTOs with 1.x field names and order. Unknown fields are preserved on read and rewrite (`#[serde(flatten)] extra`) so a 1.x reader of a 2.0 file and the reverse both work.
- `temper_baseline: 1` and `temper_findings: 1` are unchanged.

#### 2.4 Intentional divergences (D16): approve as one list

- (a) Text of JSON parse errors inside messages ("not valid JSON (<detail>)"): serde_json's wording, not V8's.
- (b) Broken pipe on stdout (`temper scan | head`): 1.x exits 0 when the pipe closes before the exit code is set. 2.0 stops writing and exits with the command's own code.
- (c) TOML is parsed by the `toml` crate: stricter than 1.x on duplicate keys and malformed values. Such manifests fall back to the line-based checks, as cargo itself would reject them.
- (d) An invalid `.claude/temper.json` still means defaults (as in 1.x), but `status` and the SessionStart context now say "config unreadable (<why>): defaults in use" instead of silently ignoring it (H-10).
- (e) Hit text truncation never splits a surrogate pair (1.x could emit a lone surrogate). Only astral characters at column 160 are affected.
- (f) git calls get a 120 s timeout (1.x had none, H-13).
- (g) New: `temper --version` prints `temper X`. `baselineRaise` also recognises `temper.exe` and `temper` invoked by absolute path. Shell writes to `.claude/temper-version` are denied like the baseline file.
- (h) Telemetry lines are JSON-equivalent but may order detail keys differently. `stats` output is identical.
- (i) **Retired tests** (H-12): the 6 `vendor.test.mjs` tests, and "CLI: report and setup say so when this copy lacks report.mjs or vendor.mjs" and "CLI: setup --vendor-only refreshes .claude/temper/ and touches nothing else" in `baseline.test.mjs`, because vendoring, `--vendor-only` and optional modules no longer exist. The `--vendor-only` test was added to this list by the owner on 2026-10-19, amending wi-348e0b (#64). Replacement tests: setup pins `.claude/temper-version`; setup reports drift between the pin and the binary; `--migrate` replaces only known templates; `--force` on a pinned repository updates only `.claude/temper-version` (replaces the `--vendor-only` test); the example workspace's pin equals the plugin version.
- (j) SessionStart's timeout rises from 30 to 900 s in `hooks.json` (D6-C).
- (k) **Retired assertion** (H-12): the `ms < 10_000` wall-clock bound in `scan.test.mjs`'s "large file: a seeded hit near the end of ~5000 lines is reported in bounded time". The Rust port keeps its hit assertion (`scan::tests::whole_file::large_file_seeded_hit_near_the_end_is_reported`), and the bound moves to the scanner corpus job (`tests/parity/corpus.mjs`, `.github/workflows/corpus.yml`) as a per-file scan-time budget, because a wall-clock unit test breaks TST-10. Decided by the owner on 2026-11-01 (wi-825595). It is measured as 1e-5 decided (wi-a24829): the wall time of a root's Rust `scan --all --json` run bounds every file in it, and a root at or over 10 s is timed file by file with `scan FILE --json`; a file fails at `ms >= 10_000`, the complement of 1.x's `ms < 10_000`. The bound is absolute, the same on every runner, with no retry.
- (l) The text form of `temper scan` sorts files by byte order; 1.x's `localeCompare` order depends on the locale (owner, wi-ecb9dd, 2026-11-01).
- (m) A Windows drive-relative argument on another drive than the working directory's (`temper scan D:src\x.rs` with the cwd on `C:`) resolves against that drive's root, `D:\`. 1.x consults the hidden `=D:` variable, which only `cmd.exe` sets; when it is unset Node gives the same answer. Decided by the owner on 2026-11-01 (wi-7d8074).

#### 2.5 What is deliberately not parity-tested

Wall-clock durations (`ms`), `at` timestamps, log contents beyond the first line, and V8- versus serde-specific JSON number formatting of non-integers. None appear in the 1.x formats.

---

### 3. Porting order and strangler path

Riskiest first: distribution (Phase 0), then the scanner (Phase 1), whose regex and lexer semantics are the largest parity risk. Each phase ends green under the gate and with an approved review. A subcommand **graduates** (enters the launcher's `RUST_READY` set) when its parity cases pass on Linux, Windows and macOS in CI. The repository went public on 2026-10-30, which ended wi-5fbb56's on-demand macOS runs, so macOS runs on every code-touching PR (wi-e85ef8).

| Phase | Content | Graduates |
|---|---|---|
| 0a | Workspace skeleton, template lints, 4 crates and xtask with charters, `temper --version`, CI Rust job (fmt, clippy, test, doc, deny on Linux, Windows and macOS (wi-e85ef8), plus an MSRV job) | none |
| 0b | **Distribution spike**: `scripts/launch.mjs` with node:test tests; `xtask npm-pack`, `check-npm-lock`, `versions`; `release.yml`; publish `2.0.0-alpha.1` to npm under dist-tag `next` from a throwaway `release/2.0.0-alpha.1` branch; `verify-install` matrix on 6 runners (`npm ci --ignore-scripts` in a copied plugin dir, 60 s budget, `--version`); verify-install also covers a claude.ai organization-synced install and a cloud environment setup-script install (manual, recorded in Progress); both must pass before 0b exits (wi-48fb44). Before the first publish, the owner publishes six `0.0.0` placeholder packages by hand with a short-lived token, revokes it, and sets up npm trusted publishing on each, because npm sets up a trusted publisher only for a package that exists (wi-428ed1; checked against npm's rule before acting). `package.json` is **not** on `main` yet. | none |
| 0c | Fallback build in the launcher: lock, `cargo build --release --locked`, atomic install, `build.failed` marker; D6 behaviours tested with the binary absent | none |
| 1 | Domain: lexer, regions, rules, finders, `Scanner`, `new_hits`, `classify_path`; scanner differential corpus job. App: minimal `discover`, `crate_of`, `rel_to` (verbatim, 8.3 and symlinked-ancestor cases). `temper scan` | none (`scan` graduates in Phase 4, 1d-10 A) |
| 2 | Manifest, config, glob; hook I/O; telemetry; skill markers. Handlers `pre-bash`, `pre-edit`, `post-tool`, `user-prompt` (the hot path) | `hook pre-bash`, `hook pre-edit`, `hook post-tool`, `hook user-prompt` |
| 3 | Fingerprint (parity corpus), sessions (base, start-fp, prune), gate-cache read, verdict extract, validate and record. Handlers `session-start`, `subagent-start`, `pre-handback`, `subagent-stop` (reviewer branch); `status` | those hooks, `status` |
| 4 | `StdProcess` with timeouts; gate steps, manifests step, scan step, cache, render; `stop` and `subagent-stop` (implementer branch); `temper gate`; bare `temper scan` (1.x `scanChanged`) | `gate`, `hook stop`, `hook subagent-stop`, `scan` (1d-10 A, wi-ca29db) |
| 5 | Cargo message parser (fixtures `clippy-*.jsonl`, `rustdoc-capped.jsonl`), collection and cache, temporary-index diff, renames, crate aliases, ratchet, tighten, init, update, check; `tests/e2e/brownfield.sh` against Rust (`TEMPER_ENGINE=rust`) | `baseline` |
| 6 | Report model, Markdown and HTML (golden sample), CLI | `report` |
| 7 | `setup` (templates from `TEMPER_PLUGIN_ROOT`, `.claude/temper-version` pin, `--migrate` with the known-template hash table), `stats` (with `--branch`), `telemetry push`; template changes (§6) prepared on a branch | `setup`, `stats`, `telemetry` |
| 8 | **Cutover (2.0.0)**: `launch.mjs` replaces `scripts/temper.mjs`; delete `scripts/lib/`, the Node tests, `tests/parity/` and the vendored copy in `templates/example-workspace/.claude/temper/`; `package.json` and `package-lock.json` land; `hooks.json` SessionStart timeout and status message; docs, CHANGELOG, Python checks, `tests/ci.sh`; release 2.0.0 (§4) | everything |

Phase 1 lands as six PRs (1d is two, wi-719373), each green with no dead code, because every helper arrives with its first consumer (wi-a5a813):
- 1a: the `js` helpers, the lexer, test regions, the counts, `Hit`, and the `Scanner` with six rules (`unsafe`, `unwrap`, `panic`, `dbg`, `print`, `expect-attr`); `regex`, `thiserror` and dev `proptest` added.
- 1b: the lexer's comment records and remaining helpers, and 40 more regex rules in 1.x table order with the `keep` and look-around predicates (46 of the 60 in all).
- 1c: 9 of the 11 remaining finders, one file per concept (`empty-if-let-err`, `spin-lock`, `serde-hashmap`, `test-branch`, `struct-bounds`, `forbid-override`, `erased-error`, `todo-comment`, `commented-code`); the regex rules `allow-attr`, `discard-ok` and `empty-err-arm` (with `otherArmBindsErr`), and `ScanOptions::forbid` with its only reader (wi-13609d). 58 of the 60 rules in all. The doc group moves to 1d (wi-9f78ea), and `Scanner::rules()` with `RuleInfo` to Phase 4, its first non-test reader (wi-ca74b3).
- 1d is two PRs (wi-719373):
  - 1d.1: the doc group (`doc-unwrap`, `doc-arg-sections`, doc blocks, the lazy `Env::doc` and `is_rust_fence`, specified in the 1c plan's Appendix A) and the 1.x tests that need the whole table; `new_hits` (wi-46a7f4) and `classify_path` with `LibTarget` (wi-208563). All 60 rules; every `scan.test.mjs` test has a Rust home.
  - 1d.2: the `temper scan --all` and `temper scan FILES` vertical slice, in two PRs (OC-5 B, wi-368b47):
    - 1d.2a: everything without processes: `Serialize` on `Hit` with `HitAt`, `format_hit`, `render_scan_text`, `PathStyle` and `unverbatim`; the app's `Files` port, Node's path rules, the Cargo-walk `discover`, `crate_of`, `rel_to` and `walk`, and `Engine::scan`; the host's `StdFiles`; the `temper scan` CLI. `serde` added.
    - 1d.2b: `ProcessRunner`, the host's `StdProcess` and the git top level in `discover`; `wait-timeout` added.
- 1e is two PRs (wi-464329):
  - 1e.0: the 1d.2b review's follow-ups in `temper-host` (#192).
  - 1e.1: the differential harness in `tests/parity/` (node:test and Python stdlib, deleted at cutover): the 13 `scan` parity cases (`scan.test.mjs`), the scanner corpus driver (`corpus.mjs`) with the D16(k) per-file bound (wi-825595, measured as 1e-5 decided), and the `scan.test.mjs` part of the test map (`test-map.json`, `check_test_map.py`; 1e-9, wi-a9dd30). The parity cases and the test map run in `tests/ci.sh` under the required `Checks` job; `.github/workflows/corpus.yml` runs the cases and the corpus on Linux, Windows and macOS, weekly, on `release/**`, by hand and on PRs that touch the engines or the scanner's inputs (owner, wi-295cf1), as an advisory check (owner, wi-68036c). Graduation moves to Phase 4 (1d-10 A); the corpus job runs the Rust binary directly.

Strangler mechanics during phases 1 to 7:
- `hooks/hooks.json` is unchanged, so users run 1.x exactly.
- Maintainers dogfood with `node scripts/launch.mjs` by setting `TEMPER_ENGINE=rust` and `TEMPER_BIN=target/release/temper`. They can point `hooks.json` at `launch.mjs` in a local `--plugin-dir` checkout.
- `launch.mjs` delegates a graduated subcommand to Rust; anything else runs the 1.x CLI **in-process** (`await import('./temper.mjs')`, so argv and stdin are unchanged).
- Mixed engines within one session are safe because of §2.3, which the cross-read parity cases verify. A Rust `stop` that starts `temper telemetry push` reaches the Node subcommand through the launcher until `telemetry` graduates.
- **1.x releases during the port** (#22). A 1.x patch or minor release still ships from `main` through the release checklist in `AGENTS.md`. Until the cutover `hooks.json` calls `scripts/temper.mjs` and `package.json` is not on `main`, so the release carries the Rust workspace and `launch.mjs` in the plugin directory unused: Claude Code runs no npm install, and users run only the Node engine. A 1.x fix that changes behaviour breaks the parity cases that cover it, so it lands in both engines in the same PR, or the Rust subcommand it touches waits to graduate. The vendored copy and its `VERSION` stay in step as today.

---

### 4. Release pipeline (`.github/workflows/release.yml`, `xtask`)

Triggered by `workflow_dispatch` with the input `version` on branch `release/<version>`. The release PR is created by `cargo xtask versions --set X`. Actions are SHA-pinned.

1. **check** (Linux): `cargo xtask versions --check X`; the full `tests/ci.sh`; `cargo publish --workspace --dry-run --locked`.
2. **build** (matrix, native runners):

   | Target | Runner | Smoke run |
   |---|---|---|
   | `x86_64-unknown-linux-musl` | `ubuntu-24.04` | native |
   | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | native |
   | `aarch64-apple-darwin` | `macos-15` | native |
   | `x86_64-apple-darwin` | `macos-15`, cross-compiled | under Rosetta (`arch -x86_64`) |
   | `x86_64-pc-windows-msvc` | `windows-2025` | native, `+crt-static` |
   | `aarch64-pc-windows-msvc` | `windows-11-arm` | native, `+crt-static` (D5) |

   Each job runs `cargo build --release --locked -p temper-cli --target <t>` and uploads an artifact. Also on each runner, `temper --version` plus a hook smoke test: pre-bash JSON with `--ignore-rust-version` must produce the exact deny JSON.
3. **package** (Linux, after all builds succeed; one broken target fails the release here, before anything is published):
   - `cargo xtask npm-pack X` writes `target/npm/@wet-ink/temper-<p>-<a>/` with a `package.json` (`name`, `version` X, `description`, `license` `MIT OR Apache-2.0` (both licence files copied into the package), `repository` (needed for provenance), `os: [p]`, `cpu: [a]`, `files: ["bin", "LICENSE-MIT", "LICENSE-APACHE"]` (npm packs a `LICENSE-*` file only when `files` names it); no `bin`, no `scripts`) and `bin/temper[.exe]`, mode 0755. Packing happens on Linux so the executable bit survives.
   - Release archives: `temper-cli-vX-<target>.tar.gz` (or `.zip` for Windows) containing `temper[.exe]`, plus `SHA256SUMS`, plus `actions/attest-build-provenance` attestations.
4. **publish-npm** (environment `release`, which needs approval, D12): `npm publish --provenance --access public` for each of the six. It is idempotent: a version that already exists with the same integrity is skipped, so a partial failure can be re-run.
5. **lockfile** (Linux, clean checkout, **no `node_modules`**): `npm install --package-lock-only --ignore-scripts --no-audit --no-fund` with npm ≥ 10.9, then `cargo xtask check-npm-lock` (invariant 6), then commit `package-lock.json` to the release branch. A lockfile is never regenerated with `node_modules` present (npm/cli#4828); this is in the root `AGENTS.md` do-not list.
6. **verify-npm** (matrix of the six runners, using `actions/setup-node` with `architecture: x64` on macOS to exercise darwin-x64): copy the plugin directory (without `.git`) to a temp dir, as Claude Code's cache does. Then:
   - `timeout 60 npm ci --ignore-scripts --no-audit --no-fund` succeeds;
   - exactly one `node_modules/@wet-ink/temper-*` exists, the matching one;
   - `node scripts/temper.mjs --version` prints `temper X`;
   - the hook smoke test passes;
   - the install took under 45 s (a margin under 60 s).
7. **publish-crates** (environment `release`): `cargo publish --workspace --locked`, after verify-npm.
8. **github-release**: create tag `vX` and the GitHub release with archives, `SHA256SUMS` and the attestations; `[package.metadata.binstall]` in `temper-cli` points at these names.
9. **Human merges** the release PR into `main`. Only then do marketplace users (whose source is `main`) see the new `plugin.json` version, `package.json` pins and lockfile. Users never see a version whose packages are not published.

- **On every PR** (`ci.yml`): `cargo xtask check-npm-lock` (after cutover) and `cargo xtask versions --check`.
- **Versioning:** `xtask versions --set X` rewrites:
  - `Cargo.toml` `[workspace.package].version` and the sibling `version` fields;
  - `.claude-plugin/plugin.json`;
  - `plugins/temper-lens/.claude-plugin/plugin.json`, from the cutover, as for `.claude-plugin/plugin.json` (both stay 1.x during the port);
  - `package.json` `version` and all six pins;
  - `templates/workspace/.claude/temper-version` and `templates/example-workspace/.claude/temper-version`;
  - it then runs `cargo update --workspace` and checks that CHANGELOG has a `## X` heading.

---

### 5. Launcher and fallback

#### 5.1 `scripts/temper.mjs` (the launcher after cutover, about 120 lines plus node:test tests in `tests/launcher.test.mjs`)

1. `pluginRoot` is `dirname(dirname(fileURLToPath(import.meta.url)))`; `version` is read from `${pluginRoot}/.claude-plugin/plugin.json`.
2. If `TEMPER_DISABLE === '1'` and `argv[2] === 'hook'`: drain stdin and exit 0. The human's escape works with no binary. Then 1.x's Node check (`requirements.mjs`, #89): on Node 15 or 16, `hook session-start` in a Rust workspace prints only the warning, and a CLI command prints it on stderr; on Node 17 SessionStart runs and adds the warning line.
3. Resolve the binary, first hit wins:
   - (a) `TEMPER_BIN` (dev, CI, eval sandboxes).
   - (b) `createRequire(import.meta.url).resolve('@wet-ink/temper-${process.platform}-${process.arch}/package.json')`, then `bin/temper` (`.exe` on win32).
   - (c) `${CLAUDE_PLUGIN_DATA}/engine/<key>/temper[.exe]` (§5.4).
4. Run it with `spawn(bin, argv.slice(2), { stdio: 'inherit', windowsHide: true, env: { ...process.env, TEMPER_PLUGIN_ROOT: pluginRoot, TEMPER_PLUGIN_VERSION: version } })`.
   - Forward `SIGINT`, `SIGTERM` and `SIGHUP` to the child.
   - On exit, set `process.exitCode = code ?? 1`. A signal-killed child gives 1, a non-blocking hook error, which fails open as in 1.x.
   - On POSIX `EACCES` (the exec bit lost in transit), `chmod 0755` once and retry.
5. No binary:
   - for `hook session-start`, run §5.4 synchronously, then exec;
   - otherwise apply the D6 policy, emitting the JSON outcomes itself. These few fixed strings are the only hook texts in JS; tests assert them.
6. No shell anywhere; paths are passed as argv. On Windows, `.exe` resolution is explicit.

#### 5.2 `hooks/hooks.json` at cutover

The commands and args are unchanged (`node ${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs hook <event>`). SessionStart gets `"timeout": 900` and `"statusMessage": "temper: starting (first run after install may compile its engine)"`. All other timeouts stay.

#### 5.3 First session after install or update (with D6-C)

| Situation | What happens |
|---|---|
| Marketplace install or update (also `claude plugin install` in a cloud environment's setup script, unverified: see Risks), npm present and fast | Claude Code copies the plugin to `cache/temper/temper/X/` and runs `npm ci --ignore-scripts`; the matching `@wet-ink/temper-*` lands in `node_modules`. The first hook resolves it (b). No build, no wait. |
| npm missing, install failed or over 60 s, or unsupported platform (freebsd, linux-ppc64, ...) | No `node_modules`. The first SessionStart runs the fallback build synchronously (about 1 to 3 min, with a status message), then works normally. If the build fails: `build.failed` is recorded, the session starts in the D6 degraded mode, and SessionStart context says how to fix it (install Rust ≥ MSRV, or `TEMPER_DISABLE=1`). |
| `--plugin-dir` or local-directory marketplace (dev) | Loaded in place, no npm install. Every SessionStart runs `cargo build --release --locked` into the dev engine dir; cargo makes it a no-op when the sources are unchanged. Maintainers can set `TEMPER_BIN`. |
| Eval sandbox that hides `~/.rustup` | The fallback cannot build; set `TEMPER_BIN` to a prebuilt binary visible in the sandbox (documented in README "Development"). |

#### 5.4 Fallback build

- **Key:** for a cached plugin (`pluginRoot` contains no `.git`), the key is `X`, since the cache directory is immutable per version. For a source checkout (dev), the key is `dev`, and cargo's own freshness decides.
- **Build:**
  - `cargo build --release --locked --manifest-path ${pluginRoot}/Cargo.toml --target-dir ${DATA}/engine/<key>/target`.
  - **cwd is `${DATA}`**, so the repository's `rust-toolchain.toml` does not trigger a rustup download of 1.98.1, and the user's default toolchain is used (≥ MSRV, D4).
  - Output goes to `${DATA}/engine/<key>/build.log`.
- **Concurrency:** an atomic `mkdir ${DATA}/engine/<key>/.lock` (stale after 15 min by mtime). A second session waits on the lock, polling with `Atomics.wait` up to the hook's budget, then falls back to the D6 policy. Cargo's own target-dir lock also serialises builds.
- **Install:** copy `target/release/temper[.exe]` to `temper.tmp-<pid>`, then rename it to `${DATA}/engine/<key>/temper[.exe]`. If the destination exists, skip, because a running `.exe` cannot be replaced on Windows.
- **Pruning:** keep the newest 2 version keys plus `dev`.

---

### 6. Migration for 1.x users; template and setup changes

**Templates** (`templates/workspace` and `templates/example-workspace` stay identical where they overlap):
- Remove `templates/example-workspace/.claude/temper/` (the vendored copy).
- Add `.claude/temper-version` containing `X`.
- `.github/workflows/rust.yml`:
  - Replace every `hashFiles('.claude/temper/temper.mjs')` with `hashFiles('.claude/temper-version')`.
  - Add, before the first use in each job that runs temper, a step `id: temper` that runs `echo "version=$(tr -d '\r\n' < .claude/temper-version)" >> "$GITHUB_OUTPUT"`, followed by `taiki-e/install-action@<sha>` with `tool: temper-cli@${{ steps.temper.outputs.version }}` (D13).
  - `node .claude/temper/temper.mjs` becomes `temper`.
  - Remove the `Vendored temper CLI` step (`id: vendor`, #106) and the comment above it, the gate job's `vendor-changed` output, and in `report-comment` the `VENDOR_CHANGED` env, its `printf` line and comment, and the `|| needs.gate.outputs.vendor-changed != ''` in the comment step's `if:`. With no vendored copy, CI installs the published release the pin names, so a PR can change which release runs (a one-line pin change, visible in review) but not that release's code. `--keep-going` on the clippy and MSRV steps (#106) stays.
- `.githooks/pre-commit`: if `.claude/temper-version` exists, it requires `temper` on PATH, or prints `temper pre-commit: temper <pin> is required: cargo binstall temper-cli@<pin> (or cargo install temper-cli --locked --version <pin>)` and exits 1. It warns when `temper --version` differs from the pin, then runs `temper baseline --check --base HEAD`, or without `--base` before the first commit.
- `.claude/settings.json` allow-list: `Bash(temper baseline --check*)`, `Bash(temper baseline --tighten*)`, `Bash(temper report*)`, `Bash(temper gate*)`.
- `AGENTS.md` command rows: keep `node "$TEMPER_CLI" gate` (the launcher path is unchanged) and add `temper gate` as an equivalent.

**`temper setup` (2.0)**:
- Templates come from `TEMPER_PLUGIN_ROOT` (set by the launcher) or `CLAUDE_PLUGIN_ROOT`. Without either, setup refuses with exit 2: "setup needs the temper plugin's templates: run /temper:setup in Claude Code".
- `vendorTool` is replaced by `pin`: create `.claude/temper-version`, or report `drift  .claude/temper-version says Y, this temper is X; rerun with --force to update it`.

**`temper setup --migrate`** (D15), offered when `.claude/temper/temper.mjs` exists:
- For each of `.github/workflows/rust.yml`, `.githooks/pre-commit` and `.claude/settings.json`: when the file's SHA-256 is in `LEGACY_TEMPLATES` (a checked-in table in `temper-app::setup::legacy`, generated once by `cargo xtask legacy-hashes` from the git history of `templates/workspace/**` at every `v1.*` tag up to the cutover, v1.0.0..v1.3.0 today; the tag list comes from `git tag -l "v1.*"`, not a hard-coded range), replace it with the 2.0 template. Otherwise print `manual .github/workflows/rust.yml: replace "node .claude/temper/temper.mjs" with "temper" (N lines) and add the "Install temper" step from <template path>`.
- For `settings.json` (customised): line-exact replacement of the four known allow entries when present; otherwise manual.
- Remove `.claude/temper/`; write `.claude/temper-version`.
- `--dry-run` shows everything without writing.

**`/temper:setup` skill:** a new step 1b: if the dry run reports `legacy .claude/temper/ (temper 1.x vendored CLI)`, explain the migration and ask with AskUserQuestion, then run `setup --migrate`. `allowed-tools` keeps `Bash(node *)` (the launcher).

**Unmigrated 1.x repositories** keep working: their CI and pre-commit hook run their own vendored 1.x copy, the baseline format is shared, and the 2.0 hooks still deny shell writes to `.claude/temper/`. The SessionStart context adds one line: "This repository vendors temper 1.x for CI (.claude/temper/); run /temper:setup to move to the temper 2 binary." `temper status` shows the same, plus pin/version drift.

### 7. Repository checks and docs touched (PROC-11)

- **`tests/ci.sh`:**
  - Run `cargo fmt --all -- --check`, clippy, `cargo test --workspace --all-features --locked` and doc first.
  - `cargo build --release -p temper-cli` with `TEMPER_BIN` exported for the e2e and `doc_examples`.
  - `node --test tests/launcher.test.mjs`.
  - The Python checks.
  - `cargo xtask versions --check` and `check-npm-lock`.
  - The vendored-copy diff is removed; a "templates' temper-version equals plugin.json" check is added.
- **`tests/release-check.sh`** (129 lines, added in 1.3.0) and its 13 tests in `tests/release-check.test.mjs`: it checks that `plugin.json`, the top CHANGELOG heading, `templates/example-workspace/.claude/temper/VERSION` and, given `--tag`, the tag agree, and that a release changing `scripts/` or `templates/workspace/` has an "Upgrading from" note. `tests/ci.sh` and the 1.x `release.yml` run it, and `release.yml` also calls it with `--notes` to write the GitHub release notes from the CHANGELOG section (`.github/workflows/release.yml:63`), so retiring the script moves that job to the port's `release.yml` or xtask. The vendored `VERSION` is deleted at cutover, and the version agreement overlaps `cargo xtask versions --check`. Decided (wi-5fbb56, amending D16(i)): the script is folded into `cargo xtask versions --check`, and its tests are ported there rather than retired. The port keeps its three jobs: writing the release notes from the CHANGELOG section (`--notes`), refusing an `## Unreleased` section above the release heading, and requiring the "Upgrading from" note.
- **`registry.py`:** scan `crates/**/*.rs` and `scripts/*.mjs` for rule IDs instead of `scripts/**/*.mjs`.
- **`lint-claims.py` and `gen_rulemap.py`:** read `(key, id)` pairs from `crates/temper-domain/src/scan/rules.rs` as text; `OUT_REL` becomes `crates/temper-domain/data/rulemap.json`.
- **`doc_examples.py`:** the scanner pass writes each block to `<tmp>/b<N>/src/lib.rs` (or `src/main.rs` when it has `fn main`) with a minimal `[package]` manifest, and runs `$TEMPER_BIN scan <files> --json` once.
- **`tests/e2e/brownfield.sh` and `smoke.sh`:** unchanged commands (the launcher path); they need `TEMPER_BIN` or a build.
- **`.github/workflows/ci.yml`:** add jobs for Rust (Linux, Windows and macOS, wi-e85ef8), MSRV (`cargo +1.96 check --workspace --locked`), the parity harness (until cutover), the launcher tests, `check-npm-lock`, and `cargo deny`. Keep the plugin-validate step.
- **Docs:**
  - README: Install (npm packages via Claude Code; the fallback), CLI (binary, `--version`; no Node dependency except the launcher), Development (cargo commands, `TEMPER_BIN`, parity harness), Brownfield rollout (pin plus install step instead of vendoring).
  - CHANGELOG 2.0.0, with a migration guide.
  - `skills/setup/SKILL.md` and `skills/audit/SKILL.md` (wording only).
  - `agents/*.md`: paths unchanged; mention `temper` on PATH.
  - `review-rubric.md`'s `commands_run` example.
  - `docs/history/2.0-rust-port/`: this plan as the spec, plus the review.
  - Root `AGENTS.md` decisions log (D1..D17).

---

## Test list (drive in this order; each item is one red-green-refactor cycle unless marked as a group)

Phase 0
- [x] `temper --version` prints `temper <CARGO_PKG_VERSION>` and exits 0 (cli/it).
- [x] An unknown subcommand prints the 1.x usage text and exits 2; no subcommand prints it and exits 0 (cli/it; expected text copied from `temper.mjs`).
- [x] Launcher: `TEMPER_DISABLE=1 hook X` exits 0 without resolving a binary; below the Node minimum it keeps 1.x's early warning (ported from the Node-minimum cases in `messages.test.mjs`, #89) (launcher.test).
- [x] Launcher: `TEMPER_BIN` wins; stdin, argv and the exit code pass through; a signal-killed child gives exit 1 (launcher.test with a stub binary script compiled per OS, or `node` as the stub).
- [x] Launcher: resolves `node_modules/@wet-ink/temper-<platform>-<arch>/bin/temper[.exe]` (fake package dir).
- [x] Launcher: without a binary, `hook pre-edit` on a `.rs` file prints the D6 deny JSON; `hook pre-bash` prints the once-per-session notice; `hook stop` prints `temper gate: UNAVAILABLE (...)`. Tested through `withoutEngine` in `fallback.test`; at process level once a hook graduates (wi-6bf67c).
- [x] Launcher: with a fake `cargo` on PATH that writes a binary, SessionStart builds into `${DATA}/engine/<key>/` and execs it; a second concurrent call waits on `.lock`; a failing build writes `build.failed` and is not retried before the next SessionStart. Tested through `buildEngine` and `withoutEngine` in `fallback.test`, plus a lock left by a dead owner; the exec half is tested at process level once `hook session-start` graduates (wi-6bf67c).
- [x] `xtask versions --check` fails when any one of the version sites differs (one case per site).
- [x] `xtask check-npm-lock` fails on: a missing platform entry (the #4828 shape), a non-exact pin, an `http` resolved URL, a missing integrity, an extra package, `overrides` or `scripts` in `package.json`, lockfileVersion 1.
- [x] `xtask npm-pack` produces six package dirs with the correct `os`/`cpu`, mode 0755 binaries and no lifecycle scripts (snapshot of the generated package.json files, insta).
- [ ] verify-install, manual, once in Phase 0b: install `2.0.0-alpha.1` through a claude.ai organization sync (`~/.claude/plugins/synced/`) and through a cloud environment's setup script (`claude plugin marketplace add` plus `claude plugin install`). Record in Progress whether `node_modules/@wet-ink/temper-*` is present in each, and if not, whether the fallback build or D6 degraded mode follows. Phase 0b does not exit until both paths have an engine (wi-48fb44): a path that skips `npm ci` gets a step in the environment's setup script that installs the npm package, and this check is run again.

Phase 1: lexer and scanner (domain; ported from `scan.test.mjs`, in its order)
- [x] masks strings, raw strings, chars and comments but keeps lifetimes (incl. `'\''`, `br#".."#`, nested block comments, unterminated string at EOF).
- [x] property: `mask_rust(src).code.len() == src.len()` and newline positions are identical for arbitrary strings (proptest, TST-06).
- [x] property: `scan` never panics and line numbers are in `1..=lines(src)` for arbitrary input (proptest; the token soup reaches every 1b predicate).
- [x] hard hits in production code (each H-rule key in the 1.x table).
- [x] test regions allow unwrap/panic but not dbg or unsafe.
- [x] `#[expect]` with a reason converts a hard hit into an exception request.
- [x] allow attribute, erased errors in pub signatures, `.ok();` discard.
- [x] `new_hits` reports only introduced violations (multiset on key+text).
- [x] path classification (`classify_path`, `is_build_script`).
- [x] no false positives on idiomatic code.
- [x] rule table: unique keys, rule ids, four contexts, fix text.
- [x] then, one per 1.x test: assert-in-prod; ignore-no-reason (incl. `cfg_attr`, `educe(Debug, ignore)` not counted; look-ahead predicate); extern-crate (look-ahead predicate); print fix text for build scripts; doc-unwrap positive and negative; empty-if-let-err depth and keep-first-error; escaped quote char and ignore counting; empty arms; thread-spawn; compiler-fence (look-behind predicate, `fn compiler_fence` and `x.compiler_fence(` not flagged) and global-allocator; map-index; filter-map-ok and parse-unwrap-or; seqcst and fetch-update; crate-lint-attr, pub-glob-reexport, prelude-mod; pub-smart-ptr; struct-bounds; ops-impl; type-alias-primitive, get-prefix (predicate), mutex-unit; doc-arg-sections; detached-spawn; idiomatic ledger module; commented-out code heuristic.
  - [x] 1b: the regex entries (assert-in-prod, ignore-no-reason, extern-crate, print fix text, thread-spawn, compiler-fence with its predicate, global-allocator, map-index, filter-map-ok and parse-unwrap-or, seqcst and fetch-update, crate-lint-attr, pub-glob-reexport, prelude-mod, pub-smart-ptr, ops-impl, type-alias-primitive, get-prefix with its predicate, mutex-unit, detached-spawn, idiomatic ledger module), with oracle tests for every predicate.
  - [x] 1c: empty-if-let-err depth and keep-first-error, the escaped quote char and ignore counting (the scan half; the lexer half landed in 1a), empty arms, struct-bounds, commented-out code; with the finder `KEY_CASES` (test-branch, serde-hashmap, spin-lock, todo-comment), forbid-override, and "large file" without its timing (D16(k)); oracle tests for every finder and helper.
  - [x] 1d: doc-unwrap positive and negative, doc-arg-sections (moved from 1c, wi-9f78ea).
- [x] unwrap-or-default: `x.parse::<u8>().unwrap_or_default()` is not flagged (look-behind predicate) but `x.unwrap_or_default()` is.
- [x] JS class translation: `\b` adjacent to a non-ASCII letter behaves as in JS (é is not a word char); `\s` matches U+00A0 and U+FEFF but not U+0085.
- [x] hit text: `js_trim` plus 160 UTF-16-unit truncation, with an astral character at the boundary (D16(e)).
- [x] differential corpus (CI job, not a unit test): zero unexplained diffs over the example workspace, the skill code blocks and the registry sources. Built in 1e.1 (`tests/parity/corpus.mjs`, `.github/workflows/corpus.yml`, #194); first green run on Linux, Windows and macOS: https://github.com/Wet-Ink-Corporation/temper/actions/runs/41340882480.
- [x] `rel_to`: verbatim `\\?\C:\...` and `//?/C:/...` inputs (domain, both `PathStyle`s); 8.3 short name vs long name is the same root (cli/it, `#[cfg(windows)]`, uses `cmd /c for %I in (..) do @echo %~sI` as 1.x does).
  - [x] 1d.2a: the verbatim half, as `temper_domain::paths::tests::unverbatim_like_1x` and `temper_app::relpath::tests::rel_to_verbatim_inputs_like_1x` (OC-1 B); the 8.3 assertions that the binary cannot observe, on a simulated short name, in `rel_to_falls_back_to_real_paths_when_lexically_outside`.
  - [x] 1d.2b: the cli/it 8.3 test, moved by the 1d.2 plan's size rule: `temper-cli/tests/it/scan.rs`, `rel_to_one_directory_spelled_with_an_8_3_short_name_and_a_long_name_is_the_same_root` (runs on the Windows CI job).
- [x] `rel_to`: one directory reached through a symlinked ancestor and through its real path is the same root (cli/it, every OS; ported from the 1.3.0 relTo symlink test; the symlink probe and the Windows privilege handling as in `safe_output_path`): `temper-cli/tests/it/scan.rs`, test of the same name, with the rest in `temper_app::relpath::tests::rel_to_falls_back_to_real_paths_when_lexically_outside` (1d.2a).
- [x] `temper scan --all --json` and `scan FILE` output equal the parity cases (cli/it with the fixture workspace): `temper-cli/tests/it/scan.rs` against 1.x's recorded output, the text form with D16(l) (1d.2a); and the parity harness's cases: `tests/parity/scan.test.mjs` (1e).

Phase 2: manifests, config, hooks hot path
- [ ] `Manifest` parses the shapes temper reads; CRLF, escapes, and invalid input give `Err` (ported toml tests; the hand-written parser's own edge tests are replaced by the same inputs through `toml`, D16(c)).
- [ ] the template and example manifests parse (cli/it reads `templates/**/Cargo.toml` via `CARGO_MANIFEST_DIR/../..`).
- [ ] non-caret requirements; major versions; path normalisation (`clean_rel`); members without cargo; dependency names in document order (ported).
- [ ] workspace root discovery follows Cargo: self, `package.workspace`, nearest ancestor, exclude (app with fake files).
- [ ] watched manifest switches carry their rule id; insertion order preserved in ask text.
- [ ] `Config::load`: defaults, a shallow merge per 1.x (`gate`, `scanner` and `manifests` merged one level, `protected` replaced), invalid JSON gives defaults plus a `ConfigProblem` (D16(d)).
- [ ] `Glob`: `**/`, `**`, `*`, `?`, escaping of regex metacharacters (ported cases plus a property test: a literal pattern matches only itself).
- [ ] `apply_edit`: Write, Edit, MultiEdit, `replace_all`, missing `old_string` gives `None`, empty `old_string` follows JS split/join semantics.
- [ ] `bash_policy`, one test per 1.x hooks.test Bash case: `--ignore-rust-version` (only as a cargo flag); shell writes to Cargo.lock, Cargo.toml, `.rs`, the baseline and `.claude/temper/`; `temper baseline --init/--update` refused unless `TEMPER_ALLOW_RAISE` (asks); git checkout/restore/stash of `.claude`; `rm`, `unlink`, `Remove-Item` (and its aliases) or `git rm` (with `--cached`) of protected config asks, and a recursive delete names the protected globs and files under it (#104, wi-2000a3); `cargo add/remove` asks unless pre-approved; `--no-verify` and `git commit -n`; `-A` and `--cap-lints`; `cargo insta accept`. New (D16(g)): `temper.exe baseline --update` and an absolute `/x/temper baseline --init` are refused; shell writes to `.claude/temper-version` are denied.
- [ ] `edit_policy` (pure) and `pre-edit` (app plus cli/it on disk), each 1.x case: a new member without inheritance is denied (MOD-09); a new member with inheritance asks as a crate boundary; an excluded crate; gaining inheritance passes; legacy problems advise; `ownLints`; a member overriding lints; `[profile]` in member vs root; `test = false`; `[patch]`; rust-version bump; non-caret advisory; Cargo.lock denied; CRLF; `lintConfigChanged` ignores plain inheritance; `[[bench]]` after dev-deps; major bump asks; `[lib] bench = false`; `#[ignore]` added asks, Miri form not; new `mod.rs` denied; removed tests ask; `.git/` and state-dir edits denied; skill gate denies when the marker is absent and the agent is not preloaded.
- [ ] hook I/O (cli/it): invalid stdin JSON exits 0 with no output; an unknown event writes stderr and exits 0; `TEMPER_DISABLE=1`; a handler error becomes the `systemMessage` JSON (forced with a fake `Files` returning `FsError` in an app test plus a cli/it case with an unreadable state dir); stdin over the 256 MiB cap fails open with a `systemMessage`.
- [ ] `post-tool` marks the skill for Skill(`temper:rust-craft`) and Read(`.../rust-craft/SKILL.md`), per agent key; `user-prompt` context only when not loaded.
- [ ] telemetry line shape (with `ccr` in a cloud session) and `stats` aggregation (sessions, events by count, blocked-by-rule, gate latency at Stop, gates cut short, review rounds, a verdict recorded at hand-back and again at `SubagentStop` counted once (#125)); rotation to `telemetry.1.jsonl` at 1 MiB, and `stats` reading both files.

Phase 3: state, fingerprint, sessions, verdicts
- [ ] `FingerprintHasher` equals a SHA-256 known-answer vector for a fixed byte stream (domain).
- [ ] fingerprint equals 1.x's on the git parity fixture: staged, unstaged, untracked, non-UTF-8, binary, rename, and the baseline tracked and untracked ("the baseline is part of the fingerprint", ported); `.cargo/config(.toml)` in any directory and the gate keys of `.claude/temper.json` move it, while hooks-only keys, a reformat or a git-ignored config do not (#87).
- [ ] no-git fingerprint walks Rust-relevant files, sorted, excluding `target`, `.git`, `node_modules` and `.temper`.
- [ ] `discover`: git top-level; no git means the outermost Cargo workspace; not Rust means `None`; the state dir uses `--git-common-dir` (worktrees).
  - [x] 1d.2b: all but the state dir, in `temper_app::project::tests` (`discover_prefers_the_git_top_level`, `discover_falls_back_without_git`, `discover_without_git_takes_the_outermost_cargo_directory`, `discover_is_none_without_a_cargo_toml`, `discover_is_none_in_a_git_repo_without_cargo_toml_above_the_cwd`) and in cli/it against real git.
- [ ] session base: set on startup, clear and fork (forced), kept on resume; an unresolvable recorded base falls back to HEAD; start-fp recorded.
- [ ] `TEMPER_BASE` from another repository falls back to HEAD with a single stderr note (CLI).
- [ ] prune sessions (14 d) and markers (7 d) by mtime, and verdicts and gate runs by age, count and bytes with the start fingerprint and the 60-minute grace kept; startup only (fake clock and files; ported from `state-retention.test.mjs`).
- [ ] `extract_verdict`: the last fenced json block containing `temper_verdict`; an unparseable one reports the parse problem.
- [ ] `validate_verdict`: one test per problem message (schema, verdict, fingerprint, findings fields, scores, `correctness`, `escape_hatches` and `craft` always rated, evidence, unread changed files, approved-with-majors, approved-with-low-scores, approved-with-red-gate, gate entries); a `files_read` entry covers a changed file only as its repo-relative path or an absolute path ending in `/` plus that path, never a bare suffix (#86).
- [ ] `pre-handback`: non-reviewers pass (a `handback` telemetry row either way); an invalid verdict is denied with the problem list; a valid one is recorded (stale flag) with session `verdict-<agent>` written.
- [ ] `subagent-stop` (reviewer): non-reviewers pass, after a `subagent-stop` telemetry row; a verdict already recorded by `pre-handback` passes; without `last_assistant_message` the report is read from `agent_transcript_path` (the last `SubagentHandback` message, else the last assistant text); a valid verdict is recorded (stale flag) with session `verdict-<agent>` written; invalid blocks, or passes with telemetry when `stop_hook_active`.
- [ ] `session-start` context text (loaded vs not, changes vs none, gate and review states); `CLAUDE_ENV_FILE` exports `TEMPER_CLI`, `TEMPER_BASE`, plus `TEMPER_BIN` and `PATH` (D17); the 1.x vendored-copy notice line; the config-problem line; the Node-minimum warning line, in the context and as a `systemMessage` (#89).
- [ ] `subagent-start` context for implementer and reviewer (the reviewer runs `cargo fmt --all` first, gets the fingerprint instruction and the diff stat last 25 lines); the 5 newest plans by mtime.
- [ ] `status` lines (root, state, base, fingerprint, changed, gate, review, baseline variants, config); the `status --json` and `stats --json` documents, `--session` and the no-project form (#108).
- [ ] state cross-read: Rust reads 1.x-written `gate/<fp>.json`, `verdicts/<fp>.json` and `sessions/*` (committed fixtures), and 1.x reads Rust-written ones (parity harness).

Phase 4: gate and completion hooks
- [ ] `StdProcess`: captures stdout and stderr; enforces the stderr cap; a timeout kills the child and returns `TimedOut` (test uses the test binary itself as the child via `std::env::current_exe` with a "hang" argument handled in `tests/it/main.rs`, so no `sleep` is needed: it blocks on reading stdin that never closes); spawn failure gives `ProcessError::Spawn`; stdin is null.
- [ ] `StdProcess` bounds `run` by its timeout even when the child spawns a grandchild that holds a pipe: each child gets its own process group (Unix, `CommandExt::process_group(0)`) or job object (Windows), and the timeout kills the whole group. Test: a child whose grandchild holds stdout returns `TimedOut` (cli/it, the test binary as the child; no wall clock). std cannot signal a group or create a job object, and H-01 rules out FFI here, so the plan must choose between spawning `kill -- -PGID` or `taskkill /T /F` and a new dependency (`process-wrap` or the like, H-16). A child that cannot be signalled at all stays a documented limit (the group signal needs the same permission). The Phase 4 plan decides whether anything more is possible without abandoning a blocked reader (H-13). From the 1d.2b review (F1, F4).
- [ ] `default_steps` with and without nextest, `allFeatures`, `doc`; `ratchet_steps` with and without scan (domain).
- [ ] `summarize` and `tail` (domain, from 1.x log samples).
- [ ] gate: cargo unusable gives UNAVAILABLE with the manifests checked TOML-only; a new manifest violation is red (ported "gate without cargo still checks member manifests").
- [ ] gate manifests step: without git every problem counts; with git only new problems fail (ported, cli/it with real cargo and git).
- [ ] gate: fmt check mode vs apply; cache hit on the same fingerprint; `--no-cache`; scan step red on new hard hits with exceptions listed; fail-fast order; `gate.steps` override; logs written to `logs/<fp>-<step>.log`.
- [ ] `render_gate` for green, red, cached, unavailable, exceptions, debt line (snapshot).
- [ ] `stop`: with `telemetry.branch` set, it starts a detached `telemetry push` for the session; no Rust changes pass; plan mode passes; unchanged since session start passes; `Status: WIP/BLOCKED` gives a notice; red blocks, then is allowed after a stuck repeat (`stop_hook_active` plus the same fp); review missing or changes-requested blocks (advisory gives a notice; off skips); green and approved gives the green notice; implementer `subagent-stop` without the review requirement.
- [ ] `parse_status`: the last `Status:` line wins; markdown decorations; a `Declined:` line in the status block (#100, #48) changes nothing (new; no 1.x test).

Phase 5: findings, baseline, ratchet (ported from `findings.test.mjs` then `baseline.test.mjs`)
- [ ] clippy JSON with `--cap-lints warn`: deny-level lints arrive as findings; lib and lib-test repeats collapse (fixture `clippy-capped.jsonl`).
- [ ] rustdoc JSON: prefix kept; clippy-reported missing_docs not double-counted.
- [ ] local `macro_rules!` expansions are attributed to each call site.
- [ ] E-codes and code-less errors are errors; notes dropped.
- [ ] paths are project-relative with forward slashes for CRLF and Windows-style input.
- [ ] crate attribution: the longest matching dir wins; `crate_table` reads inheritance and charter presence.
- [ ] scan source: hard hits are findings, asks flagged, soft and exempt dropped; forbidden lints from `[workspace.lints]` and member tables; crate-level `#![forbid]`; scanner scope members only.
- [ ] `tool_of`, severities, completeness; the findings cache keeps the newest 20; a workspace below the git root is reported plainly.
- [ ] `RuleMap::embedded()` parses; every scanner key resolves; every cited rule is defined (ported rulemap test).
- [ ] `parse_diff`: added and changed lines; deletions and pure renames add nothing; quoted octal paths; `\ No newline`.
- [ ] `changed_lines` against git: edits, insertions, deletions, renames, untracked, merge base (cli/it).
- [ ] it ignores user diff config (`mnemonicPrefix`, `noprefix`, `interHunkContext`) and sees unstaged renames (cli/it).
- [ ] the temporary index writes no untracked content into `.git/objects` (cli/it, `count-objects`).
- [ ] ratchet, one per 1.x test: legacy passes and a new line fails; a swap in the same crate fails; a touched legacy line fails (CRF-08), long spans by first line; a rename with an unchanged count passes; a count increase on an untouched line fails; fixes reported, compile errors fail, exceptions and uncollected sources excluded; a renamed crate keeps counts and `--tighten` is suggested only on a pass; no baseline is strict; forbid-override always fails; sources scope by tool; at most 30 listed; older toolchain note.
- [ ] raised since the merge base is flagged, not failed; a crate with both dir and name changed keeps counts (staged, unstaged, committed) (cli/it).
- [ ] `tighten` never raises or adds and drops zeros; the baseline file has sorted keys, 2-space JSON, a trailing newline and no zeros, and round-trips (byte-compare with a 1.x-rendered fixture).
- [ ] per-host counts (#141): a host with a `targets` section ratchets and tightens against it, the recorded host keeps `totals`; a host with none is compared with `totals` and told to `--init`; a new section is a raise labelled with its host; a baseline without `host` or `targets` behaves as 1.4.
- [ ] property: `tighten(b, c)` totals are ≤ `b` totals per (crate, lint) for arbitrary inputs (proptest).
- [ ] `safe_output_path`: under `target/` or `.temper/` after resolving symlinks, never onto one, no `..` (cli/it; the symlink cases run in CI on all OSes; locally on Windows without the privilege, the test asserts the probe failed with `ERROR_PRIVILEGE_NOT_HELD` (1314) and returns, so it is not a silent pass).
- [ ] gate ratchet mode: legacy passes, new code fails, fixes reported; strict without a baseline (cli/it with cargo).
- [ ] baseline CLI: usage errors, a bad `--base`, a bad `--findings-out` and raises inside Claude Code exit 2 before any cargo run; `--init` below the git root; `--init` refuses unformatted code; `--tighten` refuses on another host unless `--any-host`; status table layout (cli/it, texts verbatim).
- [ ] `tests/e2e/brownfield.sh` passes with `TEMPER_ENGINE=rust`.

Phase 6: report (ported from `report.test.mjs`)
- [ ] the 14 model and render tests (numbers from the ratchet; new = max(changed-line findings, count rise); counting rules; renamed crate; notes and totals-only; no ratchet result; strict; build errors; the realistic sample; raised baseline in MD and HTML; HTML escaping and no requests; HTML order; the MD line budget (56) and escaping; pipes escaped).
- [ ] golden: `render_markdown`, `render_html` and the JSON for the sample equal `docs/examples/sample-report.*` byte for byte (with an injected clock and version).
- [ ] `--out` stays under `target/` or `.temper/`; CLI strict without a baseline; json, md and html agree; an embedded ratchet result is used as-is; otherwise the ratchet runs against the committed baseline; usage errors, refused paths and unusable input exit 2.

Phase 7: setup, migration, stats
- [ ] `setup --dry-run` lists create, skip, manual and append actions exactly as 1.x for a fresh repo (minus the vendoring line, plus `create .claude/temper-version`).
- [ ] setup never overwrites without `--force`; the root `Cargo.toml` without `[workspace.lints]` gives the manual line; `.gitignore` gets `.temper/` once.
- [ ] setup reports, exactly as 1.x: a `manual <path>:<lines>` line for each workflow or contributor doc (`CLAUDE.md`, `AGENTS.md`, `CONTRIBUTING.md`, the PR template) that differs from the template and uses `-D warnings`; without `--force`, a `manual clippy.toml` line naming the `allow-*-in-tests` keys an existing `clippy.toml` lacks; and the closing `cargo machete` reminder (ported from "CLI: setup reports strict lint steps and the test exemptions an existing clippy.toml lacks").
- [ ] setup without a plugin root refuses with exit 2.
- [ ] pin drift is reported and replaced only with `--force` (replaces the retired vendor drift test).
- [ ] `setup --force` on a pinned repository updates only `.claude/temper-version` and touches nothing else (replaces the retired "setup --vendor-only" test).
- [ ] `--migrate`: a 1.x-template `rust.yml`, `pre-commit` and `settings.json` are replaced; customised ones produce manual lines and stay untouched; `.claude/temper/` is removed; `--dry-run` writes nothing.
- [ ] `--migrate` on a customised `rust.yml` (the case for both 1.x pilots, see Risks): the manual line's N equals the number of lines in the file that contain `node .claude/temper/temper.mjs`, and it names the template path of the "Install temper" step.
- [ ] `LEGACY_TEMPLATES` covers every historic version of the three files (an xtask test regenerates from git history and compares; the xtask derives the tag list from git, so a later 1.x release is covered without a code change).
- [ ] the example workspace's `.claude/temper-version` equals the plugin version (replaces "the example workspace carries an up-to-date copy").
- [ ] `telemetry push` (cli/it, ported from `telemetry-push.test.mjs`): off without `telemetry.branch`; creates the branch with only this session's file in a `[skip ci]` commit and leaves the clone untouched; unchanged rows are not pushed again; two clones push side by side; an unreachable `origin` reports why; `stats --branch` with a named branch, the config's branch, and a missing one.

Phase 8: cutover
- [ ] `tests/parity/check_test_map.py`: all 1.x engine tests (143 at 1.3.0) mapped or retired (D16(i)), every mapped Rust test exists. Run once before deleting the Node tests, and its output is recorded in Progress.
- [ ] `verify-npm` matrix green for the 2.0.0 release candidate on all six runners.
- [ ] `claude plugin validate --strict` passes with `package.json` present.
- [ ] `tests/e2e/smoke.sh` (manual, model calls) with the marketplace-style install: SessionStart and the Stop gate work.

## Risks

- **Regex semantics drift** (JS backtracking and Unicode classes vs `regex`). Caught by the scanner corpus over the registry sources on three OSes (Phase 1e); each diff becomes a unit test before the fix. D9-B is the fallback if the predicates prove unmanageable.
- **npm/cli#4828**: a lockfile regenerated with `node_modules` present drops other platforms. Caught by `check-npm-lock` on every PR and in the release job, and by `verify-npm` on all six runners.
- **The Claude Code install silently skipped** (the 60 s timeout, npm missing, restricted network): users land on the fallback. The first-session build cost is visible (status message), and D6 governs the gap. `verify-npm` measures install time.
- **Fallback toolchain too old, or no network for crates.io**: the build fails. `build.failed`, plus SessionStart context with the fix, plus D6 degraded mode. Offline vendoring is a non-goal.
- **Mixed engines mid-transition corrupt state**: prevented by §2.3 and the cross-read cases. Users never run mixed engines (D11-A).
- **Exec bit or `.exe` resolution** on unusual installs: the launcher retries `chmod` once; Windows uses an explicit `.exe`. Tested on the windows-11-arm and windows-2025 runners.
- **Orphaned cargo processes** when Claude Code kills a timed-out hook: the launcher forwards signals, but Rust cannot catch SIGTERM without a new dependency, so behaviour equals 1.x. 2.x follow-up: bound the gate to the hook's budget (the per-event constant checked against `hooks.json`).
- **Windows paths** (verbatim, 8.3, case, separators): ported tests plus `PathStyle`-parameterised domain tests that run on every OS.
- **Binary size and npm download time**: expect a 3 to 6 MB stripped musl binary (no `strip`/LTO changes beyond the template's `lto = "thin"`, `codegen-units = 1`; stripping symbols is an H-16 profile decision, not taken here).
- **Scope creep in the port** (rewriting rules "while we are here"): forbidden by the constraints. Any rule change is a separate 2.x change.
- **Install paths that may skip npm.** Cloud sessions are a primary way this organization works, and they get temper in two ways that §5.3 does not cover as verified: claude.ai's organization sync into `~/.claude/plugins/synced/`, and `claude plugin marketplace add` plus `claude plugin install` in the cloud environment's setup script (`docs/troubleshooting.md`, "Cloud sessions and claude.ai"). Nothing establishes yet whether either runs `npm ci --ignore-scripts`. If one does not, its sessions take the fallback build, which needs Rust ≥ MSRV in the image (the setup script must install it) and costs the first SessionStart 1 to 3 min, or else D6 degraded mode. The Phase 0b manual verify-install step settles both. Decided by the owner (wi-48fb44, 2026-10-07): a passing verify-install on both paths is a Phase 0b exit criterion, and a path that does not run `npm ci` installs the npm package from the environment's setup script. Prebuilt binaries in the plugin and the fallback build with Rust in the image were not chosen.
- **Customised CI files in the 1.x adopters.** The audit (`docs/history/2026-10-audit/findings.json`, port-2.0-1-13) found that both pilots (weigh-in, redkiln-rs) have a customised `rust.yml`, and weigh-in a customised `.claude/settings.json`, so their SHA-256 is not in `LEGACY_TEMPLATES`. `--migrate` will print manual steps for their `rust.yml` (and settings.json falls to the line-exact allow-entry replacement). The manual lines must be exact and complete, and the CHANGELOG 2.0.0 migration guide (§7) walks through a customised `rust.yml`. Decided (wi-5fbb56): `--migrate` stays hash-guarded as D15 says, with no line-level `rust.yml` rewrite; the manual lines carry the migration.
- **Missing upstream facts**: the npm org's existence and the availability of `windows-11-arm` and `ubuntu-24.04-arm` runners to this repository (both confirmed 2026-10-02, `docs/plans/v1-readiness.md` steps 0.8 and 0.9; the arm64 runners by `runner-probe.yml` [run 2](https://github.com/Wet-Ink-Corporation/temper/actions/runs/37043545399), on a branch; #23's arm64 box is ticked), `(?-u:\b)` acceptance in `regex` 1.13, and the `sha2` 0.11 default features. Each is verified in the phase that first uses it (PROC-01), and a failure is reported as BLOCKED.

- **The lens plugin carries its own CLI.** Since the lens moved into the `temper-lens` plugin (a mod turns a whole plugin off in the desktop app's Chat and Cowork), it runs `scripts/temper.mjs status|stats --json` from a vendored copy inside `plugins/temper-lens/`, which `tests/ci.sh` keeps equal to `scripts/`. When `status` and `stats` graduate, and at the 2.0.0 cutover (D11), that copy must follow: the lens plugin ships the launcher too, or runs the `temper` binary on PATH (D17). Either way the lens plugin's own `package.json` and engine install are in scope for Phase 7 or 8, and the `temper_status: 1` and `temper_stats: 1` contracts stay as they are.

- **Owner bandwidth and owner-only setup** (audit `port-2.0-1-17`, #64). Every phase ends with an approved review and the owner merges each PR, so one owner's review time paces the port. Some steps only the owner can take: npm and crates.io trusted publishing, revoking the one-off crates.io token (#23), and the Actions budget alerts (#7). Phase 0b cannot publish `2.0.0-alpha.1` until #23 is done. The Actions budget (v1-readiness 0.9 estimates 120 to 180 included minutes per full-matrix PR and 300 to 500 per release) is spent fastest by the Windows jobs, which count 2×. The plan gives no dates; the pilot report (2026-10-18) and v1 live come first.

## Size estimate (lower bound)

- **Rust production code** (`temper-domain` ~7,500 lines in ~32 files; `temper-app` ~4,500 in ~26; `temper-host` ~700 in 6; `temper-cli` ~900 in 6; `xtask` ~900 in 7): ~14,500 lines, ~77 files.
- **Rust tests:** domain and app unit tests ~5,000, `cli/tests/it` ~3,500 in ~14 files, fixtures.
- **Launcher** (~120 lines), launcher tests (~250), parity harness (~700, temporary), `release.yml` (~350), `ci.yml` (+~150), manifests and charters (~15 files, ~700).
- **Docs, templates, skills and Python check edits:** ~25 files, ~800 changed lines.
- **Total:** at least ~110 files and ~26,000 lines added. ~14,000 lines deleted at cutover (`scripts/lib`, the old CLI, the Node engine tests (352 on 2026-10-14) and the vendored copy in the example workspace).

## Decisions (H-16) record

| Decision | Options | Recommendation | Decided by | Outcome |
|---|---|---|---|---|
| D1 npm scope | @wet-ink / @wetink / unscoped | @wet-ink, six `temper-<platform>-<arch>` packages | owner (wi-836680) | A: @wet-ink |
| D2 crates.io | publish 4 / none / 1 | publish 4 (`temper-cli`, `temper-domain`, `temper-app`, `temper-host`) | owner (wi-222f58) | A: publish 4 |
| D3 crate split | 4 / 2 / 1 | 4 + xtask | owner (wi-0b4dc2) | A: 4 + xtask |
| D4 MSRV | 1.96 / 1.88 / 1.85 | 1.96 | owner (wi-5fc813) | A: 1.96 |
| D5 targets | 6 / 5 | 6 (incl. win32-arm64) | owner (wi-4a9bfe) | A: 6 |
| D6 pending fallback | fail closed / degrade / hybrid | hybrid (C) | owner (wi-4abeef) | C: hybrid |
| D7 release tooling | cargo-dist / hand-written | hand-written + xtask | default, two-way (wi-f22cef) | hand-written + xtask |
| D8 scanner basis | port lexer / rustc-lexer / syn | port lexer | default, two-way (wi-2f31ee) | port lexer |
| D9 regex engine | regex + predicates / fancy-regex | regex + predicates | default, two-way (wi-e9386f) | regex + predicates |
| D10 dependencies | regex, toml(+preserve_order), sha2, wait-timeout | approve all four; no clap etc. | owner (wi-b03273) | A: approve four |
| D11 Node deprecation | dev-only strangler / keep in 2.0.x / keep in 2.x | dev-only; 2.0 Rust-only | owner (wi-bf7535) | A: 2.0 Rust-only |
| D12 publishing auth | OIDC + approval env / tokens | OIDC + approval env | owner (wi-6aac03) | A: OIDC + approval env |
| D13 user CI install | install-action / cargo install / curl | install-action (binstall) | owner (wi-37c1df) | A: install-action |
| D14 pin location | `.claude/temper-version` / temper.json / workflow | `.claude/temper-version` | owner (wi-3b5428) | A: `.claude/temper-version` |
| D15 1.x migration | hash-guarded rewrite / manual / force | hash-guarded `--migrate` | owner (wi-cdcecc) | A: hash-guarded `--migrate` |
| D16 divergences + retired tests | approve list §2.4 | approve | owner (wi-348e0b) | A: approve; (i) amended by wi-5fbb56 and on 2026-10-19 (#64); (k) added by wi-825595 on 2026-11-01; (l) by wi-ecb9dd and (m) by wi-7d8074 on 2026-11-01 |
| D17 PATH export | yes / no | yes | owner (wi-1d4cd6) | A: export |
| Crate licence | MIT / MIT OR Apache-2.0 | MIT | owner (wi-4679b3) | B: `MIT OR Apache-2.0` (override); add `LICENSE-APACHE`; plugin stays MIT |
| 1e-3 corpus job triggers | weekly, `release/**`, dispatch / plus path-filtered PRs / every PR | plus path-filtered PRs | owner (wi-295cf1) | B: weekly, `release/**`, dispatch, plus a path-filtered `pull_request` (amends v1-readiness 0.9) |
| 1e-7/1e-8 corpus CI wiring | `corpus.yml` + `ci.sh` steps, advisory / required / `ci.yml` too | `corpus.yml` + `ci.sh`, advisory | owner (wi-68036c) | A: add `corpus.yml` and the `ci.sh` parity steps; the corpus job is not a required check; `ci.yml` unchanged |

## Progress

- 2026-10-01: Plan written (architect). Explored `scripts/` (all 13 modules), `hooks/hooks.json`, `tests/` (140 node tests, `ci.sh`, e2e, Python checks), the templates, `ci.yml`, README and CHANGELOG, the Claude Code docs (plugin dependency install, `CLAUDE_PLUGIN_DATA`, hook timeouts, async and exec form), local registry sources for `regex` 1.13.1, `fancy-regex` 0.19.2 (variable look-behind is behind a feature), `toml` 1.1.6 (`Table: FromStr` needs only `parse`), `sha2` 0.11.0 and `wait-timeout` 0.2.1, and the crates.io and npm name availability.
- 2026-10-02: Owner decided D1, D2, D5, D6, D11, D12, D16 as recommended and the crate licence as `MIT OR Apache-2.0` (override of the MIT recommendation; §1.1, §1.2 and the npm-pack step updated). D7, D8, D9 recorded as two-way defaults. Records in `.kb/_intake/decisions/`.
- 2026-10-02: Owner decided D3 (four crates), D4 (MSRV 1.96), D10 (regex, toml, sha2, wait-timeout) and D17 (PATH export) as recommended.
- 2026-10-02: Owner decided D13 (install-action), D14 (`.claude/temper-version`) and D15 (hash-guarded `--migrate`) as recommended. All D1..D17 and the licence are decided.
- 2026-10-02 (owner, via `docs/plans/v1-readiness.md` steps 0.8 and 0.9): the `@wet-ink` npm org exists. The four crate names are reserved as `0.0.0` placeholders. Arm64 targets use GitHub's standard arm64 runners, confirmed by `runner-probe.yml` on 2026-10-02 (all five labels run, v1-readiness 0.9; [run 2](https://github.com/Wet-Ink-Corporation/temper/actions/runs/37043545399), on a branch, not rerun on `main`). The port's CI is tiered: Linux on every PR; Windows and macOS on code-touching PRs and `main`; the corpus scan weekly and on `release/*`; the release matrix on `release/*` only.
- 2026-10-07: Facts refreshed after the 2026-10 audit: engine size and test counts rebased to 1.3.0 (5,649 lines in `scripts/temper.mjs` and 12 `scripts/lib/` modules; 143 engine tests, `baseline.test.mjs` 31), the 1.3.0 setup and `rel_to` tests added to the test list, `LEGACY_TEMPLATES` tags derived from git, `tests/release-check.sh` in §7, and risks for the cloud install paths and the pilots' customised CI files. Open for the owner at the time: the `setup --vendor-only` test and `tests/release-check.sh` against D16(i), and a line-level `rust.yml` rewrite against D15. The last two were settled on 2026-10-08 (wi-5fbb56, below); the `setup --vendor-only` test was settled on 2026-10-19 (below).
- 2026-10-07: Owner decided wi-48fb44: a passing verify-install on the claude.ai organization-sync and cloud setup-script install paths is a Phase 0b exit criterion, and a path that skips `npm ci` installs the npm package from the environment's setup script (Phase 0b, the verify-install test item, Risks).
- 2026-10-08: Owner decided wi-5fbb56: `tests/release-check.sh` folds into `cargo xtask versions --check` with its three jobs (amends D16(i)); `--migrate` stays hash-guarded with exact manual lines (D15); macOS parity runs on demand through `runner-probe.yml` until the repository is public. Applied in #93.
- 2026-10-08: Owner decided wi-6e0cc2: the port reproduces `scripts/lib/shell.mjs`'s decisions (#82 to #84) exactly, rather than re-implementing the shell-write guards as a divergence. The parity line is in §2.2 (hook cases), with `shell.test.mjs` in the test table.
- 2026-10-08: wi-103aa0 (owner): the never-firing `PreToolUse` `SubagentHandback` hook was removed in 1.x (#92), and `pre-handback` left this plan (#93).
- 2026-10-09: Owner decided wi-641f99: with `telemetry.branch` set (opt-in), Stop pushes each session's telemetry to that branch with `[skip ci]`, because cloud containers discard `.git/temper/` and the git proxy refuses hidden refs. Built in #96: `temper telemetry push`, `stats --branch`, and new telemetry fields. Added here: the module map, the network push in §1.5, the contract files, the CLI cases, the tests and Phase 7.
- 2026-10-09: #98 (fixing #97) reversed wi-103aa0 on the owner's decision. In cloud sessions a subagent's report goes only through `SubagentHandback`, and `SubagentStop` gets no `last_assistant_message`, so 1.x restored the hook and `SubagentStop` reads the report from `agent_transcript_path`. Restored here: `PreHandback` in `HookEvent`, the Phase 3 handler, the cross-read case and the test item; the transcript fallback is added.
- 2026-10-11: Facts refreshed for #82 to #89 (merged 2026-10-07), #96 and #98. The test table is recounted (339 engine tests, seven new files); §1.5 has #88's retention rules; §2.3 and the fingerprint test have #87's inputs; `validate_verdict` has #86's evidence and score rules; the launcher and SessionStart tests have #89's Node warning; §3 says how 1.x releases ship during the port (#22); a risk covers owner bandwidth and owner-only setup (`port-2.0-1-17`). #23's arm64 box is ticked.
- 2026-10-14: Facts refreshed for what merged since 2026-10-11: #100 (`Declined:` in the status block, ignored by `parse_status`), #104 (deletes of protected config ask, wi-2000a3), #106 (the template's vendored-copy step and `--keep-going`), #108/#116 (`status --json`, `stats --json`) and #125 (a verdict recorded at hand-back and at `SubagentStop` counts once). The test table is recounted (352 engine tests; `status-json.test.mjs` added), and §6 drops the vendored-copy step from the 2.0 `rust.yml`. The `setup --vendor-only` test is still pending the owner (§2.2).
- 2026-10-19: The owner retired the `setup --vendor-only` test into D16(i), amending wi-348e0b (#64). Its replacement checks that `--force` on a pinned repository updates only `.claude/temper-version`.
- 2026-10-19: The pilot findings were folded into 1.x (#22). Each changes behaviour that the parity harness must reproduce, so the port starts from the `main` that holds them, not from 1.4.0:
  - `Stop` gates the worktrees subagents change, under a time budget (#135, #144).
  - Telemetry drops quoted text from logged commands, records verdict problem kinds, and says why a gate was cut short (#137, #138, #150).
  - `temper stats` counts review rounds per change (#139, #146).
  - Patch denials name the scanned routes (#140, #145).
  - The baseline is recorded per host (#141, #147).
  - The lens and gate-progress follow-ups from the #121 and #123 reviews landed (#143, #149).
  - Five MUST rules that only review enforced gained checks, the rest are marked review-only by design, and the gate lists the MUST advisories a change adds (#58; #154, #156).

  Three findings were still open on 2026-10-19: verdict stability (#136, #148, waiting on the owner's pick), the patch-check gaps (#151, #153), and slow Stop gates (#137, waiting on telemetry from real sessions). The follow-ups #155, #157 and #158 are open too. The test table's counts are re-taken when Phase 0a starts.
- 2026-10-30: The repository is public. The owner decided wi-e85ef8 (macOS in CI on every code-touching PR, graduation on Linux, Windows and macOS; supersedes wi-5fbb56's macOS clause) and wi-428ed1 (npm placeholder packages before trusted publishing, Phase 0b). The next 1.x release is 1.5.0, after #155's core (wi-779016) and #157 (wi-0cfadc).
- 2026-10-31: 1.5.0 released (v1.5.0). Phase 0a started: the root workspace with the four crates and `xtask` and their charters, the template's lints and config, `LICENSE-APACHE` and the crates' licence copies (`cargo xtask check-licenses`), `temper --version` and the 1.x usage text, the `Rust (<os>)` jobs on Linux, Windows and macOS and the `Rust MSRV` job, and the workspace steps in `tests/ci.sh`. Two-way calls: wi-2cf4e1 (`2.0.0-alpha.0`, `publish = false`, no dependencies in 0a) and wi-e72d19 (the Rust jobs run on every PR and are not required until the owner adds them to the ruleset).
- 2026-10-31 (overnight session Ryan delegated): Phase 0a merged (#174, #175). Phase 0b's code merged:
  - xtask `versions`, `check-npm-lock` and `npm-pack` (#176);
  - the launcher `scripts/launch.mjs` and `.github/workflows/release-2.yml`, kept beside `release.yml` until the cutover (wi-96fbfe) (#177).

  A packing dry run found that npm drops `LICENSE-*` files unless `files` names them, so `files` now lists both licences. Phase 0c adds `scripts/fallback.mjs`: the fallback build and the D6-C behaviours, with the binary absent, tested with a fake `cargo`. Two-way calls: wi-654352, wi-4769ed, wi-f83377, wi-a6e98e and wi-6bf67c. Phase 0b does not exit until two owner steps are done: the first `2.0.0-alpha.1` publish, which waits on the owner's six placeholder packages and trusted publishers (wi-428ed1), and the manual verify-install on the organization-sync and cloud setup-script paths (wi-48fb44).
- 2026-11-01: Phase 1a built on `wip-1-scanner`: in `temper-domain`, the `js` helpers (`js_trim`, UTF-16 truncation, JS-to-`regex` translation), the masking lexer, test regions, the test, ignore and clone counts, `Hit` and the `Scanner` with its first six rules. The 1.x tests "masks strings…", "test regions allow…", "#[expect] with a reason…", "print: build scripts…" and "lexer: escaped quote char literal; ignore counting" are ported, with the masking and scan properties. A local differential run over 4,041 files (this repository's Rust and the cargo registry sources) gave zero diffs from 1.x for the six rules and the counts. Two-way calls: wi-a5a813, wi-466849, wi-5eed96, wi-c0f25b, wi-962170, wi-41c92b, wi-77bafb and wi-256d4d; `regex` under D10 (wi-b03273).
- 2026-11-01: Phase 1b built on `wip-1b-scanner` (plan `.temper/plans/port-phase-1b.md`): 40 more regex rules in 1.x table order (46 of 60), their look-around and keep predicates, the JS `m` and `i` flags, UTF-16 back-windows, comment records, const and trait-impl regions, and a fallible `js::translate`. `allow-attr`, `discard-ok`, `empty-err-arm` and `ScanOptions::forbid` moved to 1c (wi-13609d). §1.6's predicate table was corrected where it was not faithful to JS backtracking (wi-31f653). Calls 1b-1 to 1b-10 are two-way defaults (wi-13609d, wi-31f653, wi-afd1cd, wi-1636ef, wi-ef9d9d, wi-d31f39, wi-cbeff6, wi-5cb4c8, wi-28a72e, wi-de1207). A local differential over 9,003 files (this repository's Rust and the cargo registry sources), scanned as prod and test and again as bin and example build scripts, matched 1.x `scanSource` filtered to the 46 keys byte for byte (301,336 hits).
- 2026-11-01: Phase 1c built on `wip-1c-scanner` (plan `.temper/plans/port-phase-1c.md`): `allow-attr`, `discard-ok` and `empty-err-arm` (with `otherArmBindsErr`), and the finders `empty-if-let-err`, `spin-lock`, `serde-hashmap`, `test-branch`, `struct-bounds`, `forbid-override`, `erased-error`, `todo-comment` and `commented-code` (58 of 60 rules); `ScanOptions` gains `forbid` and a lifetime as §1.3 writes it (wi-2d7b05); comment records gain `CommentKind` (wi-91ace5); the 1b review's follow-ups F1 to F4, including the test split by 1.x group (wi-bd9fd8). The doc group moves to 1d (wi-9f78ea) and `Scanner::rules()` to Phase 4 (wi-ca74b3); `struct-bounds`' run-time regexes are hand-written matchers (wi-5842d0), and `other_arm_binds_err` lives with the `empty-if-let-err` finder (wi-5f034a). The owner retired the "large file" test's wall-clock bound into D16(k) (wi-825595). A local differential over 9,021 files (this repository's Rust and the cargo registry sources), scanned as prod and test, as bin and example build scripts, and as prod with a forbid set, matched 1.x `scanSource` filtered to the 58 keys byte for byte (449,925 hits).
- 2026-11-01: Phase 1d.1 built on `wip-1d-scanner` (plan `.temper/plans/port-phase-1d.md`): `doc-unwrap` and `doc-arg-sections` with doc blocks and the lazy `Env::doc` (60 of 60 rules), `new_hits`, and `classify_path` with `LibTarget` (owner, wi-208563); `is_build_script` moves to `scan/classify.rs` with its public path unchanged. Every `scan.test.mjs` test now has a Rust home, including the rule-table test and the every-key witness table (wi-a22589). The 1c review's follow-ups: each doc rule seen red on an empty finder, the depth walkers in `lexer/brackets.rs` and the lock and TODO key cases in their own module (wi-7d932c), `body_brace`'s case table; `ScanOptions` stays without `Copy` (wi-a4b3a7). Two-way calls: wi-719373, wi-a4b3a7, wi-46a7f4, wi-afb3e9, wi-e9f84d, wi-358920, wi-a22589, wi-7d932c. A local differential over 9,028 files, with no key filter, matched 1.x byte for byte: `scanSource` as prod and test (182,063 hits, 4,602 hard and 4,606 soft `doc-unwrap`, 274 `doc-arg-sections`), as bin and example build scripts (190,838), and as prod with a forbid set (100,866); `newHits` against each file with its first half removed (39,052 fresh hits); `classifyPath` on every path relative to its package, with both library values.
- 2026-11-01: Phase 1d.2a built on `wip-1d2-cli` (plan `.temper/plans/port-phase-1d2.md`): `temper scan --all` and `temper scan FILES` in the Rust binary, with no processes (OC-5 B). Domain: a hand-written `Serialize` for `Hit` in 1.x's key order and `HitAt` (1d-12), `format_hit`, `render_scan_text` and `NOT_A_RUST_PROJECT` (owner, OC-2 A, wi-274f9a), `PathStyle`, `unverbatim` and a public `js_trim`; the 1d.1 review's F1 and F2. App: the `Files` port (OC-9 B), Node's `path.posix` and `path.win32` as a `pub(crate)` lexical port (OC-1 B), the Cargo-walk `discover`, `crate_of`, `rel_to` with its real-path fallback, `walk`, and `Engine::new(ports, env, scanner)` with `Engine::scan` (1d-15). Host: `StdFiles` and `HostPorts`. CLI: the `scan` arguments, bare `scan` as the usage text with exit 2 (1d-14 A), and a hand-written cause-chain printer (OC-6 B). New divergences D16(l) (owner, wi-ecb9dd) and D16(m) (owner, OC-4 A, wi-7d8074). `serde` added (`std`, no `derive`). The path port matched Node 22.22.0 on about 400,000 random cases off the tree. A local differential on a git-free copy of this repository, its example workspace and 50 registry crates (8,771 hits in 795 files) matched 1.x byte for byte in `--all --json`; in the text form, the only differences (10 crates) are D16(l)'s byte order. `scan FILES` from inside 5 crates' `src/` matched too.
- 2026-11-01: Phase 1d.2b built on `wip-1d2b` (plan `.temper/plans/port-phase-1d2.md`): `discover` takes git's top level first, through the app's `ProcessRunner` port (`ProcessRequest` with `stdout_cap` and no `env`, OC-8 B, wi-b175fb) and the host's `StdProcess` on `wait-timeout` (1d-13 A, wi-f1bd32): stdin null, two named scoped readers capped at the request's `ByteCap`, a 120 s git timeout (D16(f)), kill and reap past it. A stream past its cap is `ProcessError::Overflow` (owner, OC-3 A, wi-129daa). `wait-timeout` 0.2.1 added; `cargo deny` accepts its `MIT/Apache-2.0` licence. The 1d.2a review's F1 to F4, F6 and F7 are fixed; F2 makes win32 `dirname` read `\\?\` and `\\.\` roots as Node 22 does (16,527 random Node cases, no diff). cli/it gains the git cases and the Windows 8.3 short-name test (C4).
- 2026-11-01: Phase 1e (plan `.temper/plans/port-phase-1e.md`). 1e.0 (#192): the 1d.2b review's follow-ups F1 to F5 in `temper-host` and the Phase 4 process-group item. 1e.1 on `wip-1e1`: the differential harness in `tests/parity/`, with the 13 `scan` parity cases, the scanner corpus driver with the D16(k) per-file bound, and the `scan.test.mjs` test map (all 77 tests; the 1d plan's count of 69 predates the last KEY_CASES keys). `doc_examples.py` gains `--extract`. `.github/workflows/corpus.yml` (Linux, Windows, macOS; advisory) and the `tests/ci.sh` steps, as the owner approved (wi-295cf1, wi-68036c). Two-way calls: wi-37a444, wi-fc6a1c, wi-a24829, wi-9817a6, wi-a9dd30. A local Linux run of the corpus over 73 roots (the 70 registry crates of both lockfiles, the example workspace, the skill's code blocks and the large file; 4,674 files, 58,447 hits) gave 0 mismatches and no D16(e) normalisation; 1.x took 18.8 s and Rust 3.8 s, and the slowest Rust root was `linux-raw-sys-0.12.1` at 451 ms, about 22 times inside the bound.
- Next: Phase 0a (adds `LICENSE-APACHE`), after 1.5.0 (v1 went live on 2026-10-19) and the pre-port steps in v1-readiness Phase 5 (#22 to #24): fold in the pilot findings; set up the npm and crates.io trusted publishers (D12; the `release` environment exists, #23); add the port's CI jobs as required checks. The npm org, the crate names and the arm64 runners are done (v1-readiness 0.8, 0.9; #23).
