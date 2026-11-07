# Crate charter: `temper-domain`

## Purpose
temper's pure policy: the masking lexer and scanner rules, the manifest policy, config and glob matching, the verdict and fingerprint rules, the findings, baseline, ratchet and report models with their renderers, and every message text the hooks print (port plan §1.3). Phase 1a brought the scanner's lexer, test regions, counts and first six rules, Phase 1b 40 more rules with their look-around and keep predicates, Phase 1c the finders and the last regex rules (58 of 60), Phase 1d the doc rules (60 of 60), path classification and the new-hit difference, Phase 2a the manifest policy, the config and glob matching, and Phase 2b the hook protocol types (events, outcomes, the skill marker rule and telemetry rows), `Timestamp` and the JS value model read from JSON; the rest arrives phase by phase.

## Layer
domain.

## Allowed dependencies
- Workspace crates: none.
- Third-party, each added by the phase that first uses it (D10, wi-b03273): `regex`, `toml` (features `std`, `parse`, `preserve_order`), `sha2`, `serde`, `serde_json`, `thiserror`. `serde` is in since Phase 1d.2 with `std` only: `Hit`'s `Serialize` is hand-written (1d-12), and `derive` comes with the first derived DTO. `toml` and `serde_json` are in since Phase 2a; `toml` is read through `toml::de::DeTable`, since `toml::Table` needs the `serde` feature D10 does not list. Dev only: `proptest`, `insta`.

## Public surface
Re-exported from `lib.rs`, one path each (API-14):
- `Scanner` (`new`, `scan`), `ScanOptions` (`kind`, `file`, `forbid`), `ScanReport` (`hits`, `into_hits`, `tests`, `ignores`, `clones`), `ScannerSetupError` (`rule`, `source`).
- `Hit` (`key`, `id`, `severity`, `line`, `text`, `msg`, `fix`, `exception`, and `Serialize` in 1.x's key order), `HitAt` (`hit`, `file`; `Serialize` appends `file`), `Severity`, `RuleKey`, `RuleId`, `LineNo`.
- `CodeKind`, `FileRole`, `classify_path` with `LibTarget` (`Present`, `Absent`), `is_build_script`.
- `new_hits`.
- `format_hit`, `render_scan_text` and `NOT_A_RUST_PROJECT`: `temper scan`'s texts.
- `PathStyle` (`Windows`, `Posix`), `unverbatim` and `js_trim`.
- `Manifest` (`FromStr`; since 2c `has_package`, `has_workspace`, `package_workspace`, `workspace_excludes`), `ManifestParseError`; `member_problems` with `GoverningRoot`, `RootRelation`, `OwnLints` and `ManifestProblem` (`id`, `key`, `msg`, `fix`, `new_only`); `split_problems` and `SplitProblems`; `watched_keys`, `WatchedKey` and `benign_when_new`; `dependency_names`, `dep_names`, `DepName` and `DepKind`; `dep_requirements`, `major_of` and `non_caret_requirement`; `lint_sections` and `lint_config_changed`; `manifest_checks` with `ManifestEditFacts`, `Before` and `ManifestFindings`.
- `Config` (`load`, `defaults` and a getter per key), `ConfigLoad`, `ConfigProblem` (`NotJson`, `NotAnObject`, `WrongType`), `ReviewMode`, `GateConfig` with `CargoFeatures` and `Switch`, `ExemptRule`.
- `Glob`, `Globs` and `GlobError` (`pattern`).
- The hooks (2b): `HookEvent` (`FromStr`, `name`), `UnknownHookEvent` (`name`), `ContextEvent`, `HookOutcome` (`Pass`, `Context`, `Notice`, since 2c `Deny` and `Ask`; `input_unparseable`, `handler_failed`, `to_json`), `SKILL_NOT_LOADED`, `SkillVia` and `skill_load_via`, `TelemetryEvent` and `TelemetryRow` (`to_json`, 1.x key order).
- `pre-edit`'s policy (2c): `EditOp` with `EditStep` and `Replacements`, and `apply_edit`; `edit_policy` over `EditFacts` (`EditTool`, `EditTarget` with `RustEdit`, `Exemptions`, `ManifestEdit` and `DependencyCheck`), giving a `PolicyDecision` (`outcome`, `telemetry`); the texts `STATE_DIR_DENIAL`, `lockfile_denial`, `IRON_RULES` and `skill_gate_denial`; `skill_preloaded`.
- `pre-bash`'s shell-independent checks (2d): `BashPolicy` (`new`, then `rust_source_write`, `cargo_or_baseline_write`, `verification_bypass` and `dependency_or_snapshot`, each giving `Option<PolicyDecision>` in 1.x's order), `PolicySetupError` (`rule`), `RaiseFlag`, and the telemetry variants `DenyShellWrite` with `ShellWriteFile`, `DenyShellLockfile`, `DenyBaselineRaise`, `DenyIgnoreRustVersion` and `ShellAsk` with `ShellAskVia`.
- `pre-bash`'s shell branches (2e): `BashPolicy::state_dir_write`; `ConfigAsk` (`new` over `ConfigChanges`, `telemetry`, `before`, `into_outcome`), the protected-configuration ask that goes before the dependency or snapshot ask; `ShellWriteFile::StateDir`, and `TelemetryEvent::ProtectedAsk` with `ProtectedChange` (`Written`, `Deleted`, `UnreadPatch`).
- `pre-bash`'s last branches (2f): `InterpreterCode` (`new`, `runs_patch`, `write_targets`; `BashPolicy::code`), 1.x's patterns for code an interpreter runs; `is_reviewer`, `BashPolicy::reviewer_patch` and `reviewer_opaque` with `OpaqueKind`; `RustWrites` (`check` over a `CopiedFile`, `unread` with `Unread`, `patched`, `direct`) and `BashPolicy::rust_writes`, which decides what a command's copies, moves, links and patches onto `.rs` files add up to; and the telemetry variants `DenyShellWriteVia` with `ShellDenyVia`, `DenyShellCopyScan` and `ShellCopyAsk`.
- `Timestamp` (`from_unix_millis`, `unix_millis`; `Display` is `toISOString`).
- The JS value model as JSON reads it (2b): `JsValue` (`from_json`, `get`, `as_str`, `is_truthy`, `to_js_string`, `to_json`), `JsObject` (`get`, `iter`, `is_empty`) and `JsonParseError`.

Everything else is `pub(crate)`: the other JS helpers in `js` (`truncate_utf16`, `utf16_window_start`, `translate`, the value model's `entries`, `integer_from_radix`, `js_json`, `json_string`, `js_string`, `number_from_text` and `truthy`, and `JsMap`, JS `Map` and `Set` order with hashed lookups, and since 2d `replace_all` with `Replacement`, JS's global `replace` over a hand-written matcher), `hooks::telemetry::Redactor` (1.x `redactCommand`, 2d), `hooks::setup::compile`, the lexer, regions, counts, rule table, predicates and finders under `scan`, and `hit::Exception`. Phase 4 adds `Scanner::rules()` with `RuleInfo` (wi-ca74b3); §1.3 of the port plan lists the rest. Node's lexical path algebra lives in `temper-app`, beside its readers (OC-1 B).

## Invariants
- No `std::fs`, `std::process`, `std::env`, `SystemTime::now` or threads: every input arrives as a value (ARC-02).
- Output that 1.x writes (hook messages, JSON fields, report text) is byte-identical to 1.x unless §2.4 of the port plan lists the divergence (D16).

## Errors
Typed `thiserror` enums per module, added with the code that needs them (ERR-01, H-09).

## Test strategy
- Unit tests beside the code; property tests (proptest, in use since Phase 1a) for the lexer, the scanner, the glob matcher and JSON string escaping; `insta` snapshots for rendered reports. Commit `proptest-regressions/` when proptest writes one.
- Scanner-level tests port `tests/scan.test.mjs` in its order, one module per 1.x test group (`src/scan/tests/`, support in `src/scan/tests.rs`); manifest tests port `tests/toml.test.mjs` and the domain half of `hooks.test.mjs`'s manifest cases (`src/manifest/tests/`), with each expected text printed by 1.x on the same input; lexer, region and count tests use values computed by running 1.x on the same input.
- Parity with 1.x is checked by the port's harness (§2.2), not here.
- Run: `cargo test -p temper-domain`.

## Do not
- Do not read files, spawn processes or read the clock or the environment; take the value as an argument.
- Do not add a dependency outside the list above without a decision (H-16).

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-10-02 | Four crates plus `xtask`; this is the innermost (D3). | ARC-07, H-16 | owner (wi-0b4dc2) |
| 2026-10-02 | Port the masking lexer and the regex rules faithfully, no `syn` (D8); `regex` plus explicit predicates for the look-around patterns (D9). | H-16 | two-way defaults (wi-2f31ee, wi-e9386f) |
| 2026-10-31 | Phase 1a ships a vertical slice: lexer, test regions and counts with the §1.3 `Scanner` and six rules as their consumer, so no `pub(crate)` helper lands without a caller (no `dead_code` exception, no early `pub`). | ARC-05, H-05, H-16 | two-way (wi-a5a813) |
| 2026-10-31 | `regex` added (D10), with its default features spelled out (`std`, `perf`, `unicode`), so test and release builds compile the same patterns. | ARC-12, H-16 | owner (wi-b03273); features two-way (wi-466849) |
| 2026-10-31 | `thiserror` added for `ScannerSetupError` and every later domain error. | ERR-01, H-16 | pre-approved; two-way (wi-5eed96) |
| 2026-10-31 | `proptest` added as a dev-dependency with `std` only (no `fork`, `timeout`). | TST-06, H-16 | pre-approved; two-way (wi-c0f25b) |
| 2026-10-31 | The rule table keeps 1.x's JS regex sources verbatim and `js::translate` converts them at `Scanner::new`; the JS helpers live in a crate-level `js` module. | CRF-09, ARC-05 | two-way (wi-962170, wi-41c92b) |
| 2026-11-01 | The 1b/1c cut: 1b ports 40 more regex rules; `allow-attr`, `discard-ok`, `empty-err-arm` (with `otherArmBindsErr`) and `ScanOptions::forbid` move to 1c, so each 1.x test is ported whole and `forbid` lands with its only reader. | ARC-05, H-16 | two-way (wi-13609d) |
| 2026-11-01 | Where §1.6's predicate table is not faithful, the look-around rewrites reproduce JS backtracking: `ignore-no-reason` consumes `[\],)]` and a keep trims it, `extern-crate` holds on two or more trailing spaces, and `compiler-fence` drops the redundant `fn` alternative. A rewritten pattern carries its 1.x source in a comment (D8). | CRF-09, H-16 | two-way (wi-31f653) |
| 2026-11-01 | `js::translate` is fallible: it rejects JS constructs that `regex` would read with another meaning, and `ScannerSetupError`'s private source becomes a private `SetupCause` (`Js` or `Regex`). The public surface is unchanged; `source()` no longer downcasts to `regex::Error`. | ERR-01, H-09 | two-way (wi-ef9d9d) |
| 2026-11-01 | The rule data lives in `scan/rules/table.rs`, one ordered slice in 1.x order with a positional `ctx(prod, bin, example, test)`, over ARC-06's soft ~400 lines, so it diffs 1:1 against `scan.mjs`. | ARC-06 | two-way (wi-cbeff6) |
| 2026-11-01 | The 1c/1d cut: `doc-unwrap`, `doc-arg-sections`, doc blocks, `Env::doc` and `is_rust_fence` move to 1d with the 1.x tests that need the whole table; 1c lands 58 of the 60 rules. | ARC-05, H-16 | two-way (wi-9f78ea) |
| 2026-11-01 | `Scanner::rules()` and `RuleInfo` wait for their first non-test reader (the gate's new-code-only set, Phase 4); 1d ports the rule-table test against the in-crate table. | API-14, H-16 | two-way (wi-ca74b3) |
| 2026-11-01 | The "large file" test keeps its hit assertion; its `ms < 10_000` wall-clock bound moves to Phase 1e's corpus CI job as a D16 retired assertion. | TST-10, H-12 | owner (wi-825595) |
| 2026-11-01 | `struct-bounds`' run-time regexes (1.x `new RegExp(esc(name))`) are hand-written matchers, so `scan` compiles nothing and stays total. | H-03, CRF-09 | two-way (wi-5842d0) |
| 2026-11-01 | Comment records carry `kind: CommentKind { Plain, Doc }`, not a `doc: bool`. | API-10 | two-way (wi-91ace5) |
| 2026-11-01 | The scanner tests split into one module per 1.x test group, and `key_cases` into three, bodies unchanged. | ARC-06 | two-way (wi-bd9fd8) |
| 2026-11-01 | `ScanOptions::forbid` is `Option<&'a BTreeSet<String>>`, as §1.3 writes it: the caller lends one set per crate to many scans. | OWN-01, API-14 | two-way (wi-2d7b05) |
| 2026-11-01 | `other_arm_binds_err` lives in `finders/empty_errs.rs` beside `empty-if-let-err`; its patterns are owned by `PredicatePatterns`, which `empty-err-arm`'s keep receives. | ARC-05 | two-way (wi-5f034a) |
| 2026-11-01 | Phase 1d is two PRs: 1d.1 finishes the scanner in this crate (the doc rules, `new_hits`, `classify_path`), and 1d.2 is the `temper scan` slice, each item with its first consumer. | ARC-05, H-16 | two-way (wi-719373) |
| 2026-11-01 | `ScanOptions` stays `Clone` without `Copy`: nothing copies one, and `Copy` can be added later without a break but not removed. | OWN-16, API-06 | two-way (wi-a4b3a7) |
| 2026-11-01 | `classify_path` takes `lib: LibTarget { Present, Absent }`, not §1.3's `crate_has_lib: bool`. | API-10, H-16 | owner (wi-208563) |
| 2026-11-01 | `new_hits` lands in 1d.1 with its 1.x test, before its first non-test reader (Phase 2's pre-edit): §1.3 fixes its shape. | API-14, H-16 | two-way (wi-46a7f4) |
| 2026-11-01 | Doc blocks live in `scan/doc_blocks.rs`, beside `regions.rs`, since `Env::doc` builds them; under `finders/`, `env` and `finders` would depend on each other. | ARC-02, ARC-05 | two-way (wi-afb3e9) |
| 2026-11-01 | `is_rust_fence` is private to `finders/doc_rules.rs`: its only reader is `doc-unwrap`, and 1.x's `isRustFence` assertions sit beside it. | API-14 | two-way (wi-e9f84d) |
| 2026-11-01 | `classify_path` and `is_build_script` live in `scan/classify.rs`; `is_build_script`'s public path is unchanged. | ARC-05 | two-way (wi-358920) |
| 2026-11-01 | 1.x's "every RULES key is reported" test is a witness table: each key with the first 1.x scan that reports it, so the tests need no shared state or order. | TST-10 | two-way (wi-a22589) |
| 2026-11-01 | The depth walkers move to `scan/lexer/brackets.rs`, the spin-lock and TODO key cases to `key_cases/locks_and_todos.rs`, and the doc oracle tests to `tests/docs.rs`, bodies unchanged. | ARC-06 | two-way (wi-7d932c) |
| 2026-11-01 | `Hit`'s `Serialize` is hand-written, with a private `Exception { Requested, Attribute }`, so `exception` sits before `msg` for a covering `#[expect]`'s request and after it for the `expect-attr` finder's own hit, as 1.x's object spread orders them; a derive cannot. `HitAt { hit, file }` is a borrowed view that appends `file`. | OUT-01, OWN-01 | two-way (1d-12, wi-e69138) |
| 2026-11-01 | Only `PathStyle` and `unverbatim` of §1.3's path helpers land here; Node's `resolve`, `relative`, `isAbsolute`, `dirname` and `join` are `pub(crate)` in `temper-app::paths`, where every reader is. | ARC-05, API-14 | two-way (OC-1 B, wi-30e6a5) |
| 2026-11-01 | `render_scan_text` and `NOT_A_RUST_PROJECT` are public here, beside `format_hit`, like §1.3's `render_gate`: message texts live in domain. | ARC-05, H-16 | owner (OC-2 A, wi-274f9a) |
| 2026-11-01 | The 1d.1 review's F3 (a named-field `Witness` in `every_key.rs`) lands in a test-only follow-up PR, after Phase 1e; F1 (`unhide_like_1x`) and F2 (a pre-sized doctest body) land in 1d.2a. | PROC-05 | two-way (OC-7 B, wi-fbe4e1) |
| 2026-11-02 | `toml` added with D10's features (`std`, `parse`, `preserve_order`) and `serde_json` made a normal dependency (2a). | ARC-12, H-16 | owner (D10, wi-b03273) |
| 2026-11-02 | `Manifest` holds the JS model `toml.mjs` builds, read from `toml::de::DeTable`: JS numbers, a date or time as its source text, JS property order. `toml::Table` would need `toml`'s `serde` feature, and would print datetimes in `toml`'s form. | OUT-01, H-16 | two-way (2a) |
| 2026-11-02 | `Config` is infallible and holds pattern strings (2-8); a wrongly typed key takes its default and is reported, and a document that is not an object is `ConfigProblem::NotAnObject` (OC-2 B). `gate.steps` lands with its reader (Phase 4). | ERR-01, H-10 | two-way (wi-d42162); owner (wi-470923) |
| 2026-11-02 | `GateConfig` holds `CargoFeatures` and two `Switch`es, not three bools. | API-10 | two-way (2a) |
| 2026-11-02 | `Manifest`'s getters and `workspace_excludes` wait for `workspace_root_for` (2c), their first reader. | ARC-05, API-14 | two-way (2a) |
| 2026-11-02 | The hook input is the JS value model, made public as `JsValue` and `JsObject` with `JsonParseError`, read by a hand-written `Deserialize` over `serde_json`: object keys in `JSON.parse` order (integer-like keys first, a duplicate keeps its first place and its last value), `null` as `Value::Null`, and 1.x's coercions (`String(x)`, truthiness, `JSON.stringify`) reused. `serde_json::Value` would sort keys, and its `preserve_order` needs `indexmap`, a new dependency. | OUT-05, H-16 | two-way (2b) |
| 2026-11-02 | `serde_json`'s `float_roundtrip` feature is on, so a JSON number reads as the nearest double, as `JSON.parse` reads it; the default parser can be one ULP off (a proptest found `2.0532311330605160e-270`). A feature of an approved dependency; `Cargo.lock` is unchanged. | H-16 | two-way (2b) |
| 2026-11-02 | `TelemetryRow` and `HookOutcome` write their JSON by hand (`to_json`) in 1.x key order, through the crate's `json_string`, rather than `Serialize`; `HookOutcome`'s `Deny` and `Ask` arrive with `pre-edit` (2c). | OUT-01, API-14 | two-way (2b) |
| 2026-11-02 | `edit_policy` takes the edit's facts as data, `EditFacts` with an `EditTarget` (`Rust`, `Manifest` or `Other`), not a closure over the app's readers: the app reads the workspace root, the exemptions and the config first, so the policy stays pure and is tested from a table of 1.x outputs. It returns a `PolicyDecision`, the outcome and its telemetry row, which the app writes. | ARC-05, API-10 | two-way (2c) |
| 2026-11-02 | `Exemptions` borrows the config's `scanner.exempt` keys and compiles their globs once per edit (`Exemptions::new` fails with `GlobError`); `DependencyCheck` (`Off`, `Ask { preapproved }`) carries `askOnNewDependency` and `preapprovedDependencies`. | OWN-01, API-10 | two-way (2c) |
| 2026-11-02 | The telemetry variants `DenyLockfile`, `DenySkillGate`, `DenyManifest`, `DenyScan` (with the rule ids) and `Ask` (with the ask count) mirror 1.x's rows field for field. | OUT-01 | two-way (2c) |
| 2026-11-02 | `skill_preloaded(agent_type)` is a function over 1.x's `PRELOADED` pattern, not an `AgentRole` type: its one reader asks one question, and the agent roles arrive with their Phase 3 readers. | API-14, ARC-05 | two-way (2c) |
| 2026-11-02 | The `pre-edit` denial texts live beside the policy in `hooks::edit::texts`, and `IRON_RULES` with `skill_gate_denial` in `hooks::iron_rules`, which `session-start` also quotes (Phase 3). `Manifest` gains no `package_name` getter: no 2c reader needs one. | ARC-05, API-14 | two-way (2c) |
| 2026-11-02 | `BashPolicy` compiles every pattern once in `new` (an uncompilable one is a `PolicySetupError` naming its rule) and answers through four stage methods in 1.x's order, not one `pub fn` per check: the app calls the stages and puts the 2e and 2f branches between them, and the texts stay private. | API-10, ARC-05 | two-way (2d) |
| 2026-11-02 | `redactCommand` is `Redactor`, a `pub(crate)` struct holding its compiled rules, not a free function: it compiles once per policy. Its JS look-arounds and back-references are rewritten (a trailing look-ahead consumed, a trailer group outside the replaced span, a back-referenced quote matched by hand), each checked red against a naive translation (2-16); the 32,768- and 2,048-unit cuts fall on code-point boundaries (D16(o)). | API-14, 2-16 | two-way (2d) |
| 2026-11-02 | The shell rows are new `TelemetryEvent` variants (`DenyShellWrite`, `DenyShellLockfile`, `DenyBaselineRaise`, `DenyIgnoreRustVersion`, `ShellAsk`) rather than wider `DenyLockfile` and `Ask`, so each variant writes exactly 1.x's keys. `baselineRaise` takes D16(g)'s `temper.exe` and absolute-path forms; D16(g)'s `.claude/temper-version` deny waits for its writer's phase. | OUT-01 | two-way (2d) |
| 2026-11-02 | The hand-written `pre-bash` matchers stay linear in the number of starts (H-13, review of #200): `GIT_COMMIT_N`'s option walk is shared by every `git` and visits positions in order, forward-only `Next` cursors find the next stop and cluster, `baselineCall`'s sticky rest is anchored where the CLI ends, and `HeredocScan` indexes closing lines once per pass by their text. | H-13, PERF-01 | two-way (2d review) |
| 2026-11-02 | The protected-configuration ask is its own type, `ConfigAsk`, which the app joins to `dependency_or_snapshot`'s answer with `before`, not a wider `dependency_or_snapshot` signature: the 2d stage stays as it is, and the app logs both rows in 1.x's order. Its file names are cut at 120 UTF-16 units without splitting a pair (D16(o)). | API-10, OUT-01 | two-way (2e) |
