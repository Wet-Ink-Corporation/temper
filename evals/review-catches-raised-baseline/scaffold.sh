#!/bin/bash
set -euo pipefail
# Fixture: the legacy crate and committed baseline of
# brownfield-change-under-baseline, plus an uncommitted change that adds a
# function with a new indexing_slicing finding and raises the baseline's count
# for it from 1 to 2 to keep the gate green.
mkdir -p crates/legacy/src .claude
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
cat > crates/legacy/Cargo.toml <<'TOML'
[package]
name = "legacy"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/legacy/src/lib.rs <<'RS'
//! Settings parsing, written before the workspace adopted its lint table.

use std::collections::HashMap;

/// Parses `key=value` lines into a map.
#[must_use]
pub fn parse_pairs(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').unwrap();
        map.insert(key.trim().to_owned(), value.trim().to_owned());
    }
    map
}

/// The `timeout` setting, in seconds.
#[must_use]
pub fn timeout_secs(map: &HashMap<String, String>) -> u64 {
    map.get("timeout").unwrap().parse().unwrap()
}

/// The `retries` setting.
#[must_use]
pub fn retries(map: &HashMap<String, String>) -> u32 {
    map.get("retries").unwrap().parse().unwrap()
}

/// The first configured host.
#[must_use]
pub fn first_host(hosts: &[String]) -> &str {
    &hosts[0]
}

#[cfg(test)]
mod tests {
    use super::{parse_pairs, timeout_secs};

    #[test]
    fn reads_the_timeout() {
        assert_eq!(timeout_secs(&parse_pairs("timeout = 30")), 30);
    }
}
RS
cat > .claude/temper-baseline.json <<'JSON'
{
  "temper_baseline": 1,
  "created": "2026-10-01T00:00:00.000Z",
  "updated": "2026-10-01T00:00:00.000Z",
  "commit": null,
  "toolchain": "1.98.1",
  "host": "x86_64-unknown-linux-gnu",
  "crates": {
    "legacy": "crates/legacy"
  },
  "totals": {
    "legacy": {
      "clippy::implicit_hasher": 2,
      "clippy::indexing_slicing": 1,
      "clippy::missing_panics_doc": 3,
      "clippy::unwrap_used": 5,
      "scan:unwrap": 5
    }
  }
}
JSON
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init

# The change under review.
python3 - <<'PY'
from pathlib import Path
lib = Path("crates/legacy/src/lib.rs")
lib.write_text(lib.read_text().replace("""#[cfg(test)]""", """/// The primary host: the first one configured, or `localhost` when none is.
#[must_use]
pub fn primary_host(hosts: &[String]) -> &str {
    if hosts.is_empty() { "localhost" } else { &hosts[0] }
}

#[cfg(test)]""", 1))
base = Path(".claude/temper-baseline.json")
base.write_text(base.read_text()
    .replace('"clippy::indexing_slicing": 1', '"clippy::indexing_slicing": 2')
    .replace('"updated": "2026-10-01T00:00:00.000Z"', '"updated": "2026-10-03T00:00:00.000Z"'))
PY
