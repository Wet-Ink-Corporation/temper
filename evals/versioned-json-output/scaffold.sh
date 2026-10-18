#!/bin/bash
set -euo pipefail
# Fixture: a findings crate that tallies counts per rule in a HashMap. Asked for
# JSON that other teams read, the plain shape writes the map in iteration
# order, which changes from run to run (OUT-04), with no version for readers
# to check (OUT-03). The crate stays dependency-free, so no serde.
mkdir -p crates/findings/src
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
cat > crates/findings/Cargo.toml <<'TOML'
[package]
name = "findings"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/findings/src/lib.rs <<'RS'
//! Counts of lint findings per rule, for the nightly quality report.

use std::collections::HashMap;

/// How many findings each rule produced in one run. Rule ids are short codes
/// such as `H-02`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    counts: HashMap<String, u64>,
}

impl Tally {
    /// An empty tally.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts one finding for `rule`.
    pub fn record(&mut self, rule: &str) {
        if let Some(count) = self.counts.get_mut(rule) {
            *count = count.saturating_add(1);
        } else {
            self.counts.insert(rule.to_owned(), 1);
        }
    }

    /// The number of findings for `rule`.
    #[must_use]
    pub fn count(&self, rule: &str) -> u64 {
        self.counts.get(rule).copied().unwrap_or(0)
    }

    /// The number of findings across all rules.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.counts
            .values()
            .fold(0, |sum, count| sum.saturating_add(*count))
    }
}

#[cfg(test)]
mod tests {
    use super::Tally;

    #[test]
    fn counts_each_rule() {
        let mut tally = Tally::new();
        tally.record("H-02");
        tally.record("H-02");
        tally.record("API-10");
        assert_eq!(tally.count("H-02"), 2);
        assert_eq!(tally.count("API-10"), 1);
        assert_eq!(tally.count("H-09"), 0);
        assert_eq!(tally.total(), 3);
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
