//! Which changed files a verdict's `files_read` covers (1.x `coveredBy`), without 1.x's scan of
//! every changed path per entry.

use std::collections::{HashMap, HashSet};

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

/// The normalised changed paths, indexed for 1.x's `coveredBy`: a set for the whole-path match,
/// and a trie over the paths read from their ends for the absolute-entry match.
///
/// [`SuffixIndex::covers`] walks an entry from its end once, at most as far as the longest
/// changed path, so checking every entry costs their total length plus the paths' (P1), where
/// 1.x compares every entry with every changed path.
#[derive(Debug)]
pub(crate) struct SuffixIndex<'c> {
    whole: HashSet<&'c str>,
    /// Trie edges: (node, byte) to the child node. Node 0 is the root (the empty suffix).
    edges: HashMap<(usize, u8), usize>,
    /// Whether a non-empty changed path ends at the node.
    ends: Vec<bool>,
}

impl<'c> SuffixIndex<'c> {
    /// Indexes `changed`, each path normalised as 1.x's `normPath` leaves it.
    pub(crate) fn new(changed: &'c [String]) -> Self {
        let mut index = Self {
            whole: changed.iter().map(String::as_str).collect(),
            edges: HashMap::new(),
            ends: vec![false],
        };
        for path in changed.iter().filter(|f| !f.is_empty()) {
            let mut node = 0;
            for byte in path.bytes().rev() {
                let next = index.ends.len();
                node = *index.edges.entry((node, byte)).or_insert(next);
                if node == next {
                    index.ends.push(false);
                }
            }
            if let Some(end) = index.ends.get_mut(node) {
                *end = true;
            }
        }
        index
    }

    /// 1.x `coveredBy(read, changed)[0]`: `read` itself when it is a changed path; else, for an
    /// absolute `read`, the longest non-empty changed path that `read` ends with after a `/`.
    /// The answer is a suffix of `read` equal to that changed path.
    pub(crate) fn covers<'r>(&self, read: &'r str) -> Option<&'r str> {
        if self.whole.contains(read) {
            return Some(read);
        }
        if !is_abs(read) {
            return None;
        }
        let bytes = read.as_bytes();
        let mut node = 0;
        let mut longest = None;
        for (at, byte) in bytes.iter().enumerate().rev() {
            let Some(&next) = self.edges.get(&(node, *byte)) else {
                break;
            };
            node = next;
            let ends_here = self.ends.get(node).copied().unwrap_or(false);
            let after_slash = at.checked_sub(1).and_then(|before| bytes.get(before)) == Some(&b'/');
            if ends_here && after_slash {
                longest = Some(at);
            }
        }
        // `at` follows a `/`, so it is a char boundary.
        longest.and_then(|at| read.get(at..))
    }
}

/// The changed `.rs` files that no `files_read` entry covers, in `changed` order, as 1.x
/// lists them.
pub(super) fn unreviewed<'c>(changed_rust: &'c [String], files_read: &[Value]) -> Vec<&'c str> {
    let changed: Vec<String> = changed_rust.iter().map(|f| norm_path(f)).collect();
    let index = SuffixIndex::new(&changed);
    let reads: Vec<String> = files_read
        .iter()
        .map(|r| norm_path(&js_string(r)))
        .collect();
    let seen: HashSet<&str> = reads.iter().filter_map(|r| index.covers(r)).collect();
    changed_rust
        .iter()
        .zip(&changed)
        .filter(|(f, norm)| f.ends_with(".rs") && !seen.contains(norm.as_str()))
        .map(|(f, _)| f.as_str())
        .collect()
}

#[cfg(test)]
mod tests;
