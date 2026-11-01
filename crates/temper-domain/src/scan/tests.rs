//! Scanner-level tests: the port of `tests/scan.test.mjs`, one module per 1.x test group in its
//! order, and the support they share.

mod api;
mod attributes;
mod concurrency;
mod core;
mod discards;
mod docs;
mod engine;
mod every_key;
mod key_cases;
mod output;
mod whole_file;

use std::collections::BTreeSet;
use std::sync::LazyLock;

use super::{CodeKind, FileRole, Hit, ScanOptions, ScanReport, Scanner, Severity};

const ALL_KINDS: [CodeKind; 4] = [
    CodeKind::Prod,
    CodeKind::Bin,
    CodeKind::Example,
    CodeKind::Test,
];

/// One scanner for every test in this binary, as the composition root builds one per process.
static SCANNER: LazyLock<Scanner> =
    LazyLock::new(|| Scanner::new().expect("every built-in pattern compiles"));

fn scanner() -> &'static Scanner {
    &SCANNER
}

fn scan_as(src: &str, kind: CodeKind, file: FileRole) -> ScanReport {
    scanner().scan(
        src,
        &ScanOptions {
            kind,
            file,
            forbid: None,
        },
    )
}

fn scan(src: &str, kind: CodeKind) -> ScanReport {
    scan_as(src, kind, FileRole::Source)
}

/// `key:severity` for each hit, as the 1.x tests' `keys` helper prints them.
fn keys(report: &ScanReport) -> Vec<String> {
    report
        .hits()
        .iter()
        .map(|h| format!("{}:{}", h.key(), severity_name(h.severity())))
        .collect()
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Hard => "hard",
        Severity::Ask => "ask",
        Severity::Soft => "soft",
    }
}

/// `(key, severity, line)` for each hit.
fn summary(report: &ScanReport) -> Vec<(&'static str, Severity, usize)> {
    report
        .hits()
        .iter()
        .map(|h| (h.key().as_str(), h.severity(), h.line().get()))
        .collect()
}

/// One rule's hits as `id:severity@line` (1.x `hitsOf`).
fn hits_of(src: &str, key: &str, kind: CodeKind) -> Vec<String> {
    scan(src, kind)
        .hits()
        .iter()
        .filter(|h| h.key().as_str() == key)
        .map(|h| format!("{}:{}@{}", h.id(), severity_name(h.severity()), h.line()))
        .collect()
}

/// One rule's hits (1.x `has`).
fn has(src: &str, key: &str, kind: CodeKind) -> Vec<Hit> {
    scan(src, kind)
        .into_hits()
        .into_iter()
        .filter(|h| h.key().as_str() == key)
        .collect()
}

/// One rule's severities (1.x `sevOf`).
fn sev_of(src: &str, key: &str, kind: CodeKind) -> Vec<&'static str> {
    has(src, key, kind)
        .iter()
        .map(|h| severity_name(h.severity()))
        .collect()
}

/// The options for a source file of `kind`, with the crate's forbidden lints.
fn options(kind: CodeKind, forbid: Option<&BTreeSet<String>>) -> ScanOptions<'_> {
    ScanOptions {
        kind,
        file: FileRole::Source,
        forbid,
    }
}

/// A forbidden-lint set.
fn lints(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|&name| name.to_owned()).collect()
}

/// One rule's hits under `options` as `id:severity@line` (1.x `hitsOf` with `opts`).
fn hits_with(src: &str, key: &str, options: &ScanOptions<'_>) -> Vec<String> {
    scanner()
        .scan(src, options)
        .hits()
        .iter()
        .filter(|h| h.key().as_str() == key)
        .map(|h| format!("{}:{}@{}", h.id(), severity_name(h.severity()), h.line()))
        .collect()
}

/// The whole report as `key id severity@line`, the oracle's "full" list.
fn full(src: &str, kind: CodeKind) -> Vec<String> {
    full_with(src, &options(kind, None))
}

/// [`full`] under `options`.
fn full_with(src: &str, options: &ScanOptions<'_>) -> Vec<String> {
    scanner()
        .scan(src, options)
        .hits()
        .iter()
        .map(|h| {
            let sev = severity_name(h.severity());
            format!("{} {} {sev}@{}", h.key(), h.id(), h.line())
        })
        .collect()
}

/// One rule's hits as `line:msg`.
fn msgs_of(src: &str, key: &str, kind: CodeKind) -> Vec<String> {
    msgs_with(src, key, &options(kind, None))
}

/// [`msgs_of`] under `options`.
fn msgs_with(src: &str, key: &str, options: &ScanOptions<'_>) -> Vec<String> {
    scanner()
        .scan(src, options)
        .hits()
        .iter()
        .filter(|h| h.key().as_str() == key)
        .map(|h| format!("{}:{}", h.line(), h.msg()))
        .collect()
}

fn hit<'r>(report: &'r ScanReport, key: &str) -> &'r Hit {
    report
        .hits()
        .iter()
        .find(|h| h.key().as_str() == key)
        .expect("the scan reports a hit for the key")
}

const TEST_REGIONS_SRC: &str = "\npub fn ok() -> u8 { 1 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { assert_eq!(Some(ok()).unwrap(), 1); dbg!(1); }\n    #[test]\n    fn u() { unsafe { } }\n}";
const IGNORE_SRC: &str = "#[test]\n#[ignore = \"slow\"]\nfn a() {}\n#[test]\n#[cfg_attr(miri, ignore = \"spawns a process\")]\nfn b() {}\n#[test]\n#[cfg_attr(windows, ignore = \"unix sockets\")]\nfn c() {}\n#[test]\n#[ignore]\nfn d() {}";
