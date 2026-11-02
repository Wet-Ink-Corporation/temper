//! What `pre-bash` answers on its shell-independent checks (1.x `hooks.mjs` `RS_SHELL_DENY`,
//! `MANIFEST_SHELL_DENY`, `BASELINE_SHELL_DENY`, `VENDORED_SHELL_DENY` and the texts inline in
//! `preBash`).

/// A shell write to a `.rs` file.
pub(super) const RUST_SOURCE: &str = "temper: write Rust sources with the Edit or Write tools, not the shell, so the standard's checks run on them (H-12). Use the Edit tool for this change.";
/// A shell write to a `Cargo.toml`.
pub(super) const MANIFEST: &str = "temper: edit Cargo.toml with the Edit or Write tools, not the shell, so the manifest checks run on it (MOD-09, PERF-09, H-12, H-16). Use the Edit tool for this change.";
/// A shell write to the lint baseline.
pub(super) const BASELINE: &str = "temper: .claude/temper-baseline.json records the accepted lint debt and is written only by `temper baseline` (H-12). After fixing findings, lower it with `node \"$TEMPER_CLI\" baseline --tighten`. Raising it (`baseline --update`) is a human decision: ask the human.";
/// A shell write into the vendored CLI.
pub(super) const VENDORED: &str = "temper: .claude/temper/ is the vendored temper CLI that CI and the pre-commit hook run (the lint ratchet and the baseline-raise check live in it). It is written only by `temper setup`, never by hand (H-12). Leave it unchanged, or ask the human to re-vendor it.";
/// A command that sets `TEMPER_ALLOW_RAISE`.
pub(super) const ALLOW_RAISE: &str = "temper: this command sets TEMPER_ALLOW_RAISE, which lets `temper baseline --init`/`--update` record every current finding as accepted legacy debt, so the gate and CI stop failing on it. Raising the baseline weakens verification (H-12): approve only if you asked for it.";
/// `--ignore-rust-version` on a cargo command.
pub(super) const IGNORE_RUST_VERSION: &str = "temper: --ignore-rust-version bypasses the MSRV check, so green would not mean the declared rust-version builds (MOD-01, H-12). If a dependency needs a newer Rust, raising rust-version is a human decision (H-16): say so and ask.";
/// `--no-verify` or `git commit -n`.
pub(super) const NO_VERIFY: &str =
    "temper: skipping git hooks (--no-verify) is not allowed (H-12). Fix what the hook reports.";
/// `-A`, `--allow` or `--cap-lints` on the command line.
pub(super) const LOWERS_LINTS: &str = "temper: lowering lint levels on the command line is not allowed (H-05). Fix the code the lint points at.";
/// `cargo insta accept`.
pub(super) const ACCEPTS_SNAPSHOTS: &str = "temper: accepting all snapshot changes at once can hide regressions (H-12). Approve only after reviewing the snapshot diffs (cargo insta review).";

/// `temper baseline <flag>`.
pub(super) fn baseline_raise(flag: &str) -> String {
    format!(
        "temper: `temper baseline {flag}` records every current finding as accepted legacy debt (H-12), so it is the human's decision, and the CLI refuses it inside Claude Code. To request it, run the same command prefixed with `TEMPER_ALLOW_RAISE=1`: the human will be asked to approve it. (`baseline --tighten` and `--check` never raise it and run freely.)"
    )
}

/// A git command that restores older `.claude/` configuration; `restore` is its segment, cut
/// to 160 UTF-16 units.
pub(super) fn claude_restore(restore: &str) -> String {
    format!(
        "temper: `{restore}` puts older verification configuration back into .claude/ (the lint baseline, settings or the vendored CLI). An older baseline can accept more debt (H-12): approve only if you asked for it."
    )
}

/// `cargo add` or `cargo remove`; `what` lists each command and its crates.
pub(super) fn dependency_change(what: &str) -> String {
    format!(
        "temper: `{what}` changes the dependency graph without the manifest checks. Adding or removing a dependency is a human decision (H-16, ARC-12). Approve only if you asked for it."
    )
}
