# Shipping native binaries outside npm: how other ecosystems do it (as of October 2026)

Scope: approaches outside npm that matter for an editor/agent plugin that must run a Rust CLI from short-lived hooks on Linux (x64/arm64, musl), macOS (x64/arm64) and Windows x64. Sources were fetched 2026-10-01 unless a date is given. Some primary pages could only be read in summarized form; exact quotes are marked as quotes.

## VS Code platform-specific extensions, user-provided binary paths, and download-on-activation

### Takeaway
VS Code's Marketplace supports one VSIX per target (`vsce package/publish --target`). Each user gets the VSIX that matches their platform, and a VSIX published without `--target` is the fallback for every other platform. The mature Rust-tool extensions (rust-analyzer, ruff) bundle the binary this way, and they all also let the user point at their own binary. Downloading the binary on activation (clangd, and rust-analyzer before Dec 2021) has caused trouble with offline and remote hosts, Apple Silicon, and repeated prompts.

### Cited Findings
- Supported targets: `win32-x64`, `win32-arm64`, `linux-x64`, `linux-arm64`, `linux-armhf`, `alpine-x64`, `alpine-arm64`, `darwin-x64`, `darwin-arm64`, `web`. Alpine targets are the musl builds. — [VS Code: Publishing extensions, platform-specific](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#platformspecific-extensions)
- "Starting from version `1.99.0`, vsce supports a `--target` parameter". You can run `vsce package --target win32-x64` and then `vsce publish --packagePath ...`, or `vsce publish --target win32-x64 win32-arm64`. — [same](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#platformspecific-extensions)
- Fallback: a package published without the flag "will be used as a fallback for all platforms that have no platform-specific package." The docs recommend CI to manage the per-platform packages and link a sample repo. — [same](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#platformspecific-extensions)
- rust-analyzer moved (Dec 2021) "to bundle the server binary with the extension, instead of downloading it on the first load". At that time Open VSX (used by VSCodium) did not support platform-specific VSIXes and sometimes served the wrong platform's binary, so users had to pick the VSIX by hand from GitHub releases. — [rust-analyzer changelog #109 (2021-12-27)](https://rust-analyzer.github.io/thisweek/2021/12/27/changelog-109.html); [rust-analyzer issue #11080](https://github.com/rust-lang/rust-analyzer/issues/11080). The Open VSX gap is dated 2021. I did not verify Open VSX's current support (see Gaps).
- rust-analyzer today stores the bundled server in the extension install directory. On unsupported platforms, users install `rust-analyzer-no-server.vsix` and set `"rust-analyzer.server.path"`. Offline install works with `code --install-extension /path/to/rust-analyzer.vsix`. NixOS is a special case (globalStorage). — [rust-analyzer book: VS Code](https://rust-analyzer.github.io/book/vs_code.html)
- There is an open request that rust-analyzer download a server matching the toolchain file. Today it warns instead. This shows a mismatch between the bundled binary and the project-pinned version. — [rust-analyzer issue #21261](https://github.com/rust-lang/rust-analyzer/issues/21261)
- ruff-vscode resolution: `ruff.path` is "A list of path to `ruff` executables. The first executable in the list which is exists is used. This setting takes precedence over `ruff.importStrategy`". `importStrategy`: "`fromEnvironment` finds Ruff in the environment, falling back to the bundled version. `useBundled` uses the version bundled with the extension." The default is `fromEnvironment`. — [Ruff editor settings](https://docs.astral.sh/ruff/editors/settings/)
- Version handshake in ruff-vscode: the native server needs Ruff >= 0.5.3. If an older ruff is found, the extension falls back to `ruff-lsp` and shows a warning. — [Ruff editor settings](https://docs.astral.sh/ruff/editors/settings/)
- Distros sometimes replace the bundled binary. For example, nixpkgs patches the ruff extension to use nixpkgs' ruff. — [nixpkgs PR #384869](https://github.com/NixOS/nixpkgs/pull/384869)
- Biome's VS Code extension bundles no binary. It finds Biome in the project's `node_modules` (`@biomejs/biome` is "a tiny wrapper" that delegates to `@biomejs/cli-<platform>` packages) or on PATH. `biome.lsp.bin` overrides this with a string path or a per-platform map such as `"darwin-arm64": "/path/to/biome"`. — [Biome VS Code reference](https://biomejs.dev/reference/vscode/)
- Known problems with clangd's download-on-activation:
  - auto-install fails on offline remote hosts (`FetchError` reaching api.github.com) and writes `clangd.path` to the local rather than the remote config, which causes reinstall loops — [vscode-clangd #112](https://github.com/clangd/vscode-clangd/issues/112), [#25](https://github.com/clangd/vscode-clangd/issues/25), [#55](https://github.com/clangd/vscode-clangd/issues/55)
  - auto-install did not work on Apple M1 — [#194](https://github.com/clangd/vscode-clangd/issues/194)
  - users get a repeated "language server is not installed" prompt — [#267](https://github.com/clangd/vscode-clangd/issues/267)

### Inferences
- The pattern that holds up best is: bundled per-platform binary, plus a user override path, plus an optional "prefer environment" mode, plus a minimum-version check with a clear warning. ruff-vscode is the cleanest example.
- Downloading on first use adds failure modes on offline machines, remote/SSH/WSL hosts, behind proxies, and on new architectures. A plugin run from short-lived hooks has no UI to show "installing..." progress, which makes these failures worse.
- Every channel needs a "universal" or "no-binary" fallback artifact. Without one, unsupported platforms get no package or a wrong one.

### Gaps
- I did not confirm the current (2026) Open VSX support for platform-specific VSIXes. The cited issue is from 2021.
- I did not fetch the vscode-ruff or Biome extension source to confirm whether either downloads binaries. The Biome doc summary suggests it does not. A third-party DeepWiki summary claims a download fallback, which I left out as unverified.

## Zed extensions (WASM API that downloads language-server binaries)

### Takeaway
Zed extensions are Rust compiled to WASM. They do not ship binaries. In `language_server_command` they resolve or download one, using host functions to query GitHub releases, download and extract, `chmod +x`, and report install status. All of this is limited by user-configurable capabilities (`process:exec`, `download_file`, `npm:install`).

### Cited Findings
- `zed_extension_api` v0.7.0 (released 2026-07-24) exposes:
  - `latest_github_release`
  - `github_release_by_tag_name`
  - `download_file` ("Downloads a file from the given URL and saves it to the given path within the extension's working directory")
  - `make_file_executable`
  - `current_platform` ("Gets the current operating system and architecture")
  - `set_language_server_installation_status`
  - the types `Os`, `Architecture`, `DownloadedFileType`, `GithubReleaseOptions`, `GithubReleaseAsset`

  — [docs.rs zed_extension_api](https://docs.rs/zed_extension_api/latest/zed_extension_api/)
- Language servers are registered in `extension.toml` (`[language_servers.x]`), and the extension implements `language_server_command` returning `zed::Command { command, args, env }`. — [Zed docs: Language extensions](https://zed.dev/docs/extensions/languages)
- The built-in HTML extension shows the usual caching pattern:
  - it stores `cached_binary_path`
  - it checks whether the server exists and whether the installed version is current (`npm_package_installed_version` against `npm_package_latest_version`)
  - it reports `CheckingForUpdate` and `Downloading` statuses

  It uses npm, not GitHub releases. — [zed/extensions/html/src/html.rs](https://raw.githubusercontent.com/zed-industries/zed/main/extensions/html/src/html.rs)
- Capabilities: `process:exec` (allows invoking commands, which can be limited by command and args), `download_file` (can be limited by host and path), and `npm:install`. Users restrict them with `granted_extension_capabilities`. "Restricting or removing a capability will cause an error to be returned when an extension attempts to call the corresponding extension API". Removing all capabilities "will likely make many extensions non-functional." — [Zed docs: capabilities](https://zed.dev/docs/extensions/capabilities)

### Inferences
- The usual Zed GitHub-release pattern is:
  1. Prefer a binary on the worktree PATH.
  2. Otherwise call `latest_github_release`, build the asset name from `current_platform()`, and download into a version-named directory.
  3. `make_file_executable`, cache the path in memory, and delete older version directories.

  The WASM code cannot spawn arbitrary processes on its own. It returns a command for Zed to run, or calls `process:exec` only if that capability is granted.
- Version checks against GitHub on every server start can hit API rate limits or fail offline. The cached-path-plus-existence check is how extensions avoid re-checking within a session.

### Gaps
- The `zed-extensions/ruff` source URL returned 404. I did not get a verified GitHub-release-based example showing `worktree.which`, version-directory cleanup, or asset naming. These come from general knowledge and should be checked against an extension such as zed-extensions' Rust-based ones before being quoted.
- I did not find documentation on how Zed behaves when a GitHub API rate limit is hit.

## GitHub CLI precompiled extensions

### Takeaway
`gh extension install` finds a release asset whose name ends in `OS-ARCH[.exe]`. On Apple Silicon it falls back to a darwin-amd64 asset under Rosetta. If nothing matches, it fails with a message that tells the user to open an issue. If the release has no binaries, gh treats the repo as a script extension and clones it.

### Cited Findings
- "Binary executables attached to releases must follow a naming convention and have a suffix of OS-ARCHITECTURE[EXTENSION]". Windows assets must end in `.exe`. Examples: `gh-whoami-windows-amd64.exe`, `gh-whoami-linux-386`. The `cli/gh-extension-precompile` Action cross-compiles Go and scaffolds non-Go builds. Checksums and attestations are not mentioned. — [GitHub Docs: Creating GitHub CLI extensions](https://docs.github.com/en/github-cli/github-cli/creating-github-cli-extensions)
- gh "first looks for the release artifacts assuming that it's a binary extension". If none are found, "the repository itself is cloned assuming that it's a script extension". `--pin` selects a tag (binary) or commit (script). — [gh extension install manual](https://cli.github.com/manual/gh_extension_install)
- `gh extension create --precompiled=go|other` scaffolds precompiled extensions. — [gh extension create manual](https://cli.github.com/manual/gh_extension_create)
- In the source, asset matching is `strings.HasSuffix(a.Name, platform+ext)`. On darwin arm64 with no arm64 asset it looks for a `darwin-amd64` asset and requires Rosetta (otherwise it tells the user to run `softwareupdate --install-rosetta`). With no match at all it prints an "unsupported for <platform>" error and suggests a `gh issue create` command. — [cli/cli pkg/cmd/extension/manager.go](https://raw.githubusercontent.com/cli/cli/trunk/pkg/cmd/extension/manager.go)

### Inferences
- A fixed, suffix-based naming convention is enough for a client to work without a manifest. The cost is that no libc variant is encoded (Go binaries are typically static) and nothing is verified (no checksum or signature). A Rust tool that copies this approach should name musl assets explicitly and publish checksums.

### Gaps
- I found no gh documentation on verifying the integrity of downloaded extension binaries.

## Python wheels for Rust binaries (maturin `bindings = "bin"`) and `uvx`

### Takeaway
ruff and uv ship as `py3-none-<platform>` wheels that contain only the Rust executable. pip and uv already know how to pick the right wheel for the machine, including the glibc vs musl distinction, and `uvx` gives cached, version-pinned one-shot runs. This works well because the packaging ecosystem's platform-tag resolution does the hard part.

### Cited Findings
- maturin "supports distributing binary applications written in Rust as Python packages using the `bin` bindings. Binaries are packaged into the wheel as 'scripts' and are available on the user's PATH". It is auto-detected when there is only a bin target and no pyo3 or cdylib. — [maturin: Bindings](https://www.maturin.rs/bindings.html)
- ruff 0.16.10 (released 2026-10-01) publishes wheels for:
  - `musllinux_1_2` x86_64, aarch64, i686, armv7l
  - `manylinux_2_17` x86_64, aarch64, armv7l, i686
  - `macosx_11_0_arm64`, `macosx_10_12_x86_64`
  - `win_amd64`, `win_arm64`, `win32`
  - `linux_armv6l`

  All are tagged `py3-none-*` (no Python ABI dependency). — [PyPI: ruff files](https://pypi.org/project/ruff/#files)
- `uvx` runs tools in "temporary, isolated environments". Versions are pinned with `uvx ruff@0.6.0` or `--from 'pkg==ver'`, and `@latest` gets the newest. `uv tool install` puts executables in a bin directory on PATH. — [uv docs: Using tools](https://docs.astral.sh/uv/guides/tools/)

### Inferences
- The things worth copying are:
  - platform-tagged artifacts chosen by the installer, not by custom code
  - separate musl artifacts that the resolver chooses automatically
  - an exact-version pin per invocation
  - a shared on-disk cache, so repeated short-lived invocations stay fast after the first one
- The cost is that it needs a Python/uv toolchain on the user's machine, and the first run needs network access to PyPI or a mirror.

### Gaps
- The uv tools guide did not cover `uvx` cache semantics or offline behavior (`--offline`). I did not verify these.

## cargo-binstall, cargo install --locked, cargo-dist installers, attestations, checksums

### Takeaway
cargo-binstall works out the release asset URL from `[package.metadata.binstall]` templates or default naming conventions. It then falls back to QuickInstall and finally to `cargo install` from source. It can verify minisign signatures. cargo-dist ("dist") generates the release pipeline and the installers (shell, powershell, npm, homebrew, msi), writes per-archive `.sha256` files by default, and supports GitHub attestations as an experimental feature. GitHub artifact attestations are created with `actions/attest` and checked with `gh attestation verify`.

### Cited Findings
- binstall metadata:
  - `pkg-url` is a template using `{ repo }`, `{ version }`, `{ name }`, `{ target }` and `{ archive-suffix }`
  - `bin-dir` is the binary's path inside the archive (`.exe` is added on Windows)
  - `pkg-fmt` defaults to `tgz`
  - the default naming tries patterns such as `{ name }-{ target }-v{ version }{ archive-suffix }` under `{ repo }/releases/download/v{ version }/`
  - per-target overrides accept exact triples or `cfg()` expressions

  Fallbacks are QuickInstall, then compiling from source, and `disabled-strategies` can turn either off. — [cargo-binstall SUPPORT.md](https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/SUPPORT.md)
- binstall signing: minisign is the only supported algorithm. `[package.metadata.binstall.signing]` takes `algorithm = "minisign"`, `pubkey`, and an optional `file` template (default `{ url }.sig`). binstall downloads the `.sig` and verifies it, and shows the trusted comment if there is one. — [cargo-binstall SIGNING.md](https://github.com/cargo-bins/cargo-binstall/blob/main/SIGNING.md)
- dist config (docs reference 0.32.0):
  - `checksum = "sha256"` is the default and produces a `.sha256` per archive. sha512, sha3 and blake2 are alternatives, and `false` disables checksums.
  - Targets include x86_64/aarch64 for apple-darwin, Windows MSVC, and Linux gnu and musl.
  - Installers: `shell`, `powershell`, `npm`, `homebrew`, `msi`.
  - `install-path` accepts `CARGO_HOME`, `~/subdir` or `$VAR/path`, with cascading fallbacks since 0.14.0.
  - Archives default to `.tar.xz` on Unix and `.zip` on Windows.
  - `github-attestations = true` is experimental, with phase and glob filtering.

  — [dist config reference](https://axodotdev.github.io/cargo-dist/book/reference/config.html). The docs host `opensource.axo.dev` did not resolve during this research. The book is still served at axodotdev.github.io.
- dist's shell installer "detects the current platform, fetches the best possible archive ..., copies the binary into your install-path, and attempts to add that path to the user's PATH". Limitations from the docs:
  - it needs `tar`/`unzip`/`curl`/`wget`
  - it cannot automatically tell when static (musl) builds are appropriate
  - it keeps only binaries
  - it writes an `env` script and sources it from `~/.profile`

  — [dist: shell installer](https://axodotdev.github.io/cargo-dist/book/installers/shell.html)
- dist signing status: Windows codesigning is available. macOS codesigning, Linux codesigning, Sigstore and Windows Trusted Signing are listed as planned. — [dist: supply-chain security](https://axodotdev.github.io/cargo-dist/book/supplychain-security/index.html)
- GitHub artifact attestations are generated with `actions/attest@v4` and permissions `id-token: write, contents: read, attestations: write`, using `subject-path` for binaries. They are verified with `gh attestation verify PATH -R OWNER/REPO`. Offline (air-gapped) verification has separate guidance. — [GitHub Docs: Using artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds)

### Inferences
- For a plugin's own launcher, the useful conventions to copy are:
  - asset names that include the Rust target triple, e.g. `name-x86_64-unknown-linux-musl.tar.gz`
  - per-asset `.sha256` files and/or a combined SHA256SUMS
  - optional attestation for users who want provenance

  Following binstall's default naming means `cargo binstall <crate>` works in CI for free.
- `cargo install --locked` is the last-resort, build-from-source path for any platform without a prebuilt binary. It is slow and needs a Rust toolchain, so it suits CI but not a hook.

### Gaps
- I did not fetch the binstall docs for the exact behavior of `--only-signed` / `--skip-signatures`, or what happens when a `.sig` is missing.
- I did not get a primary source for `cargo install --locked` semantics (the Cargo book), or for whether dist emits a combined `sha256.sum`.
- I found no evidence that dist's own installers verify GitHub attestations.

## Homebrew, winget and Scoop as secondary channels

### Takeaway
dist can generate a Homebrew formula/tap installer, so a Rust tool released with dist gets Homebrew nearly for free. I did not research winget or Scoop.

### Cited Findings
- `"homebrew"` is one of dist's installer types. — [dist config reference](https://axodotdev.github.io/cargo-dist/book/reference/config.html)

### Inferences
- These channels suit users who want the CLI on PATH on its own. A plugin launcher could then prefer a PATH binary (ruff's `fromEnvironment` model) and treat these channels as a way to offer offline or managed installs.

### Gaps
- No sources were gathered on winget manifests, Scoop buckets, or whether dist supports them. This was deliberately skipped within the tool-call budget.

## Cross-cutting practices (musl, macOS, Windows ARM64, signing/quarantine, version handshake, fallback, offline)

### Takeaway
The evidence points to these practices:

- **Static musl builds and asset naming.** Publish static musl Linux builds. VS Code (`alpine-*`), PyPI (`musllinux`) and dist (musl targets) all treat musl as its own target. Encode platform plus libc in asset names.
- **Fallback for unsupported platforms.** Always provide a fallback: a universal or no-server package plus a user path override, Rosetta fallback, or build from source.
- **Version handshake.** Check the binary's version and degrade gracefully.
- **Signing and notarization.** Notarize macOS binaries if users may download them in a browser.

### Cited Findings
- musl as a separate target:
  - VS Code `alpine-x64`/`alpine-arm64` targets — [VS Code docs](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#platformspecific-extensions)
  - ruff `musllinux_1_2` wheels — [PyPI ruff](https://pypi.org/project/ruff/#files)
  - dist musl targets — [dist config](https://axodotdev.github.io/cargo-dist/book/reference/config.html)
  - dist's shell installer cannot itself detect when musl is appropriate — [dist shell installer](https://axodotdev.github.io/cargo-dist/book/installers/shell.html)
- macOS: ruff ships separate `macosx_11_0_arm64` and `macosx_10_12_x86_64` wheels rather than a universal2 wheel. — [PyPI ruff](https://pypi.org/project/ruff/#files) gh falls back to amd64 under Rosetta. — [cli/cli manager.go](https://raw.githubusercontent.com/cli/cli/trunk/pkg/cmd/extension/manager.go)
- Windows ARM64 is now commonly shipped: VS Code `win32-arm64`, ruff `win_arm64`, and dist Windows ARM64 MSVC. — [VS Code docs](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#platformspecific-extensions); [PyPI ruff](https://pypi.org/project/ruff/#files); [dist config](https://axodotdev.github.io/cargo-dist/book/reference/config.html)
- Gatekeeper and quarantine:
  - Browsers set the `com.apple.quarantine` attribute, and Gatekeeper then checks the binary before it runs. curl does not set it. — [Unit 42: Gatekeeper bypass](https://unit42.paloaltonetworks.com/gatekeeper-bypass-macos/)
  - Projects report that unsigned or ad-hoc-signed CLI binaries downloaded through a browser are blocked, and add notarization or document `xattr -d com.apple.quarantine`. — [crgx #12](https://github.com/yfedoseev/crgx/issues/12); [vaultspec-core #336](https://github.com/nevenincs/vaultspec-core/issues/336); [jdx/usage PR #1378](https://github.com/jdx/usage/pull/1378)
  - An Apple forum thread notes that Gatekeeper does not lift the quarantine attribute even on notarized downloaded apps. — [Apple Developer Forums 732370](https://developer.apple.com/forums/thread/732370)
- Version handshake examples:
  - ruff-vscode requires >= 0.5.3 and otherwise falls back with a warning — [Ruff settings](https://docs.astral.sh/ruff/editors/settings/)
  - Zed's HTML extension compares the installed version to the latest — [html.rs](https://raw.githubusercontent.com/zed-industries/zed/main/extensions/html/src/html.rs)
  - rust-analyzer warns on a mismatch with the toolchain file — [issue #21261](https://github.com/rust-lang/rust-analyzer/issues/21261)
- Unsupported-platform fallbacks:
  - rust-analyzer `no-server.vsix` plus `server.path` — [rust-analyzer book](https://rust-analyzer.github.io/book/vs_code.html)
  - binstall falls back to source — [SUPPORT.md](https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/SUPPORT.md)
  - gh gives a clear error with an issue link — [manager.go](https://raw.githubusercontent.com/cli/cli/trunk/pkg/cmd/extension/manager.go)
- Offline:
  - VSIX sideload with `code --install-extension file.vsix` — [rust-analyzer book](https://rust-analyzer.github.io/book/vs_code.html)
  - clangd's auto-download fails without internet — [vscode-clangd #112](https://github.com/clangd/vscode-clangd/issues/112)
  - GitHub attestation verification has a separate offline path — [GitHub Docs](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds)

### Inferences
- Bundling the binary inside the plugin artifact avoids the quarantine and network issues, because no runtime download happens. For a hook-driven plugin, a runtime download is the riskiest option.
- A protocol or version handshake (`binary --version`, or a JSON `protocol_version`) plus graceful degradation follows the ruff-vscode precedent. A hook should no-op with a single warning rather than fail the user's action.
- ZIP archives are known to drop Unix permissions depending on the tool that made them. Any launcher that extracts a binary should `chmod +x` explicitly, as Zed's API does with `make_file_executable`. The ZIP point is my inference and has no source here.

### Gaps
- No primary Apple source was fetched on notarization requirements for standalone command-line binaries, or on whether non-browser downloaders (Node `fetch`, VS Code marketplace extraction) set quarantine.
- No sources on universal2 (lipo) trade-offs.
- No source on executable-bit loss in ZIP or VSIX extraction.

## WASM/WASI as an alternative to native binaries

### Takeaway
WASM gives one portable artifact, which is how Zed avoids per-platform extension builds. The costs are that the sandbox cannot spawn processes and needs host capabilities for exec, download and filesystem access, and Node's WASI is still experimental and is not a security sandbox.

### Cited Findings
- Zed extensions call into the host for downloads and process execution, gated by `process:exec`/`download_file` capabilities. Without them those APIs return errors. — [Zed capabilities](https://zed.dev/docs/extensions/capabilities)
- Node.js v26.10.0 docs: `node:wasi` is "Stability: 1 - Experimental". It "does not currently provide the comprehensive file system security properties provided by some WASI runtimes ... do not rely on it to run untrusted code". It gives capabilities such as `preopens`, `env` and stdio. — [Node.js WASI docs](https://nodejs.org/api/wasi.html)

### Inferences
- Compiling the Rust CLI to `wasm32-wasip1`/`wasip2` and running it under Node's WASI would remove per-platform binaries. However, it would lose subprocess spawning (git and similar), native filesystem performance, and threads. File access is limited to preopened directories, and the runtime is still experimental. It fits best as a fallback for an unsupported platform, not as the primary path.

### Gaps
- I did not research Extism or wasmtime embedding, WASI preview-2 adoption, or WASM startup-time numbers relevant to short-lived hooks.
