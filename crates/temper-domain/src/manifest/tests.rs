//! Ports of `tests/toml.test.mjs` (in its order, one module per area) and the domain half of
//! the manifest checks in `tests/hooks.test.mjs`.

use crate::js::Value;

use super::Manifest;

mod checks;
mod deps;
mod parse;
mod problems;
mod sections;
mod watched;

/// Parses a test manifest that must be valid TOML.
fn toml(text: &str) -> Manifest {
    text.parse().expect("the test manifest is valid TOML")
}

/// 1.x `m?.a?.b?.c` on a parsed manifest.
fn at<'m>(m: &'m Manifest, path: &[&str]) -> Option<&'m Value> {
    let (first, rest) = path.split_first()?;
    rest.iter().try_fold(m.get(first)?, |v, k| v.get(k))
}

/// `hooks.test.mjs`'s `ROOT`: the workspace manifest from the temper template.
const ROOT: &str = r#"# Workspace manifest (from the temper template).
[workspace]
resolver = "3"
members = ["crates/*"]
exclude = ["crates/scratch", "./tools/gen/"]

[workspace.package]
edition = "2024"
rust-version = "1.96"
version = "0.1.0"
publish = false

[workspace.dependencies]
thiserror = "2"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
unwrap_used = "deny"

[profile.release]
lto = "thin"
"#;

/// `hooks.test.mjs`'s `MEMBER`: a member that inherits the workspace lints.
const MEMBER: &str = r#"[package]
name = "ledger-domain"
description = "Pure account rules"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true

[dependencies]
thiserror = { workspace = true }
"#;

/// `hooks.test.mjs`'s `NO_LINTS`: `MEMBER` without its `[lints]` table.
fn no_lints() -> String {
    MEMBER.replace("[lints]\nworkspace = true\n\n", "")
}

fn text(s: &str) -> Value {
    Value::String(s.to_owned())
}
