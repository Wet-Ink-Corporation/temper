#!/bin/bash
set -euo pipefail
# Fixture: a session that records its last activity. Asked for an expiry check
# with tests, including an exact 30-minute boundary, the standard wants the
# current time injected (a parameter or a clock trait) so tests are
# deterministic (TST-10); the plain shape reads the wall clock inside.
mkdir -p crates/sessions/src
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
cat > crates/sessions/Cargo.toml <<'TOML'
[package]
name = "sessions"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/sessions/src/lib.rs <<'RS'
//! Login sessions.

use std::time::SystemTime;

/// A signed-in user's session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    user: String,
    last_active: SystemTime,
}

impl Session {
    /// A session for `user` that was last active at `at`.
    #[must_use]
    pub fn new(user: &str, at: SystemTime) -> Self {
        Self {
            user: user.to_owned(),
            last_active: at,
        }
    }

    /// The signed-in user.
    #[must_use]
    pub fn user(&self) -> &str {
        &self.user
    }

    /// Records activity now.
    pub fn touch(&mut self) {
        self.last_active = SystemTime::now();
    }
}

#[cfg(test)]
mod tests {
    use super::Session;
    use std::time::SystemTime;

    #[test]
    fn keeps_the_user() {
        let session = Session::new("ada", SystemTime::UNIX_EPOCH);
        assert_eq!(session.user(), "ada");
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
