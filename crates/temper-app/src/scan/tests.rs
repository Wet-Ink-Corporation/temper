use std::path::PathBuf;

use temper_domain::PathStyle::{self, Posix, Windows};
use temper_domain::{NOT_A_RUST_PROJECT, Scanner};

use super::walk;
use crate::engine::{
    CommandError, CommandOutput, Engine, Exit, OutputFormat, RunEnv, ScanArgs, ScanTargets,
};
use crate::fakes::{FakeFiles, FakePorts};
use crate::ports::{EntryKind, FsOp};

const PACKAGE: &str = "[package]\nname = \"core\"\n";
const WORKSPACE: &str = "[workspace]\nmembers = [\"crates/*\"]\n";
const DBG: &str = "fn f() {\n    dbg!(1);\n}\n";
const SLEEP: &str = "fn f() {\n    std::thread::sleep(d);\n}\n";

fn engine(files: FakeFiles, style: PathStyle, cwd: &str) -> Engine<FakePorts> {
    let env = RunEnv {
        cwd: PathBuf::from(cwd),
        style,
    };
    let scanner = Scanner::new().expect("every built-in pattern compiles");
    Engine::new(FakePorts::new(files), env, scanner)
}

fn args(targets: ScanTargets, format: OutputFormat) -> ScanArgs {
    ScanArgs { targets, format }
}

fn files_arg(names: &[&str]) -> ScanTargets {
    ScanTargets::Files(names.iter().map(|&name| name.to_owned()).collect())
}

/// `(file, key)` of each hit in a JSON result.
fn json_hits(out: &CommandOutput) -> Vec<(String, String)> {
    let hits: Vec<serde_json::Value> =
        serde_json::from_str(&out.stdout).expect("scan --json writes a JSON array");
    hits.iter()
        .map(|hit| {
            let text = |key: &str| {
                hit.get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned()
            };
            (text("file"), text("key"))
        })
        .collect()
}

#[test]
fn walk_keeps_listing_order_and_skips_build_dirs() {
    use EntryKind::{Dir, File, Other, Symlink};
    let files = FakeFiles::new(Posix)
        .with_listing(
            "/ws",
            &[
                ("z.rs", File),
                ("src", Dir),
                ("a.rs", File),
                ("target", Dir),
                (".git", Dir),
                ("node_modules", Dir),
                (".temper", Dir),
                ("linked", Symlink),
                ("x.rs", Symlink),
                ("notes.txt", File),
                ("fifo.rs", Other),
                ("bad\u{fffd}.rs", File),
            ],
        )
        .with_listing(
            "/ws/src",
            &[("lib.rs", File), ("target", Dir), ("deep", Dir)],
        )
        .with_listing("/ws/src/deep", &[("m.rs", File)]);

    let walked = walk(&files, Posix, "/ws").expect("every directory lists");

    assert_eq!(
        walked,
        [
            "/ws/z.rs",
            "/ws/src/lib.rs",
            "/ws/src/deep/m.rs",
            "/ws/a.rs",
            "/ws/x.rs",
            "/ws/fifo.rs",
            "/ws/bad\u{fffd}.rs",
        ],
        "depth-first in listing order; skipped names at any depth; links not entered"
    );
}

#[test]
fn walk_fails_on_an_unreadable_directory() {
    let files = FakeFiles::new(Posix)
        .with_listing("/ws", &[("src", EntryKind::Dir)])
        .with_unlistable("/ws/src");

    let failed = walk(&files, Posix, "/ws").err();

    assert_eq!(
        failed.map(|e| (e.op(), e.path().to_path_buf())),
        Some((FsOp::ReadDir, PathBuf::from("/ws/src"))),
        "the directory that cannot be listed"
    );
}

/// A workspace with two crates and a loose script, as `oracle/mk.py` lays it out.
fn workspace(style: PathStyle) -> FakeFiles {
    let p = |path: &str| match style {
        Windows => format!("C:{}", path.replace('/', "\\")),
        Posix => path.to_owned(),
    };
    FakeFiles::new(style)
        .with_file(&p("/ws/Cargo.toml"), WORKSPACE)
        .with_file(&p("/ws/crates/core/Cargo.toml"), PACKAGE)
        .with_file(&p("/ws/crates/core/src/lib.rs"), DBG)
        .with_file(&p("/ws/crates/core/src/asks.rs"), SLEEP)
        .with_file(
            &p("/ws/crates/core/tests/it.rs"),
            "fn t() { Some(1).unwrap(); }\n",
        )
        .with_file(&p("/ws/crates/tool/Cargo.toml"), PACKAGE)
        .with_file(
            &p("/ws/crates/tool/src/main.rs"),
            "fn main() { x.unwrap(); }\n",
        )
        .with_file(&p("/ws/scripts/gen.rs"), "fn f() { unsafe {} }\n")
        .with_file(&p("/ws/target/x.rs"), DBG)
}

#[test]
fn scan_all_json_lists_hits_in_scan_order_with_root_relative_files() {
    let engine = engine(workspace(Posix), Posix, "/ws/crates/core");

    let out = engine
        .scan(&args(ScanTargets::All, OutputFormat::Json))
        .expect("the scan runs");

    assert_eq!(
        json_hits(&out),
        [
            ("crates/core/src/asks.rs".to_owned(), "sleep".to_owned()),
            ("crates/core/src/lib.rs".to_owned(), "dbg".to_owned()),
            ("crates/tool/src/main.rs".to_owned(), "unwrap".to_owned()),
            ("scripts/gen.rs".to_owned(), "unsafe".to_owned()),
        ],
        "walk order; tests/ is test code, where unwrap is allowed; target/ is skipped"
    );
    assert_eq!(
        (out.stdout.ends_with("]\n"), out.stderr.as_str(), out.exit),
        (true, "", Exit::Fail),
        "pretty JSON and a newline; hard hits fail"
    );
}

#[test]
fn scan_files_resolve_against_the_cwd_in_windows_style() {
    let engine = engine(workspace(Windows), Windows, r"C:\ws\crates\core");

    let out = engine
        .scan(&args(
            files_arg(&[r"src\lib.rs", r"..\..\scripts\gen.rs"]),
            OutputFormat::Json,
        ))
        .expect("the scan runs");

    assert_eq!(
        json_hits(&out),
        [
            ("crates/core/src/lib.rs".to_owned(), "dbg".to_owned()),
            ("scripts/gen.rs".to_owned(), "unsafe".to_owned()),
        ],
        "files relative to the cwd, reported relative to the root with `/`"
    );
}

#[test]
fn scan_skips_unreadable_and_missing_files() {
    let files = workspace(Posix).with_unreadable("/ws/crates/core/src/locked.rs");
    let engine = engine(files, Posix, "/ws/crates/core");

    let out = engine
        .scan(&args(
            files_arg(&["src/locked.rs", "src/missing.rs", "src/lib.rs"]),
            OutputFormat::Json,
        ))
        .expect("the scan runs");

    assert_eq!(
        json_hits(&out),
        [("crates/core/src/lib.rs".to_owned(), "dbg".to_owned())],
        "only the readable file"
    );
}

#[test]
fn scan_exits_fail_only_with_a_hard_hit() {
    let engine = engine(workspace(Posix), Posix, "/ws/crates/core");
    let exit = |names: &[&str]| {
        engine
            .scan(&args(files_arg(names), OutputFormat::Text))
            .map(|out| out.exit)
            .ok()
    };

    assert_eq!(
        [exit(&["src/asks.rs"]), exit(&[]), exit(&["src/lib.rs"])],
        [Some(Exit::Ok), Some(Exit::Ok), Some(Exit::Fail)],
        "soft only, nothing, and a hard hit"
    );
}

#[test]
fn scan_text_is_the_domain_report() {
    let engine = engine(workspace(Posix), Posix, "/ws/crates/core");

    let out = engine
        .scan(&args(files_arg(&["src/asks.rs"]), OutputFormat::Text))
        .expect("the scan runs");

    assert!(
        out.stdout
            .starts_with("[soft] TST-10 crates/core/src/asks.rs:2: ")
            && out
                .stdout
                .ends_with("\n\n0 blocking, 0 exception request(s), 1 advisory\n"),
        "{}",
        out.stdout
    );
}

#[test]
fn scan_outside_a_rust_project_is_refused_with_exit_2() {
    let engine = engine(FakeFiles::new(Posix), Posix, "/elsewhere");

    let refused = engine
        .scan(&args(ScanTargets::All, OutputFormat::Json))
        .err();

    assert!(
        matches!(
            &refused,
            Some(CommandError::Refused { exit: Exit::Usage, message }) if message == NOT_A_RUST_PROJECT
        ),
        "{refused:?}"
    );
}
