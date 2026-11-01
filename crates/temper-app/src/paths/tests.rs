//! Node's `path` functions, checked against Node 22.22.0 (`.temper/plans/port-phase-1d2-oracle/
//! paths.mjs` and the 1d.2 plan's oracle tables), then as properties in both styles.

use proptest::prelude::*;

use super::{basename, dirname, is_absolute, join, norm, relative, resolve};
use temper_domain::PathStyle::{self, Posix, Windows};

const WIN_CWD: &str = r"C:\ws\crates\core";
const POSIX_CWD: &str = "/ws/crates/core";

/// The rows of a table whose result differs from Node's, with what this port gave.
fn wrong<'a>(rows: &[(&'a str, &'a str)], f: impl Fn(&str) -> String) -> Vec<(&'a str, String)> {
    rows.iter()
        .filter(|(input, want)| f(input) != *want)
        .map(|(input, _)| (*input, f(input)))
        .collect()
}

#[test]
fn resolve_like_node_win32() {
    let rows = [
        ("src/lib.rs", r"C:\ws\crates\core\src\lib.rs"),
        (r"..\..\scripts\gen.rs", r"C:\ws\scripts\gen.rs"),
        ("C:/ws/x.rs", r"C:\ws\x.rs"),
        (r"\x.rs", r"C:\x.rs"),
        ("/x.rs", r"C:\x.rs"),
        (r"D:\y\..\z.rs", r"D:\z.rs"),
        ("C:z.rs", r"C:\ws\crates\core\z.rs"),
        (r"\\h\s\a\..\b.rs", r"\\h\s\b.rs"),
        ("//h/s", r"\\h\s\"),
        ("", WIN_CWD),
        (".", WIN_CWD),
        ("a//b/./c/", r"C:\ws\crates\core\a\b\c"),
        (r"C:\ws\x.rs.", r"C:\ws\x.rs."),
        (r"src\..\..\..\..\..\up.rs", r"C:\up.rs"),
    ];

    let wrong = wrong(&rows, |p| resolve(Windows, WIN_CWD, p));

    assert_eq!(wrong, Vec::new(), "resolved as path.win32.resolve does");
}

#[test]
fn resolve_like_node_posix() {
    let rows = [
        ("src/lib.rs", "/ws/crates/core/src/lib.rs"),
        ("../../scripts/gen.rs", "/ws/scripts/gen.rs"),
        ("/x.rs", "/x.rs"),
        ("//x//y/../z.rs", "/x/z.rs"),
        ("", POSIX_CWD),
        (".", POSIX_CWD),
        ("a//b/./c/", "/ws/crates/core/a/b/c"),
        ("../../../../../up.rs", "/up.rs"),
        (r"src\lib.rs", r"/ws/crates/core/src\lib.rs"),
    ];

    let wrong = wrong(&rows, |p| resolve(Posix, POSIX_CWD, p));

    assert_eq!(wrong, Vec::new(), "resolved as path.posix.resolve does");
}

/// `(from, to, want)` rows whose result differs from Node's, with what this port gave.
fn wrong_relative<'a>(
    style: PathStyle,
    cwd: &str,
    rows: &[(&'a str, &'a str, &'a str)],
) -> Vec<(&'a str, &'a str, String)> {
    rows.iter()
        .filter(|(from, to, want)| relative(style, cwd, from, to) != *want)
        .map(|(from, to, _)| (*from, *to, relative(style, cwd, from, to)))
        .collect()
}

#[test]
fn relative_like_node_win32() {
    let rows = [
        (r"C:\ws", r"C:\ws\src\lib.rs", r"src\lib.rs"),
        (r"C:\ws", r"c:\WS\Src\lib.rs", r"Src\lib.rs"),
        (r"C:\WS\", r"C:\ws\a.rs", "a.rs"),
        (r"C:\ws", r"C:\elsewhere.rs", r"..\elsewhere.rs"),
        (r"C:\ws", r"D:\x.rs", r"D:\x.rs"),
        (r"C:\ws", r"C:\ws", ""),
        (r"C:\ws\a", r"C:\ws\ab\c.rs", r"..\ab\c.rs"),
        (r"\\h\s\ws", r"\\h\s\ws\a.rs", "a.rs"),
        (r"\\h\s\ws", r"\\H\S\other\a.rs", r"..\other\a.rs"),
        (r"C:\", r"C:\x\y.rs", r"x\y.rs"),
        (r"C:\ws", r"C:\ws\..foo.rs", "..foo.rs"),
        (r"C:\ws\a\b", r"C:\ws", r"..\.."),
        (r"C:\ws\a", r"C:\", r"..\.."),
        // Equal UTF-16 lengths after lowering: compared case-insensitively unit by unit.
        ("C:\\WS\\\u{c9}", "C:\\ws\\\u{e9}\\x", "x"),
    ];

    let wrong = wrong_relative(Windows, WIN_CWD, &rows);

    assert_eq!(wrong, Vec::new(), "relative as path.win32.relative is");
}

#[test]
fn relative_like_node_posix() {
    let rows = [
        ("/ws", "/ws/src/lib.rs", "src/lib.rs"),
        ("/ws", "/WS/a.rs", "../WS/a.rs"),
        ("/ws", "/elsewhere.rs", "../elsewhere.rs"),
        ("/ws", "/ws", ""),
        ("/ws/a", "/ws/ab/c.rs", "../ab/c.rs"),
        ("/", "/x/y.rs", "x/y.rs"),
        ("/ws", "/ws/..foo.rs", "..foo.rs"),
        ("/ws/", "/ws/a.rs", "a.rs"),
        ("/ws/a/b", "/ws", "../.."),
        ("/ws/a", "/", "../.."),
    ];

    let wrong = wrong_relative(Posix, POSIX_CWD, &rows);

    assert_eq!(wrong, Vec::new(), "relative as path.posix.relative is");
}

#[test]
fn is_absolute_like_node_win32() {
    let rows = [
        (r"..\x", false),
        (r"D:\x", true),
        ("D:/x", true),
        ("D:x", false),
        (r"\x", true),
        ("/x", true),
        ("..foo", false),
        ("src", false),
        ("", false),
    ];

    let wrong: Vec<&str> = rows
        .iter()
        .filter(|(p, want)| is_absolute(Windows, p) != *want)
        .map(|(p, _)| *p)
        .collect();

    assert_eq!(wrong, Vec::<&str>::new(), "path.win32.isAbsolute");
    assert_eq!(
        [
            is_absolute(Posix, "/x"),
            is_absolute(Posix, r"\x"),
            is_absolute(Posix, "x")
        ],
        [true, false, false],
        "path.posix.isAbsolute"
    );
}

#[test]
fn dirname_basename_join_like_node() {
    let dirnames = [
        (Windows, r"C:\", r"C:\"),
        (Windows, r"C:\a", r"C:\"),
        (Windows, r"C:\ws\a.rs", r"C:\ws"),
        (Windows, r"\\h\s\a", r"\\h\s\"),
        (Windows, r"\\h\s\", r"\\h\s\"),
        (Posix, "/", "/"),
        (Posix, "/a", "/"),
        (Posix, "/ws/a.rs", "/ws"),
    ];
    let basenames = [
        (Windows, r"C:\ws\a.rs", "a.rs"),
        (Windows, r"C:\", ""),
        (Windows, r"\\h\s\a", "a"),
        (Posix, "/ws/a.rs", "a.rs"),
        (Posix, "/", ""),
    ];
    let joins = [
        (Windows, r"C:\", "x", r"C:\x"),
        (Windows, r"C:\ws", "x", r"C:\ws\x"),
        (Windows, r"\\h\s\", "x", r"\\h\s\x"),
        (Posix, "/", "x", "/x"),
        (Posix, "/ws", "x", "/ws/x"),
    ];

    let got = (
        dirnames.map(|(style, p, _)| dirname(style, p).to_owned()),
        basenames.map(|(style, p, _)| basename(style, p).to_owned()),
        joins.map(|(style, dir, name, _)| join(style, dir, name)),
    );

    assert_eq!(
        got,
        (
            dirnames.map(|(.., want)| want.to_owned()),
            basenames.map(|(.., want)| want.to_owned()),
            joins.map(|(.., want)| want.to_owned()),
        ),
        "path.dirname, path.basename and path.join"
    );
}

#[test]
fn norm_turns_backslashes_into_slashes() {
    assert_eq!(
        [norm(r"src\lib.rs"), norm("src/lib.rs")],
        ["src/lib.rs", "src/lib.rs"],
        "1.x norm"
    );
}

/// A relative path of plain segments: no `.`, `..`, empty segment, separator or drive colon.
fn plain_rel() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-zA-Z0-9_é-]{1,6}(\\.rs)?", 1..5)
}

/// An absolute, already resolved directory in each style.
fn absolute_dir() -> impl Strategy<Value = (PathStyle, String)> {
    prop_oneof![
        prop::collection::vec("[a-z0-9]{1,5}", 0..4)
            .prop_map(|segs| (Posix, format!("/{}", segs.join("/")))),
        prop::collection::vec("[a-z0-9]{1,5}", 0..4)
            .prop_map(|segs| (Windows, format!("C:\\{}", segs.join("\\")))),
    ]
}

proptest! {
    #[test]
    fn resolve_is_idempotent(
        (style, cwd) in absolute_dir(),
        path in "[a-zA-Z./\\\\:]{0,12}",
    ) {
        let once = resolve(style, &cwd, &path);
        let twice = resolve(style, &cwd, &once);

        prop_assert_eq!(twice, once);
    }

    #[test]
    fn relative_undoes_resolve_for_plain_segments(
        (style, from) in absolute_dir(),
        segs in plain_rel(),
    ) {
        let sep = match style {
            Windows => "\\",
            Posix => "/",
        };
        let rel = segs.join(sep);
        let to = resolve(style, &from, &rel);

        prop_assert_eq!(relative(style, &from, &from, &to), rel);
    }
}
