//! What changed in the tree, and its fingerprint (1.x `project.mjs`: `changedFiles`,
//! `rustChanges`, `changedGateConfig`, `walkRust` and `fingerprint`).

mod walk;

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use temper_domain::{
    EXCLUDED_DIRS, FINGERPRINT_PATHSPECS, Fingerprint, FingerprintHasher, TEMPER_CONFIG_FILE,
    gate_config, in_fingerprint, is_rust_relevant, js_sort,
};

use crate::engine::RunEnv;
use crate::git;
use crate::paths::{self, PathEnv};
use crate::ports::{Files, Ports, ReadInto};
use crate::project::{self, Workspace};
use crate::relpath::{LongPaths, Relater};
use crate::state::read_js_text;

use walk::walk_rust;

/// The most `readFileSync` reads: Node's `kIoMaxLength`, 2^31 - 1 bytes. A larger file throws
/// `ERR_FS_FILE_TOO_LARGE`, which 1.x's fingerprint catches, hashing the file's frame only.
const NODE_READ_LIMIT: u64 = (1 << 31) - 1;

/// 1.x `fingerprint(proj, base)`: a stable fingerprint of the Rust-relevant tree relative to
/// `base`.
///
/// In git, the head and revision (`base` when it resolves, else HEAD), `git diff` against the
/// revision streamed whole (OC-1 A), and each untracked file in the fingerprint, sorted in
/// UTF-16 order, with its content. Outside git, every walked file 1.x's walk keeps, by sorted
/// absolute path. Then the gate config when what the gate reads changed. It never fails: each
/// git or file failure gives 1.x's value (Invariant 4).
pub(crate) fn fingerprint(
    ports: &impl Ports,
    env: &RunEnv,
    ws: &Workspace,
    base: Option<&str>,
) -> Fingerprint {
    let base = base.filter(|base| !base.is_empty());
    let mut hasher = match ws.git_root() {
        Some(git_root) => hash_git_tree(ports, env, git_root, base),
        None => hash_walked_tree(ports.files(), env, &ws.root),
    };
    if let Some(config) = changed_gate_config(ports, env, ws, base) {
        hasher.gate_config(&config);
    }
    hasher.finish()
}

/// 1.x `changedFiles(proj, base)`: in git, the files changed since `base` (committed or not),
/// then the untracked ones, each once, in first-seen order; outside git, every Rust-relevant
/// file of the walk, in its order, relative to the root.
pub(crate) fn changed_files(
    ports: &impl Ports,
    env: &RunEnv,
    ws: &Workspace,
    base: Option<&str>,
) -> Vec<String> {
    let base = base.filter(|base| !base.is_empty());
    let Some(git_root) = ws.git_root() else {
        let files = ports.files();
        let found = walk_rust(files, env.style, &ws.root, |path| {
            is_rust_relevant(&paths::norm(path))
        });
        return relative(files, env, &ws.root, &found);
    };
    let at = Path::new(git_root);
    let tracked = git::diff_names(ports.process(), at, base);
    let all: Vec<String> = tracked
        .into_iter()
        .chain(git::untracked(ports.process(), at))
        .collect();
    let first_seen: Vec<bool> = {
        let mut seen = HashSet::with_capacity(all.len());
        all.iter().map(|name| seen.insert(name.as_str())).collect()
    };
    all.into_iter()
        .zip(first_seen)
        .filter_map(|(name, first)| first.then_some(name))
        .collect()
}

/// 1.x `rustChanges(proj, base)`: the changed files that are Rust-relevant, then
/// `.claude/temper.json` when what the gate reads from it changed.
pub(crate) fn rust_changes(
    ports: &impl Ports,
    env: &RunEnv,
    ws: &Workspace,
    base: Option<&str>,
) -> Vec<String> {
    let mut changes: Vec<String> = changed_files(ports, env, ws, base)
        .into_iter()
        .filter(|name| is_rust_relevant(&paths::norm(name)))
        .collect();
    if changed_gate_config(ports, env, ws, base.filter(|base| !base.is_empty())).is_some() {
        changes.push(TEMPER_CONFIG_FILE.to_owned());
    }
    changes
}

/// The git half of [`fingerprint`], before the gate config.
fn hash_git_tree(
    ports: &impl Ports,
    env: &RunEnv,
    git_root: &str,
    base: Option<&str>,
) -> FingerprintHasher {
    let mut hasher = FingerprintHasher::new();
    let (process, at) = (ports.process(), Path::new(git_root));
    let head = git::resolve_rev(process, at, "HEAD");
    let based = base.and_then(|base| git::resolve_rev(process, at, base));
    let rev = based.as_deref().or(head.as_deref());
    hasher.head_and_rev(head.as_deref(), rev);
    let excludes = EXCLUDED_DIRS.map(|dir| format!(":(exclude){dir}"));
    let mut args = vec!["diff", "--no-color", "--binary", "--no-ext-diff"];
    args.extend(rev);
    args.push("--");
    args.extend(FINGERPRINT_PATHSPECS);
    args.extend(excludes.iter().map(String::as_str));
    let mut diff = hasher.diff();
    match git::stream_into(process, at, &args, &mut diff) {
        // An exit of any code, a signal or the timeout (`Ran`) leaves the diff 1.x hashes; a
        // git that did not start or whose output broke off (`Lost`) adds nothing beyond what
        // streamed, as 1.x's `r.stdout ?? ''`. Either way the hash goes on.
        git::Streamed::Ran | git::Streamed::Lost => {}
    }
    diff.finish();
    let mut names: Vec<String> = git::untracked(process, at)
        .into_iter()
        .filter(|name| in_fingerprint(&paths::norm(name)))
        .collect();
    js_sort(&mut names);
    for name in &names {
        let mut content = hasher.untracked(name);
        // 1.x's `path.join(proj.gitRoot, f)`: git's names hold no `.` or `..` components.
        let path = paths::resolve(env.style, git_root, name);
        if read_whole(ports.files(), &path, &mut content) {
            hasher.accept(content);
        }
    }
    hasher
}

/// The walk half of [`fingerprint`], outside git, before the gate config.
fn hash_walked_tree(files: &impl Files, env: &RunEnv, root: &str) -> FingerprintHasher {
    let mut hasher = FingerprintHasher::new();
    let style = env.style;
    // 1.x keeps `RUST_RELEVANT || CHARTER` on the absolute path; `in_fingerprint` adds only
    // the relative baseline path, which no absolute path equals.
    let mut found = walk_rust(files, style, root, |path| {
        in_fingerprint(&paths::norm(path))
    });
    let claude = paths::join(style, root, ".claude");
    let baseline = paths::join(style, &claude, "temper-baseline.json");
    if project::exists(files, &baseline) {
        found.push(baseline);
    }
    js_sort(&mut found);
    let cwd = env.cwd.to_string_lossy();
    let mut memo = LongPaths::default();
    let mut relater = Relater::new(files, &mut memo);
    for path in &found {
        let rel = relater.rel_to(PathEnv { style, cwd: &cwd }, root, path);
        let mut content = hasher.file(&rel);
        if read_whole(files, path, &mut content) {
            hasher.accept(content);
        }
    }
    hasher
}

/// 1.x `changedGateConfig(proj, base)`: the gate config now, when it differs from the one at
/// the revision `fingerprint` diffs against (the defaults outside git); else `None`. The file
/// is read directly, so one git ignores still counts.
fn changed_gate_config(
    ports: &impl Ports,
    env: &RunEnv,
    ws: &Workspace,
    base: Option<&str>,
) -> Option<String> {
    let (files, process, style) = (ports.files(), ports.process(), env.style);
    let file = paths::join(
        style,
        &paths::join(style, &ws.root, ".claude"),
        "temper.json",
    );
    let now = gate_config(read_js_text(files, &file).as_deref());
    let before = ws.git_root().and_then(|git_root| {
        let at = Path::new(git_root);
        let rev = base
            .and_then(|base| git::resolve_rev(process, at, base))
            .or_else(|| git::resolve_rev(process, at, "HEAD"))?;
        let cwd = env.cwd.to_string_lossy();
        let rel = crate::relpath::rel_to(files, PathEnv { style, cwd: &cwd }, git_root, &file);
        git::show_at(process, at, &rev, &rel)
    });
    (now != gate_config(before.as_deref())).then_some(now)
}

/// Reads the file at `path` whole into `sink`; `false` for anything 1.x's
/// `catch { /* vanished */ }` covers: gone, unreadable, too large, and since OC-1 A (b) not a
/// regular file (D16(t)).
fn read_whole(files: &impl Files, path: &str, sink: &mut dyn Write) -> bool {
    match files.read_into(Path::new(path), NODE_READ_LIMIT, sink) {
        Ok(ReadInto::Read) => true,
        Ok(ReadInto::NotFound | ReadInto::NotAFile | ReadInto::TooLarge) | Err(_) => false,
    }
}

/// `paths` relative to `root`, as 1.x's `relTo` writes them.
fn relative(files: &impl Files, env: &RunEnv, root: &str, found: &[String]) -> Vec<String> {
    let cwd = env.cwd.to_string_lossy();
    let at = PathEnv {
        style: env.style,
        cwd: &cwd,
    };
    let mut memo = LongPaths::default();
    let mut relater = Relater::new(files, &mut memo);
    found
        .iter()
        .map(|path| relater.rel_to(at, root, path))
        .collect()
}

#[cfg(test)]
mod tests;
