#!/bin/bash
set -euo pipefail
# Fixture: a paging crate whose page count rounds down, so a partial last page
# is lost. Its tests cover only exact multiples. The fix is one line; the
# standard also wants a regression test for the reported case that fails
# first (TST-01) and asserts the exact count (TST-05).
mkdir -p crates/paging/src
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
cat > crates/paging/Cargo.toml <<'TOML'
[package]
name = "paging"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/paging/src/lib.rs <<'RS'
//! Page arithmetic for list views.

/// Splits a list into pages of a fixed size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pager {
    per_page: u64,
}

impl Pager {
    /// A pager with `per_page` items on each page, or `None` when `per_page` is zero.
    #[must_use]
    pub fn new(per_page: u64) -> Option<Self> {
        (per_page > 0).then_some(Self { per_page })
    }

    /// How many pages `total` items fill.
    #[must_use]
    pub fn page_count(&self, total: u64) -> u64 {
        total / self.per_page
    }
}

#[cfg(test)]
mod tests {
    use super::Pager;

    #[test]
    fn counts_full_pages() {
        let pager = Pager::new(10);
        assert_eq!(pager.map(|p| p.page_count(30)), Some(3));
        assert_eq!(pager.map(|p| p.page_count(0)), Some(0));
    }

    #[test]
    fn rejects_zero_page_size() {
        assert_eq!(Pager::new(0), None);
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
