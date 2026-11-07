//! Where the project and each file's crate are (1.x `project.mjs`: `discover`, `loadConfig`,
//! `findUp`, `crateOf`).

use std::path::Path;

use temper_domain::{Config, ConfigProblem, LibTarget, js_trim};

use crate::engine::RunEnv;
use crate::git;
use crate::paths::{self, PathEnv};
use crate::ports::{Files, Ports};

/// The project a command runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Project {
    /// The project root, resolved: git's top level when the working directory is in a
    /// repository, otherwise the outermost directory above it that has a `Cargo.toml`.
    pub(crate) root: String,
}

/// The project a hook runs in, with what 1.x's `discover` returns beside the root.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Workspace {
    /// The project root, as [`Project::root`].
    pub(crate) root: String,
    /// Where temper keeps its state: `<git common dir>/temper` in a repository, so every
    /// worktree shares one, otherwise `<root>/target/temper`.
    pub(crate) state_dir: String,
    /// `.claude/temper.json` under the root, or the defaults.
    pub(crate) config: Config,
    /// What was wrong with the config file, when something was (D16(d)).
    pub(crate) config_problem: Option<ConfigProblem>,
    /// The root is git's top level (1.x's `proj.gitRoot` is set).
    pub(crate) git: bool,
}

impl Workspace {
    /// 1.x's `proj.gitRoot`: the root when it is git's top level, which 1.x's `discover` makes
    /// the root whenever it finds one.
    pub(crate) fn git_root(&self) -> Option<&str> {
        self.git.then_some(self.root.as_str())
    }
}

/// 1.x `discover(cwd)` for the working directory: the project root, or `None` when this is not
/// a Rust project, which is when neither the root nor any ancestor of the working directory
/// has a `Cargo.toml`.
pub(crate) fn discover(ports: &impl Ports, env: &RunEnv) -> Option<Project> {
    let cwd = env.cwd.to_string_lossy();
    locate(ports, env, &cwd).map(|found| Project {
        root: found.into_root(),
    })
}

/// 1.x `discover(start)` in full: the root, the state dir and the config, for an absolute
/// `start`.
pub(crate) fn discover_at(ports: &impl Ports, env: &RunEnv, start: &str) -> Option<Workspace> {
    let style = env.style;
    let located = locate(ports, env, start)?;
    let state_dir = match &located {
        Located::Git(top) => {
            let common = match git::common_dir(ports.process(), Path::new(top)) {
                Some(dir) => paths::resolve(style, top, js_trim(&dir)),
                // 1.x falls back to `<git root>/.git` when git cannot say.
                None => paths::join(style, top, ".git"),
            };
            paths::join(style, &common, "temper")
        }
        Located::Manifest(root) => {
            paths::join(style, &paths::join(style, root, "target"), "temper")
        }
    };
    let git = matches!(located, Located::Git(_));
    let root = located.into_root();
    let config = paths::join(style, &paths::join(style, &root, ".claude"), "temper.json");
    let load = match ports.files().read_text(Path::new(&config)) {
        Ok(Some(text)) => Config::load(Some(&text)),
        // 1.x's `readText` turns any failure into null, which means the defaults.
        Ok(None) | Err(_) => Config::load(None),
    };
    Some(Workspace {
        root,
        state_dir,
        config: load.config,
        config_problem: load.problem,
        git,
    })
}

/// Where `discover` found the project root, and so whether it is also the git root.
enum Located {
    /// Git's top level, which is the root.
    Git(String),
    /// The outermost directory with a `Cargo.toml`, outside any repository.
    Manifest(String),
}

impl Located {
    fn into_root(self) -> String {
        match self {
            Self::Git(root) | Self::Manifest(root) => root,
        }
    }
}

/// The root seen from `start`: git's top level when `start` is in a repository, otherwise the
/// outermost directory above `start` with a `Cargo.toml`; `None` when this is not a Rust
/// project.
fn locate(ports: &impl Ports, env: &RunEnv, start: &str) -> Option<Located> {
    let files = ports.files();
    let cwd = env.cwd.to_string_lossy();
    let at = PathEnv {
        style: env.style,
        cwd: &cwd,
    };
    let has_manifest = |dir: &str| exists(files, &paths::join(at.style, dir, "Cargo.toml"));
    match git::toplevel(ports.process(), Path::new(start)) {
        Some(top) => {
            let root = paths::resolve(at.style, at.cwd, js_trim(&top));
            // A git repository is a Rust project only with a manifest at its top or above the
            // start; one below it does not count.
            let rust = has_manifest(&root) || find_up(at, start, has_manifest).is_some();
            rust.then_some(Located::Git(root))
        }
        None => outermost_manifest_dir(at, start, has_manifest).map(Located::Manifest),
    }
}

/// The outermost ancestor of `start`, itself included, that has a manifest.
fn outermost_manifest_dir(
    at: PathEnv<'_>,
    start: &str,
    has_manifest: impl Fn(&str) -> bool,
) -> Option<String> {
    let mut outer = find_up(at, start, &has_manifest)?;
    loop {
        let parent = paths::dirname(at.style, &outer);
        if parent == outer {
            break;
        }
        match find_up(at, parent, &has_manifest) {
            Some(dir) => outer = dir,
            None => break,
        }
    }
    Some(outer)
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
    manifest
        .match_indices("[package]")
        .any(|(at, _)| after_line_start(manifest, at))
}

/// Would `/^\s*/m` reach byte `at` of `text`? Only when the JS spaces before it run back to the
/// text's start or hold a line end, since `^` under `m` also matches after `\n`, `\r`, U+2028
/// and U+2029.
pub(crate) fn after_line_start(text: &str, at: usize) -> bool {
    let before = text.get(..at).unwrap_or("");
    let rest = before.trim_end_matches(is_js_space);
    let spaces = before.get(rest.len()..).unwrap_or("");
    rest.is_empty() || spaces.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// Is `c` in JS's `\s`? The set is defined once, by the domain's `js_trim`.
pub(crate) fn is_js_space(c: char) -> bool {
    js_trim(c.encode_utf8(&mut [0; 4])).is_empty()
}

#[cfg(test)]
mod tests;
