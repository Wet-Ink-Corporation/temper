#!/bin/bash
set -euo pipefail
# Fixture: a small git-tracked Rust workspace with the temper lint template.
mkdir -p crates/calc/src
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
cat > crates/calc/Cargo.toml <<'TOML'
[package]
name = "calc"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/calc/src/lib.rs <<'RS'
//! Small arithmetic helpers.

/// Adds two numbers, returning `None` on overflow.
#[must_use]
pub fn add(a: u32, b: u32) -> Option<u32> {
  a.checked_add(b)
}

#[cfg(test)]
mod tests {
  use super::add;

  #[test]
  fn adds_small_numbers() {
      assert_eq!(add(2, 3), Some(5));
  }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init

# An uncommitted change with two ownership defects: &String parameter (OWN-03)
# and a clone whose only purpose is to satisfy the borrow checker (H-08).
cat >> crates/calc/src/lib.rs <<'RS'

/// Returns the longest word and the total word count.
#[must_use]
pub fn longest_word(text: &String) -> (String, usize) {
    let words: Vec<String> = text.split_whitespace().map(|w| w.to_string()).collect();
    let mut best = String::new();
    for w in words.clone() {
        if w.len() > best.len() {
            best = w.clone();
        }
    }
    (best, words.len())
}
RS
