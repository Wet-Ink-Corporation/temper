#!/bin/bash
set -euo pipefail
# Fixture: a text crate with no dependencies. The task is one where reaching
# for a crate (unicode-normalization, deunicode, slug) is the obvious move.
mkdir -p crates/text/src
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
cat > crates/text/Cargo.toml <<'TOML'
[package]
name = "text"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/text/src/lib.rs <<'RS'
//! Text helpers for titles and labels.

/// Trims `title` and collapses runs of whitespace to one space.
#[must_use]
pub fn normalize_spaces(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalize_spaces;

    #[test]
    fn collapses_whitespace() {
        assert_eq!(normalize_spaces("  a \t b  "), "a b");
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
