use temper_domain::PathStyle::{self, Posix, Windows};
use temper_domain::{
    Config, GoverningRoot, Manifest, ManifestProblem, OwnLints, RootRelation, member_problems,
};

use super::{RootFor, own_lints_member, workspace_root_for};
use crate::fakes::FakeFiles;
use crate::paths::PathEnv;

const ROOT: &str = "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"tools/x/\"]\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n";
const A: &str = "[package]\nname = \"a\"\n";

fn toml(text: &str) -> Manifest {
    text.parse().expect("the test manifest is valid TOML")
}

fn at(style: PathStyle) -> PathEnv<'static> {
    PathEnv {
        style,
        cwd: match style {
            Posix => "/elsewhere",
            Windows => r"C:\elsewhere",
        },
    }
}

/// What `workspace_root_for` found: `self`, the root's excludes (which tell the roots of a test
/// apart), or none.
fn found(root: Option<&RootFor>) -> Option<String> {
    root.map(|root| match root {
        RootFor::Itself => "self".to_owned(),
        RootFor::Member(m) => m.workspace_excludes().collect::<Vec<_>>().join(","),
    })
}

fn ids(problems: &[ManifestProblem]) -> Vec<&'static str> {
    problems.iter().map(ManifestProblem::id).collect()
}

/// Ported from `toml.test.mjs`, "workspace root discovery follows Cargo: self,
/// package.workspace, nearest ancestor, exclude", on fake files.
#[test]
fn workspace_root_discovery_follows_cargo_self_package_workspace_nearest_ancestor_exclude() {
    let files = FakeFiles::new(Posix).with_file("/ws/Cargo.toml", ROOT);
    let a = toml(A);
    let own = toml(
        "[package]\nname = \"r\"\n[workspace]\n[profile.release]\nlto = true\n[lints]\nworkspace = true\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n",
    );

    let root = workspace_root_for(&files, at(Posix), "/ws/crates/a/Cargo.toml", &a);
    let excluded = workspace_root_for(&files, at(Posix), "/ws/tools/x/Cargo.toml", &a);
    let itself = workspace_root_for(&files, at(Posix), "/ws/tools/x/Cargo.toml", &own);
    let nowhere = workspace_root_for(&files, at(Posix), "/tmp/no-such-dir-temper/Cargo.toml", &a);

    let Some(RootFor::Member(root_manifest)) = &root else {
        panic!(
            "the workspace root governs crates/a: {:?}",
            found(root.as_ref())
        );
    };
    let governing = GoverningRoot {
        manifest: root_manifest,
        relation: RootRelation::Member,
    };
    assert_eq!(
        ids(&member_problems(&a, Some(&governing), OwnLints::NotListed)),
        ["MOD-09"],
        "the member does not inherit the root's lints"
    );
    assert_eq!(
        [found(excluded.as_ref()), found(itself.as_ref())],
        [None, Some("self".to_owned())],
        "an excluded crate has no root; a manifest with [workspace] is its own"
    );
    let own_root = GoverningRoot {
        manifest: &own,
        relation: RootRelation::Itself,
    };
    assert_eq!(
        ids(&member_problems(&own, Some(&own_root), OwnLints::NotListed)),
        Vec::<&str>::new(),
        "a root may carry a profile and inherit its own lints"
    );
    assert_eq!(found(nowhere.as_ref()), None, "no manifest above");
}

#[test]
fn package_workspace_names_the_root_and_nothing_else_is_searched() {
    let files = FakeFiles::new(Posix)
        .with_file("/ws/Cargo.toml", ROOT)
        .with_file("/ws/other/Cargo.toml", A)
        .with_file("/ws/broken/Cargo.toml", "[workspace\n");
    let explicit = |path: &str| {
        toml(&format!(
            "[package]\nname = \"b\"\nworkspace = \"{path}\"\n"
        ))
    };

    let got = [
        "../..",
        "/ws/",
        "../../other",
        "../../broken",
        "../../missing",
        "",
    ]
    .map(|path| {
        found(
            workspace_root_for(
                &files,
                at(Posix),
                "/ws/crates/b/Cargo.toml",
                &explicit(path),
            )
            .as_ref(),
        )
    });

    assert_eq!(
        got,
        [
            Some("tools/x/".to_owned()),
            Some("tools/x/".to_owned()),
            None,
            None,
            None,
            None,
        ],
        "`path.resolve(crateDir, explicit, 'Cargo.toml')` must hold a [workspace]"
    );
}

/// 1.x tests `/^\s*\[\s*workspace\s*[\].]/m` before it parses an ancestor, and goes on up past
/// one that does not parse or has no `[workspace]` table.
#[test]
fn the_nearest_ancestor_with_a_workspace_table_governs() {
    let near =
        "# root\n  [ workspace.package ]\nversion = \"1\"\n[workspace]\nexclude = [\"near\"]\n";
    let files = FakeFiles::new(Posix)
        .with_file("/ws/Cargo.toml", ROOT)
        .with_file("/ws/a/Cargo.toml", near)
        .with_file("/ws/b/Cargo.toml", "[workspace\nmembers = [\n")
        .with_file("/ws/c/Cargo.toml", "x = \"[workspace]\"\n")
        .with_unreadable("/ws/d/Cargo.toml");
    let a = toml(A);

    let got = ["a", "b", "c", "d"].map(|dir| {
        found(
            workspace_root_for(&files, at(Posix), &format!("/ws/{dir}/m/Cargo.toml"), &a).as_ref(),
        )
    });

    assert_eq!(
        got,
        [
            Some("near".to_owned()),
            Some("tools/x/".to_owned()),
            Some("tools/x/".to_owned()),
            Some("tools/x/".to_owned()),
        ],
        "an unparsable, a mentioned-only or an unreadable manifest is passed over"
    );
}

/// `rel === x || rel.startsWith(x + '/')` over `cleanRel`'d paths, so `./tools/gen/` excludes
/// `tools/gen` and `tools/gen/sub` but not `tools/generate`, on either platform.
#[test]
fn an_exclude_entry_is_compared_by_path_components() {
    let root = "[workspace]\nexclude = [\"./tools/gen/\", 5]\n";
    let posix = FakeFiles::new(Posix).with_file("/ws/Cargo.toml", root);
    let windows = FakeFiles::new(Windows).with_file(r"C:\ws\Cargo.toml", root);
    let a = toml(A);

    let got = ["tools/gen", "tools/gen/sub", "tools/generate", "5"].map(|dir| {
        let posix = workspace_root_for(&posix, at(Posix), &format!("/ws/{dir}/Cargo.toml"), &a);
        let path = format!(r"C:\ws\{}\Cargo.toml", dir.replace('/', r"\"));
        let windows = workspace_root_for(&windows, at(Windows), &path, &a);
        (posix.is_some(), windows.is_some())
    });

    assert_eq!(
        got,
        [(false, false), (false, false), (true, true), (false, false)],
        "excluded, excluded, a member, excluded by `String(5)`"
    );
}

/// 1.x `ownLintsMember`: the manifest's directory relative to the project root, against each
/// `manifests.ownLints` glob, both through `cleanRel`.
#[test]
fn own_lints_members_are_matched_by_directory_glob() {
    let config = Config::load(Some(
        r#"{"manifests":{"ownLints":["./crates/foo-sys/", "vendor/*"]}}"#,
    ))
    .config;

    let got = [
        "/ws/crates/foo-sys/Cargo.toml",
        "/ws/vendor/zlib/Cargo.toml",
        "/ws/vendor/zlib/sub/Cargo.toml",
        "/ws/crates/bar-sys/Cargo.toml",
    ]
    .map(|path| own_lints_member(at(Posix), &config, "/ws", path).expect("the globs compile"));

    assert_eq!(
        got,
        [
            OwnLints::Listed,
            OwnLints::Listed,
            OwnLints::NotListed,
            OwnLints::NotListed,
        ],
        "globMatch(cleanRel(e), dir)"
    );
}
