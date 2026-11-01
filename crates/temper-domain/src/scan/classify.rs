//! What a file is from its path relative to its crate root (1.x `classifyPath` and
//! `isBuildScript`). Only the path text is read: `\` counts as `/`, and nothing is resolved.

use std::borrow::Cow;

use super::scanner::CodeKind;

/// Whether a crate has a library target (`src/lib.rs`), which decides whether its other `src/`
/// files are library or binary code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibTarget {
    /// The crate has a library target.
    Present,
    /// The crate has only binary targets.
    Absent,
}

/// The code context of a file from its path relative to its crate root (1.x `classifyPath`).
///
/// `tests/` is test, `examples/` and `benches/` are example, and `build.rs`, `src/main.rs` and
/// `src/bin/` are bin. Then `test.rs`, `tests.rs` and anything under a `tests/` directory are
/// test, other `src/` files are bin in a crate without a library, and everything else is prod.
/// `\` counts as `/`.
///
/// # Examples
///
/// ```
/// use temper_domain::{CodeKind, LibTarget, classify_path};
///
/// assert_eq!(classify_path("src/lib.rs", LibTarget::Present), CodeKind::Prod);
/// assert_eq!(classify_path("src\\cli.rs", LibTarget::Absent), CodeKind::Bin);
/// assert_eq!(classify_path("src/parser/tests.rs", LibTarget::Present), CodeKind::Test);
/// ```
#[must_use]
pub fn classify_path(rel_to_crate: &str, lib: LibTarget) -> CodeKind {
    let p = if rel_to_crate.contains('\\') {
        Cow::Owned(rel_to_crate.replace('\\', "/"))
    } else {
        Cow::Borrowed(rel_to_crate)
    };
    let p = p.as_ref();
    if p.starts_with("tests/") {
        return CodeKind::Test;
    }
    if p.starts_with("examples/") || p.starts_with("benches/") {
        return CodeKind::Example;
    }
    if p == "build.rs" || p == "src/main.rs" || p.starts_with("src/bin/") {
        return CodeKind::Bin;
    }
    // 1.x `/(^|\/)tests?\.rs$/` and `/\/tests\//`.
    let test_file = ["test.rs", "tests.rs"]
        .iter()
        .any(|name| p == *name || p.strip_suffix(name).is_some_and(|dir| dir.ends_with('/')));
    if test_file || p.contains("/tests/") {
        return CodeKind::Test;
    }
    if p.starts_with("src/") && lib == LibTarget::Absent {
        return CodeKind::Bin;
    }
    CodeKind::Prod
}

/// Is the crate-relative path `rel_to_crate` the crate's build script (`build.rs` at its root)?
///
/// 1.x compares after turning `\` into `/`; `build.rs` has no separator, so that is plain
/// equality.
#[must_use]
pub fn is_build_script(rel_to_crate: &str) -> bool {
    rel_to_crate == "build.rs"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_path_like_1x() {
        use LibTarget::{Absent, Present};
        let cases = [
            ("src/lib.rs", Present, CodeKind::Prod),
            ("lib.rs", Absent, CodeKind::Prod),
            ("./src/lib.rs", Present, CodeKind::Prod),
            ("src/tests.rs.bak", Present, CodeKind::Prod),
            ("xtests/a.rs", Present, CodeKind::Prod),
            ("src/attests.rs", Present, CodeKind::Prod),
            ("examples", Present, CodeKind::Prod),
            ("Tests/a.rs", Present, CodeKind::Prod),
            ("build.rs", Present, CodeKind::Bin),
            ("build.rs", Absent, CodeKind::Bin),
            ("src/bin/x.rs", Present, CodeKind::Bin),
            ("src\\bin\\x.rs", Present, CodeKind::Bin),
            ("src/x.rs", Absent, CodeKind::Bin),
            ("src/main.rs", Absent, CodeKind::Bin),
            ("tests\\a.rs", Present, CodeKind::Test),
            ("src/test.rs", Present, CodeKind::Test),
            ("src/a/tests/b.rs", Present, CodeKind::Test),
            ("test.rs", Present, CodeKind::Test),
            ("a/tests/b.rs", Absent, CodeKind::Test),
        ];

        for (path, lib, want) in cases {
            assert_eq!(classify_path(path, lib), want, "{path:?} ({lib:?})");
        }
    }
}
