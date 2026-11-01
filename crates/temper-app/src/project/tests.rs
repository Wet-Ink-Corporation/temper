use std::path::PathBuf;

use temper_domain::LibTarget;
use temper_domain::PathStyle::{Posix, Windows};

use super::{CrateRoot, Project, crate_of, declares_package, discover, exists};
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::paths::PathEnv;
use crate::ports::{ExitKind, Stream};

const PACKAGE: &str = "[package]\nname = \"core\"\n";
const WORKSPACE: &str = "[workspace]\nmembers = [\"crates/*\"]\n";

/// 1.x's `/^\s*\[package\]/m`, run under Node 22.22.0 on each row.
#[test]
fn declares_package_like_1x() {
    let declares = [
        "[package]",
        "  [package]\nname=1",
        "x = 1\n[package]",
        "\u{feff}[package]",
        "\u{a0}[package]",
        "a\r[package]",
        "a\u{2028} [package]",
        "[workspace]\n\n\t [package]",
        "x\n  \n  [package]",
        "\"\"\"\n[package]\n\"\"\"",
        "x = 1 [package]\n[package]",
    ];
    let does_not = [
        "x = 1 [package]",
        "\u{85}[package]",
        "# [package]",
        "[ package ]",
        "[package.metadata]",
        "",
    ];

    let wrong: Vec<&str> = declares
        .iter()
        .filter(|text| !declares_package(text))
        .chain(does_not.iter().filter(|text| declares_package(text)))
        .copied()
        .collect();

    assert_eq!(
        wrong,
        Vec::<&str>::new(),
        "manifests read as 1.x reads them"
    );
}

#[test]
fn crate_of_finds_the_nearest_package_manifest() {
    let files = FakeFiles::new(Posix)
        .with_file("/ws/Cargo.toml", WORKSPACE)
        .with_file("/ws/crates/core/Cargo.toml", PACKAGE)
        .with_file("/ws/crates/core/src/lib.rs", "")
        .with_file("/ws/crates/core/src/a/b.rs", "")
        .with_unreadable("/ws/crates/core/src/a/Cargo.toml")
        .with_file("/ws/crates/tool/Cargo.toml", PACKAGE)
        .with_file("/ws/crates/tool/src/main.rs", "")
        .with_file("/ws/scripts/gen.rs", "");
    let at = PathEnv {
        style: Posix,
        cwd: "/ws/crates/core",
    };
    let root = |dir: &str, lib| {
        Some(CrateRoot {
            dir: dir.to_owned(),
            lib,
        })
    };

    let found = [
        "src/a/b.rs",
        "/ws/crates/tool/src/main.rs",
        "../../scripts/gen.rs",
    ]
    .map(|file| crate_of(&files, at, file));

    assert_eq!(
        found,
        [
            // The unreadable `src/a/Cargo.toml` is passed over.
            root("/ws/crates/core", LibTarget::Present),
            root("/ws/crates/tool", LibTarget::Absent),
            // A `[workspace]`-only manifest is not a crate.
            None,
        ],
        "the nearest [package] directory, with its library target"
    );
}

#[test]
fn discover_without_git_takes_the_outermost_cargo_directory() {
    let files = FakeFiles::new(Windows)
        .with_file(r"C:\ws\Cargo.toml", WORKSPACE)
        .with_file(r"C:\ws\crates\core\Cargo.toml", PACKAGE)
        .with_file(r"C:\ws\crates\core\src\lib.rs", "");
    let env = RunEnv {
        cwd: PathBuf::from(r"C:\ws\crates\core\src"),
        style: Windows,
    };

    assert_eq!(
        discover(&FakePorts::new(files), &env),
        Some(Project {
            root: r"C:\ws".to_owned()
        }),
        "the workspace root, not the crate"
    );
}

#[test]
fn discover_is_none_without_a_cargo_toml() {
    let files = FakeFiles::new(Posix).with_file("/elsewhere/Cargo.toml", PACKAGE);
    let env = RunEnv {
        cwd: PathBuf::from("/ws/src"),
        style: Posix,
    };

    assert_eq!(
        discover(&FakePorts::new(files), &env),
        None,
        "not a Rust project"
    );
}

#[test]
fn exists_is_false_when_the_path_cannot_be_resolved() {
    let files = FakeFiles::new(Posix)
        .with_file("/ws/Cargo.toml", PACKAGE)
        .with_file("/ws/locked/Cargo.toml", PACKAGE)
        .with_unresolvable("/ws/locked/Cargo.toml");

    assert_eq!(
        [
            exists(&files, "/ws/Cargo.toml"),
            exists(&files, "/ws/locked/Cargo.toml"),
        ],
        [true, false],
        "existsSync answers false on any error"
    );
}

/// `git rev-parse --show-toplevel` in `cwd` answering `reply`.
fn git_in(cwd: &str, reply: Reply) -> FakeProcess {
    FakeProcess::new().answering(&["git", "rev-parse", "--show-toplevel"], cwd, reply)
}

fn exit(status: ExitKind, stdout: &str) -> Reply {
    Reply::Exit {
        status,
        stdout: stdout.to_owned(),
    }
}

#[test]
fn discover_prefers_the_git_top_level() {
    let cwd = r"C:\g\rust\crates\core";
    let ports = FakePorts {
        files: FakeFiles::new(Windows)
            .with_file(r"C:\g\rust\Cargo.toml", WORKSPACE)
            .with_file(r"C:\g\rust\crates\core\Cargo.toml", PACKAGE),
        process: git_in(cwd, exit(ExitKind::Code(0), "C:/g\n")),
    };
    let env = RunEnv {
        cwd: PathBuf::from(cwd),
        style: Windows,
    };

    assert_eq!(
        discover(&ports, &env),
        Some(Project {
            root: r"C:\g".to_owned()
        }),
        "git's answer, trimmed and resolved, even without a Cargo.toml at the top"
    );
}

#[test]
fn discover_falls_back_without_git() {
    let cwd = "/g/rust/crates/core";
    let replies = [
        exit(ExitKind::Code(128), "/g\n"),
        exit(ExitKind::Signal, "/g\n"),
        exit(ExitKind::TimedOut, "/g\n"),
        Reply::SpawnFails,
        Reply::Overflows(Stream::Stdout),
    ];
    let env = RunEnv {
        cwd: PathBuf::from(cwd),
        style: Posix,
    };

    let roots = replies.map(|reply| {
        let ports = FakePorts {
            files: FakeFiles::new(Posix)
                .with_file("/g/rust/Cargo.toml", WORKSPACE)
                .with_file("/g/rust/crates/core/Cargo.toml", PACKAGE),
            process: git_in(cwd, reply),
        };
        discover(&ports, &env).map(|project| project.root)
    });

    assert_eq!(
        roots,
        [(); 5].map(|()| Some("/g/rust".to_owned())),
        "1.x reads any failed git run as no git root"
    );
}

#[test]
fn discover_is_none_in_a_git_repo_without_cargo_toml_above_the_cwd() {
    let found = ["/g", "/g/docs"].map(|cwd| {
        let ports = FakePorts {
            files: FakeFiles::new(Posix)
                .with_file("/g/rust/Cargo.toml", WORKSPACE)
                .with_file("/g/docs/guide.md", ""),
            process: git_in(cwd, exit(ExitKind::Code(0), "/g\n")),
        };
        let env = RunEnv {
            cwd: PathBuf::from(cwd),
            style: Posix,
        };
        discover(&ports, &env)
    });

    assert_eq!(
        found,
        [None, None],
        "a Cargo.toml below the cwd does not count"
    );
}
