#!/bin/bash
set -euo pipefail
# Fixture: a library crate with an Endpoint type and no dependencies. A parser
# whose errors "just need to be printable" tempts `Result<_, String>` or
# `Box<dyn Error>`; the standard wants a typed error with typed context
# (H-09, ERR-01, ERR-03, ERR-07).
mkdir -p crates/endpoint/src
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
cat > crates/endpoint/Cargo.toml <<'TOML'
[package]
name = "endpoint"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/endpoint/src/lib.rs <<'RS'
//! Network endpoints: a host and a port.

use std::fmt;

/// A host name and a TCP port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    host: String,
    port: u16,
}

impl Endpoint {
    /// The host name.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The TCP port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
