use super::{
    BASELINE_FILE, EXCLUDED_DIRS, FINGERPRINT_PATHSPECS, RELEVANT_PATHSPECS, in_fingerprint,
    is_rust_relevant,
};
use crate::js_sort;

/// Each row: the path, 1.x's `RUST_RELEVANT.test(path)` and its `inFingerprint(path)`, printed
/// by Node 22. Absolute paths are in the table because 1.x's walk tests the absolute path.
const ROWS: [(&str, bool, bool); 36] = [
    ("src/lib.rs", true, true),
    ("lib.rs\n", false, false),
    ("a.rsx", false, false),
    ("x.snap", true, true),
    ("x.stderr", true, true),
    ("tests/a.txt", true, true),
    ("a/tests/b", true, true),
    ("atests/b", false, false),
    ("tests", false, false),
    ("/home/u/tests/p/notes.md", true, true),
    ("Cargo.toml", true, true),
    ("xCargo.toml", false, false),
    ("sub/Cargo.lock", true, true),
    ("Cargo.toml.bak", false, false),
    ("rust-toolchain", true, true),
    ("rust-toolchain.toml", true, true),
    ("a/.clippy.toml", true, true),
    ("clippy.toml", true, true),
    ("xclippy.toml", false, false),
    (".rustfmt.toml", true, true),
    ("deny.toml", true, true),
    ("a/deny.toml", true, true),
    ("x.cargo/config", false, false),
    (".cargo/config", true, true),
    ("a/.cargo/config.toml", true, true),
    (".cargo/config.json", false, false),
    ("AGENTS.md", false, true),
    ("crates/a/AGENTS.md", false, true),
    ("XAGENTS.md", false, false),
    (".claude/temper-baseline.json", false, true),
    ("x/.claude/temper-baseline.json", false, false),
    (".claude/temper.json", false, false),
    ("README.md", false, false),
    ("", false, false),
    ("/abs/src/x.rs", true, true),
    ("C:/w/tests/x", true, true),
];

#[test]
fn relevance_answers_as_1x_on_every_row() {
    for (path, relevant, fingerprinted) in ROWS {
        assert_eq!(
            (is_rust_relevant(path), in_fingerprint(path)),
            (relevant, fingerprinted),
            "{path:?}"
        );
    }
}

#[test]
fn the_pathspecs_are_1xs_verbatim() {
    assert_eq!(RELEVANT_PATHSPECS.first(), Some(&":(glob)**/*.rs"));
    assert_eq!(
        FINGERPRINT_PATHSPECS.get(15..),
        Some(
            &[
                ":(literal).claude/temper-baseline.json",
                ":(glob)**/AGENTS.md"
            ][..]
        )
    );
    assert_eq!(
        FINGERPRINT_PATHSPECS.get(..15),
        Some(&RELEVANT_PATHSPECS[..])
    );
    assert_eq!(EXCLUDED_DIRS, [".temper", "target"]);
    assert_eq!(BASELINE_FILE, ".claude/temper-baseline.json");
}

/// Node 22's `[...names].sort()`: UTF-16 code units, so U+10000 and U+1F600 (surrogates
/// 0xD800 up) sort before U+E000 and U+FFFF, where UTF-8 byte order puts them after.
#[test]
fn js_sort_orders_by_utf16_units_not_bytes() {
    let mut names = [
        "\u{1F600}",
        "\u{E000}",
        "a",
        "Z",
        "é",
        "\u{FFFF}",
        "\u{10000}",
    ]
    .map(String::from);
    js_sort(&mut names);
    assert_eq!(
        names,
        [
            "Z",
            "a",
            "é",
            "\u{10000}",
            "\u{1F600}",
            "\u{E000}",
            "\u{FFFF}"
        ]
    );
}
