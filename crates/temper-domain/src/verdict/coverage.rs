//! Which changed files a verdict's `files_read` covers (1.x `coveredBy`).

use std::collections::HashSet;

use crate::js::{Value, js_string, js_trim};

/// 1.x `normPath`: `String(f).trim()`, `\` as `/`, and no leading `./`.
fn norm_path(path: &str) -> String {
    let slashed = js_trim(path).replace('\\', "/");
    let mut rest = slashed.as_str();
    while let Some(after) = rest.strip_prefix("./") {
        rest = after;
    }
    rest.to_owned()
}

/// 1.x `isAbs` on a normalised path: `/x` or `C:/x`.
fn is_abs(path: &str) -> bool {
    path.starts_with('/')
        || matches!(path.as_bytes(), [drive, b':', b'/', ..] if drive.is_ascii_alphabetic())
}

/// 1.x `coveredBy(read, changed)`.
fn covered_by<'c>(read: &str, changed: &'c [String]) -> Option<&'c str> {
    if let Some(same) = changed.iter().find(|f| *f == read) {
        return Some(same);
    }
    if !is_abs(read) {
        return None;
    }
    changed
        .iter()
        .filter(|f| !f.is_empty() && read.ends_with(&format!("/{f}")))
        .max_by_key(|f| f.len())
        .map(String::as_str)
}

/// The changed `.rs` files that no `files_read` entry covers, in `changed` order, as 1.x
/// lists them.
pub(super) fn unreviewed<'c>(changed_rust: &'c [String], files_read: &[Value]) -> Vec<&'c str> {
    let changed: Vec<String> = changed_rust.iter().map(|f| norm_path(f)).collect();
    let seen: HashSet<&str> = files_read
        .iter()
        .filter_map(|r| covered_by(&norm_path(&js_string(r)), &changed))
        .collect();
    changed_rust
        .iter()
        .filter(|f| f.ends_with(".rs") && !seen.contains(norm_path(f).as_str()))
        .map(String::as_str)
        .collect()
}
