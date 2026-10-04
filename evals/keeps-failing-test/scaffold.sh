#!/bin/bash
set -euo pipefail
# Fixture: a stats crate whose median is wrong for even lengths (it averages
# the elements at mid and mid + 1, not mid - 1 and mid), so one test fails.
mkdir -p crates/stats/src
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
cat > crates/stats/Cargo.toml <<'TOML'
[package]
name = "stats"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/stats/src/lib.rs <<'RS'
//! Summary statistics.

/// The median of `values`, rounded down, or `None` when there are none.
#[must_use]
pub fn median(values: &[u32]) -> Option<u32> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        return sorted.get(mid).copied();
    }
    let low = sorted.get(mid)?;
    let high = sorted.get(mid + 1)?;
    Some(low.midpoint(*high))
}

#[cfg(test)]
mod tests {
    use super::median;

    #[test]
    fn median_of_nothing_is_none() {
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn median_of_an_odd_count_is_the_middle_value() {
        assert_eq!(median(&[9, 1, 5]), Some(5));
    }

    #[test]
    fn median_of_an_even_count_averages_the_middle_pair() {
        assert_eq!(median(&[7, 1, 3, 5]), Some(4));
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
