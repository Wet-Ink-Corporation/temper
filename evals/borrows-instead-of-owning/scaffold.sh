#!/bin/bash
set -euo pipefail
# Fixture: a tags crate whose parse_tags returns owned tags, and a report crate
# that only counts them. A filter over those tags tempts
# `with_prefix(tags: Vec<String>, prefix: String) -> Vec<String>`. The standard
# borrows in parameters (OWN-01, OWN-03) and returns an iterator or slice when
# the caller only iterates (OWN-13).
mkdir -p crates/tags/src crates/report/src
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
cat > crates/tags/Cargo.toml <<'TOML'
[package]
name = "tags"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/tags/src/lib.rs <<'RS'
//! Tags parsed from a comma-separated line, such as `"lang:rust, cli, lang:c"`.

/// Splits `line` at commas into trimmed, non-empty tags, in order.
#[must_use]
pub fn parse_tags(line: &str) -> Vec<String> {
    line.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_tags;

    #[test]
    fn splits_and_trims() {
        assert_eq!(parse_tags(" lang:rust, cli,, "), ["lang:rust", "cli"]);
    }
}
RS
cat > crates/report/Cargo.toml <<'TOML'
[package]
name = "report"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[dependencies]
tags = { path = "../tags" }

[lints]
workspace = true
TOML
cat > crates/report/src/lib.rs <<'RS'
//! One-line summaries of a project's tags.

use tags::parse_tags;

/// Summarizes the comma-separated tags in `line`.
#[must_use]
pub fn summary(line: &str) -> String {
    let tags = parse_tags(line);
    format!("{} tags", tags.len())
}

#[cfg(test)]
mod tests {
    use super::summary;

    #[test]
    fn counts_the_tags() {
        assert_eq!(summary("lang:rust, cli, lang:c"), "3 tags");
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
