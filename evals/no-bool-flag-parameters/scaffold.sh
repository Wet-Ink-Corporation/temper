#!/bin/bash
set -euo pipefail
# Fixture: a small terminal-text crate. Three independent styles tempt a
# `styled(text, bold: bool, italic: bool, underline: bool)` signature, which
# reads as `styled(t, true, false, true)` at the call site. The standard allows
# at most one bool parameter and never Option<bool> (API-10, API-02).
mkdir -p crates/term/src
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
cat > crates/term/Cargo.toml <<'TOML'
[package]
name = "term"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/term/src/lib.rs <<'RS'
//! Helpers for styling terminal text with ANSI escape codes.

/// The escape sequence that resets every style.
pub const RESET: &str = "\x1b[0m";

/// Wraps `text` in red, followed by a reset.
#[must_use]
pub fn red(text: &str) -> String {
    format!("\x1b[31m{text}{RESET}")
}

#[cfg(test)]
mod tests {
    use super::red;

    #[test]
    fn wraps_text_in_red() {
        assert_eq!(red("hi"), "\x1b[31mhi\x1b[0m");
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
