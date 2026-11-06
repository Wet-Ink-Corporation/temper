//! A file's path relative to a root, as 1.x reports it (`project.mjs` `relTo`, `longPath`).

use std::borrow::Cow;
use std::path::Path;

use temper_domain::{PathStyle, unverbatim};

use crate::paths::{self, PathEnv};
use crate::ports::Files;

/// 1.x `relTo(root, file)`: `file` relative to `root`, `/`-separated, both resolved against
/// the working directory with any Windows verbatim prefix stripped.
///
/// One directory can be spelled two ways: through a symlinked ancestor (macOS's `/var`, a link
/// to `/private/var`) or, on Windows, with an 8.3 short name. When the lexical answer puts the
/// file outside the root, the real, long forms are compared instead.
pub(crate) fn rel_to(files: &impl Files, env: PathEnv<'_>, root: &str, file: &str) -> String {
    Relater::new(files, &mut LongPaths::default()).rel_to(env, root, file)
}

/// [`rel_to`] for the many files a caller relates in one go, through the long paths found so
/// far, which the caller keeps: the file system does not change while a hook answers.
#[derive(Debug)]
pub(crate) struct Relater<'r, F> {
    files: &'r F,
    memo: &'r mut LongPaths,
}

impl<'r, F: Files> Relater<'r, F> {
    pub(crate) fn new(files: &'r F, memo: &'r mut LongPaths) -> Self {
        Self { files, memo }
    }

    /// [`rel_to`] of `file`, through the long paths found before.
    pub(crate) fn rel_to(&mut self, env: PathEnv<'_>, root: &str, file: &str) -> String {
        let PathEnv { style, cwd } = env;
        let root = unverbatim(root, style);
        let file = paths::resolve(style, cwd, &unverbatim(file, style));
        let rel = paths::relative(style, cwd, &root, &file);
        if rel.starts_with("..") || paths::is_absolute(style, &rel) {
            let root = long_path(self.files, style, &root, self.memo);
            let file = long_path(self.files, style, &file, self.memo);
            return normed(paths::relative(style, cwd, &root, &file));
        }
        normed(rel)
    }
}

/// The long path of the directory a file was last related from, kept between files: a
/// line's names mostly share their directory, so the next name there asks `real_path` about
/// itself alone.
#[derive(Debug, Default)]
pub(crate) struct LongPaths {
    last: Option<(String, String)>,
}

/// 1.x `norm` on an owned path, without a copy when it has no `\`.
fn normed(path: String) -> String {
    match paths::norm(&path) {
        Cow::Borrowed(_) => path,
        Cow::Owned(slashed) => slashed,
    }
}

/// 1.x `longPath(path)` of an absolute path that may not exist yet: the real path of its
/// nearest ancestor that resolves, itself included, joined with the names below it.
fn long_path(files: &impl Files, style: PathStyle, path: &str, memo: &mut LongPaths) -> String {
    if let Some(real) = real_path(files, style, path) {
        return real;
    }
    let dir = paths::dirname(style, path);
    if dir == path {
        return path.to_owned(); // a root that does not resolve
    }
    let known = match memo.last.take() {
        Some((last, long)) if last == dir => (last, long),
        last => {
            let long = dir_long(files, style, dir, last.as_ref());
            (dir.to_owned(), long)
        }
    };
    let long = paths::join(style, &known.1, paths::basename(style, path));
    memo.last = Some(known);
    long
}

/// [`long_path`] of a directory, as 1.x's walk up finds it: the directory itself, then its
/// parent (from `last`, the directory related before, when that is the parent), and only
/// past those the nearest ancestor that resolves ([`nearest_long`]).
fn dir_long(
    files: &impl Files,
    style: PathStyle,
    dir: &str,
    last: Option<&(String, String)>,
) -> String {
    if let Some(real) = real_path(files, style, dir) {
        return real;
    }
    let parent = paths::dirname(style, dir);
    if parent == dir {
        return dir.to_owned(); // a root that does not resolve
    }
    let name = paths::basename(style, dir);
    if let Some((_, long)) = last.filter(|(p, _)| p == parent) {
        return paths::join(style, long, name);
    }
    if let Some(real) = real_path(files, style, parent) {
        return paths::join(style, &real, name);
    }
    nearest_long(files, style, dir)
}

/// The long path of a directory neither it nor its parent resolves in. A path resolves only
/// when its parent does (each name is looked up in its parent, and a normal path has no
/// `..`), so the ancestors that resolve are the shallow ones. The deepest of them, which
/// 1.x's walk up finds first, is found by galloping up from the parent (2, 4, 8, ... names
/// up) and bisecting within the step that found one: a few `real_path` asks per doubling of
/// the distance, each about a path no longer than the directory.
fn nearest_long(files: &impl Files, style: PathStyle, dir: &str) -> String {
    // `dir` and its ancestors, deepest first; `dirname` is a prefix of its argument.
    let mut chain = vec![dir];
    while let Some(&below) = chain.last() {
        let up = paths::dirname(style, below);
        if up == below {
            break;
        }
        chain.push(up);
    }
    // `dir` (index 0) and its parent (1) are known not to resolve. The first index that
    // resolves is in `lo..=hi`; `hi == chain.len()` when none does.
    let (mut lo, mut hi) = (2, chain.len());
    let mut real = None;
    let mut step = 2_usize;
    while lo < hi {
        let at = step.min(hi.saturating_sub(1));
        if let Some(found) = chain.get(at).and_then(|p| real_path(files, style, p)) {
            hi = at;
            real = Some(found);
            break;
        }
        lo = at.saturating_add(1);
        step = step.saturating_mul(2);
    }
    while lo < hi {
        let mid = lo.saturating_add(hi.saturating_sub(lo) >> 1);
        match chain.get(mid).and_then(|p| real_path(files, style, p)) {
            Some(found) => {
                hi = mid;
                real = Some(found);
            }
            None => lo = mid.saturating_add(1),
        }
    }
    // Nothing resolves: the root, as written, with every name below it.
    // `chain` keeps the names below the base.
    let base = match real {
        Some(found) => {
            chain.truncate(hi);
            found
        }
        None => chain.pop().unwrap_or(dir).to_owned(),
    };
    chain.iter().rev().fold(base, |long, p| {
        paths::join(style, &long, paths::basename(style, p))
    })
}

/// Past this many bytes a Windows path is longer than Windows opens: 32,767 UTF-16 units with
/// a verbatim prefix, each at most three bytes of UTF-8. There `realpathSync.native` fails on
/// it, so it is not asked. A POSIX path has no such bound: glibc's `realpath` resolves any
/// length whose links keep each prefix it builds under `PATH_MAX`.
const LONGEST_WINDOWS_PATH: usize = 3 * 32_767;

/// 1.x `realpathSync.native(path)` inside `longPath`'s `try`.
fn real_path(files: &impl Files, style: PathStyle, path: &str) -> Option<String> {
    if style == PathStyle::Windows && path.len() > LONGEST_WINDOWS_PATH {
        return None; // as Windows fails to open it
    }
    match files.real_path(Path::new(path)) {
        Ok(found) => found.map(|real| real.to_string_lossy().into_owned()),
        // 1.x catches every failure, not only a missing path, and tries the parent.
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use temper_domain::PathStyle::{Posix, Windows};

    use super::{LongPaths, Relater, rel_to};
    use crate::fakes::FakeFiles;
    use crate::paths::PathEnv;

    #[test]
    fn rel_to_verbatim_inputs_like_1x() {
        let windows = FakeFiles::new(Windows);
        let posix = FakeFiles::new(Posix);
        let in_windows = PathEnv {
            style: Windows,
            cwd: r"C:\x",
        };
        let in_posix = PathEnv {
            style: Posix,
            cwd: "/ws",
        };

        let got = [
            rel_to(&windows, in_windows, r"C:\ws", r"\\?\C:\ws\src\lib.rs"),
            rel_to(&windows, in_windows, "//?/C:/ws", r"c:\WS\Src\lib.rs"),
            rel_to(&posix, in_posix, "/ws", r"\\?\C:\ws\src\lib.rs"),
        ];

        assert_eq!(
            got,
            ["src/lib.rs", "Src/lib.rs", "//?/C:/ws/src/lib.rs"],
            "verbatim prefixes are stripped on Windows only, as 1.x does"
        );
    }

    /// The 1.x `relTo` tests' assertions that the binary cannot observe, on simulated links.
    #[test]
    fn rel_to_falls_back_to_real_paths_when_lexically_outside() {
        let posix = FakeFiles::new(Posix)
            .with_link("/var", "/private/var")
            .with_file("/private/var/ws/crates/legacy/src/lib.rs", "")
            .with_file("/private/var/ws/..foo.rs", "")
            .with_file("/b/private/ws/Cargo.toml", "")
            .with_file("/b/elsewhere.rs", "");
        let windows = FakeFiles::new(Windows)
            .with_link(r"C:\Users\RUNNER~1", r"C:\Users\runneradmin")
            .with_file(r"C:\Users\runneradmin\ws\src\lib.rs", "");
        let legacy = "crates/legacy/src/lib.rs";
        let posix_env = PathEnv {
            style: Posix,
            cwd: "/",
        };
        let windows_env = PathEnv {
            style: Windows,
            cwd: r"C:\",
        };
        let posix_rel = |root: &str, file: &str| rel_to(&posix, posix_env, root, file);
        let windows_rel = |root: &str, file: &str| rel_to(&windows, windows_env, root, file);

        let got = [
            posix_rel("/var/ws", &format!("/private/var/ws/{legacy}")),
            posix_rel("/private/var/ws", &format!("/var/ws/{legacy}")),
            windows_rel(
                r"C:\Users\RUNNER~1\ws",
                r"C:\Users\runneradmin\ws\src\lib.rs",
            ),
            windows_rel(
                r"C:\Users\runneradmin\ws",
                r"C:\Users\RUNNER~1\ws\src\lib.rs",
            ),
            // Not on disk yet: its parent resolves through the link.
            posix_rel("/private/var/ws", "/var/ws/src/new.rs"),
            posix_rel("/b/private/ws", "/b/elsewhere.rs"),
            posix_rel("/var/ws", "/var/ws/..foo.rs"),
        ];

        assert_eq!(
            got,
            [
                legacy,
                legacy,
                "src/lib.rs",
                "src/lib.rs",
                "src/new.rs",
                "../../elsewhere.rs",
                "..foo.rs",
            ],
            "the real, long forms are compared"
        );
    }

    /// glibc's `realpath`, which 1.x's `realpathSync.native` calls, resolves a path of any
    /// length whose links keep each prefix it builds under `PATH_MAX` (#202 re-review M1:
    /// Node resolved 116,900 bytes), so a long path outside the root may land inside it.
    #[test]
    fn rel_to_resolves_a_path_longer_than_any_one_platform_limit_through_its_links() {
        let chain = format!("/o{}", "/d".repeat(60_000)); // 120,002 bytes
        let files = FakeFiles::new(Posix)
            .with_link(&format!("{chain}/w"), "/ws")
            .with_file("/ws/clippy.toml", "");
        let env = PathEnv {
            style: Posix,
            cwd: "/",
        };

        assert_eq!(
            rel_to(&files, env, "/ws", &format!("{chain}/w/clippy.toml")),
            "clippy.toml",
            "the real path is asked for whatever its length"
        );
    }

    /// A line of names outside the root asks `real_path` a few times per name, not once per
    /// ancestor of each, as 1.x's `longPath` walks them (#202 re-review M1: the speed the
    /// length cut-off had bought). The deepest ancestor that resolves is still the one 1.x
    /// finds, here one that links into the root.
    #[test]
    fn a_relater_asks_a_few_real_paths_per_name_however_deep_the_names_are() {
        let files = FakeFiles::new(Posix)
            .with_file("/o/a/a/k", "")
            .with_link("/o/a/a/a", "/ws/in")
            .with_file("/ws/in/k", "");
        let env = PathEnv {
            style: Posix,
            cwd: "/",
        };
        let mut memo = LongPaths::default();
        let mut relater = Relater::new(&files, &mut memo);
        let n = 400;
        let mut dir = String::from("/o");
        let mut got = Vec::new();
        for _ in 0..n {
            dir.push_str("/a");
            got.push(relater.rel_to(env, "/ws", &format!("{dir}/x")));
        }
        let chain_calls = files.real_path_calls();
        for _ in 0..n {
            got.push(relater.rel_to(env, "/ws", &format!("{dir}/y")));
        }
        let same_dir_calls = files.real_path_calls().saturating_sub(chain_calls);

        let short: Vec<&str> = got.iter().take(4).map(String::as_str).collect();
        assert_eq!(short, ["../o/a/x", "../o/a/a/x", "in/x", "in/a/x"]);
        assert_eq!(
            got.last().map(String::len),
            Some(2 + 2 * (n - 3) + 2),
            "in/a/.../y"
        );
        // The root, the name and its directory: a directory below the last one is related
        // from that one's long path, without asking about the ancestors again.
        assert_eq!(chain_calls, 3 * n, "asks for {n} directories");
        assert_eq!(
            same_dir_calls,
            2 * n,
            "the root and the name, per name in a directory already found"
        );
    }

    /// #202 re-review F1: a new name in a directory that resolves asks about the name and the
    /// directory alone, and one in a new directory whose parent resolves about the parent
    /// too, as 1.x's `longPath` does, however deep they are: a bisection from the middle of
    /// a 1900-name chain asked about a dozen paths near its full length, each a `realpath`
    /// that follows every link on the way (46.8 s where 1.x asks in 5.7 s). Only past those
    /// does the walk up gallop, so it asks a few times per doubling of the distance.
    #[test]
    fn a_name_whose_directory_or_its_parent_resolves_asks_about_those_alone() {
        let deep = format!("/o{}", "/d".repeat(1900));
        let files = FakeFiles::new(Posix)
            .with_file(&format!("{deep}/k"), "")
            .with_file("/ws/Cargo.toml", "");
        let env = PathEnv {
            style: Posix,
            cwd: "/",
        };
        let asked = |file: &str| {
            let (calls, bytes) = (files.real_path_calls(), files.real_path_bytes());
            let rel = rel_to(&files, env, "/ws", file);
            let calls = files.real_path_calls().saturating_sub(calls);
            (rel, calls, files.real_path_bytes().saturating_sub(bytes))
        };
        let len = deep.len();

        let in_dir = asked(&format!("{deep}/x"));
        assert_eq!(in_dir.0.len(), 2 + len + 2, "../o/d/.../x");
        assert_eq!(
            (in_dir.1, in_dir.2),
            (3, 3 + (len + 2) + len),
            "root, name, dir"
        );
        let in_new = asked(&format!("{deep}/n/x"));
        assert_eq!(
            (in_new.1, in_new.2),
            (4, 3 + (len + 4) + (len + 2) + len),
            "root, name, new dir, its parent"
        );
        // 1024 new names below the deepest that resolves: a gallop and a bisection.
        let far = format!("{deep}{}/x", "/n".repeat(1 << 10));
        let up = asked(&far);
        assert!(up.0.ends_with("/n/x"));
        // Root, name, directory, parent, a gallop up from the parent (2, 4, ..., 1024) and a
        // bisection within its last step. The whole-path bisection this replaced asks
        // (13, 47,449) here, fewer this far up, but a dozen near-full paths for a name close
        // to one that resolves (the cases above).
        assert_eq!(
            (up.1, up.2),
            (23, 107_201),
            "root, name, a gallop and a bisection"
        );
    }

    /// A Windows path longer than Windows opens is not asked: `realpathSync.native` fails
    /// on it there, so 1.x's `longPath` goes on to the parent.
    #[test]
    fn rel_to_does_not_resolve_a_windows_path_longer_than_windows_opens() {
        let name = "d".repeat(250);
        let chain = format!(r"C:\o{}", format!(r"\{name}").repeat(400)); // 100,404 bytes
        let files = FakeFiles::new(Windows)
            .with_link(&format!(r"{chain}\w"), r"C:\ws")
            .with_file(r"C:\ws\clippy.toml", "");
        let env = PathEnv {
            style: Windows,
            cwd: r"C:\",
        };

        let rel = rel_to(&files, env, r"C:\ws", &format!(r"{chain}\w\clippy.toml"));

        assert!(
            rel.starts_with("../o/ddd"),
            "{}",
            rel.get(..40).unwrap_or("")
        );
    }

    /// `realpathSync.native` failing for another reason than a missing path (a permission
    /// error) sends 1.x's `longPath` to the parent, as a missing path does.
    #[test]
    fn rel_to_takes_the_parent_when_real_path_fails() {
        let files = FakeFiles::new(Posix)
            .with_link("/var", "/private/var")
            .with_file("/private/var/ws/src/locked.rs", "")
            .with_unresolvable("/private/var/ws/src/locked.rs");
        let env = PathEnv {
            style: Posix,
            cwd: "/",
        };

        assert_eq!(
            rel_to(&files, env, "/private/var/ws", "/var/ws/src/locked.rs"),
            "src/locked.rs",
            "the parent's real path, joined with the file name"
        );
    }
}
