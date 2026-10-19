#!/bin/bash
set -euo pipefail
# Fixture: an API gateway with no rate limiting. Adding a per-key limit over a
# rolling minute is a new type and module, with a clock to inject and a
# window to bound, so the standard wants a written plan with a test list
# before any code (PROC-03), and the rust-architect for a change this size.
# The repository's CLAUDE.md says where plans go, so both arms know the place;
# it does not ask for one.
mkdir -p crates/gateway/src
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
cat > CLAUDE.md <<'MD'
# gateway

- Build and test with `cargo test --workspace`; clippy runs with `-D warnings`.
- Scratch files go in `.temper/`, which git ignores. Plans and design notes,
  when someone writes one, go in `.temper/plans/<topic>.md`.
MD
cat > crates/gateway/Cargo.toml <<'TOML'
[package]
name = "gateway"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/gateway/src/status.rs <<'RS'
//! Response statuses.

/// The status the gateway answers a request with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 200: the request was served.
    Ok,
    /// 401: the request had no API key.
    Unauthorized,
    /// 404: no route matches the path.
    NotFound,
    /// 429: the key sent too many requests.
    TooManyRequests,
}
RS
cat > crates/gateway/src/lib.rs <<'RS'
//! An API gateway: authenticates requests by API key and routes them.

mod status;

pub use status::Status;

/// One incoming request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The caller's API key; empty when the request had none.
    pub api_key: String,
    /// The request path, such as `/v1/orders`.
    pub path: String,
}

/// The gateway's routing state.
#[derive(Debug, Default)]
pub struct Gateway {
    routes: Vec<String>,
}

impl Gateway {
    /// A gateway that serves the given path prefixes.
    #[must_use]
    pub fn new(routes: Vec<String>) -> Self {
        Self { routes }
    }

    /// Answers `request`, which arrived at `_now_ms` milliseconds on the
    /// gateway's monotonic clock.
    pub fn handle(&mut self, request: &Request, _now_ms: u64) -> Status {
        if request.api_key.is_empty() {
            return Status::Unauthorized;
        }
        if self
            .routes
            .iter()
            .any(|route| request.path.starts_with(route.as_str()))
        {
            Status::Ok
        } else {
            Status::NotFound
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Gateway, Request, Status};

    fn request(key: &str, path: &str) -> Request {
        Request {
            api_key: key.to_owned(),
            path: path.to_owned(),
        }
    }

    #[test]
    fn serves_a_known_route() {
        let mut gateway = Gateway::new(vec!["/v1/orders".to_owned()]);
        assert_eq!(
            gateway.handle(&request("k1", "/v1/orders/7"), 0),
            Status::Ok
        );
    }

    #[test]
    fn rejects_a_request_without_a_key() {
        let mut gateway = Gateway::new(vec!["/v1/orders".to_owned()]);
        assert_eq!(
            gateway.handle(&request("", "/v1/orders"), 0),
            Status::Unauthorized
        );
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
