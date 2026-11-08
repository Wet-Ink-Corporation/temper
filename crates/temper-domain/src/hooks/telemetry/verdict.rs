//! The details of the verdict rows (1.x `hooks.mjs` `preHandback`, `storeVerdict` and
//! `subagentStop`).

use super::{js_bool, push_field};
use crate::Fingerprint;
use crate::js::json_string;
use crate::verdict::{ProblemKind, VerdictOutcome};

/// Which hook read the verdict (a row's `via`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictVia {
    /// `handback`: the reviewer's `SubagentHandback` call (`pre-handback`).
    Handback,
    /// `subagent-stop`: the reviewer's final message or transcript (`subagent-stop`).
    SubagentStop,
}

impl VerdictVia {
    const fn name(self) -> &'static str {
        match self {
            Self::Handback => "handback",
            Self::SubagentStop => "subagent-stop",
        }
    }
}

/// A `verdict` row: a valid verdict was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictRow {
    /// Which hook recorded it.
    pub via: VerdictVia,
    /// The input's `agent_id` as `JSON.stringify` writes it; `None` (absent) leaves it out.
    pub agent_id: Option<String>,
    /// The verdict's `verdict`.
    pub outcome: VerdictOutcome,
    /// The verdict's `fingerprint` (`fp`).
    pub fingerprint: Fingerprint,
    /// Whether the fingerprint is no longer the tree's.
    pub stale: bool,
    /// How many findings it has.
    pub findings: usize,
    /// The record's truthy `kept` as `JSON.stringify` writes it; `None` leaves it out.
    pub kept: Option<String>,
}

/// The detail of a `verdict-invalid` or `verdict-invalid-final` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidVerdict {
    /// How many problems the verdict has.
    pub problems: usize,
    /// Their distinct kinds, in order ([`crate::problem_kinds`]).
    pub kinds: Vec<ProblemKind>,
    /// The input's `agent_id` as `JSON.stringify` writes it; `None` (absent) leaves it out.
    pub agent_id: Option<String>,
}

/// Appends a `verdict` row's detail.
pub(super) fn push_verdict(out: &mut String, row: &VerdictRow) {
    push_field(out, "via", &json_string(row.via.name()));
    if let Some(id) = &row.agent_id {
        push_field(out, "agent_id", id);
    }
    let outcome = match row.outcome {
        VerdictOutcome::Approved => "approved",
        VerdictOutcome::ChangesRequested => "changes-requested",
    };
    push_field(out, "verdict", &json_string(outcome));
    push_field(out, "fp", &json_string(&row.fingerprint.to_string()));
    push_field(out, "stale", js_bool(row.stale));
    push_field(out, "findings", &row.findings.to_string());
    if let Some(kept) = &row.kept {
        push_field(out, "kept", kept);
    }
}

/// Appends a `verdict-invalid` row's detail; `via` is `None` for `verdict-invalid-final`.
pub(super) fn push_invalid(out: &mut String, via: Option<VerdictVia>, invalid: &InvalidVerdict) {
    if let Some(via) = via {
        push_field(out, "via", &json_string(via.name()));
    }
    push_field(out, "problems", &invalid.problems.to_string());
    let kinds: Vec<String> = invalid
        .kinds
        .iter()
        .map(|kind| json_string(kind.name()))
        .collect();
    push_field(out, "kinds", &format!("[{}]", kinds.join(",")));
    if let Some(id) = &invalid.agent_id {
        push_field(out, "agent_id", id);
    }
}

#[cfg(test)]
mod tests;
