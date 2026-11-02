//! The shell-independent checks of `pre-bash` (1.x `hooks.mjs` `preBash`): shell writes to
//! Rust sources, `Cargo.lock`, `Cargo.toml`, the lint baseline and the vendored CLI; baseline
//! raises; restores of older `.claude/` configuration; flags that skip hooks, lower lints or
//! bypass the MSRV check; dependency changes and snapshot acceptance.
//!
//! 1.x interleaves these with checks that read the shell command's words and the file system
//! (the reviewer's patch checks, copies onto `.rs`, state-dir writes, protected configuration),
//! which live in the application. So the policy is four stages, each answering in 1.x's order
//! within it, and the handler runs the others between them.

mod deps;
mod flags;
mod prose;
mod raise;
mod texts;
mod writes;

pub use raise::RaiseFlag;

use self::deps::DepPatterns;
use self::flags::FlagPatterns;
use self::prose::ProsePatterns;
use self::raise::RaisePatterns;
use self::writes::{BASELINE, LOCKFILE, MANIFEST, RUST_SOURCE, ShellWrites, VENDORED};
use super::setup::PolicySetupError;
use super::telemetry::Redactor;
use super::{
    DependencyCheck, HookOutcome, PolicyDecision, ShellAskVia, ShellWriteFile, TelemetryEvent,
    lockfile_denial,
};
use crate::js::truncate_utf16;

/// How much of a restoring segment the ask quotes, in UTF-16 units (1.x `slice(0, 160)`).
const RESTORE_UNITS: usize = 160;

/// `pre-bash`'s compiled patterns.
///
/// # Examples
///
/// ```
/// use temper_domain::{BashPolicy, HookOutcome};
///
/// let policy = BashPolicy::new()?;
/// let decision = policy.rust_source_write("echo x > src/lib.rs");
/// assert!(decision.is_some_and(|d| matches!(d.outcome, HookOutcome::Deny { .. })));
/// assert!(policy.rust_source_write("cat src/lib.rs").is_none());
/// # Ok::<(), temper_domain::PolicySetupError>(())
/// ```
#[derive(Debug)]
pub struct BashPolicy {
    rust_source: ShellWrites,
    lockfile: ShellWrites,
    manifest: ShellWrites,
    baseline: ShellWrites,
    vendored: ShellWrites,
    prose: ProsePatterns,
    flags: FlagPatterns,
    raise: RaisePatterns,
    deps: DepPatterns,
    redactor: Redactor,
}

impl BashPolicy {
    /// Compiles every pattern, once per hook run.
    ///
    /// # Errors
    ///
    /// [`PolicySetupError`] when a built-in pattern does not compile, which is a bug in temper.
    pub fn new() -> Result<Self, PolicySetupError> {
        Ok(Self {
            rust_source: ShellWrites::new(RUST_SOURCE)?,
            lockfile: ShellWrites::new(LOCKFILE)?,
            manifest: ShellWrites::new(MANIFEST)?,
            baseline: ShellWrites::new(BASELINE)?,
            vendored: ShellWrites::new(VENDORED)?,
            prose: ProsePatterns::new()?,
            flags: FlagPatterns::new()?,
            raise: RaisePatterns::new()?,
            deps: DepPatterns::new()?,
            redactor: Redactor::new()?,
        })
    }

    /// 1.x's first check: a shell write to a `.rs` file is denied, with a `deny-shell-write`
    /// row.
    #[must_use]
    pub fn rust_source_write(&self, cmd: &str) -> Option<PolicyDecision> {
        self.rust_source
            .test(cmd)
            .then(|| self.shell_write(cmd, ShellWriteFile::RustSource, texts::RUST_SOURCE))
    }

    /// Shell writes to `Cargo.lock`, `Cargo.toml` and the lint baseline, in that order.
    #[must_use]
    pub fn cargo_or_baseline_write(&self, cmd: &str) -> Option<PolicyDecision> {
        if self.lockfile.test(cmd) {
            let event = TelemetryEvent::DenyShellLockfile {
                cmd: self.redactor.redact(cmd),
            };
            return Some(deny(lockfile_denial("Cargo.lock"), event));
        }
        if self.manifest.test(cmd) {
            return Some(self.shell_write(cmd, ShellWriteFile::Manifest, texts::MANIFEST));
        }
        self.baseline
            .test(cmd)
            .then(|| self.shell_write(cmd, ShellWriteFile::Baseline, texts::BASELINE))
    }

    /// The checks between the state-dir write and the protected configuration, in 1.x's
    /// order: `TEMPER_ALLOW_RAISE` asks; a shell write into the vendored CLI and a baseline
    /// raise are denied; a restore of older `.claude/` configuration asks;
    /// `--ignore-rust-version`, skipped git hooks and lowered lints are denied. Message values,
    /// here-documents and search patterns do not count for the last two.
    #[must_use]
    pub fn verification_bypass(&self, cmd: &str) -> Option<PolicyDecision> {
        if cmd.contains("TEMPER_ALLOW_RAISE") {
            let event = TelemetryEvent::ShellAsk {
                via: ShellAskVia::AllowRaise,
            };
            return Some(ask(texts::ALLOW_RAISE.to_owned(), Some(event)));
        }
        if self.vendored.test(cmd) {
            return Some(self.shell_write(cmd, ShellWriteFile::Vendored, texts::VENDORED));
        }
        if let Some(flag) = self.raise.baseline_raise(cmd) {
            let event = TelemetryEvent::DenyBaselineRaise {
                flag,
                cmd: self.redactor.redact(cmd),
            };
            return Some(deny(texts::baseline_raise(flag.flag()), event));
        }
        if let Some(restore) = self.raise.claude_restore(cmd) {
            let event = TelemetryEvent::ShellAsk {
                via: ShellAskVia::ClaudeRestore,
            };
            let quoted = truncate_utf16(&restore, RESTORE_UNITS);
            return Some(ask(texts::claude_restore(quoted), Some(event)));
        }
        if self.flags.ignores_rust_version(cmd) {
            let event = TelemetryEvent::DenyIgnoreRustVersion {
                cmd: self.redactor.redact(cmd),
            };
            return Some(deny(texts::IGNORE_RUST_VERSION.to_owned(), event));
        }
        let text = self.prose.blank(cmd);
        let flag = if self.flags.skips_git_hooks(&text) {
            texts::NO_VERIFY
        } else if self.flags.lowers_lints(&text) {
            texts::LOWERS_LINTS
        } else {
            return None;
        };
        Some(PolicyDecision {
            outcome: HookOutcome::Deny {
                reason: flag.to_owned(),
                context: None,
            },
            telemetry: None,
        })
    }

    /// The last checks: `cargo add` and `cargo remove` of crates not pre-approved ask, when
    /// `dependencies` asks at all, and so does `cargo insta accept`.
    #[must_use]
    pub fn dependency_or_snapshot(
        &self,
        cmd: &str,
        dependencies: &DependencyCheck<'_>,
    ) -> Option<PolicyDecision> {
        if let DependencyCheck::Ask { preapproved } = dependencies {
            let segments = self.flags.segments(cmd);
            let changes: Vec<String> = self
                .deps
                .commands(&segments)
                .into_iter()
                .filter(|c| {
                    c.crates.is_empty()
                        || c.crates
                            .iter()
                            .any(|name| !preapproved.iter().any(|p| p == name))
                })
                .map(|c| {
                    let crates = if c.crates.is_empty() {
                        "(no crate named)".to_owned()
                    } else {
                        c.crates.join(" ")
                    };
                    format!("cargo {} {crates}", c.op.name())
                })
                .collect();
            if !changes.is_empty() {
                let event = TelemetryEvent::ShellAsk {
                    via: ShellAskVia::CargoDeps,
                };
                return Some(ask(
                    texts::dependency_change(&changes.join("; ")),
                    Some(event),
                ));
            }
        }
        self.deps
            .accepts_snapshots(cmd)
            .then(|| ask(texts::ACCEPTS_SNAPSHOTS.to_owned(), None))
    }

    /// A `deny-shell-write` of `file` with the redacted command.
    fn shell_write(&self, cmd: &str, file: ShellWriteFile, reason: &str) -> PolicyDecision {
        let event = TelemetryEvent::DenyShellWrite {
            file,
            cmd: self.redactor.redact(cmd),
        };
        deny(reason.to_owned(), event)
    }
}

/// A denial with its row.
fn deny(reason: String, event: TelemetryEvent) -> PolicyDecision {
    PolicyDecision {
        outcome: HookOutcome::Deny {
            reason,
            context: None,
        },
        telemetry: Some(event),
    }
}

/// An ask, with its row if it writes one.
fn ask(reason: String, telemetry: Option<TelemetryEvent>) -> PolicyDecision {
    PolicyDecision {
        outcome: HookOutcome::Ask {
            reason,
            context: None,
        },
        telemetry,
    }
}

#[cfg(test)]
mod tests;
