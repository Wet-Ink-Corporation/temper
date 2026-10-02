# Distributing native binaries (Rust/Go/Zig) through npm: per-platform packages vs. postinstall/first-run download (as of October 2026)

Research date: 2026-10-01. About 22 tool calls. Primary sources (source code, maintainer issues, official docs) were used where they were available. Where a fact was not verified in this pass, it appears under Gaps, not as a finding.

## 1. The optionalDependencies per-platform pattern: resolution, launcher, failure modes

### Takeaway
The mature projects (esbuild since 0.13.0 in Sept 2021, Sentry CLI, Biome, napi-rs-generated addons) publish one npm package per os/cpu(/libc) and list them all as exact-version `optionalDependencies` of a wrapper package. The package manager installs only the one that matches the machine, using the `os`/`cpu`/`libc` fields. A small JS launcher then `require.resolve`s that package and spawns the binary. The main failure modes are:
- lockfiles generated on another platform (npm/cli #4828, fixed in npm 11.3.0, April 2025)
- `--omit=optional` / `--no-optional` / Yarn `ignore-optional`
- copying `node_modules` across OSes
- libc detection

Mature projects add a postinstall fallback on top of the optional packages rather than relying on either mechanism alone.

### Cited Findings
**History and rationale (esbuild)**
- esbuild 0.13.0 switched to `optionalDependencies` to install the platform binary. Before that, a post-install script downloaded the binary from platform packages such as `esbuild-darwin-64`. That broke under `npm --ignore-scripts`, "and some people enable this option so that malicious packages must be run first…". The new strategy "depend[s] on all esbuild packages for all platforms but only ha[s] the one for the current platform be installed, which is a built-in part of the package manager." — [esbuild v0.13.0 release](https://github.com/evanw/esbuild/releases/tag/v0.13.0)
- Contemporary reaction from Evan You: "Yarn 1/2 will download all binaries before picking the right one. Other (up to date) package managers only download the matching one." — [Evan You on X](https://twitter.com/youyuxi/status/1440864427532644352?lang=en)

**esbuild install script (`lib/npm/node-install.ts`)**
- Step 1: resolve the platform package from optionalDependencies via `require.resolve()`.
- Step 2: if it is missing (for example `--no-optional`), run `npm install` of the platform package in a temp directory.
- Step 3: as a last resort, download the tarball directly from the npm registry.
- The source comments: "Package installation uses 'optionalDependencies' for a reason: manually downloading the package has a lot of obscure edge cases that fail because people have customized their environment in some strange way."
- Source: [esbuild node-install.ts](https://raw.githubusercontent.com/evanw/esbuild/main/lib/npm/node-install.ts)

**esbuild launcher optimisation**
- On non-Windows systems, the install script replaces the JS wrapper `bin` with the real binary, which removes the node-shim overhead.
- It skips this under Yarn: "Avoid idempotency issues by just not optimizing when using Yarn".
- It also skips it for WASM, and the optimisation does not happen at all when `--ignore-scripts` prevents the script from running.
- Source: [esbuild node-install.ts](https://raw.githubusercontent.com/evanw/esbuild/main/lib/npm/node-install.ts)

**esbuild documented flag behaviour**
- `--ignore-scripts`: the binary "should still be automatically selected and installed from esbuild's optional dependencies by npm". The cost is a slower JS-shim path.
- `--no-optional`: the install script "attempts to download it manually from the npm registry", which can fail with proxies or custom registries.
- Both flags together: "the installed `esbuild` package is broken… I consider this to be a user error."
- Source: [esbuild Getting Started](https://esbuild.github.io/getting-started/)

**esbuild cross-OS copies and Yarn**
- "You cannot install esbuild on one OS, copy the `node_modules` directory to another OS without reinstalling, and then run esbuild on that other OS."
- The documented Yarn fix is `supportedArchitectures` in `.yarnrc.yml`.
- Yarn PnP "is supported natively by esbuild".
- Source: [esbuild Getting Started](https://esbuild.github.io/getting-started/)
- esbuild later fixed `npm i --no-optional` handling for scoped packages (commit for #2720). — [esbuild commit a2a2dd0](https://github.com/evanw/esbuild/commit/a2a2dd0cda849981f367917ac0572d454804d0cf)

**Biome launcher (`packages/@biomejs/biome/bin/biome`)**
- Maps `process.platform` × `process.arch` to packages such as `@biomejs/cli-linux-x64`.
- On Linux it detects musl by running `ldd --version` via `execSync` and checking for "musl", including in the stderr of a failed call.
- `BIOME_BINARY` env var overrides the binary path.
- It runs the binary with `spawnSync(..., {stdio: "inherit"})` and passes `process.argv.slice(2)`.
- It injects `BIOME_DISTRIBUTION=npm`, `JS_RUNTIME_VERSION`, `JS_RUNTIME_NAME`, and `NODE_PACKAGE_MANAGER` (from `npm_config_user_agent`).
- If no package matches, it prints an error and exits 1.
- So Biome keeps a JS shim on every invocation; it does not replace the bin.
- Source: [Biome launcher](https://raw.githubusercontent.com/biomejs/biome/main/packages/@biomejs/biome/bin/biome)

**napi-rs (Node-API addons, which load in-process instead of spawning)**
- "The root package lists exact versions of those packages in `optionalDependencies`; its generated loader then loads the package matching the running system."
- Packages are named like `@scope/addon-linux-x64-gnu` and `@scope/addon-linux-x64-musl`. Each declares `os`, `cpu` and, where applicable, `libc`.
- `napi pre-publish` syncs versions and optional deps and publishes the platform packages.
- Source: [napi-rs release docs](https://napi.rs/docs/deep-dive/release)

**oxlint**
- The npm `oxlint` bin is now just `#!/usr/bin/env node import "../dist/cli.js";`. This suggests oxlint loads a native addon in-process (napi-style) rather than spawning a separate binary through a shim. The resolution logic was not inspected; see Gaps.
- Source: [oxlint bin](https://raw.githubusercontent.com/oxc-project/oxc/main/npm/oxlint/bin/oxlint)

**Sentry CLI**
- Platform packages: `@sentry/cli-darwin`, `-linux-arm`, `-linux-arm64`, `-linux-i686`, `-linux-x64`, `-win32-i686`, `-win32-x64`, `-win32-arm64`. — [sentry-cli package.json](https://github.com/getsentry/sentry-cli/blob/master/package.json)
- Sentry's write-up (Luca Forstner, 2024-01-05) explains the dual design:
  - "To maximize the likelihood of success, we found it necessary to try both approaches when installing the package."
  - The postinstall checks whether the optional package landed and downloads it if not.
  - "we haven't found a reliable way to ship binaries if both optionalDependencies and postinstall scripts are disabled."
  - Source: [Sentry blog: Publishing binaries on npm](https://blog.sentry.io/publishing-binaries-on-npm)
- Failure in practice: a user with Yarn `--install.ignore-optional true` got "Sentry CLI binary for this platform/architecture not found!" (Dec 2023, issue titled "@sentry/cli optional dependencies are not optional"). — [getsentry/sentry-cli #1862](https://github.com/getsentry/sentry-cli/issues/1862)
- A related report: "Vercel build errors in runtime due to Sentry CLI binary for arch not found". — [sentry-cli #1849](https://github.com/getsentry/sentry-cli/issues/1849)

**npm lockfile bug (npm/cli #4828)**
- Opened 2022-04-29 against npm 8.8.0. If you regenerate `package-lock.json` while `node_modules` exists, npm records only the current platform's optional package. When a colleague on another arch runs `npm ci`, npm silently skips their binary. The example in the issue is `@swc/core` on arm64 Mac vs. x64. — [npm/cli #4828](https://github.com/npm/cli/issues/4828)
- Fixed by PR #8184, "fix(arborist): omit failed optional dependencies from installed deps". It was merged 2025-04-03 and shipped in npm 11.3.0. Failed optional deps are marked "inert" so they stay in the lockfile. — [npm/cli PR #8184](https://github.com/npm/cli/pull/8184)

**Cross-platform install controls**
- npm v10+ has limited `--os`, `--cpu`, `--libc` install flags.
- pnpm has `supportedArchitectures` (os/cpu/libc), and pnpm 10.14 added `--os/--cpu/--libc` CLI flags.
- Source: [pnpm dependency-resolution settings](https://pnpm.io/settings/dependency-resolution); [pnpm 10.14 release](https://pnpm.io/blog/releases/10.14); [npm RFC #612](https://github.com/npm/rfcs/issues/612)
- pnpm's libc support has had its own bugs, e.g. "pnpm 10.14.0 with supportedArchitectures.libc breaks AWS CodeArtifact downloads". — [pnpm #9862](https://github.com/pnpm/pnpm/issues/9862)
- Another pnpm limitation: supportedArchitectures takes a Cartesian product, not tuples. — [pnpm #13903](https://github.com/pnpm/pnpm/issues/13903)

### Inferences
- Two launcher styles exist:
  - A persistent JS shim that spawns the binary (Biome; esbuild on Windows, under Yarn, or with ignore-scripts).
  - A postinstall step that overwrites the `bin` with the native executable (esbuild on POSIX).
  
  In-process napi addons (napi-rs, apparently oxlint) are a third model. They avoid a child process but still pay node startup.
- The "converged" design is: per-platform optionalDependencies with exact versions, plus a postinstall that verifies and falls back, plus an env-var override for the binary path. esbuild and Sentry both landed on this independently.
- Pinning to npm ≥ 11.3.0 removes the #4828 lockfile hazard. Older npm 8–10 lockfiles committed from one platform remain a risk.

### Gaps
- Not verified in this pass: launcher source for turbo, @swc/core, @rollup/rollup-*, lightningcss, rspack and Bun's npm package. Bun is believed to use per-platform optional packages plus a postinstall, but that is unverified. They are not described here to avoid unsourced claims.
- Not inspected: the exact musl detection in esbuild/napi-rs loaders (e.g. `process.report.getReport().header.glibcVersionRuntime` vs. reading `/usr/bin/ldd`).
- No primary-source issue was found specifically on `--omit=optional` breaking a particular tool in 2025–2026, beyond the Sentry and esbuild docs above.

## 2. Postinstall / first-run download installers: proxies, checksums, CI caching

### Takeaway
Download installers break whenever scripts are disabled or the network is restricted.
- With `--ignore-scripts`, the install appears to succeed but no binary arrives.
- Corporate proxies, custom registries and air-gapped CI break the fetch.
- cargo-dist's npm installer, inspected on main in Oct 2026, verifies no checksums.

esbuild abandoned this as its primary path in 2021, and Sentry keeps it only as a fallback.

### Cited Findings
- The old esbuild postinstall-download design broke under `--ignore-scripts`. Avoiding that was the stated reason for the 0.13.0 switch. — [esbuild v0.13.0](https://github.com/evanw/esbuild/releases/tag/v0.13.0)
- esbuild warns that its own registry-download fallback is less robust and can fail with proxies or custom registries. — [esbuild Getting Started](https://esbuild.github.io/getting-started/)
- Sentry calls disabling postinstall "generally recommended… due to being prone to supply chain attacks", which is why a download-only installer is fragile. — [Sentry blog](https://blog.sentry.io/publishing-binaries-on-npm)
- Recent third-party projects have opened issues to move from postinstall download to optionalDependencies. They cite npm 11 prompting before running install scripts, and the fact that a deferred download lets `npm install` succeed without a binary. These are secondary signals, not maintainer statements from major projects.
  - Source: [vestige #220](https://github.com/samvallad33/vestige/issues/220); [agnostic-ai #942](https://github.com/Chemaclass/agnostic-ai/issues/942)
  - Related: a downstream PR approving install scripts via `allowScripts`. — [pubky-app PR #2464](https://github.com/pubky/pubky-app/pull/2464)
- Sentry CLI users asked for install without postinstall. — [sentry-cli #1360](https://github.com/getsentry/sentry-cli/issues/1360)

### Inferences
- CI caching: optional-package binaries live in the npm cache and lockfile, so ordinary `~/.npm` or pnpm store caching covers them. Download installers fetch from GitHub Releases or a CDN outside the npm cache, so each fresh runner re-downloads unless `node_modules` itself is cached. This is inferred from the mechanisms; no benchmark was found.
- Download-on-first-run in particular (cargo-dist's `run()`) moves the failure from install time to the first command. For a hook that runs on every tool call, that means a network fetch and possible latency or failure in the middle of a session.

### Gaps
- Prisma engines' download/checksum/`PRISMA_ENGINES_MIRROR` behaviour was not researched in this pass.
- The `binary-install` npm library (Cloudflare origin) was not re-checked for maintenance status as of 2026.
- No quantified data was found on corporate-proxy failure rates.

## 3. cargo-dist (dist) npm installer specifically

### Takeaway
As of v0.33.0 (2026-09-11), dist's npm installer is still a single package. It downloads the platform archive from the release URL at install (postinstall), and again on first run if the binary is missing. It does **not** use per-platform optionalDependencies. It has no checksum verification and does not implement fallback URLs. It is maturing slowly; the recent work is dependency removal, not architecture changes.

### Cited Findings
- From the dist docs: "The npm package will fetch your prebuilt archives and install your binaries to node_modules, exposing them as commands."
- Config options:
  - `npm-scope`
  - `npm-package`
  - `npm-shrinkwrap`, which is on by default to lock the installer's own deps
- Source: [dist npm installer docs](https://axodotdev.github.io/cargo-dist/book/installers/npm.html)
- Template files: `binary-install.js`, `binary.js`, `install.js`, `run.js.j2`. — [cargo-dist npm templates](https://github.com/axodotdev/cargo-dist/tree/main/cargo-dist/templates/installer/npm)
- Platform detection in `binary.js`:
  - Uses `libc.familySync()`; musl gives the `unknown-linux-musl-dynamic` target.
  - If the glibc version is below `glibcMinimum` from package.json, it falls back to static musl ("Your glibc isn't compatible; trying static musl binary instead").
  - Only `artifactDownloadUrls[0]` is used, with a "FIXME: implement NPM installer handling of fallback download URLs".
  - No checksum verification.
  - Source: [binary.js](https://raw.githubusercontent.com/axodotdev/cargo-dist/main/cargo-dist/templates/installer/npm/binary.js)
- Download mechanics in `binary-install.js`:
  - Uses Node built-in `http`/`https`, honours `HTTPS_PROXY`/`HTTP_PROXY`/`NO_PROXY`, and supports CONNECT tunnelling with Basic auth.
  - Extracts with the system `tar`, or `unzip`/PowerShell `Expand-Archive` on Windows, into `node_modules/.bin_real`.
  - Checksum verification is "Not implemented".
  - `run()` installs on first execution if the binary is missing: `!this.exists() ? this.install(true) : …`.
  - Source: [binary-install.js](https://raw.githubusercontent.com/axodotdev/cargo-dist/main/cargo-dist/templates/installer/npm/binary-install.js)
- Releases:
  - Latest is v0.33.0, 2026-09-11.
  - v0.32.0 (2026-05-22): "The npm installer now relies on builtin node modules in place of the external axios and rimraf libraries", and the minimum Node is now 14.14.
  - v0.31.0 added `npm-shrinkwrap = false`.
  - Releases come roughly quarterly.
  - Source: [cargo-dist releases](https://github.com/axodotdev/cargo-dist/releases)
  - Caveat: these dates come from a summarised fetch of the releases page and should be spot-checked.

### Inferences
- dist's installer has the same failure profile esbuild left behind in 2021:
  - It breaks under ignore-scripts until first run.
  - It needs network access at install or run time.
  - It fetches from GitHub Releases rather than the npm registry, so registry mirrors and npm caches do not help.
  - It has no integrity check beyond TLS.
- For a plugin whose hook fires on every tool call, adopting it unchanged would mean those weaknesses apply to the hook too. A napi-rs-style or hand-rolled optionalDependencies layout is the more robust path.

### Gaps
- No primary evidence of a dist roadmap item for optionalDependencies-style npm packaging.
- "cargo-npm" and similar community tools were not verified. No authoritative source was found in this pass.

## 4. Provenance: npm provenance, GitHub artifact attestations, sigstore, SHA-256 pinning

### Takeaway
npm provenance is Sigstore-backed and is automatic with trusted publishing. Consumers check it with `npm audit signatures`. It covers every per-platform package, because each is an ordinary npm package. Download installers fall outside this. They need separate verification, either GitHub artifact attestations (`gh attestation verify`) or hard-coded SHA-256s in the launcher, and dist's npm installer does neither.

### Cited Findings
**npm provenance**
- Provenance is "a verifiable link between published packages and their source code and build instructions".
- Requirements:
  - npm ≥ 9.5.0
  - GitHub Actions or GitLab CI/CD on cloud-hosted runners
  - `id-token: write`
  - `npm publish --provenance`
- "If you use trusted publishing, provenance attestations are automatically generated."
- Consumers verify with `npm audit signatures`. The underlying technology is Sigstore ephemeral certificates plus a transparency log.
- Caveat from the docs: provenance "does not guarantee the package has no malicious code."
- Source: [npm docs: Generating provenance statements](https://docs.npmjs.com/generating-provenance-statements)

**GitHub artifact attestations**
- Generated with the attest action, which the docs now reference as `actions/attest@v4`.
- Permissions: `id-token: write`, `contents: read`, `attestations: write`.
- Verified with `gh attestation verify PATH/TO/BINARY -R ORG/REPO`.
- Source: [GitHub docs: artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds)
- dist's npm installer performs no checksum or attestation verification of downloaded archives. — [binary-install.js](https://raw.githubusercontent.com/axodotdev/cargo-dist/main/cargo-dist/templates/installer/npm/binary-install.js)

### Inferences
- The per-platform pattern inherits npm's registry integrity (lockfile `integrity` SHA-512) and provenance for free. A launcher-embedded SHA-256 table is mainly needed for download-based installers or env-var override paths.
- `gh attestation verify` needs the `gh` CLI and network access, so it suits release or CI checks, not a runtime launcher.

### Gaps
- Not confirmed which of esbuild, Biome, turbo, SWC and rollup publish with provenance. This could be checked on npmjs.com package pages.
- No major npm launcher was found that pins a SHA-256 of its binary at runtime.

## 5. Version skew handling

### Takeaway
esbuild explicitly checks that the binary version matches the wrapper (`validateBinaryVersion` runs the binary with `--version` at install). The pattern's main skew defence is exact-version pins in optionalDependencies. Biome's launcher was not observed doing a runtime version check.

### Cited Findings
- esbuild's `validateBinaryVersion()` runs the binary with `--version` and compares the result to package.json. — [esbuild node-install.ts](https://raw.githubusercontent.com/evanw/esbuild/main/lib/npm/node-install.ts)
- napi-rs pins platform packages to exact versions in `optionalDependencies`, and `napi pre-publish` syncs versions. — [napi-rs release docs](https://napi.rs/docs/deep-dive/release)
- Sentry's guide uses a hardcoded `BINARY_DISTRIBUTION_VERSION`, with no automatic compatibility check described. — [Sentry blog](https://blog.sentry.io/publishing-binaries-on-npm)
- Biome's launcher spawns the resolved binary without any described version check. It passes runtime info through env vars instead. — [Biome launcher](https://raw.githubusercontent.com/biomejs/biome/main/packages/@biomejs/biome/bin/biome)

### Inferences
- Skew mainly arises from env-var overrides (`BIOME_BINARY`, `ESBUILD_BINARY_PATH`), stale `node_modules` copied across platforms, or partial lockfiles (#4828).
- A cheap defence is an install-time `--version` check, as esbuild does, rather than a per-call check that would add latency.

### Gaps
- Not verified whether turbo or SWC check versions at runtime.

## 6. Overhead of a Node shim (startup + spawn)

### Takeaway
Bare Node startup is about 20 ms on Node 18/20 and about 27–31 ms on Node 22/23 (macOS, hyperfine). A JS shim that then `spawnSync`s the native binary adds child-process creation on top. For a hook fired on every tool call, a node shim costs several tens of milliseconds per call before the binary does any work. Replacing the bin (esbuild-style) or invoking the binary directly avoids it.

### Cited Findings
- Hyperfine (`--warmup 20`) on a one-line JS file, run on macOS. The issue notes the official `startup-core` benchmark showed only about 5.8% regression, so methodology matters. — [nodejs/performance #180](https://github.com/nodejs/performance/issues/180)

  | Node version | Mean startup |
  |---|---|
  | 18 | 19.5 ms |
  | 20 | 19.6 ms |
  | 22 | 27.5 ms |
  | 23 | 30.5 ms |

- Earlier measurements put Node startup around 23 ms elapsed. — [Keyhan Vakil, Node.js startup series](https://www.kvakil.me/posts/2023-05-09-nodejs-startup-series-intro-and-measuring-startup-time.html)
- esbuild replaces the JS wrapper with the native binary where possible, specifically to skip this overhead. The Getting Started docs describe the `--ignore-scripts` path, which keeps the shim, as having a performance cost. — [esbuild node-install.ts](https://raw.githubusercontent.com/evanw/esbuild/main/lib/npm/node-install.ts); [esbuild Getting Started](https://esbuild.github.io/getting-started/)

### Inferences
- Estimated cost per call:

  | Path | Estimated cost per call |
  |---|---|
  | Node startup | ~20–30 ms |
  | Biome-style launcher work (module load, platform resolution, `ldd --version` exec on Linux for musl detection) | a few ms, plus one extra process spawn on Linux |
  | Launcher-plus-binary overall | plausibly ~30–60 ms |
  | Calling the native binary directly | typically single-digit ms of process startup |

  These are inferences; no source measured them directly.
- Windows process creation is slower than POSIX, so the shim tax is likely higher there. This is unmeasured in the sources found.

### Gaps
- No primary benchmark was found of "node shim + spawnSync(native)" vs. direct native exec for Biome or turbo, and no Windows-specific numbers.
