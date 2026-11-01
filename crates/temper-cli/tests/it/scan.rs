//! `temper scan --all` and `temper scan FILES..` against 1.x's recorded output on the same
//! tree (`.temper/plans/port-phase-1d2-oracle/`, recorded under Node 22.22.0).

use std::path::Path;

use crate::cli::USAGE;
use crate::support::{TestResult, git_init, temper_in, temper_in_into_closed_pipe, write_tree};

/// The 1d.2 fixture, as `oracle/mk.py` writes it.
const FIXTURE: [(&str, &str); 18] = [
    (
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n",
    ),
    (
        "crates/core/Cargo.toml",
        "[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    ),
    (
        "crates/core/src/lib.rs",
        "//! Core.\n\n/// Parses.\npub fn parse(v: Option<u8>) -> u8 {\n    v.unwrap()\n}\n\npub fn label(v: Option<&str>) -> &str {\n    v.expect(\"\u{e9}\u{1}\tx\")\n}\n",
    ),
    (
        "crates/core/src/asks.rs",
        "#[expect(clippy::unwrap_used, reason = \"the caller checked it\")]\npub fn checked(v: Option<u8>) -> u8 {\n    v.unwrap()\n}\n\npub fn wait(d: std::time::Duration) {\n    std::thread::sleep(d);\n}\n",
    ),
    ("crates/core/src/B.rs", "pub fn b() {\n    dbg!(1);\n}\n"),
    ("crates/core/src/a.rs", "pub fn a() {\n    dbg!(2);\n}\n"),
    (
        "crates/core/src/clean.rs",
        "pub fn add(a: u8, b: u8) -> Option<u8> {\n    a.checked_add(b)\n}\n",
    ),
    (
        "crates/core/src/crlf.rs",
        "pub fn c(v: Option<u8>) -> u8 {\r\n    v.unwrap()\r\n}\r\n",
    ),
    (
        "crates/core/tests/it.rs",
        "#[test]\nfn t() {\n    Some(1).unwrap();\n    dbg!(2);\n}\n",
    ),
    (
        "crates/core/build.rs",
        "fn main() {\n    println!(\"cargo::rerun-if-changed=build.rs\");\n}\n",
    ),
    (
        "crates/tool/Cargo.toml",
        "[package]\nname = \"tool\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    ),
    (
        "crates/tool/src/main.rs",
        "fn main() {\n    let x: Option<u8> = None;\n    x.unwrap();\n}\n",
    ),
    (
        "crates/tool/src/cli.rs",
        "pub fn run() {\n    std::process::exit(2);\n}\n",
    ),
    ("scripts/gen.rs", "fn f() {\n    unsafe {}\n}\n"),
    ("target/debug/x.rs", "fn f() { dbg!(1); }\n"),
    (".temper/x.rs", "fn f() { dbg!(1); }\n"),
    ("node_modules/x.rs", "fn f() { dbg!(1); }\n"),
    ("notes.txt", "dbg!(1)\n"),
];

/// A temporary directory with the fixture, without the files named in `leave_out`.
fn fixture(leave_out: &[&str]) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let files: Vec<(&str, &str)> = FIXTURE
        .into_iter()
        .filter(|(rel, _)| !leave_out.contains(rel))
        .collect();
    write_tree(dir.path(), &files)?;
    Ok(dir)
}

const NOT_A_RUST_PROJECT: &str = "temper: not inside a Rust project (no Cargo.toml found)\n";

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn bare_scan_prints_the_usage_text_and_exits_2() -> TestResult {
    let dir = fixture(&[])?;
    for args in [&["scan"][..], &["scan", "--json"]] {
        let run = temper_in(dir.path(), args)?;
        assert_eq!(
            (run.stdout.as_str(), run.stderr.as_str(), run.code),
            (USAGE, "", Some(2)),
            "{args:?} waits for Phase 4 (1d-14 A)"
        );
    }
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn scan_outside_a_rust_project_exits_2() -> TestResult {
    let dir = tempfile::tempdir()?;
    for args in [
        &["scan", "--all"][..],
        &["scan", "--all", "--json"],
        &["scan", "x.rs"],
    ] {
        let run = temper_in(dir.path(), args)?;
        assert_eq!(
            (run.stdout.as_str(), run.stderr.as_str(), run.code),
            ("", NOT_A_RUST_PROJECT, Some(2)),
            "{args:?}"
        );
    }
    Ok(())
}

/// Without `B.rs`: NTFS lists `a.rs` before `B.rs` where Unix sorts by bytes, so 1.x's own
/// JSON order differs by OS there.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn scan_all_json_matches_1x() -> TestResult {
    let dir = fixture(&["crates/core/src/B.rs"])?;

    let run = temper_in(dir.path(), &["scan", "--all", "--json"])?;

    assert_eq!(
        (run.stdout.as_str(), run.stderr.as_str(), run.code),
        (include_str!("oracle/all-noB.json"), "", Some(1)),
        "1.x's JSON, byte for byte"
    );
    Ok(())
}

/// 1.x sorts files with `localeCompare`, which puts `a.rs` before `B.rs`; 2.0 sorts by bytes
/// (D16(l), owner, wi-ecb9dd), so the expectation is 1.x's text with those two blocks swapped.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn scan_all_text_matches_1x_in_byte_order() -> TestResult {
    let dir = fixture(&[])?;

    let run = temper_in(dir.path(), &["scan", "--all"])?;

    assert_eq!(
        (run.stdout.as_str(), run.stderr.as_str(), run.code),
        (include_str!("oracle/all-bytes.txt"), "", Some(1)),
        "1.x's text with D16(l)"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn scan_files_from_a_crate_directory_match_1x() -> TestResult {
    let dir = fixture(&[])?;
    let core = dir.path().join("crates/core");
    let args = [
        "scan",
        "src/lib.rs",
        "../../scripts/gen.rs",
        "missing.rs",
        "-x",
        "--bogus",
    ];

    let json = temper_in(&core, &[&args[..], &["--json"]].concat())?;
    let text = temper_in(&core, &args)?;

    assert_eq!(
        [
            (json.stdout.as_str(), json.stderr.as_str(), json.code),
            (text.stdout.as_str(), text.stderr.as_str(), text.code),
        ],
        [
            (include_str!("oracle/files.json"), "", Some(1)),
            (include_str!("oracle/files.txt"), "", Some(1)),
        ],
        "relative to the cwd, reported relative to the root; missing files skipped"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn scan_without_hard_hits_exits_0() -> TestResult {
    let dir = fixture(&[])?;
    let core = dir.path().join("crates/core");

    let asks = temper_in(&core, &["scan", "src/asks.rs"])?;
    let clean = [
        temper_in(&core, &["scan", "src/clean.rs"])?,
        temper_in(&core, &["scan", "src/clean.rs", "--json"])?,
        temper_in(&core, &["scan", "src/clean.rs", "--all", "--json"])?,
    ];

    assert!(
        asks.stdout
            .starts_with("[ask] H-05 crates/core/src/asks.rs:1: ")
            && asks
                .stdout
                .ends_with("\n\n0 blocking, 2 exception request(s), 1 advisory\n"),
        "{}",
        asks.stdout
    );
    assert_eq!(asks.code, Some(0), "exception requests and advice pass");
    assert_eq!(
        clean.map(|run| (run.stdout, run.code)),
        [
            (
                "\n0 blocking, 0 exception request(s), 0 advisory\n".to_owned(),
                Some(0)
            ),
            ("[]\n".to_owned(), Some(0)),
            ("[]\n".to_owned(), Some(0)),
        ],
        "no hits; files win over --all"
    );
    Ok(())
}

/// D16(b): on a closed stdout pipe, 2.0 stops writing and keeps the command's own code.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn scan_into_a_closed_pipe_keeps_exit_1() -> TestResult {
    let dir = fixture(&[])?;

    let code = temper_in_into_closed_pipe(dir.path(), &["scan", "--all", "--json"])?;

    assert_eq!(code, Some(1), "the hard hits' code");
    Ok(())
}

/// D16(b), the other way: a run without hard hits still exits 0 on a closed pipe.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn scan_into_a_closed_pipe_keeps_exit_0() -> TestResult {
    let dir = fixture(&[])?;

    let code =
        temper_in_into_closed_pipe(&dir.path().join("crates/core"), &["scan", "src/asks.rs"])?;

    assert_eq!(code, Some(0), "exception requests and advice pass");
    Ok(())
}

/// A port of 1.x's `baseline.test.mjs` test of the same name, for its scan-observable half
/// (the rest is `rel_to_falls_back_to_real_paths_when_lexically_outside` in `temper-app`).
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn rel_to_one_directory_reached_through_a_symlinked_ancestor_and_through_its_real_path_is_the_same_root()
-> TestResult {
    let base = tempfile::tempdir()?;
    write_tree(
        base.path(),
        &[
            (
                "private/ws/Cargo.toml",
                "[workspace]\nmembers = [\"crates/*\"]\n",
            ),
            (
                "private/ws/crates/legacy/Cargo.toml",
                "[package]\nname = \"legacy\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            ),
            (
                "private/ws/crates/legacy/src/lib.rs",
                "pub fn f() {\n    dbg!(1);\n}\n",
            ),
            ("elsewhere.rs", "fn f() {\n    dbg!(2);\n}\n"),
        ],
    )?;
    if symlink_dir(&base.path().join("private"), &base.path().join("var")).is_err() {
        // This host refuses symlinks (Windows without developer mode), as 1.x's
        // `can-symlink.mjs` probe skips; there is nothing to test.
        return Ok(());
    }
    let through_link = base.path().join("var/ws/crates/legacy/src/lib.rs");
    let through_link = through_link.to_str().ok_or("a UTF-8 temporary path")?;

    let run = temper_in(
        &base.path().join("private/ws"),
        &["scan", through_link, "../../elsewhere.rs", "--json"],
    )?;

    let files: Vec<&str> = run
        .stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix("\"file\": "))
        .collect();
    assert_eq!(
        files,
        ["\"crates/legacy/src/lib.rs\"", "\"../../elsewhere.rs\""],
        "{}",
        run.stdout
    );
    Ok(())
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

/// The `file` values of `temper scan --json` output.
fn files_of(json: &str) -> Vec<&str> {
    json.lines()
        .filter_map(|line| line.trim().strip_prefix("\"file\": "))
        .collect()
}

/// A temporary directory with the fixture in `g/rust` and a `g/docs` beside it.
fn nested_fixture() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    write_tree(&dir.path().join("g/rust"), &FIXTURE)?;
    write_tree(&dir.path().join("g/docs"), &[("guide.md", "# Guide\n")])?;
    Ok(dir)
}

/// 1.x's `discover` takes git's top level as the root when there is one (Node-verified:
/// `rust/crates/tool/src/cli.rs` with git, `crates/tool/src/cli.rs` without).
#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn scan_paths_are_relative_to_the_git_top_level() -> TestResult {
    let without_git = nested_fixture()?;
    let with_git = nested_fixture()?;
    git_init(&with_git.path().join("g"))?;
    let scan_cli = |dir: &tempfile::TempDir| {
        temper_in(
            &dir.path().join("g/rust/crates/tool"),
            &["scan", "src/cli.rs", "--json"],
        )
    };

    let runs = [scan_cli(&with_git)?, scan_cli(&without_git)?];

    assert_eq!(
        runs.each_ref().map(|run| (files_of(&run.stdout), run.code)),
        [
            (vec!["\"rust/crates/tool/src/cli.rs\""], Some(1)),
            (vec!["\"crates/tool/src/cli.rs\""], Some(1)),
        ],
        "{runs:?}"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]
fn a_git_repository_without_cargo_toml_above_the_cwd_is_not_a_rust_project() -> TestResult {
    let dir = nested_fixture()?;
    let top = dir.path().join("g");
    git_init(&top)?;

    let runs = [
        temper_in(&top, &["scan", "--all"])?,
        temper_in(&top.join("docs"), &["scan", "--all"])?,
    ];

    assert_eq!(
        runs.map(|run| (run.code, run.stdout, run.stderr)),
        [(); 2].map(|()| (Some(2), String::new(), NOT_A_RUST_PROJECT.to_owned())),
        "the Cargo.toml in g/rust is below the cwd"
    );
    Ok(())
}
