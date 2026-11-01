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
    let PathEnv { style, cwd } = env;
    let root = unverbatim(root, style);
    let file = paths::resolve(style, cwd, &unverbatim(file, style));
    let rel = paths::relative(style, cwd, &root, &file);
    if rel.starts_with("..") || paths::is_absolute(style, &rel) {
        let root = long_path(files, style, &root);
        let file = long_path(files, style, &file);
        return normed(paths::relative(style, cwd, &root, &file));
    }
    normed(rel)
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
fn long_path(files: &impl Files, style: PathStyle, path: &str) -> String {
    // The names below the ancestor that resolves, innermost first.
    let mut names = Vec::new();
    let mut current = path;
    let base = loop {
        if let Some(real) = real_path(files, current) {
            break real;
        }
        let parent = paths::dirname(style, current);
        if parent == current {
            break current.to_owned();
        }
        names.push(paths::basename(style, current));
        current = parent;
    };
    names
        .iter()
        .rev()
        .fold(base, |dir, name| paths::join(style, &dir, name))
}

/// 1.x `realpathSync.native(path)` inside `longPath`'s `try`.
fn real_path(files: &impl Files, path: &str) -> Option<String> {
    match files.real_path(Path::new(path)) {
        Ok(found) => found.map(|real| real.to_string_lossy().into_owned()),
        // 1.x catches every failure, not only a missing path, and tries the parent.
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use temper_domain::PathStyle::{Posix, Windows};

    use super::rel_to;
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
}
