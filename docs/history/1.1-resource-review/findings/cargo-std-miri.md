# tooling: findings (Cargo Book, std front page, Miri)

Author: `tooling`. Three resources, one section each. Every claim below marked "verified" was checked on the pinned toolchain (rustc/cargo/clippy 1.98.1) or, for Miri, on `nightly-x86_64-unknown-linux-gnu` (rustc 1.101.0-nightly d080e7dff 2026-09-27, miri 0.1.0). Scratch crates live under `scratchpad/tooling/`. The original `templates/example-workspace` was not modified; all Miri runs used copies (`scratchpad/tooling/exws-miri`, `exws-miri2`).

Line numbers `L1234` refer to `scratchpad/res/cargo-book.md`. Inside table cells, `\|` is Markdown escaping for a literal `|` in regexes and shell pipes.

---

## 1. The Cargo Book

Source: https://doc.rust-lang.org/cargo/. Read: Features (L4774-5247) and Features Examples (L5248-5371); Workspaces (L2637-2926); Specifying Dependencies (L2927-3577) and Overriding Dependencies (L3578-3927); Dependency Resolution incl. resolver versions, Rust version, recommendations, troubleshooting (L4075-4773); Profiles (L5372-5807); Configuration (L5808-7414, all key sections, incl. `build.warnings` L6271, `[env]` L6553, `[resolver]` L6971); Environment variables Cargo sets for crates/build scripts (L7560-7620, L7650-7720); Build Scripts and Build Script Examples (L7780-8835); Cargo Targets (L2109-2467); Rust Version (L2468-2636); Manifest format incl. `[lints]`, include/exclude, publish (L1476-2108); SemVer Compatibility, whole catalogue (L10546-12839); Cargo lints (L12945-13510); Unstable Features list plus minimal-versions, public-dependency, msrv-policy, panic-abort-tests, `[lints.cargo]`, feature-unification, hint-msrv, min-publish-age (L13511-13930, L14159, L15235, L15353, L15557-15684); Stabilized features (L15685-15972); Guide chapters Package Layout, Cargo.toml vs Cargo.lock, Tests, Continuous Integration, Publishing, Optimizing Build Performance (L441-1441); commands cargo-test (L19927-20408), cargo-tree (L21972-22369), cargo-update (L22370-22575), cargo-info (L20836-20993), cargo-add (L20422-20560), cargo-package (L24116-24300), cargo-miri (L18882); FAQ (L25255-25527); future-incompat report and `--timings` (L12840-12944).

### Template audit (what the template actually does)

| Question | Answer (from `templates/workspace/`, verified) |
|---|---|
| `workspace.package` inheritance? | Yes: `edition`, `rust-version = "1.96"`, `version`, `license`, `publish = false`. Members inherit (`edition.workspace = true` etc.). |
| `workspace.dependencies`? | Yes (`thiserror = "2"`; the example adds serde, anyhow, tracing, proptest, insta, tempfile, and the internal path crates). All use implicit minimum versions (`"2"`, `"1"`, `"0.1"`). |
| `[lints] workspace = true`? | Yes in every example member. Not enforced by any hook or gate for new crates (only a setup reminder at `scripts/temper.mjs:170`). |
| resolver? | `resolver = "3"` set explicitly on the virtual manifest (correct: a virtual manifest has no edition to infer it from, L2718-2723). |
| CI `--locked`? | **No.** None of clippy, nextest, `test --doc`, doc or the MSRV `check` pass `--locked` or `--frozen`. |
| Warnings policy | `CARGO_BUILD_WARNINGS: deny` at workflow level plus `clippy -- -D warnings` plus `RUSTDOCFLAGS: -D warnings` on the doc step; MSRV job uses `RUSTFLAGS=-D warnings` (correct: Cargo < 1.97 ignores `build.warnings`, L6288). |
| Features tested | Only `--all-features` (no `--no-default-features`, no per-package builds). |
| Profiles | `[profile.release] lto = "thin"`, `codegen-units = 1`. No dev-profile tuning, `overflow-checks` default (false in release). |
| deny.toml | `wildcards = "deny"`, `unknown-git = "deny"`, `unknown-registry = "deny"`, `yanked = "deny"`, `multiple-versions = "warn"`. |
| semver-checks / Miri / latest-deps jobs | None. |

### Already covered (summary)

Virtual workspace with shared package/deps/lints, edition 2024, resolver 3, declared `rust-version` → ARC-01, MOD-01, MOD-09 (template verified). Features additive, positive `std` feature, no mutually exclusive features, test `--no-default-features`/`--all-features`, `cargo hack --feature-powerset` → API-16. SemVer items already listed (new enum variant or pub field without `#[non_exhaustive]`, non-defaulted trait item, tightening bounds, RPIT capture changes, removing a feature, MSRV bump by policy, turbofish) and `cargo semver-checks` → API-17, API-05, API-12, MOD-06. `build.warnings` instead of `RUSTFLAGS`, and the MSRV-job gotcha → MOD-13 (book confirms "Respected as of 1.97", L6288, stabilized 1.97, L15969). `Cargo.lock` committed → architecture.md layout. Dependency justification, minimal features at workspace level, `cargo deny`, `cargo tree -d`, `cargo tree -e features` → ARC-12. `cargo tree -e normal -p <crate>` for layering → ARC-02. Release profile knobs and a custom `profiling` profile → PERF-09. One integration binary `tests/it/main.rs` (Cargo Targets L2229-2235 gives the same compile-time reason) → TST-14; `harness = false` benches → TST-16; nextest does not run doctests → TST-15. `CARGO_BIN_EXE_<name>` for CLI end-to-end tests → example `ledger-cli/tests/it`. `*` requirements, unknown git sources, yanked crates → deny.toml. xtask alias → architecture.md. `incompatible_msrv` → MOD-01 table. Glob imports of external crates are a SemVer forward-compatibility hazard (L10709-10747) → already linted by `clippy::wildcard_imports` and `clippy::enum_glob_use` (pedantic, on via `pedantic = "warn"`; the rules never say why).

### Candidates

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement | Value | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| CB-1 | commands/cargo-test.html#option-cargo-test---locked (L20280); reference/resolver.html#dependency-updates (L4506-4522); faq.html#why-have-cargolock-in-version-control (L25349-25388) | `--locked` errors if Cargo.lock is missing or would change; "may be used ... such as in CI pipelines". A committed lock makes CI fail only for new commits. | tooling (refine ARC-12) | CI MUST pass `--locked` to every cargo command that resolves dependencies (clippy, nextest, `test --doc`, doc, MSRV check, miri). A manifest change without the matching `Cargo.lock` change fails CI. The local gate stays without `--locked` (it would block every dependency edit before cargo refreshes the lock). | Template `rust.yml`: append `--locked` to lines `cargo clippy ...`, `cargo nextest run ...`, `cargo test --doc ...`, `cargo doc ...`, MSRV `cargo check ...`. Verified: adding `itoa = "1"` to a member makes `cargo check --workspace --locked` fail with "cannot update the lock file ... because --locked was passed"; `cargo +1.96 check --workspace --all-targets --all-features --locked` passes on the example lockfile. | high | Agents edit `Cargo.toml` and commit without the lock, or cargo silently re-resolves in CI to versions nobody reviewed; both slip through today. |
| CB-2 | reference/features.html#feature-unification (L5013-5034); resolver.html#features (L4394-4416) and #feature-resolver-version-2 dev-deps bullet (L4467-4484); guide/build-performance.html#resolve-features-for-the-whole-workspace ("A package activating a feature can mask bugs in other packages that should activate it but don't", L1383) | Building several workspace members together unifies dependency features; dev-dependency features leak into lib builds of test targets. | new ARC (ARC-16) + tooling | Each crate MUST enable, in its own manifest, every dependency feature its code uses, so it builds alone and without its dev-dependencies. CI MUST check each member separately: `cargo metadata --no-deps --format-version 1 \| jq -r '.packages[].name' \| xargs -I{} cargo check --locked -p {} --lib --bins` (or `cargo hack check --workspace --locked`). | CI step as written (verified command set). Verified repro (`scratchpad/tooling/unify`): member `a` uses `#[derive(serde::Serialize)]` but only sibling `b` enables `derive`: `cargo check --workspace` and `cargo doc --workspace --no-deps` are green, `cargo check -p a` fails E0433. Member `c` gets `derive` only from its own dev-dependency: `cargo check -p c --all-targets` green, `cargo check -p c` fails. The template's CI runs only workspace-wide builds, so it cannot see either. | med-high | Agents run `cargo test -p <crate>` for a fast loop and hit baffling missing-derive errors; a crate that later becomes a dependency elsewhere breaks for its users. |
| CB-3 | reference/lints.html#missing_lints_inheritance (L13076-13113): "Many people mistakenly think that `workspace.lints` is implicitly inherited when it is not." The Cargo lint is nightly-only (L12947, L15235). | Members must opt in with `[lints] workspace = true`; stable Cargo says nothing when they don't. | tooling (refine ARC-01, MOD-09) | A package manifest in a workspace that defines `[workspace.lints]` MUST contain `[lints]` with `workspace = true`; a member without it is a merge blocker. | temper pre-edit hook: when a `Cargo.toml` with a `[package]` table is written and the workspace root has `[workspace.lints]`, deny unless the after-text has a `[lints]` table containing `workspace = true`. Gate: same check over every `manifest_path` from `cargo metadata --no-deps`. Today `hooks.mjs` only asks when existing `[lints]`/`[profile]` text changes (`lintSections`), so a *new* crate without the table passes silently. | high | A new crate created by an agent without the table compiles with `unwrap`, `panic!`, `as` casts and `#[allow]` accepted by clippy; only the regex scanner stands between it and merge. This is the single largest enforcement hole found. |
| CB-4 | reference/cargo-targets.html#the-test-field, #the-doctest-field, #the-harness-field, #target-auto-discovery (L2315-2466); commands/cargo-test.html target selection (L20062-20073: "Setting targets to `test = false` will stop them from being tested by default", "Doc tests ... may be disabled by setting `doctest = false`"); overriding-dependencies.html#the-patch-section (L3823-3870) | Manifest keys can silently stop tests, doctests or auto-discovered test targets from running, and `[patch]` swaps a dependency's source. | tooling (enforces H-12, H-16) | Changing any of these in a manifest is a human decision: `test`, `doctest`, `harness`, `required-features` on `[lib]`/`[[bin]]`/`[[test]]`/`[[bench]]`/`[[example]]`; `autotests`/`autobenches`/`autoexamples`/`autobins`/`autolib`/`build` in `[package]`; any `[patch.*]` or `[replace]` entry; `rust-version`, `edition`, `resolver`, and the `default` list in `[features]`. | Extend `hooks.mjs` `lintSections()` (currently only `[lints]`/`[profile]` headers, L120-129) to also capture these keys and tables, turning edits into asks with rule IDs H-12 (targets) and H-16 (`[patch]`, MSRV, defaults). `depNames()` (L132-150) does not see `[patch]`, so a git override is currently unasked; `cargo deny` catches `unknown-git` only in CI. | med-high | `doctest = false` or `test = false` is the easiest route around a red gate (an H-12 violation the scanner cannot see because it is TOML, not Rust). |
| CB-5 | reference/semver.html: #struct-add-private-field-when-public (L11482), #enum-fields-new (L11678), #trait-object-safety (L11857), #trait-new-default-item (L11806), #impl-item-new (L11943), #struct-private-fields-with-private tuple note (L11585-11608), #attr-adding-non-exhaustive (L12461-12542), #fn-generalize-compatible inference note (L12330-12356), #generic-generalize-different (L12110), #item-remove incl. "adding any sort of cfg attribute" (L10679-10687) | The full list of major and possibly-breaking changes, with mitigations (`#[deprecated]` plus `pub use` for renames, non_exhaustive at introduction, sealed traits, defaults for new trait items). | refine API-17 (H-16 detection) | Add to API-17's breaking list: adding a private field to an all-`pub`-field struct; adding a field to an enum variant (use `#[non_exhaustive]` on variants or a struct payload); adding `#[non_exhaustive]` to an existing type or variant (decide it when the type is introduced); adding an associated const or generic method that makes a trait no longer dyn-compatible; shifting the index of a `pub` tuple field; putting an existing public item behind a `cfg`. Possibly breaking (flag for H-16): a new defaulted trait method or inherent method whose name can collide, and generalizing a concrete parameter or return type to a generic (inference failures). Mitigate renames with `#[deprecated]` plus a `pub use` of the old name. Also: `cargo semver-checks` does not detect type changes of fields or parameters, generic or lifetime changes, or breaks that appear only under a subset of features (its README), so reviewers MUST check those by hand. For library crates with consumers outside the workspace, run `cargo semver-checks --baseline-rev origin/main -p <crate>` on PRs. | Review (rubric API dimension) + optional CI job `obi1kenobi/cargo-semver-checks-action@v2` for published crates. Inside one workspace the compiler already catches breaks against sibling crates, so H-16 "API break" means `pub` items with external consumers. | high | H-16 asks agents to recognise API breaks; the current list misses the struct-literal, variant-field, dyn-compatibility and "non_exhaustive later" cases that agents actually make, and over-trusts semver-checks. |
| CB-6 | reference/features.html#semver-compatibility (L5185-5205); semver.html#cargo-feature-remove, #cargo-feature-remove-another, #cargo-remove-opt-dep (L12672-12786) | Removing a feature, removing a feature from `default`, removing an optional dependency, or moving existing public code behind a feature is breaking; adding a feature or optional dep is minor. | refine API-17 / API-16 | A feature change is breaking (H-16) when it removes a feature, removes an entry from `default`, removes an optional dependency that is exposed as a feature, or moves existing public items behind a feature. Deprecate a feature by keeping it as a no-op. | Review; `cargo semver-checks` for published crates. | med | API-17 lists only "removing a feature". |
| CB-7 | reference/resolver.html#recommendations (L4602-4651); specifying-dependencies.html#version-metadata recommendation box (L3074-3106); lints.html#implicit_minimum_version_req (L13026-13074) | Use caret requirements with all three components set to the version you actually need, raise the floor when you start using newer APIs; avoid `*`, open `>=`, `~`, `=` and upper bounds (except `=` for a tightly coupled proc-macro twin); control exact versions through the lockfile. | refine ARC-12 | Declare each requirement in `[workspace.dependencies]` as a full `major.minor.patch` caret floor equal to the oldest version whose API you use (when adding a dependency, the version cargo just locked). Raise the floor in the same change that starts using a newer API. Never use `~`, `=`, `<`, `>=`-only or `*`; pin within the lock with `cargo update -p <crate> --precise <ver>` instead. Exception: `=` for a proc-macro companion you re-export. | Review; optional hook advisory on `Cargo.toml` dependency values matching `/=\s*"\s*(?:[~=<>*]\|\d+(?:\.\d+)?")/` (`"1"`, `"1.0"`, operators). Optional nightly job for published crates: `cargo +nightly generate-lockfile -Zdirect-minimal-versions && cargo +nightly check --workspace --all-targets` (L13722-13739). | med | The template itself writes `thiserror = "2"`, `serde = "1"`; agents copy it. A too-low floor compiles here (lock has a newer version) and breaks downstream `cargo update -p` users (L4635-4639). |
| CB-8 | specifying-dependencies.html#inheriting-a-dependency-from-a-workspace (L3545-3576); workspaces.html#the-dependencies-table (L2832-2873) | Inherited dependencies accept only `features` (additive) and `optional`; `[workspace.dependencies]` cannot be optional. | refine ARC-12 | Put `default-features = false` and the base feature set in `[workspace.dependencies]`; members only add `features` or mark `optional`. | Cargo (verified hard error on edition 2024: "`default-features = false` cannot override workspace's `default-features`"). Add one line to architecture.md ARC-12. | low-med | Saves a failed build round; the architecture.md example already does it right but never says why. |
| CB-9 | features.html#optional-dependencies (L4878-4930) and `?` weak features (L4971-4995); semver.html#cargo-remove-opt-dep (L12720-12786) | `dep:name` hides the implicit feature an optional dependency creates, so it can be swapped later without a breaking change; `pkg?/feat` enables a feature only if something else enabled the optional dep. | refine API-16 | Every optional dependency MUST be referenced as `dep:<name>` in `[features]`; a feature that forwards to another optional dependency's feature uses `<name>?/<feat>`. | Review; hook advisory when a table has `optional = true` and `[features]` lacks `dep:<name>`. | med-low | Keeps optional deps out of the public feature surface (API-14 for features). |
| CB-10 | features.html#the-features-section naming and #mutually-exclusive-features (L4833-4839, L5055-5077) | Features are positive and additive. | tooling (refine API-16) | Enable `clippy::negative_feature_names = "deny"` and `clippy::redundant_feature_names = "deny"`. Both are in the `clippy::cargo` group, which `all`/`pedantic` do not include, so they must be listed explicitly. | `[workspace.lints.clippy]`. Verified (lintcheck ok; both fire on `no-std` and `use-serde` features on 1.98.1). | low-med | Mechanises API-16's "never a `no_std` feature". |
| CB-11 | features.html#feature-documentation-and-discovery (L5207-5231) | Document available features at the top of `lib.rs`; mark internal or unstable ones; docs.rs metadata controls which features are documented. | refine API-15 / API-16 | A crate with features MUST document each public feature in its crate-level docs (`//! # Feature flags`), and mark any internal or experimental feature as such. | Review. | low-med | Features are API (CB-6); undocumented ones get enabled by accident. |
| CB-12 | cargotoml-vs-cargolock.html ("maintained by Cargo and should not be manually edited", L506-507); faq.html#what-does-version-conflict-mean (L25511-25517); resolver.html#semver-breaking-patch-release-breaks-the-build (L4739-4760); commands/cargo-update `--precise`, `--workspace`, `--dry-run` | Change the lock only through cargo; resolve lockfile merge conflicts by taking one side and re-running cargo; avoid a bad release with `cargo update <crate> --precise <ver>`. | new PROC (PROC-13) + tooling | Never edit `Cargo.lock` by hand. Change it with cargo: `cargo update -p <crate> --precise <ver>` to pin or roll back, `cargo update --workspace` after editing versions, `cargo update --dry-run` to preview. Resolve a lockfile conflict by checking out the base `Cargo.lock` and re-running a cargo command. | temper pre-edit hook: deny `Write`/`Edit` on `**/Cargo.lock` with that fix text. | med | Agents "fix" lock conflicts or pin versions by editing the lockfile, producing checksums and graphs cargo never computed. |
| CB-13 | commands/cargo-info.html (L20836-20993); cargo-tree.html `-e features -i`, `--depth workspace`, `--target` default host, `-i` workspace note (L21972-22369); resolver.html#troubleshooting (L4653-4721) | `cargo info <spec>` shows a package's features and picks an MSRV-compatible version; `cargo tree` explains why a crate or feature is present; `--depth workspace` shows only workspace members; the default target filter is the host. | refine PROC-01, ARC-02 | Before declaring features on a dependency, read them from `cargo info <crate>@<locked version>` (features, defaults, `rust-version`, license), not from memory. Explain an unexpected crate or feature with `cargo tree --workspace --target all -e features -i <crate>`. Check layering with `cargo tree --workspace --depth workspace -e normal` (one screen shows the whole crate DAG) and run ARC-02 checks with `--target all` so platform-specific dependencies are not skipped. | Add the commands to process.md's lookup table and to architecture.md ARC-02 **Detect**. Verified `--depth workspace` output on the example workspace. | med | PROC-01 forbids writing dependency APIs from memory but gives no command for features; ARC-02's check misses `[target.'cfg(..)'.dependencies]`. |
| CB-14 | build-scripts.html#outputs-of-the-build-script ("should not modify any files outside of that directory", L7856-7872), #change-detection (L8142-8188), #rustc-check-cfg (L8048-8079), inputs note on `cfg!` vs `CARGO_CFG_*` (L7847-7854, L8725-8753), `cargo::error` note (L8098-8109); build-script-examples.html#code-generation (L8379-8405); cargo-package step 4 "Check that build scripts did not modify any source files" (L24156) | Write only to `OUT_DIR`; always emit `rerun-if-changed`; declare custom cfgs with `rustc-check-cfg`; read target facts from `CARGO_CFG_*`; prefer returning errors. | new ARC (ARC-17) | `build.rs` MUST write only under `OUT_DIR`, emit `cargo::rerun-if-changed` for every input (or `=build.rs` when it has none), declare every emitted cfg with `cargo::rustc-check-cfg`, read target properties from `CARGO_CFG_*` (never `cfg!`), and report failure by returning `Err` from `main`. It writes directives with `writeln!` on a locked stdout. Generated code that is checked in is produced by an xtask and guarded by a freshness test, not by build.rs. | Review; existing lints and scanner already bite: `[lints]` apply to build scripts (verified: `unwrap_used` and `print_stderr` fire in build.rs), and the scanner classifies `build.rs` as `bin` and hard-blocks `println!` (verified). Note clippy's `print_stdout` does **not** fire on `println!` in build.rs (verified), so only the scanner enforces H-06 there: its `fix` text for the `print` rule should show the build-script form (`writeln!(io::stdout().lock(), "cargo::...")?`). | med | Every Cargo example uses `println!` and `unwrap` in build.rs; agents copy them and get refused by the hook with a fix message aimed at binaries. |
| CB-15 | build-script-examples.html#code-generation second bullet (L8390-8396) | For checked-in generated files, have a test regenerate to a temp location and fail on any difference; update by copying the fresh output. | pattern | See "Generated-file freshness test" below. | patterns.md entry; review. | med-low | Generalises OUT-07's schema snapshot to any generated artifact (SQL, OpenAPI, protobuf output) without build.rs writing into `src/`. |
| CB-16 | publishing.html#before-publishing-a-new-crate and #packaging-a-crate (L922-999); manifest include/exclude (L1827-1916), publish (L1918-1941); specifying-dependencies.html#local-paths-in-published-crates (L3283-3302); cargo-package (L24126-24160) | crates.io needs description, license, repository, readme (keywords, categories advised); check `cargo package --list`; path deps need `version`; `cargo publish --dry-run` builds from the tarball. | new API (API-21) + tooling | Before a crate's `publish` is turned on: fill `description`, `license`, `repository`, `readme`, `keywords`, `categories`; give every internal path dependency a `version`; review `cargo package --list` and prefer an `include` allowlist; CI runs `cargo publish --dry-run --locked -p <crate>` for every publishable crate. Turning `publish` on is H-16. | Enable `clippy::cargo_common_metadata = "warn"` (cargo group, needs enabling). Verified: it is silent for `publish = false` crates and lists each missing field once `publish` is unset, so it costs nothing for the template's unpublished crates. `cargo package --list` works even with `publish = false` (verified). | med | Only matters when publishing, but then it prevents shipping test data or a crate that cannot build from its own tarball. |
| CB-17 | guide/continuous-integration.html#verifying-latest-dependencies (L765-823), #checking-for-warnings last considerations (L873-878) | With a committed lock, a scheduled or continue-on-error job (`CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=allow`, `cargo update`, build, test) or Dependabot/Renovate PRs detect breakage from new releases; pin the toolchain and bump it by automated PR. | tooling | SHOULD: add a weekly `latest-deps` job (`continue-on-error: true`, `cargo update --verbose`, then the gate commands) or a Dependabot/Renovate config; dependency bump PRs go through the normal gate and review. | Template `rust.yml` (`on: schedule`). | med-low | Otherwise the first sign of an incompatible release is the day someone runs `cargo update`. |
| CB-18 | config.html#buildwarnings (L6271-6288) and CI guide `--keep-going` example (L853-872) | `build.warnings = "deny"` fails the command; `--keep-going` shows warnings for all crates. | refine MOD-13 | Note in MOD-13: `build.warnings = "deny"` also covers rustdoc lints under `cargo doc`, so `RUSTDOCFLAGS=-D warnings` is redundant in CI on Cargo ≥ 1.97; pass `--keep-going` to CI build commands so one run reports every crate. | Template `rust.yml` doc step: drop `RUSTDOCFLAGS`; add `--keep-going` to clippy/check. Verified on 1.98.1: `CARGO_BUILD_WARNINGS=deny cargo doc` fails on unresolved intra-doc links with "warnings are denied by `build.warnings` configuration". | low | Smaller, simpler CI; fewer fix-and-rerun cycles. |
| CB-19 | guide/build-performance.html#reduce-amount-of-generated-debug-information (L1263-1293) | `[profile.dev] debug = "line-tables-only"`, `[profile.dev.package."*"] debug = false`, and a `debugging` profile that restores full debug info. | tooling (refine TST-15, PERF-09) | CONSIDER: adopt the book's dev profile in the template to speed agent build-test loops (backtraces keep file and line); full debug info via `--profile debugging`. It is a profile change, so H-16. | Template root `Cargo.toml`. | med-low | Agents rebuild constantly; link time and `target/` size dominate. |
| CB-20 | profiles.html#overflow-checks and default release profile (L5500-5509, L5615-5636) | Release builds default to `overflow-checks = false`: overflow wraps silently. | refine ERR-09 / PERF-09 (CONSIDER, H-16) | CONSIDER for money, balance and counter-heavy domains: `overflow-checks = true` in `[profile.release]` as a backstop, so arithmetic that escaped review fails loudly instead of producing a wrong balance. It does not replace ERR-09 (checked arithmetic stays mandatory). | Template `[profile.release]`; owner decision because it trades a silent wrong value for a panic. | med | The example workspace is a ledger; debug and release currently disagree on overflow (verified with the panic probe: `u32 + overflow`, `pow`, `abs(MIN)`, `<< 32`, `Iterator::sum` panic in debug and pass silently in release). |
| CB-21 | workspaces.html (L2650-2652); profiles.html (L5392-5394) | `[profile.*]` is read only from the workspace root manifest; member profiles are ignored. | refine PERF-09 | Put profiles only in the root manifest. | Review (one line in PERF-09). | low | Agents add `[profile.release]` to a member crate and believe it applies. |
| CB-22 | rust-version.html#uses (L2492, L2504); config.html#resolverincompatible-rust-versions overrides (L7000-7004); semver.html#env-new-rust (L12546-12576) | `--ignore-rust-version` bypasses the MSRV check; raising `rust-version` is conventionally a minor change but a policy decision. | refine MOD-01 + tooling | Never pass `--ignore-rust-version` to get green (H-12). When a dependency needs a newer Rust, raising `rust-version` is an H-16 decision recorded with the MSRV policy. | temper Bash hook: deny `--ignore-rust-version` next to the existing `-A`/`--cap-lints` checks. | med | The natural agent response to "package X requires rustc 1.99" is the flag the error message suggests. |
| CB-23 | resolver.html#dev-dependency-cycles (L4548-4569) | A dev-dependency that depends back on the crate under test links two copies of it into unit tests; their types are incompatible. | pattern (refine TST-14) | A shared test-support crate that depends on crate X is used only from X's `tests/` (integration tests), never from X's `#[cfg(test)]` unit tests; otherwise expose fakes behind a `test-support` feature. | Review. | low-med | Produces "expected `X::Id`, found `X::Id`" errors that agents fix with conversions or clones. |
| CB-24 | overriding-dependencies.html (L3578-3870) | `[patch]` is for temporary local or git forks; the patched version must match; delete it once the fix is published; local-only experiments can use `--config 'patch.crates-io.x.path=".."'`. | refine ARC-12 / H-16 | A committed `[patch]` is an H-16 decision, carries a tracking-issue comment and a removal condition, and is deleted once upstream releases the fix. Local experiments use `--config` and are not committed. | Hook ask (CB-4); `cargo deny` `unknown-git`. | low-med | Silent source swaps are supply-chain changes. |
| CB-25 | reporting-build-timings (L12875-12943) | `cargo build --timings` shows per-crate time and the critical path. | refine ARC-07 | A crate split justified by compile time cites `cargo build --timings` before and after. | Review. | low | ARC-07 allows "compile time" as a reason but asks for no evidence. |
| CB-26 | rust-version.html#setting-and-updating-rust-version, #verifying-rust-version (L2534-2590, L825-851) | Pick and document an MSRV policy (e.g. "N-2"); `cargo check` on the MSRV is usually sufficient. | refine MOD-01 | Record the MSRV policy (for example "latest stable minus 2, raised deliberately") in the root AGENTS.md; the MSRV CI job checks, it does not need to test. | Review. | low | Makes MOD-01's "raise it deliberately" checkable. |
| CB-27 | build-performance.html#removing-unused-dependencies (L1388-1421); lints.html (L12945-13510); unstable.html#direct-minimal-versions (L13722-13739) | Nightly `-Zcargo-lints` reports unused (workspace) dependencies and missing lints inheritance; `-Zdirect-minimal-versions` verifies lower bounds. | tooling (optional) | CONSIDER a weekly nightly job: `cargo +nightly check -Zcargo-lints --workspace --all-targets` (unused_dependencies, unused_workspace_dependencies, missing_lints_inheritance) and, for published crates, the minimal-versions check from CB-7. | Template `rust.yml` optional job. | low | Complements `cargo machete` (CRF-05) with Cargo's own analysis. |
| CB-28 | cargo-test.html#working-directory-of-tests (L19985-19998); environment-variables.html `CARGO_MANIFEST_DIR`, `CARGO_TARGET_TMPDIR` (L7576-7613) | Tests run with the package root as working directory (doctests compile from the workspace root); integration tests get a scratch dir in `CARGO_TARGET_TMPDIR`. | refine TST-10 | Locate fixtures with `Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/..")`, not paths relative to the shell's directory; keep `tempfile::tempdir()` per test for writable state. | Review. | low | Fixture paths that work in `cargo test` and break under `cargo test -p` from another directory, doctests, or nextest. |

### Conflicts

- **Build script examples use `println!` and `.unwrap()`** (build-scripts.html L7793-7802, L8358-8376, L8468-8483) vs H-02 and H-06 (rust-craft treats `build.rs` as production `bin` code; the scanner hard-blocks both, verified). Compatible nugget: keep every directive, but write them with `writeln!(io::stdout().lock(), ..)?` from a `main` that returns `Result` (pattern below, clippy- and scanner-clean).
- **"If using RUSTFLAGS to pass -Dwarnings, also add the -A flag ... such as -Adeprecated"** (semver.html#new-lints, L12640-12641) vs H-05 (never lower lint levels; the Bash hook denies `-A`). Nugget: expect `deprecated` warnings when a dependency bump lands and migrate off the deprecated API in the same bump PR.
- Not conflicts, but stricter than the book: the book allows mutually exclusive features guarded by `compile_error!` and suggests `cfg-if` (L5055-5077); rust-craft forbids mutually exclusive features (API-16) and prefers `cfg_select!` (MOD-12).

### Patterns / examples worth copying

**Build script under rust-craft lints** (clippy `-D warnings` and scanner clean on 1.98.1, verified in `scratchpad/tooling/clippyprobe`):

```rust
//! Build script: generates code into `OUT_DIR` only and reports failures as errors.
use std::error::Error;
use std::io::{self, Write};
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var_os("OUT_DIR").ok_or("OUT_DIR is not set by cargo")?;
    let schema = std::fs::read_to_string("schema/events.json")?;
    let generated = format!("/// Schema bytes.\npub const SCHEMA_LEN: usize = {};\n", schema.len());
    std::fs::write(Path::new(&out_dir).join("schema.rs"), generated)?;
    let mut out = io::stdout().lock();
    writeln!(out, "cargo::rerun-if-changed=schema/events.json")?;
    Ok(())
}
```

**Generated-file freshness test** (from L8390-8396): the generator is a plain function in an xtask or a `codegen` module; the test compares, never writes.

```rust
#[test]
fn checked_in_schema_is_fresh() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/events.json");
    let on_disk = std::fs::read_to_string(&path)?;
    let fresh = ledger_schema::render()?; // same code the xtask uses to write the file
    assert_eq!(on_disk, fresh, "run `cargo xtask codegen` and commit the result");
    Ok(())
}
```

**Per-package check step** (CB-2):

```yaml
      - name: Each crate builds alone (no feature unification, no dev-deps)
        run: |
          cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name' |
            xargs -I{} cargo check --locked -p {} --lib --bins
```

### Notable skips

- Registries, credential providers, source replacement and vendoring (L9308-10545, L3928-4074): relevant only to private registries or air-gapped builds.
- `links`, `*-sys` packages, native linking directives (L8190-8303): FFI, excluded by H-01.
- SemVer `repr` layout rules (L10749-11480): only observable through `unsafe`/FFI/transmute, all excluded by H-01/H-07.
- Unstable features except those listed in CB-7/CB-27 (build-std, cargo script, artifact deps, trim-paths, GenMC-style tooling).
- `[hints]`, `hint-mostly-unused`, cranelift, parallel frontend, alternative linkers (L1295-1359): nightly or environment-specific.

---

## 2. The std documentation front page, and verification of the panicking-API table

Source: https://doc.rust-lang.org/std/#what-is-in-the-standard-library-documentation. Read: all of `std-index.md` (804 lines: how to read the docs, what is in them, the tour, before/after `main`, primitives, modules, macros, keywords, attributes). Verification pass: std source from `rustup component add rust-src --toolchain 1.98.1` (installed for this pass; `rust-docs` is not installed, so `rustup doc --path` was unavailable), plus a probe binary run in debug and release (`scratchpad/tooling/panicprobe`) and a clippy probe under the template lints (`scratchpad/tooling/clippyprobe`).

### What the page itself says that agents should follow

- The docs link to source and "you are encouraged to read it" (L45-49).
- Methods on primitives are documented on the primitive pages (`char`, `str`, `slice`), not on the same-named modules (`std::char` holds only related types) (L60-75).
- Many `String` and `Vec<T>` methods are really `str` and `[T]` methods reached by deref coercion (L77-80).
- The prelude brings a small set of mostly-trait items into every module (L82-85); standard macros are in scope everywhere (L87-91).
- `format!` converts to strings, `FromStr` converts from strings (L137-138); `Rc` + `Cell`/`RefCell` and `Arc` + `Mutex` are the shared-mutability pairs (L140-144).
- Using std before or after `main` (for example from thread-local destructors) is best effort; `core` and `alloc` are more robust there (L162-184).
- The page lists nightly-only items with an "Experimental" badge next to stable ones (L261-267, L467-506, L632-645) and shows the stabilisation version in the header ("1.0.0 · Source", L3). It does **not** itself mention `# Panics`/`# Errors` sections; that convention is in the item pages (the 1.98.1 source has 437 `/// # Panics` and 150 `/// # Errors` sections in core/alloc/std).

### Already covered (summary)

`format!`/`FromStr` conversions → API-07. Rc/RefCell and Arc/Mutex → OWN-11, OWN-12 (rust-craft is stricter: justification required). Collections tour → PERF-06. Redundant integer modules (`std::u32::MAX`, "Deprecation planned") → `clippy::legacy_numeric_constants` (style, denied through `all`). `core::error::Error` for no_std → MOD-08.

### Candidates

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement | Value | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| STD-1 | #how-to-read-this-documentation ("Source" link, L45-49); item pages' `# Panics`/`# Errors` convention (437/150 sections in std 1.98.1 source) | Read the docs and the source of what you call. | refine PROC-01 | Before calling a std or dependency API on data the code does not control, read its `# Panics` and `# Errors` sections (and the source when the docs are vague). Any documented panic condition that input can reach is an H-03 finding unless a type rules it out. | Review; escape-hatches.md table (STD-7). | high | The clippy probe shows the template lints catch 4 of about 35 panicking calls (below); the rest are caught only by an agent that reads `# Panics`. |
| STD-2 | #what-is-in-the-standard-library-documentation points 1-2 and the `str`/`[T]` note (L60-80) | Primitive methods live on the primitive page; `Vec`/`String` inherit most methods from `[T]`/`str`. | refine PROC-01 | When checking a `Vec` or `String` method's panics, read the `[T]` or `str` page (for example `<[T]>::split_at`, `str::split_at`), and for `char`/integer methods read the primitive page, not the module. | Review. | med | Agents look up `std::vec::Vec` and conclude a method is undocumented or panic-free. |
| STD-3 | "Experimental" badges (L261-267, L467-506, L632-645) and the version header (L3) | Items show whether they are stable and since which version. | refine MOD-07 | Before using a std item, check that it is not "Experimental" (nightly-only) and that its "since" version is at or below `rust-version`. | `clippy::incompatible_msrv` (in `all`, denied) catches stable-but-too-new APIs; the compiler rejects experimental ones. Review for plans. | low-med | Agents reading nightly docs propose `std::random` or `hash_map!`, then burn a round on E0658. |
| STD-4 | Verified lint gap (clippy probe) | `clippy::indexing_slicing` flags only slice/array/`Vec` indexing. `h[k]` on `HashMap`, `m[&k]` on `BTreeMap` and `d[i]` on `VecDeque` are **not** flagged (verified on 1.98.1 under the template lints), yet all three panic (`HashMap`'s `Index` is literally `self.get(key).expect("no entry found for key")`, std `hash/map.rs:1516-1518`). `clippy::disallowed_methods` on `std::ops::Index::index` does not fire on `[]` syntax (verified). | tooling (refine H-04) | H-04 covers map and deque indexing: use `get`/`get_mut`/`entry`. State in rules.md that the lint does not cover them. | Scanner: add a soft rule (prod/bin/example; ignore tests) `map-index`, id H-04, `re: /(?:\b[A-Za-z_][A-Za-z0-9_]*\|[)\]])\[\s*&/g` (the `map[&key]` shape; tested: matches `balances[&account_id]`, `self.map[&key]`, `f()[&k]`; does not match `[&a, &b]`, `let [&first, ..] =`, `vec![&a]`, `&[&str]`, `matches!(v, [&1, ..])`). Review checklist line for `map[key]` with owned or `&str` keys. | high | H-04 claims lint enforcement that does not exist for the most common "known key" lookup in domain code (projections, read models). |
| STD-5 | Verified panics (probe) + `clippy::disallowed_methods` behaviour (verified) | `std::env::args()` panics when an argument is not UTF-8 (verified by running the probe with `$'a\xffb'`); `Duration::from_secs_f64`/`f32` panic on negative, NaN or overflowing input; `mul_f64`/`div_f64` likewise. | tooling (clippy.toml) | Add to `disallowed-methods`: `std::env::args` ("H-03: panics on non-UTF-8; use `args_os`"), `std::env::vars` ("use `vars_os`"), `std::time::Duration::from_secs_f64`, `from_secs_f32` ("use `try_from_secs_f64`/`f32`, 1.66"), `std::time::Duration::mul_f64`, `mul_f32`, `div_f64`, `div_f32` ("use `Duration::try_from_secs_f64(d.as_secs_f64() * k)`"). Keep `std::thread::spawn` and `RefCell::borrow_mut` review-only: `disallowed_methods` has no test exemption and tests use both. | `templates/workspace/clippy.toml` (protected file, H-05 approval). Verified that `disallowed_methods` fires for `std::env::args`, `std::time::Duration::from_secs_f64`, `std::cell::RefCell::borrow_mut` and `std::thread::spawn` paths. | med-high | Every example in the references already uses `args_os`, but nothing stops a new binary from using `args()`; float-seconds config values (timeouts) are the classic `from_secs_f64` panic. |
| STD-6 | Verified clippy behaviour | `clippy::unchecked_time_subtraction` (pedantic, on; formerly `unchecked_duration_subtraction`, renamed) flags `Duration - Duration` and suggests `a.checked_sub(b).unwrap()`, which H-02 denies. | refine (escape-hatches.md H-02/H-03) | When clippy suggests `checked_*(..).unwrap()`, write `checked_*(..).ok_or(Error::..)?` or the documented `saturating_*`. | Doc line in escape-hatches.md. | low-med | Agents apply clippy's suggestion verbatim and hit the next denial. |
| STD-7 | Verification of escape-hatches.md "Panicking APIs" table (below) | One correction, about 20 additions. | pattern (escape-hatches.md) | See the table below; each row gives the fallible counterpart and its stabilisation version from the 1.98.1 source. | Review; STD-4/STD-5 for the lintable subset. | high | The table is the only enforcement for panicking APIs the lints miss. |

### Lint coverage probe (template lints, 1.98.1)

Calling about 35 panicking std APIs with runtime arguments under the template's `[workspace.lints]` and `clippy.toml` produced only: `unchecked_time_subtraction` (Duration subtraction), `reversed_empty_ranges` (a literal `5..1`), `iterator_step_by_zero` (literal `0`), `missing_panics_doc` (an `assert!`), and `indexing_slicing` on slice/`Vec` only. **Not flagged**: `Vec::remove/insert/swap_remove/split_off/drain/swap`, `slice::split_at`, `chunks(n)`/`windows(n)` (even with a literal `0`), `String::insert/remove/truncate`, `Duration + Duration`, `Instant ± Duration`, integer `/`, `%`, `pow`, `abs`, `div_ceil`, map and deque indexing, `RefCell::borrow_mut`, `char::from_digit(x, 37)` (literal radix), `env::args`, `Iterator::sum`, `f64::clamp`, `Duration::from_secs_f64`, `thread::spawn`, `u32::from_str_radix`. For `chunks_exact(0)` clippy's `chunks_exact_to_as_chunks` suggests `as_chunks::<0>()`, which also panics.

Rejected tooling: `clippy::panic_in_result_fn` (restriction) would catch `assert!` in `Result`-returning production functions, but it also fires inside `#[cfg(test)]` `#[test] fn t() -> Result<..>` bodies that use `assert_eq!` (verified), which conflicts with TST-04's test style. Keep the scanner's soft `assert-in-prod` rule.

### Verification of escape-hatches.md "Panicking APIs" table

"Verified" means the probe panicked (debug and release unless noted) and the `# Panics` doc says so in the 1.98.1 source.

**Existing rows**

| Row | Verdict |
|---|---|
| `a + b`, `a * b`, `a - b` on integers | Correct (debug panics, release wraps; verified). Add `pow`, unary `-`, `abs()` on `MIN`, `<<`/`>>` by ≥ bit width: same debug-only behaviour (verified). |
| `strict_*` (1.91) | Correct (`strict_overflow_ops`, since 1.91.0; panics in release too, verified). |
| `Instant + Duration`, `Instant - Instant`, `Duration - Duration` | **Correction**: `Instant - Instant` does **not** panic on current std: `duration_since`, `elapsed` and `Sub` saturate to zero (std `time.rs:294-295`, `465-466`: "Previous Rust versions panicked ... Currently this method saturates. Future versions may reintroduce the panic in some circumstances"; verified). Keep the row but say: use `saturating_duration_since` (1.39) or `checked_duration_since` (1.39) to make the intent explicit. `Instant + Duration`, `Instant - Duration` (`checked_add`/`checked_sub`, 1.34) and `Duration - Duration` (`checked_sub` 1.16, `saturating_sub` 1.53) do panic, in release too (verified). |
| `split_at`, `remove`, `insert`, `swap`, `drain` | Correct (verified). `split_at_checked` is 1.80. Note `Vec::insert(len, x)` is valid (appends); `Vec::truncate(n > len)` is a no-op (both verified). |
| `chunks(0)`, `windows(0)`, `as_chunks::<0>()`, `step_by(0)` | Correct (verified). `as_chunks::<0>` panics at run time (`assert!(N != 0)` in `slice/mod.rs:1400`), it is not a compile error. Add `array_windows::<0>()` (1.94, same runtime assert, verified) and `chunks_exact(0)`/`rchunks(0)`. |
| `RefCell::borrow_mut` | Correct (verified). Add `RefCell::borrow` while mutably borrowed → `try_borrow` (1.13). |
| `HashMap::get_disjoint_mut` overlapping keys | Correct (doc example `should_panic`, `hash/map.rs:1157-1163`). |
| tokio `spawn`/`Handle::current()` outside a runtime, `tracing_subscriber` `init()` | Not std; not re-verified here. |
| `mutex.lock().unwrap()`, `String::from_utf8(..).unwrap()` | Correct (these are H-02 shapes). |
| Brief's candidate `Rc::try_unwrap` | Does not panic (returns `Err` when shared, verified); only `.unwrap()` on it is H-02. No row needed. `Option::unwrap_or_default` does not panic (verified). `Path::display` does not panic: it is lossy (replaces invalid UTF-8 with U+FFFD, verified); use `to_str()` when losslessness matters. |

**Additions** (all verified to panic on 1.98.1)

| Panics | When | Use instead (since) |
|---|---|---|
| `a / b`, `a % b` on integers | `b == 0`, and `MIN / -1` — **in release too** | `checked_div`/`checked_rem` (1.0), or a `NonZero*` divisor type |
| `ilog2`, `ilog10`, `ilog` | argument `0` (or base < 2), release too | `checked_ilog2`/`checked_ilog10`/`checked_ilog` (1.67) |
| `iN::isqrt` | negative input, release too | `checked_isqrt` (1.84) |
| `uN::div_ceil`, `uN::next_multiple_of` | divisor `0`, release too | `checked_div` first, `checked_next_multiple_of` (1.73) |
| `div_euclid`, `rem_euclid` | divisor `0`, release too | `checked_div_euclid`/`checked_rem_euclid` (1.38) |
| `Iterator::sum`/`product` of integers | overflow (debug; wraps in release) | `try_fold(0, u64::checked_add)` (1.27) returning `Option`, or `fold(0, u64::saturating_add)` when saturation is the documented meaning |
| `Iterator::sum::<Duration>()` | overflow, **release too** (`checked_add(..).expect("overflow in iter::sum over durations")`, `core/src/time.rs:1350`) | `try_fold(Duration::ZERO, Duration::checked_add)` |
| `Duration + Duration`, `Duration * n`, `Duration / 0` | overflow or zero divisor, release too | `checked_add`/`checked_mul`/`checked_div` (1.16), `saturating_add`/`saturating_mul` (1.53) |
| `Duration::from_secs_f64`/`f32` | negative, NaN or overflow | `try_from_secs_f64`/`try_from_secs_f32` (1.66) |
| `Duration::mul_f64`/`div_f64` (and `f32`) | result negative, NaN or overflow | `Duration::try_from_secs_f64(d.as_secs_f64() * k)` (1.66) |
| `Duration::from_mins`/`from_hours` (1.91), `Duration::new(secs, nanos)` | overflow (nanos carry for `new`) | `minutes.checked_mul(60).map(Duration::from_secs)` |
| `SystemTime + Duration` | overflow | `SystemTime::checked_add` (1.34) |
| `Vec::swap_remove`, `split_off`, `extend_from_within`, `splice` | index or range out of bounds | length check or `get`; for untrusted indices, a checked helper returning `Option` |
| `Vec::with_capacity`, `reserve` | capacity over `isize::MAX` bytes | `try_reserve`/`try_reserve_exact` (1.57) after capping the length (PERF-10) |
| `slice::copy_from_slice`/`clone_from_slice` | lengths differ | compare lengths first, or `iter_mut().zip(src)` |
| `slice::rotate_left`/`rotate_right`, `select_nth_unstable` | `k > len`, `index >= len` | bound check first |
| `String::insert`, `insert_str`, `remove`, `truncate`, `split_off`, `drain`, `replace_range`; `str::split_at` | index not on a `char` boundary (or past the end) | `str::is_char_boundary` (1.9), `floor_char_boundary`/`ceil_char_boundary` (1.91), `split_at_checked` (1.80), or `char_indices` |
| `HashMap`/`BTreeMap` `map[&k]`, `VecDeque` `d[i]` | missing key or index | `get`/`get_mut`/`entry` (see STD-4: not linted) |
| `BTreeMap::range`/`BTreeSet::range` | `start > end`, or `start == end` with both bounds `Excluded` — **only when the map is non-empty** (verified: an empty map returns an empty iterator, a 10-entry map panics) | check `from < to` before calling (pattern below). Tests with empty fixtures pass; production with data panics. |
| `slice::sort`, `sort_by`, `sort_unstable_by` (and `_by_key`) | "May panic if the implementation of `Ord` ... does not implement a total order" (`slice/mod.rs:3090-3176`, `alloc/src/slice.rs:75-106`). Not reproduced in 16 adversarial runs on 1.98.1, so treat as documented, input-dependent | `f64::total_cmp` (1.62) for floats; never `partial_cmp(..).unwrap_or(..)` comparators |
| `Ord::clamp`, `f64::clamp` | `min > max`, or a NaN bound (floats) | validate the bounds once, at config parse, into a type that guarantees `min <= max` |
| `char::from_digit` (radix > 36), `char::to_digit`/`is_digit` and `uN::from_str_radix` (radix < 2 or > 36) | bad radix, release too; clippy does not flag even a literal `37` | a named radix constant in `2..=36` |
| `std::env::args()`, `std::env::vars()` | an argument or variable is not UTF-8 (iteration panics) | `args_os`, `vars_os` (1.0); convert with `into_string()` and report (STD-5) |
| `thread::spawn`, `thread::Scope::spawn` | the OS cannot create a thread | `thread::Builder::new().spawn(..)` (1.0) or `.spawn_scoped(..)` (1.63), which return `io::Result` |
| `format!`, `.to_string()` | a `Display` impl returns `Err` that the formatter did not produce (`alloc/src/string.rs:2943`, verified) | `Display` impls only propagate `fmt::Error` from `write!`; validation belongs in the constructor (API-03) |
| (tokio, verified in the local registry copy of 1.53.1; outside the std scope) `tokio::sync::mpsc::channel(0)`, `broadcast::channel(0)`, `tokio::time::interval(Duration::ZERO)` | zero capacity or period | carry capacities and periods in `NonZeroUsize` / a validated interval type from config |

### Patterns / examples worth copying

Clippy `-D warnings` clean under the template lints and behaviour-tested (`scratchpad/tooling/clippyprobe`, 3 tests pass):

```rust
/// Events after `after` up to and including `upto`. `BTreeMap::range` panics on an
/// inverted range once the map holds data, so an inverted request yields nothing.
pub fn events_between<E>(
    log: &BTreeMap<Version, E>,
    after: Version,
    upto: Version,
) -> impl Iterator<Item = (&Version, &E)> {
    let bounds = (after < upto).then_some((Bound::Excluded(after), Bound::Included(upto)));
    bounds.into_iter().flat_map(move |b| log.range(b))
}

/// `from_secs_f64` panics on negative, NaN or huge input; `try_from_secs_f64` reports it.
pub fn timeout(seconds: f64) -> Result<Duration, ConfigError> {
    Duration::try_from_secs_f64(seconds)
        .map_err(|source| ConfigError::InvalidTimeout { seconds, source })
}

/// `thread::spawn` panics if the OS refuses a thread; `Builder::spawn` returns the error.
pub fn start_worker() -> std::io::Result<JoinHandle<u64>> {
    thread::Builder::new().name("worker".to_owned()).spawn(|| 42)
}
```

### Conflicts

None. The page presents `Rc<RefCell<_>>`/`Arc<Mutex<_>>` as the common sharing pairs; rust-craft is stricter (OWN-11 justification) but not contradicted.

### Notable skips

- Before/after-`main` caveats (L162-184): only relevant to thread-local destructors or at-exit code, which ARC-14's explicit shutdown already avoids.
- `core`/`alloc` layering: the page mentions it only in the before/after-`main` note. The restriction lints `std_instead_of_core`, `std_instead_of_alloc`, `alloc_instead_of_core` (all verified `ok`, need enabling) matter only if a domain crate targets `no_std`; not proposed.

---

## 3. Miri

Source: https://github.com/rust-lang/miri (cloned at `scratchpad/res/miri`). Read: README.md in full (699 lines); `doc/genmc.md` in full; `doc/tracing.md` (headings and intro: it is for profiling Miri itself); `ci/ci.sh` and `ci/build-all-targets.sh` in full; the Cargo Book's `cargo-miri(1)` page (L18882-18904).

### What Miri adds to a codebase that forbids `unsafe` (verified, not assumed)

| Capability | Relevance under H-01 | Evidence |
|---|---|---|
| UB in dependencies (out-of-bounds, use-after-free, uninitialised reads, invalid values, alignment, aliasing) | Real: our tests drive `unsafe` inside dependencies (serde_json's number formatting, memchr, allocator paths). The README's bug list includes `regex`, `rand`, `once_cell`, `crossbeam`, `arc-swap`, `oneshot`, `mockall`. It can only find UB on paths our tests exercise. | README L7-24, L604-671. Run 5 below exercised serde_json/tempfile/insta paths: no UB found. |
| Memory leaks | Real in safe code: `Rc` cycles (OWN-11 graphs). | Verified: an `Rc<RefCell<Option<Rc<_>>>>` cycle in a test is reported as `error: memory leaked` although the test itself "passed". README L25-27. |
| Detached threads | Enforces ASY-04/H-13 for std threads in tests. | Verified: `thread::spawn` without join → `error: the main thread terminated without waiting for all remaining threads`. |
| Deadlocks | Real in safe code (lock order). Natively the test hangs until a timeout. | Verified: two-mutex lock-order inversion → `error: the evaluated program deadlocked`. |
| Data races | Safe code cannot race; only `unsafe` in dependencies can. Weak-memory emulation can surface wrong `Ordering` choices in safe atomics code (ASY-10) as test failures. | README L18-19, L76-79. `cargo miri test` detects races between tests sharing state; `cargo miri nextest run` does not (README L200-202). |
| Schedule exploration | `-Zmiri-many-seeds=0..N` re-runs with different scheduling and allocation choices. | Verified: `-Zmiri-many-seeds=0..4` on `ledger-app` integration tests, 8.7 s. |
| Determinism probe | Isolation (default) gives a fake clock, no host env and no file system: any test in a pure crate that touches them fails loudly (a TST-02/TST-10 smell detector). | Verified: default isolation aborted the domain crate because proptest's failure persistence calls `getcwd`, and the store tests because `tempfile` calls `mkdir`. |
| Panics | Same as a debug build; nothing extra. | — |

**Costs** (measured on this 2-CPU box): nightly only (Cargo Book L18893, README L102-109); `cargo miri setup` builds a sysroot (37.6 s, cached in `~/.cache/miri`); interpretation is 3-4 orders of magnitude slower (the domain crate's 31 tests take 0.12 s natively; with proptest cut to 8 cases they take 71-90 s under Miri; with the default 256 cases a single property test took about 16 minutes); isolation blocks the file system and real clock unless `-Zmiri-disable-isolation`; no process spawning even with isolation off (`posix_spawn`/`socketpair` unsupported: CLI tests using `CARGO_BIN_EXE_*` and `insta`'s internal `cargo metadata` call fail); no networking, no FFI (README L69-75); one execution per seed, so it proves nothing about soundness (README L81-87); `cargo miri nextest run` recompiles the test crate once per test (README L192-195).

### Candidates

| # | Source (section/item, with anchor or item number) | Guidance (the source's point, 1-2 lines) | Status | Proposed rule text (imperative, testable; MUST/SHOULD) | Enforcement | Value | Why it matters for rust-craft |
|---|---|---|---|---|---|---|---|
| MIRI-1 | README "Running Miri on CI" (L222-252), "Testing multiple different executions" (L206-220), flags (L319-414) | Install nightly with the miri component, run `cargo miri setup` separately, then `cargo miri test`; use many seeds to explore schedules. | tooling | Optional scheduled CI job (exact YAML below); failures are real findings, never silenced with unsound flags. | Template `rust.yml`. | med | Gives the owner a ready, measured switch instead of a research task. |
| MIRI-2 | README `#[cfg_attr(miri, ignore)]` (L127-137) vs rust-craft "`#[ignore]` needs a reason" | Miri-incompatible tests are ignored with `cfg_attr`. | tooling (scanner, H-12) | The scanner's `ignore-no-reason` rule MUST also catch reasonless `#[cfg_attr(<pred>, ignore)]`. Replace its regex with `/#\s*\[\s*(?:cfg_attr\s*\([^\]]*?,\s*)?ignore\s*(?=[\],)])/g`. | `scripts/lib/scan.mjs` RULES `ignore-no-reason`. Verified against the real scanner: the current regex reports 0 hits for `#[cfg_attr(miri, ignore)]`; the proposed regex flags it and passes `#[cfg_attr(miri, ignore = "..")]` and `#[ignore = ".."]` (11/11 cases incl. nested `any(miri, all(unix, not(..)))`). Clippy's `ignore_without_reason` (pedantic, on) sees the attribute only when `cfg(miri)` is active, i.e. never in the stable gate (verified). | high | Without it, adopting Miri opens a silent way to disable any test (`cfg_attr(any(), ignore)` works too). |
| MIRI-3 | README L127-147 (explicit "unsupported operation" errors) | Tests that need what Miri cannot do are ignored under `cfg(miri)`. | new TST (TST-17) | A test that Miri cannot run (process spawning, network, FFI) carries `#[cfg_attr(miri, ignore = "<what Miri cannot do>")]`. Never remove tests from Miri with `#![cfg(not(miri))]` or a `cfg` on the module: ignored tests stay visible in the count. Never put `cfg!(miri)`/`cfg(miri)` in production code. | Scanner (MIRI-2); review. Verified: the six `ledger-cli` end-to-end tests show as `ignored, Miri cannot spawn processes; ...` and stable clippy `-D warnings` stays green with the attribute. | med | Keeps H-12 intact while making a Miri job feasible. |
| MIRI-4 | README "Common problems" and `-Zmiri-env-set` (L345-351); proptest config env vars (verified in proptest 1.11 `config.rs`: `PROPTEST_CASES`, `PROPTEST_DISABLE_FAILURE_PERSISTENCE`) | Shrink expensive tests under Miri. | pattern (TST) | Shrink property tests for Miri from the CI job, not the code: `MIRIFLAGS="-Zmiri-env-set=PROPTEST_CASES=8 -Zmiri-env-set=PROPTEST_DISABLE_FAILURE_PERSISTENCE=1"`. When a test needs a smaller loop, use a test-only constant `const ROUNDS: u32 = if cfg!(miri) { 16 } else { 10_000 };` inside `#[cfg(test)]` code only. | CI job env; review. Both variants verified (see code below). | med | Without shrinking, one 256-case property test costs ~16 minutes under Miri; with it the whole example workspace runs in 1.5 minutes. |
| MIRI-5 | README L25-27 (leaks), `-Zmiri-ignore-leaks` "also allows some remaining threads" (L352-353); verified probes | Miri turns leaks, dangling threads and deadlocks into errors. | refine ASY-04, H-13, OWN-11 | A crate that spawns threads, uses locks or builds `Rc` graphs SHOULD run its tests under Miri with `-Zmiri-many-seeds=0..16`; a Miri leak, "main thread terminated without waiting" or deadlock report is a blocker under ASY-04/H-13/OWN-11. | Optional CI step (YAML). | med | Today ASY-04 ("every task is owned") has review-only enforcement for std threads. |
| MIRI-6 | README isolation (L39-46, L337-340) | Isolation replaces host clock, env, entropy and FS with deterministic fakes. | refine TST-10 / TST-02 | Run domain and application crates under Miri with isolation on: a unit test there that needs the file system, environment or processes is testing through the wrong seam. Adapters run with `-Zmiri-disable-isolation`. | CI job splits the two groups (YAML). | med-low | Cheap mechanical signal for "pure" crates staying pure (ARC-02, ARC-04). |
| MIRI-7 | README L81-87 ("Miri fundamentally cannot ensure that your code is sound ... When Miri finds UB, your code is definitely unsound") | Miri is a necessary, not sufficient, check for unsafe code. | refine H-01 / H-05 exception protocol | If a human approves an `unsafe` exception (a `-sys` or FFI-free unsafe crate), that crate MUST have a Miri job with `-Zmiri-many-seeds` and tests that exercise every unsafe block; FFI calls cannot be interpreted and need `cfg_attr(miri, ignore = "FFI")` plus separate review. | Exception checklist in escape-hatches.md. | med (conditional) | Gives the exception protocol a concrete verification step. |
| MIRI-8 | README "Cross-interpretation" (L149-160) | `cargo miri test --target s390x-unknown-linux-gnu` runs tests as big-endian. | refine (CONSIDER) | CONSIDER for crates with binary codecs or byte-level parsing: one Miri run on `s390x-unknown-linux-gnu` to catch `to_ne_bytes`/endianness assumptions. | Optional CI step. | low | JSON-only workspaces do not need it. |

### Proposed optional CI job

Append to `templates/workspace/.github/workflows/rust.yml` (and add `schedule:` / `workflow_dispatch:` to `on:`). The job is off for pushes and PRs; it runs weekly or on demand.

```yaml
on:
  push:
    branches: [main]
  pull_request:
  schedule:
    - cron: "17 4 * * 1"      # weekly, Monday 04:17 UTC
  workflow_dispatch:

jobs:
  # ... gate and msrv unchanged ...

  miri:
    # Optional (see "When to turn it on"). Nightly-only and about 1000x slower than native tests.
    if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'
    runs-on: ubuntu-latest
    timeout-minutes: 60
    env:
      # Nightly adds lints before stable does; keep this job about UB, not lint churn.
      CARGO_BUILD_WARNINGS: warn
      MIRI_TOOLCHAIN: nightly-2026-09-28    # pin; bump in the same PR as rust-toolchain.toml
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: nightly-2026-09-28
          components: miri, rust-src
      - uses: Swatinem/rust-cache@v2
        with:
          key: miri
      - run: cargo +$MIRI_TOOLCHAIN miri setup
      - name: Pure crates (isolation on)
        run: cargo +$MIRI_TOOLCHAIN miri test --locked --no-fail-fast -p ledger-domain -p ledger-app -p ledger-cli
        env:
          MIRIFLAGS: >-
            -Zmiri-env-set=PROPTEST_CASES=8
            -Zmiri-env-set=PROPTEST_DISABLE_FAILURE_PERSISTENCE=1
      - name: Adapter crates (file system needed)
        run: cargo +$MIRI_TOOLCHAIN miri test --locked --no-fail-fast -p ledger-store-jsonl
        env:
          MIRIFLAGS: >-
            -Zmiri-disable-isolation
            -Zmiri-env-set=PROPTEST_CASES=8
            -Zmiri-env-set=PROPTEST_DISABLE_FAILURE_PERSISTENCE=1
            -Zmiri-env-set=INSTA_WORKSPACE_ROOT=${{ github.workspace }}
            -Zmiri-env-set=INSTA_UPDATE=no
      # Enable per crate that spawns threads, uses locks, atomics or Rc graphs (MIRI-5).
      # - name: Thread schedules
      #   run: cargo +$MIRI_TOOLCHAIN miri test --locked -p <crate>
      #   env:
      #     MIRIFLAGS: -Zmiri-many-seeds=0..16 -Zmiri-env-set=PROPTEST_CASES=4 -Zmiri-env-set=PROPTEST_DISABLE_FAILURE_PERSISTENCE=1
```

Notes: `cargo +<toolchain>` overrides the repository's `rust-toolchain.toml` pin (verified locally: the copy pins 1.98.1 and `cargo +nightly miri test` ran on nightly). The dated nightly is the dist date rustup reported for the build used here ("latest update on 2026-09-28 for version 1.101.0-nightly (d080e7dff 2026-09-27)"). `INSTA_WORKSPACE_ROOT` is read at run time by insta 1.48 (`env.rs:459`) and skips the `cargo metadata` subprocess Miri cannot spawn. Never add `-Zmiri-ignore-leaks`, `-Zmiri-disable-stacked-borrows`, `-Zmiri-disable-validation`, `-Zmiri-disable-data-race-detector` or `-Zmiri-disable-alignment-check` to get green: the README marks several of them unsound (L416-458), and they weaken verification (H-12). Prefer `cargo miri test` over `cargo miri nextest run` for these small crates (README L192-202). Package selection with `-p` must list every library crate; a new crate needs a line here (or switch to `--workspace` once every crate passes with isolation off).

### Test idioms (compile- and Miri-verified in `scratchpad/tooling/ptcfg`: stable `cargo test` 3 passed; `cargo miri test` 2 passed, 1 ignored)

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    /// Miri interprets every case; keep the property meaningful but cheap there.
    const CASES: u32 = if cfg!(miri) { 8 } else { 256 };
    /// Loop-heavy example tests shrink the same way.
    const ROUNDS: u32 = if cfg!(miri) { 16 } else { 10_000 };

    /// Fewer cases under Miri, and no failure-persistence file (isolation blocks the file system).
    fn miri_aware_config() -> ProptestConfig {
        let base = ProptestConfig::with_cases(CASES);
        if cfg!(miri) { ProptestConfig { failure_persistence: None, ..base } } else { base }
    }

    proptest! {
        #![proptest_config(miri_aware_config())]
        #[test]
        fn add_commutes(a in any::<u32>(), b in any::<u32>()) {
            prop_assert_eq!(a.wrapping_add(b), b.wrapping_add(a));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
    fn spawns_a_process() { /* ... */ }
}
```

`cfg(miri)` is a well-known cfg name: no `unexpected_cfgs` warning on 1.98.1 (verified).

### When an owner should turn it on

Turn the job on (schedule, or per PR via `workflow_dispatch` when needed) when any of these holds:

1. A human approved an `unsafe` exception anywhere in the workspace (then it is MUST, MIRI-7).
2. A crate spawns threads, holds locks across function boundaries, uses atomics, or builds `Rc`/`Arc` graphs (leaks, deadlocks, dangling threads, weak-memory ordering bugs).
3. A new dependency with non-trivial `unsafe` sits on a hot path the tests exercise (parsers, codecs, allocators, lock-free collections), or `Cargo.lock` changes such a dependency's major version.
4. The team wants a mechanical purity check for domain crates (isolation on).

For a workspace like the example (single-threaded, safe, JSON I/O), it found nothing and costs about 1.5 minutes per run with the shrink settings: keep it scheduled weekly at most.

### Run results on a copy of `templates/example-workspace`

| Run | Settings | Wall time | Result |
|---|---|---|---|
| 0 | `cargo +nightly miri setup` | 37.6 s | sysroot built |
| 1 | defaults (isolation on), `--workspace --no-fail-fast`, unmodified copy | 1m13s incl. build | 17 passed; 3 targets aborted: `ledger-domain` unit tests (proptest failure persistence → `getcwd` unsupported under isolation), `ledger-store-jsonl` `it` (`tempfile` → `mkdir`), `ledger-cli` `it` (`tempfile` → `mkdir`). Miri aborts the whole test binary on the first unsupported operation. |
| 2 | `-Zmiri-disable-isolation`, default 256 proptest cases, unmodified copy | killed by the 25-minute timeout (exit 124) | `ledger-app` 6 and `ledger-cli` unit 8 passed; `ledger-cli` `it` aborted (`posix_spawnattr_init`: process spawning is unsupported even without isolation); in `ledger-domain` the first 256-case property test took about 16 minutes and the second was still running at the timeout. |
| 3 | isolation on, `-Zmiri-env-set=PROPTEST_CASES=8 -Zmiri-env-set=PROPTEST_DISABLE_FAILURE_PERSISTENCE=1`, `--locked`; copy with `#[cfg_attr(miri, ignore = "...")]` on the 6 CLI end-to-end tests | 1m52s incl. build | 48 passed, 6 ignored; `ledger-store-jsonl` `it` aborted (`mkdir`). Domain crate: 31 passed in 90.3 s. |
| 4 | as 3 plus `-Zmiri-disable-isolation` | 1m27s | 5 store tests passed, then `format::writes_one_current_schema_record_per_line` aborted: `insta` spawns `cargo metadata` (`socketpair` unsupported). |
| 5 | as 4 plus `-Zmiri-env-set=INSTA_WORKSPACE_ROOT=<workspace> -Zmiri-env-set=INSTA_UPDATE=no` | 1m26s | **Green: 58 passed, 6 ignored, 0 failed** (app `it` 6, cli unit 8, domain 31 in 71.5 s, store `it` 10, doctests 3). No UB, leaks or data races reported. |
| 6 | `-Zmiri-many-seeds=0..4 -p ledger-app` | 8.7 s | 4 seeds × 6 tests passed. |
| native | `cargo +1.98.1 test --workspace --locked` (built) | 0.64 s | all pass (domain 31 tests in 0.12 s with 256 proptest cases). |

Safe-code probes (`scratchpad/tooling/miriprobe`, Miri default flags): `Rc` cycle → `memory leaked`; unjoined `thread::spawn` → `the main thread terminated without waiting for all remaining threads`; two-mutex lock-order inversion → `the evaluated program deadlocked`.

### Conflicts

- **README's `#[cfg_attr(miri, ignore)]`** (L132-134) has no reason, which H-12 and the scanner's `#[ignore]`-needs-a-reason rule forbid. Nugget: the same attribute with `ignore = "<why>"` (MIRI-3), plus the scanner fix (MIRI-2) so the reasonless form cannot slip in.
- Policy note rather than a source conflict: the README documents flags that make Miri miss bugs (`-Zmiri-ignore-leaks`, `-Zmiri-disable-*`); using them to get a green job would weaken verification (H-12).

### Notable skips

- GenMC mode (`doc/genmc.md`): work in progress, requires building Miri from source, no `cargo miri test` support.
- `doc/tracing.md`: profiling Miri itself.
- `-Zmiri-native-lib` (README L464-481): experimental and unsound, and FFI is excluded by H-01.
- Tree Borrows and other aliasing-model flags: only matter for code with raw pointers.

---

## Counts by status (all three resources)

| Status | Cargo Book | std | Miri | Total |
|---|---|---|---|---|
| refine | 15 (CB-5,6,7,8,9,11,13,18,20,21,22,24,25,26,28) | 4 (STD-1,2,3,6) | 4 (MIRI-5,6,7,8) | 23 |
| new | 4 (CB-2 ARC, CB-12 PROC, CB-14 ARC, CB-16 API) | 0 | 1 (MIRI-3 TST) | 5 |
| pattern | 2 (CB-15, CB-23) | 1 (STD-7: 1 correction, ~20 additions) | 1 (MIRI-4) | 4 |
| tooling | 7 (CB-1,3,4,10,17,19,27) | 2 (STD-4, STD-5) | 2 (MIRI-1, MIRI-2) | 11 |
| conflict | 2 | 0 | 1 (+1 policy note) | 3 |
| skip (notable) | 5 | 2 | 4 | 11 |
