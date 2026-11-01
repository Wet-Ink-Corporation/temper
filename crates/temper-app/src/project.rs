//! Where the project and each file's crate are (1.x `project.mjs`: `discover`, `findUp`,
//! `crateOf`).

use std::path::Path;

use temper_domain::{LibTarget, js_trim};

use crate::engine::RunEnv;
use crate::paths::{self, PathEnv};
use crate::ports::Files;

/// The project a command runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Project {
    /// The project root, resolved: the outermost directory above the working directory that
    /// has a `Cargo.toml`. Phase 1d.2b puts the git top level first.
    pub(crate) root: String,
}

/// 1.x `discover(cwd)` without its git step: the outermost ancestor of the working directory
/// that has a `Cargo.toml`, or `None` when none has one (not a Rust project).
///
/// 1.x then also checks that the root or some ancestor has a `Cargo.toml`, which a root found
/// this way always has; that check matters once the git top level can be the root (1d.2b).
pub(crate) fn discover(files: &impl Files, env: &RunEnv) -> Option<Project> {
    let cwd = env.cwd.to_string_lossy();
    let at = PathEnv {
        style: env.style,
        cwd: &cwd,
    };
    let has_manifest = |dir: &str| exists(files, &paths::join(at.style, dir, "Cargo.toml"));
    let mut outer = find_up(at, &cwd, has_manifest)?;
    loop {
        let parent = paths::dirname(at.style, &outer);
        if parent == outer {
            break;
        }
        match find_up(at, parent, has_manifest) {
            Some(dir) => outer = dir,
            None => break,
        }
    }
    Some(Project { root: outer })
}

/// 1.x `findUp(start, predicate)`: the nearest ancestor of `start` (resolved against the
/// working directory), itself included, for which `holds` is true.
fn find_up(at: PathEnv<'_>, start: &str, holds: impl Fn(&str) -> bool) -> Option<String> {
    let mut dir = paths::resolve(at.style, at.cwd, start);
    loop {
        if holds(&dir) {
            return Some(dir);
        }
        // `dirname` is a prefix of its argument.
        let parent = paths::dirname(at.style, &dir).len();
        if parent == dir.len() {
            return None;
        }
        dir.truncate(parent);
    }
}

/// 1.x `existsSync(path)`, which follows symlinks.
pub(crate) fn exists(files: &impl Files, path: &str) -> bool {
    match files.real_path(Path::new(path)) {
        Ok(found) => found.is_some(),
        // `existsSync` answers false on any error, a permission error included.
        Err(_) => false,
    }
}

/// The crate a file belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CrateRoot {
    /// The crate's directory, resolved.
    pub(crate) dir: String,
    /// Whether the crate has `src/lib.rs`.
    pub(crate) lib: LibTarget,
}

/// 1.x `crateOf(file)`: the nearest directory above `file` (resolved against the working
/// directory) whose `Cargo.toml` declares a `[package]`.
pub(crate) fn crate_of(files: &impl Files, at: PathEnv<'_>, file: &str) -> Option<CrateRoot> {
    let file = paths::resolve(at.style, at.cwd, file);
    let declares = |dir: &str| {
        let manifest = paths::join(at.style, dir, "Cargo.toml");
        match files.read_text(Path::new(&manifest)) {
            Ok(Some(text)) => declares_package(&text),
            // 1.x's `readText` turns any failure into null: an unreadable manifest is none.
            Ok(None) | Err(_) => false,
        }
    };
    let dir = find_up(at, paths::dirname(at.style, &file), declares)?;
    let lib_rs = paths::join(at.style, &paths::join(at.style, &dir, "src"), "lib.rs");
    let lib = if exists(files, &lib_rs) {
        LibTarget::Present
    } else {
        LibTarget::Absent
    };
    Some(CrateRoot { dir, lib })
}

/// 1.x's `/^\s*\[package\]/m` on a manifest's text: some `[package]` preceded on its line, or
/// across blank lines, only by JS spaces. With `m`, `^` also matches after `\r`, U+2028 and
/// U+2029, so the run of spaces before it must reach the text's start or hold a line end.
pub(crate) fn declares_package(manifest: &str) -> bool {
    manifest.match_indices("[package]").any(|(at, _)| {
        let before = manifest.get(..at).unwrap_or("");
        let text = before.trim_end_matches(is_js_space);
        let spaces = before.get(text.len()..).unwrap_or("");
        text.is_empty() || spaces.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
    })
}

/// Is `c` in JS's `\s`? The set is defined once, by the domain's `js_trim`.
fn is_js_space(c: char) -> bool {
    js_trim(c.encode_utf8(&mut [0; 4])).is_empty()
}

#[cfg(test)]
mod tests;
