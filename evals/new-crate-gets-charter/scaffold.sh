#!/bin/bash
set -euo pipefail
# Fixture: a workspace with one crate, `report`, that has a charter
# (AGENTS.md, imported by CLAUDE.md). Asked for a second crate, the standard
# wants it to get its own charter (ARC-08); the plain shape writes code only.
mkdir -p crates/report/src
cat > Cargo.toml <<'TOML'
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
rust-version = "1.96"
version = "0.1.0"
publish = false

[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
indexing_slicing = "deny"
as_conversions = "deny"
allow_attributes = "deny"
TOML
cat > crates/report/Cargo.toml <<'TOML'
[package]
name = "report"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/report/AGENTS.md <<'MD'
# report

- **Purpose:** the counts of lint findings per rule for one nightly run.
- **Layer:** domain. It does no I/O and depends on no other crate in the workspace.
- **Allowed dependencies:** none.
- **Public surface:** `Report` and its methods.
- **Invariants:** a rule id appears at most once; counts never decrease within a run.
- **Errors:** none; every operation is total.
- **Tests:** unit tests beside the code.
MD
printf '@AGENTS.md\n' > crates/report/CLAUDE.md
cat > crates/report/src/lib.rs <<'RS'
//! Counts of lint findings per rule for one nightly run.

use std::collections::BTreeMap;

/// How many findings each rule produced, keyed by rule id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    counts: BTreeMap<String, u64>,
}

impl Report {
    /// An empty report.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts one finding for `rule`.
    pub fn record(&mut self, rule: &str) {
        let count = self.counts.entry(rule.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
    }

    /// Each rule id with its count, in rule id order.
    pub fn counts(&self) -> impl Iterator<Item = (&str, u64)> {
        self.counts
            .iter()
            .map(|(rule, count)| (rule.as_str(), *count))
    }
}

#[cfg(test)]
mod tests {
    use super::Report;

    #[test]
    fn counts_in_rule_order() {
        let mut report = Report::new();
        report.record("H-09");
        report.record("H-02");
        report.record("H-09");
        assert_eq!(
            report.counts().collect::<Vec<_>>(),
            vec![("H-02", 1), ("H-09", 2)]
        );
    }
}
RS
cat > AGENTS.md <<'MD'
# Workspace

Each crate under `crates/` has a charter in its own `AGENTS.md`.
MD
printf '@AGENTS.md\n' > CLAUDE.md
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
