//! `SubagentStart`'s context (1.x `hooks.mjs` `subagentStart`): the tree a temper agent works
//! on, its scope, fingerprint, gate and plans, the session's other changed worktrees, and for
//! a reviewer what its verdict must say.

use crate::js::JsTypeError;
use crate::{Fingerprint, GateRecord};

/// How many changed files the context names.
const LISTED_CHANGES: usize = 40;
/// How many of the diff stat's last lines a reviewer is shown.
const STAT_LINES: usize = 25;

/// The tree the agent is to work on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// The checkout it starts in.
    Here,
    /// Another worktree, the only place this session changed Rust (#135).
    Worktree {
        /// Its path for messages: relative to the main checkout when inside it.
        label: &'a str,
        /// Its root with `/` separators.
        at: &'a str,
    },
}

/// Another worktree this session changed, listed for the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtherTree<'a> {
    /// Its path for messages.
    pub label: &'a str,
    /// Its root with `/` separators.
    pub at: &'a str,
    /// Its fingerprint against its base.
    pub fingerprint: Fingerprint,
    /// How many Rust-relevant files it changed.
    pub rust_files: usize,
    /// Its base, when it has one.
    pub base: Option<&'a str>,
    /// Its gate command, from [`tree_gate_command`].
    pub gate_command: &'a str,
}

/// What a reviewer is told beyond the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewerExtras<'a> {
    /// The earlier review of this fingerprint (1.x `earlierReview`), when one is recorded and
    /// not stale.
    pub earlier: Option<&'a str>,
    /// `git diff --stat` of the target and its untracked Rust files (1.x `diffStat`).
    pub diff_stat: &'a str,
}

/// What [`subagent_start_context`] says.
#[derive(Debug, Clone, Copy)]
pub struct SubagentStartFacts<'a> {
    /// `String(input.agent_type ?? '')`.
    pub agent_type: &'a str,
    /// The tree to work on.
    pub target: Target<'a>,
    /// The target's root, as discovered.
    pub root: &'a str,
    /// The target's base, `None` outside git.
    pub base: Option<&'a str>,
    /// The target's fingerprint against its base.
    pub fingerprint: Fingerprint,
    /// The target's Rust changes.
    pub changes: &'a [String],
    /// The cached gate for the fingerprint, as `cachedGate` read it.
    pub gate: Option<&'a GateRecord>,
    /// The target's gate command, from [`tree_gate_command`].
    pub gate_command: &'a str,
    /// The newest plans (1.x `listPlans`).
    pub plans: &'a [String],
    /// The session's other changed worktrees.
    pub also: &'a [OtherTree<'a>],
    /// For a reviewer, its extras; `None` for the implementer and the architect.
    pub reviewer: Option<ReviewerExtras<'a>>,
}

/// 1.x's `gateIn(t)`: the gate command, after `TEMPER_BASE=<base> ` when the tree's base has
/// to be named (`base_to_name`), and with ` --check` for a reviewer.
///
/// # Examples
///
/// ```
/// use temper_domain::tree_gate_command;
///
/// assert_eq!(tree_gate_command("temper gate", Some("abc"), true), "TEMPER_BASE=abc temper gate --check");
/// assert_eq!(tree_gate_command("temper gate", None, false), "temper gate");
/// ```
#[must_use]
pub fn tree_gate_command(gate_command: &str, base_to_name: Option<&str>, reviewer: bool) -> String {
    let base = base_to_name.map_or_else(String::new, |base| format!("TEMPER_BASE={base} "));
    let check = if reviewer { " --check" } else { "" };
    format!("{base}{gate_command}{check}")
}

/// 1.x `subagentStart`'s context text, byte for byte, without a final newline.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x's `g.steps.filter(…)` throws on a red gate record
/// ([`GateRecord::failed_steps`]), outside any `try` (F5).
pub fn subagent_start_context(facts: &SubagentStartFacts<'_>) -> Result<String, JsTypeError> {
    let base = facts.base.unwrap_or("");
    let mut lines = vec![format!("temper context for {}:", facts.agent_type)];
    let scope_dir = match facts.target {
        Target::Here => String::new(),
        Target::Worktree { label, at } => {
            lines.push(format!(
                "- Target: the worktree {label}. This session changed Rust there and not in the main checkout, so that worktree is the tree to work on: run every command in it (`git -C \"{at}\" …`, and the gate as `cd \"{at}\" && {}`).",
                facts.gate_command
            ));
            format!("-C \"{at}\" ")
        }
    };
    lines.push(format!("- Project root: {}", facts.root));
    lines.push(format!(
        "- Base revision: {}; review scope: `git {scope_dir}diff {base}` plus untracked files.",
        facts.base.unwrap_or("none (no git)")
    ));
    lines.push(format!("- Current tree fingerprint: {}", facts.fingerprint));
    let listed = facts
        .changes
        .iter()
        .take(LISTED_CHANGES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    lines.push(format!(
        "- Changed Rust files ({}): {}",
        facts.changes.len(),
        if listed.is_empty() { "none" } else { &listed }
    ));
    let gate = match facts.gate.filter(|g| g.is_present()) {
        Some(g) if g.ok() => "green".to_owned(),
        Some(g) => format!("red ({})", g.failed_steps()?),
        None => "not run yet".to_owned(),
    };
    lines.push(format!(
        "- Gate for this fingerprint: {gate}. Gate command: {}",
        facts.gate_command
    ));
    lines.push(if facts.plans.is_empty() {
        "- Plans: none under .temper/plans/".to_owned()
    } else {
        format!("- Plans: {}", facts.plans.join(", "))
    });
    lines.extend(facts.also.iter().map(also_line));
    if let Some(reviewer) = facts.reviewer {
        push_reviewer_lines(&mut lines, facts, reviewer);
    }
    Ok(lines.join("\n"))
}

/// 1.x's `- Also changed in this session: …` line for another worktree.
fn also_line(t: &OtherTree<'_>) -> String {
    format!(
        "- Also changed in this session: the worktree {} (fingerprint {}, {} Rust file(s)). Its scope is `git -C \"{}\" diff {}` plus its untracked files, and its gate runs inside it (`cd \"{}\" && {}`); a review of it uses that fingerprint.",
        t.label,
        t.fingerprint,
        t.rust_files,
        t.at,
        t.base.unwrap_or(""),
        t.at,
        t.gate_command
    )
}

/// A reviewer's lines: the fingerprint its verdict names, the earlier review, the diff stat.
fn push_reviewer_lines(
    lines: &mut Vec<String>,
    facts: &SubagentStartFacts<'_>,
    reviewer: ReviewerExtras<'_>,
) {
    let or_listed = if facts.also.is_empty() {
        ""
    } else {
        ", or, if you were asked to review one of the worktrees listed above, that worktree's fingerprint"
    };
    lines.push(format!(
        "- Your verdict's \"fingerprint\" field must be {}{or_listed}. If the tree changes during your review the verdict is recorded as stale and a new review will be needed.",
        facts.fingerprint
    ));
    lines.extend(reviewer.earlier.map(str::to_owned));
    if !reviewer.diff_stat.is_empty() {
        lines.push(format!("- Diff stat:\n{}", last_lines(reviewer.diff_stat)));
    }
}

/// `stat.split('\n').slice(-25).join('\n')`.
fn last_lines(stat: &str) -> &str {
    match stat.rmatch_indices('\n').nth(STAT_LINES - 1) {
        Some((at, _)) => stat.get(at + 1..).unwrap_or(stat),
        None => stat,
    }
}

#[cfg(test)]
mod tests;
