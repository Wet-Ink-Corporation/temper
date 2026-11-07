//! 1.x `walkRust(root, keep)`: the files below the root that `keep` takes.

use std::path::Path;
use std::vec;

use temper_domain::PathStyle;

use crate::paths;
use crate::ports::{DirEntry, EntryKind, Files};

/// The directory names the walk never enters, at any depth.
const SKIPPED: [&str; 4] = ["target", ".git", "node_modules", ".temper"];

/// 1.x `walkRust(root, keep)`: every entry below `root`, depth first in listing order, that is
/// not a directory and whose path `keep` takes. Directories named in [`SKIPPED`] are left out,
/// a linked directory is an entry like a file (Node's `Dirent#isDirectory` does not follow
/// links), and a directory that cannot be listed is passed over.
///
/// The walk keeps a stack of listings instead of recursing, so a deep chain of directories
/// costs memory, not stack (P6).
pub(crate) fn walk_rust(
    files: &impl Files,
    style: PathStyle,
    root: &str,
    keep: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<(String, vec::IntoIter<DirEntry>)> = Vec::new();
    if let Some(entries) = listing(files, root) {
        stack.push((root.to_owned(), entries));
    }
    while let Some((dir, entries)) = stack.last_mut() {
        let Some(entry) = entries.next() else {
            stack.pop();
            continue;
        };
        let name = entry.name.to_string_lossy();
        if SKIPPED.contains(&name.as_ref()) {
            continue;
        }
        let path = paths::join(style, dir, &name);
        if entry.kind == EntryKind::Dir {
            if let Some(entries) = listing(files, &path) {
                stack.push((path, entries));
            }
        } else if keep(&path) {
            out.push(path);
        }
    }
    out
}

/// The entries of `dir`, or `None` when it cannot be listed (1.x's `catch { return; }`).
fn listing(files: &impl Files, dir: &str) -> Option<vec::IntoIter<DirEntry>> {
    match files.read_dir(Path::new(dir)) {
        Ok(entries) => Some(entries.into_iter()),
        Err(_) => None,
    }
}
