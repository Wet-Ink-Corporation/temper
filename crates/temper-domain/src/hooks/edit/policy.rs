//! What `pre-edit` decides about an edit's result: the protected-config ask, the manifest
//! findings, the new-hit scan of a Rust file, and the answer they add up to (1.x `preEdit`
//! from `const denies = []` to its end).

use crate::config::ExemptRule;
use crate::glob::{GlobError, Globs};
use crate::hooks::{ContextEvent, HookOutcome, TelemetryEvent};
use crate::manifest::{DepKind, DepName, ManifestFindings, dep_names, lint_config_changed};
use crate::scan::{
    CodeKind, Hit, ScanOptions, ScanReport, Scanner, Severity, format_hit, new_hits,
};

/// At most this many denials are listed; the rest are counted.
const DENIES_SHOWN: usize = 10;
/// At most this many advisories go in an answer that only advises.
const ADVICE_SHOWN: usize = 8;

/// Which tool made the edit, as far as the policy cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTool {
    /// `Write`: the whole file.
    Write,
    /// `Edit`, `MultiEdit` or any other tool name, which 1.x reads as `Edit`.
    Replace,
}

/// What [`edit_policy`] needs about one edit, gathered by the application.
#[derive(Debug)]
pub struct EditFacts<'a> {
    /// The file's path relative to the project root, `/`-separated, as the messages print it.
    pub rel: &'a str,
    /// The tool.
    pub tool: EditTool,
    /// The file's text before the edit (`''` when it does not exist or cannot be read).
    pub before: &'a str,
    /// The text the edit leaves.
    pub after: &'a str,
    /// The `protected` globs of `.claude/temper.json`.
    pub protected: &'a Globs,
    /// What kind of file it is.
    pub target: EditTarget<'a>,
}

/// The kind of file edited, with what its checks need.
#[derive(Debug)]
pub enum EditTarget<'a> {
    /// A `.rs` file: scanned before and after.
    Rust(RustEdit<'a>),
    /// A `Cargo.toml`: the manifest checks.
    Manifest(ManifestEdit<'a>),
    /// Anything else: only the protected-config check applies.
    Other,
}

/// The scan of a Rust edit.
#[derive(Debug)]
pub struct RustEdit<'a> {
    /// How the file is scanned: its crate's classification.
    pub options: ScanOptions<'a>,
    /// The `scanner.exempt` entries.
    pub exempt: &'a Exemptions<'a>,
}

/// The checks of a manifest edit.
#[derive(Debug)]
pub struct ManifestEdit<'a> {
    /// What [`manifest_checks`](crate::manifest_checks) found; empty when the new text is not
    /// valid TOML.
    pub findings: ManifestFindings,
    /// Whether new dependencies ask.
    pub dependencies: DependencyCheck<'a>,
}

/// `askOnNewDependency` and `preapprovedDependencies`.
#[derive(Debug, Clone, Copy)]
pub enum DependencyCheck<'a> {
    /// `askOnNewDependency: false`.
    Off,
    /// A new dependency asks unless its name is in `preapproved`.
    Ask {
        /// `preapprovedDependencies`.
        preapproved: &'a [String],
    },
}

/// A hook's answer and the telemetry row it writes, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    /// The answer.
    pub outcome: HookOutcome,
    /// The row to log before answering.
    pub telemetry: Option<TelemetryEvent>,
}

/// The compiled `scanner.exempt` entries: rules the scanner does not report under some paths.
#[derive(Debug, Clone, Default)]
pub struct Exemptions<'a>(Vec<(&'a str, Globs)>);

impl<'a> Exemptions<'a> {
    /// Compiles each entry's globs.
    ///
    /// # Errors
    ///
    /// The first [`GlobError`].
    pub fn new(rules: &'a [ExemptRule]) -> Result<Self, GlobError> {
        rules
            .iter()
            .map(|rule| {
                let paths = Globs::new(rule.paths().iter().map(String::as_str))?;
                Ok((rule.rule(), paths))
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }

    /// Is `hit` exempt under `rel`? (`e.rule === h.id && e.paths.some(..)`.)
    pub(crate) fn covers(&self, hit: &Hit, rel: &str) -> bool {
        self.0
            .iter()
            .any(|(rule, paths)| *rule == hit.id().as_str() && paths.any_match(rel))
    }
}

/// The findings of one edit, each list in 1.x's order.
#[derive(Debug, Default)]
pub(crate) struct Findings {
    pub(crate) denies: Vec<String>,
    pub(crate) asks: Vec<String>,
    pub(crate) advice: Vec<String>,
}

/// 1.x `preEdit`'s checks after the skill gate, with its texts and telemetry.
///
/// They are the protected-config ask, then the manifest checks, or the new escape hatches, lint
/// exceptions, removed or ignored tests and new clones of a Rust edit. Any denial refuses the
/// edit, else any ask asks (with the advisories as context), else the advisories (at most 8)
/// are context.
#[must_use]
pub fn edit_policy(facts: EditFacts<'_>, scanner: &Scanner) -> PolicyDecision {
    let EditFacts {
        rel,
        tool,
        before,
        after,
        protected,
        target,
    } = facts;
    let texts = Texts {
        rel,
        tool,
        before,
        after,
    };
    let mut found = Findings::default();
    if protected.any_match(rel) {
        found.asks.push(format!("{rel} is verification/agent configuration. Changing it can weaken the checks (H-05, H-12). Approve only if you asked for this change."));
    }
    let manifest = matches!(target, EditTarget::Manifest(_));
    match target {
        EditTarget::Manifest(edit) => manifest_findings(&mut found, texts, edit),
        EditTarget::Rust(edit) => rust_findings(&mut found, texts, &edit, scanner),
        EditTarget::Other => {}
    }
    decide(rel, found, manifest)
}

/// The edit itself, shared by the checks.
#[derive(Clone, Copy)]
struct Texts<'a> {
    rel: &'a str,
    tool: EditTool,
    before: &'a str,
    after: &'a str,
}

fn manifest_findings(found: &mut Findings, texts: Texts<'_>, edit: ManifestEdit<'_>) {
    let Texts {
        rel, before, after, ..
    } = texts;
    found.denies.extend(edit.findings.denies);
    found.asks.extend(edit.findings.asks);
    found.advice.extend(edit.findings.advice);
    if lint_config_changed(before, after) {
        found.asks.push(format!(
            "{rel}: the [lints]/[profile] sections change (H-05). Approve only if intended."
        ));
    }
    if let DependencyCheck::Ask { preapproved } = edit.dependencies {
        let was = dep_names(before);
        let added: Vec<DepName> = dep_names(after)
            .into_iter()
            .filter(|d| !was.contains(d) && !preapproved.iter().any(|p| p == short_name(d)))
            .collect();
        if !added.is_empty() && !before.is_empty() {
            let plural = if added.len() > 1 { "ies" } else { "y" };
            let names: Vec<String> = added
                .iter()
                .map(|d| match d.kind {
                    DepKind::Dev => format!("{} (dev)", short_name(d)),
                    DepKind::Normal | DepKind::Build => short_name(d).to_owned(),
                })
                .collect();
            found.asks.push(format!(
                "{rel}: new dependenc{plural} {} (H-16, ARC-12). Approve if the addition is justified.",
                names.join(", ")
            ));
        }
    }
}

/// 1.x `d.split(':')[1]` of `kind:name`: the name up to any `:` in it.
fn short_name(dep: &DepName) -> &str {
    dep.name.split(':').next().unwrap_or(&dep.name)
}

fn rust_findings(found: &mut Findings, texts: Texts<'_>, edit: &RustEdit<'_>, scanner: &Scanner) {
    let Texts {
        rel,
        tool,
        before,
        after,
    } = texts;
    let kind = edit.options.kind;
    if tool == EditTool::Write && before.is_empty() && is_mod_rs(rel) && kind != CodeKind::Test {
        found.denies.push(format!(
            "ARC-06 {rel}: new mod.rs file.\n    fix: use `foo.rs` plus `foo/bar.rs`."
        ));
    }
    let b = scanner.scan(before, &edit.options);
    let a = scanner.scan(after, &edit.options);
    let (was, now) = (Counts::of(&b), Counts::of(&a));
    let mut fresh = new_hits(b.hits(), a.into_hits());
    fresh.retain(|hit| !edit.exempt.covers(hit, rel));
    hit_findings(found, &fresh, rel);
    if now.tests < was.tests {
        found.asks.push(format!("{rel}: this edit removes {} test(s) (H-12). Approve only if the test is being moved or intentionally retired.", was.tests.saturating_sub(now.tests)));
    }
    // `a.ignores - b.ignores - Math.max(0, a.tests - b.tests)`, when it is positive.
    let newly_ignored = now
        .ignores
        .saturating_sub(was.ignores)
        .saturating_sub(now.tests.saturating_sub(was.tests));
    if newly_ignored > 0 {
        found.asks.push(format!("{rel}: this edit adds `#[ignore]` to {newly_ignored} existing test(s) (H-12). Approve only if you asked for the test to stop running."));
    }
    if now.clones > was.clones {
        found.advice.push(format!("H-08 {rel}: {} new .clone() call(s). Each needs an ownership reason (callee stores it, cheap handle, or genuine duplication).", now.clones.saturating_sub(was.clones)));
    }
}

/// A scan's counts of tests, `#[ignore]`s and clones.
#[derive(Clone, Copy)]
pub(crate) struct Counts {
    pub(crate) tests: usize,
    pub(crate) ignores: usize,
    pub(crate) clones: usize,
}

impl Counts {
    pub(crate) fn of(report: &ScanReport) -> Self {
        Self {
            tests: report.tests(),
            ignores: report.ignores(),
            clones: report.clones(),
        }
    }
}

/// 1.x `/(^|\/)mod\.rs$/`.
pub(crate) fn is_mod_rs(rel: &str) -> bool {
    rel == "mod.rs" || rel.ends_with("/mod.rs")
}

/// The new hits as denies, asks and advice. One approval line per exception: an ask that an
/// `#[expect]` attribute's own ask already covers (its lint text contains the hit's lint) is
/// dropped, since the attribute carries the reason.
pub(crate) fn hit_findings(found: &mut Findings, fresh: &[Hit], rel: &str) {
    // 1.x's `String(l)` of a missing exception is `undefined`.
    let attr_lints: Vec<&str> = fresh
        .iter()
        .filter(|h| h.key().as_str() == "expect-attr" && h.severity() == Severity::Ask)
        .map(|h| h.exception().unwrap_or("undefined"))
        .collect();
    for hit in fresh {
        let lint = hit.exception().unwrap_or("");
        let covered = hit.severity() == Severity::Ask
            && hit.key().as_str() != "expect-attr"
            && !lint.is_empty()
            && attr_lints.iter().any(|l| l.contains(lint));
        if covered {
            continue;
        }
        match hit.severity() {
            Severity::Hard => found.denies.push(format_hit(hit, Some(rel))),
            Severity::Ask => found.asks.push(format!(
                "Lint exception requested at {rel}:{} ({}): `{}`. Approve only if the stated reason holds (H-05).",
                hit.line(),
                hit.exception().unwrap_or_else(|| hit.id().as_str()),
                hit.text()
            )),
            Severity::Soft => found.advice.push(format_hit(hit, Some(rel))),
        }
    }
}

/// The answer the findings add up to, and its row.
fn decide(rel: &str, found: Findings, manifest: bool) -> PolicyDecision {
    let Findings {
        denies,
        asks,
        advice,
    } = found;
    if !denies.is_empty() {
        let rules = denies
            .iter()
            .map(|d| d.split(' ').next().unwrap_or(d).to_owned())
            .collect();
        let file = rel.to_owned();
        let telemetry = if manifest {
            TelemetryEvent::DenyManifest { file, rules }
        } else {
            TelemetryEvent::DenyScan { file, rules }
        };
        return PolicyDecision {
            outcome: HookOutcome::Deny {
                reason: deny_reason(&denies, manifest),
                context: None,
            },
            telemetry: Some(telemetry),
        };
    }
    if !asks.is_empty() {
        let context = (!advice.is_empty())
            .then(|| format!("temper advisories for this edit:\n{}", advice.join("\n")));
        return PolicyDecision {
            outcome: HookOutcome::Ask {
                reason: format!("temper needs your approval:\n- {}", asks.join("\n- ")),
                context,
            },
            telemetry: Some(TelemetryEvent::Ask {
                file: rel.to_owned(),
                count: asks.len(),
            }),
        };
    }
    let outcome = if advice.is_empty() {
        HookOutcome::Pass
    } else {
        let shown = advice.get(..ADVICE_SHOWN).unwrap_or(&advice);
        HookOutcome::Context {
            event: ContextEvent::PreToolUse,
            text: format!(
                "temper advisories for this edit (not blocking, but a reviewer will ask):\n{}",
                shown.join("\n")
            ),
        }
    };
    PolicyDecision {
        outcome,
        telemetry: None,
    }
}

/// The deny text: a header, at most [`DENIES_SHOWN`] denials and a count of the rest, and for a
/// Rust edit the `#[expect]` hint.
fn deny_reason(denies: &[String], manifest: bool) -> String {
    let n = denies.len();
    let mut text = if manifest {
        format!(
            "temper: this manifest edit breaks {n} workspace rule(s), so it was not applied. Fix the manifest text:"
        )
    } else {
        format!(
            "temper: this edit introduces {n} escape hatch(es), so it was not applied. Rewrite it without them:"
        )
    };
    for deny in denies.iter().take(DENIES_SHOWN) {
        text.push('\n');
        text.push_str(deny);
    }
    if let Some(more) = n.checked_sub(DENIES_SHOWN).filter(|&more| more > 0) {
        text.push_str("\n... and ");
        text.push_str(&more.to_string());
        text.push_str(" more.");
    }
    if !manifest {
        text.push_str("\nIf one of these is a genuine exception, put `#[expect(<lint>, reason = \"...\")]` on the item; the human will be asked to approve it. See rust-craft references/escape-hatches.md.");
    }
    text
}

#[cfg(test)]
mod tests;
