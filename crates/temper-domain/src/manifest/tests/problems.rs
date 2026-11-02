use super::{MEMBER, ROOT, no_lints, toml};
use crate::manifest::{
    GoverningRoot, MOCKING_CRATES, Manifest, ManifestProblem, OwnLints, RootRelation,
    member_problems, split_problems,
};

fn keys(problems: &[ManifestProblem]) -> Vec<String> {
    problems.iter().map(|p| p.key().into_owned()).collect()
}

fn ids(problems: &[ManifestProblem]) -> Vec<&'static str> {
    problems.iter().map(ManifestProblem::id).collect()
}

fn member(root: &Manifest) -> GoverningRoot<'_> {
    GoverningRoot {
        manifest: root,
        relation: RootRelation::Member,
    }
}

#[test]
fn a_mocking_crate_in_dependencies_is_a_member_problem() {
    let root_manifest = toml("[workspace]\nmembers = [\"crates/*\"]\n");
    let root = member(&root_manifest);
    let keys_of = |toml_text: &str| {
        let m = toml(&format!("[package]\nname = \"a\"\n{toml_text}"));
        keys(&member_problems(&m, Some(&root), OwnLints::NotListed))
    };
    let optional = "[dependencies]\nmockito = { version = \"1\", optional = true }\n";
    let mockall = "[dependencies]\nmockall = { version = \"0.13\", optional = true }\n";
    let cases: [(String, &[&str]); 9] = [
        ("[dependencies]\nmockall = \"0.13\"\nserde = \"1\"\n".to_owned(), &["TST-09:mockall"]),
        (
            "[target.'cfg(unix)'.dependencies]\nfake-http = { package = \"wiremock\", version = \"0.6\" }\n".to_owned(),
            &["TST-09:wiremock"],
        ),
        ("[dev-dependencies]\nmockall = \"0.13\"\nmockito = \"1\"\n".to_owned(), &[]),
        (format!("{optional}[features]\ntest-util = [\"dep:mockito\"]\n"), &[]),
        ("[build-dependencies]\nfaux = \"0.1\"\n".to_owned(), &[]),
        // An optional mocking crate that the default features turn on ships in production code.
        (format!("{optional}[features]\ndefault = [\"test-util\"]\ntest-util = [\"dep:mockito\"]\n"), &["TST-09:mockito"]),
        (
            format!("{mockall}[features]\ndefault = [\"std\"]\nstd = [\"mocks\"]\nmocks = [\"mockall/nightly\"]\n"),
            &["TST-09:mockall"],
        ),
        (format!("{mockall}[features]\ndefault = [\"mockall\"]\n"), &["TST-09:mockall"]),
        (
            format!("{mockall}serde = {{ version = \"1\", optional = true }}\n[features]\ndefault = [\"serde\"]\nserde = [\"dep:serde\", \"mockall?/serde\"]\n"),
            &[],
        ),
    ];
    for (text, want) in &cases {
        assert_eq!(keys_of(text), *want, "{text}");
    }
    let self_root = toml("[workspace]\n[workspace.dependencies]\nmockall = \"0.13\"\n");
    let itself = GoverningRoot {
        manifest: &self_root,
        relation: RootRelation::Itself,
    };
    let r = toml(
        "[package]\nname = \"r\"\n[workspace]\n[workspace.dependencies]\nmockall = \"0.13\"\n",
    );
    assert_eq!(member_problems(&r, Some(&itself), OwnLints::NotListed), []);
    let mry = toml("[package]\nname = \"a\"\n[dependencies]\nmry = \"0.10\"\n");
    let problems = member_problems(&mry, Some(&root), OwnLints::NotListed);
    let p = problems.first().expect("mry is a mocking crate");
    assert_eq!(p.id(), "TST-09");
    assert!(p.fix().contains("[dev-dependencies]"), "{}", p.fix());
    for name in ["mockall", "mockito", "faux", "mry"] {
        assert!(MOCKING_CRATES.contains(&name), "{name}");
    }
    // The ratchet compares problems by key, so a second mocking crate is a new problem.
    let before = toml("[package]\nname = \"a\"\n[dependencies]\nmockall = \"0.13\"\n");
    let after =
        toml("[package]\nname = \"a\"\n[dependencies]\nmockall = \"0.13\"\nfaux = \"0.1\"\n");
    let split = split_problems(
        &member_problems(&before, Some(&root), OwnLints::NotListed),
        member_problems(&after, Some(&root), OwnLints::NotListed),
    );
    assert_eq!(keys(&split.fresh), ["TST-09:faux"]);
    assert_eq!(keys(&split.legacy), ["TST-09:mockall"]);
}

#[test]
fn lint_inheritance_and_profiles_follow_the_root() {
    let root_manifest = toml(ROOT);
    let root = member(&root_manifest);
    let bare = toml("[package]\nname = \"a\"\n");
    assert_eq!(
        ids(&member_problems(&bare, Some(&root), OwnLints::NotListed)),
        ["MOD-09"]
    );
    assert_eq!(
        member_problems(&bare, None, OwnLints::NotListed),
        [],
        "a standalone package"
    );
    assert_eq!(
        member_problems(&toml(MEMBER), Some(&root), OwnLints::NotListed),
        []
    );
    let profiled = toml(&MEMBER.replace(
        "[dependencies]",
        "[profile.dev]\nopt-level = 1\n\n[dependencies]",
    ));
    assert_eq!(
        ids(&member_problems(
            &profiled,
            Some(&root),
            OwnLints::NotListed
        )),
        ["PERF-09"]
    );
    let own = toml(
        "[package]\nname = \"r\"\n[workspace]\n[profile.release]\nlto = true\n[lints]\nworkspace = true\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n",
    );
    let itself = GoverningRoot {
        manifest: &own,
        relation: RootRelation::Itself,
    };
    assert_eq!(
        member_problems(&own, Some(&itself), OwnLints::NotListed),
        [],
        "a root may hold profiles"
    );
    let no_ws_lints = toml("[workspace]\nmembers = [\"crates/*\"]\n");
    let quiet = member(&no_ws_lints);
    assert_eq!(
        member_problems(&bare, Some(&quiet), OwnLints::NotListed),
        [],
        "no [workspace.lints] to inherit"
    );
}

#[test]
fn an_own_lints_member_needs_its_own_table() {
    let root_manifest = toml(ROOT);
    let root = member(&root_manifest);
    let own_table = toml(&no_lints().replace(
        "[dependencies]",
        "[lints.rust]\nunsafe_code = \"deny\"\n\n[dependencies]",
    ));
    assert_eq!(
        member_problems(&own_table, Some(&root), OwnLints::Listed),
        []
    );
    let listed_bare = member_problems(&toml(&no_lints()), Some(&root), OwnLints::Listed);
    let [p] = listed_bare.as_slice() else {
        panic!("one problem: {listed_bare:?}")
    };
    assert_eq!(p.id(), "MOD-09");
    assert!(
        p.msg()
            .starts_with("the package is listed in manifests.ownLints"),
        "{}",
        p.msg()
    );
    assert_eq!(
        p.fix(),
        "give it its full, human-approved `[lints]` table (MOD-09, H-05)."
    );
    let unlisted = member_problems(&own_table, Some(&root), OwnLints::NotListed);
    let [p] = unlisted.as_slice() else {
        panic!("one problem: {unlisted:?}")
    };
    assert_eq!(
        p.msg(),
        "the package does not inherit the workspace lints, so clippy would accept unwrap, panic!, as casts and #[allow] in it"
    );
    assert!(!p.new_only(), "MOD-09 is no new check");
}

#[test]
fn split_problems_compares_by_key() {
    let root_manifest = toml(ROOT);
    let root = member(&root_manifest);
    let before = member_problems(
        &toml("[package]\nname = \"a\"\n"),
        Some(&root),
        OwnLints::NotListed,
    );
    let after = member_problems(
        &toml(
            "[package]\nname = \"a\"\n[profile.dev]\nopt-level = 1\n[dependencies]\nfaux = \"1\"\n",
        ),
        Some(&root),
        OwnLints::NotListed,
    );
    let split = split_problems(&before, after);
    assert_eq!(keys(&split.fresh), ["PERF-09", "TST-09:faux"]);
    assert_eq!(keys(&split.legacy), ["MOD-09"]);
    assert!(
        split.fresh.iter().any(ManifestProblem::new_only),
        "TST-09 is new in 1.x"
    );
}
