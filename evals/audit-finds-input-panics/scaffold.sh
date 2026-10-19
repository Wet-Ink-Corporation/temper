#!/bin/bash
set -euo pipefail
# Fixture: a legacy upload parser, with no workspace lints, to be audited. Next
# to an obvious `unwrap` it holds four failures on untrusted input that no
# default lint reports and that the standard names: a `HashMap` index on a
# header the upload may omit (H-04), an integer division by a count the upload
# sets (H-03), `Duration::from_secs_f64` on an uploaded number (H-03, the
# escape-hatches table), and an `as u16` length that truncates bodies over
# 64 KiB (H-11). It also has idiom debt (`&String`, `println!` in a library),
# which ranks below them. The audit writes a report and changes no code
# (/temper:audit).
mkdir -p crates/ingest/src
cat > Cargo.toml <<'TOML'
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
version = "0.1.0"
publish = false
TOML
cat > crates/ingest/Cargo.toml <<'TOML'
[package]
name = "ingest"
edition.workspace = true
version.workspace = true
publish.workspace = true
TOML
cat > crates/ingest/src/lib.rs <<'RS'
//! Reads metric uploads from partner devices. An upload is a block of
//! `key: value` header lines, a blank line, then the body. Uploads arrive
//! over the public API, unauthenticated until `verify` runs on the result.

use std::collections::HashMap;
use std::time::Duration;

pub struct Upload {
    pub kind: String,
    pub average: u64,
    pub retry_after: Duration,
    pub frame: Vec<u8>,
}

pub fn parse_upload(text: &str) -> Upload {
    let (head, body) = text.split_once("\n\n").unwrap_or((text, ""));
    let headers: HashMap<&str, &str> = head
        .lines()
        .filter_map(|line| line.split_once(": "))
        .collect();

    let kind = headers["kind"].to_string();
    let total: u64 = headers.get("total").unwrap_or(&"0").parse().unwrap();
    let count: u64 = headers.get("count").unwrap_or(&"0").parse().unwrap_or(0);
    let average = total / count;
    let retry: f64 = headers.get("retry").unwrap_or(&"1").parse().unwrap_or(1.0);
    let retry_after = Duration::from_secs_f64(retry);

    Upload {
        kind,
        average,
        retry_after,
        frame: frame(body.as_bytes()),
    }
}

/// Prefixes `body` with its length as two big-endian bytes, for the
/// downstream queue.
fn frame(body: &[u8]) -> Vec<u8> {
    let len = body.len() as u16;
    let mut out = len.to_be_bytes().to_vec();
    out.extend_from_slice(body);
    out
}

pub fn describe(kind: &String) -> String {
    println!("describing {}", kind);
    format!("upload of kind {}", kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_complete_upload() {
        let upload = parse_upload("kind: cpu\ntotal: 90\ncount: 3\nretry: 2.5\n\nabc");
        assert_eq!(upload.kind, "cpu");
        assert_eq!(upload.average, 30);
        assert_eq!(upload.retry_after, Duration::from_millis(2500));
        assert_eq!(upload.frame, vec![0, 3, b'a', b'b', b'c']);
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
