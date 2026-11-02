//! `manifest_checks`, one case per message text. Each expected text was printed by 1.x's
//! `manifestChecks` on the same manifests, with `ROOT` as the workspace root two levels up.

use super::{MEMBER, ROOT, no_lints, toml};
use crate::manifest::{
    Before, GoverningRoot, Manifest, ManifestEditFacts, ManifestFindings, OwnLints, RootRelation,
    manifest_checks,
};

const REL: &str = "crates/ledger-domain/Cargo.toml";

/// 1.x `manifestChecks(file, REL, before, after, proj)` for a member under `ROOT`.
fn check(before: &str, after: &str) -> ManifestFindings {
    let root = toml(ROOT);
    let after = toml(after);
    let parsed_before = before.parse::<Manifest>();
    let before = match (before, &parsed_before) {
        ("", _) => Before::Empty,
        (_, Ok(m)) => Before::Parsed(m),
        (_, Err(_)) => Before::Invalid,
    };
    let root = GoverningRoot {
        manifest: &root,
        relation: RootRelation::Member,
    };
    let facts = ManifestEditFacts {
        rel: REL,
        before,
        after: &after,
        root: Some(root),
        own_lints: OwnLints::NotListed,
    };
    manifest_checks(&facts)
}

#[test]
fn a_new_member_without_inheritance_is_denied_and_asks_as_a_new_crate() {
    let found = check("", &no_lints());
    assert_eq!(
        found.denies,
        [
            "MOD-09 crates/ledger-domain/Cargo.toml: the package does not inherit the workspace lints, so clippy would accept unwrap, panic!, as casts and #[allow] in it.\n    fix: add a `[lints]` table containing only `workspace = true` (MOD-09, ARC-01). A crate that needs its own lints (an approved FFI crate) is listed by a human in `.claude/temper.json` under `manifests.ownLints`."
        ]
    );
    assert_eq!(
        found.asks,
        [
            "crates/ledger-domain/Cargo.toml: new crate `ledger-domain` in the workspace. Crate boundaries are a human decision (H-16, ARC-07), and the gate fails until the crate has its charter (`AGENTS.md`, ARC-08)."
        ]
    );
    assert_eq!(found.advice, [""; 0]);
}

#[test]
fn a_problem_the_manifest_already_had_only_advises() {
    let found = check(
        &no_lints(),
        &no_lints().replace("Pure account rules", "Account rules"),
    );
    assert_eq!(found.denies, [""; 0]);
    assert_eq!(
        found.advice,
        [
            "MOD-09 crates/ledger-domain/Cargo.toml (already the case before this edit): the package does not inherit the workspace lints, so clippy would accept unwrap, panic!, as casts and #[allow] in it.\n    fix: add a `[lints]` table containing only `workspace = true` (MOD-09, ARC-01). A crate that needs its own lints (an approved FFI crate) is listed by a human in `.claude/temper.json` under `manifests.ownLints`."
        ]
    );
}

#[test]
fn a_watched_switch_change_asks() {
    let found = check(
        MEMBER,
        &MEMBER.replace("[lints]", "[lib]\ntest = false\n\n[lints]"),
    );
    assert_eq!(
        found.asks,
        [
            "crates/ledger-domain/Cargo.toml: `[lib] test` changes (unset -> false); this switch can stop tests, doctests or targets from running (H-12)."
        ]
    );
    let removed = check(
        &MEMBER.replace("[lints]", "[lib]\ntest = false\n\n[lints]"),
        MEMBER,
    );
    assert_eq!(
        removed.asks,
        [
            "crates/ledger-domain/Cargo.toml: `[lib] test` changes (false -> unset); this switch can stop tests, doctests or targets from running (H-12)."
        ]
    );
}

#[test]
fn a_long_watched_value_is_cut_at_77_units() {
    let long = format!(
        "[patch.crates-io]\nfoo = {{ path = \"{}\" }}\n",
        "x".repeat(80)
    );
    let found = check(MEMBER, &format!("{MEMBER}{long}"));
    let cut = format!(r#"{{"crates-io":{{"foo":{{"path":"{}..."#, "x".repeat(48));
    assert_eq!(
        found.asks,
        [format!(
            "crates/ledger-domain/Cargo.toml: `patch` changes (unset -> {cut}); it overrides where a dependency comes from (H-16)."
        )]
    );
}

#[test]
fn an_astral_char_at_the_cut_is_left_out_whole() {
    // D16(o): the emoji fills units 76 and 77, and 1.x's `slice(0, 77)` keeps its high
    // surrogate alone (Node prints `\ud83d`); the port cuts before the char.
    let path = format!("{}\u{1F600}{}", "x".repeat(47), "y".repeat(30));
    let found = check(
        MEMBER,
        &format!("{MEMBER}[patch.crates-io]\nfoo = {{ path = \"{path}\" }}\n"),
    );
    let cut = format!(r#"{{"crates-io":{{"foo":{{"path":"{}..."#, "x".repeat(47));
    assert!(
        found
            .asks
            .first()
            .is_some_and(|ask| ask.contains(&format!("(unset -> {cut})"))),
        "{:?}",
        found.asks
    );
}

#[test]
fn benign_values_in_a_new_manifest_do_not_ask() {
    let new = MEMBER.replace(
        "[lints]",
        "[features]\ndefault = [\"std\"]\nstd = []\n\n[lints]",
    );
    let found = check(
        "",
        &new.replace("edition.workspace = true", "edition = \"2024\""),
    );
    assert_eq!(found.asks.len(), 1, "only the new crate: {:?}", found.asks);
    let old_edition = check(
        "",
        &MEMBER.replace("edition.workspace = true", "edition = \"2021\""),
    );
    assert_eq!(
        old_edition.asks.get(1).map(String::as_str),
        Some(
            "crates/ledger-domain/Cargo.toml: `package.edition` changes (unset -> \"2021\"); MSRV, edition and resolver changes are human decisions (H-16)."
        )
    );
}

#[test]
fn a_major_bump_asks_and_a_non_caret_requirement_advises() {
    let before = MEMBER.replace(
        "thiserror = { workspace = true }",
        "serde = \"1.0\"\nrand = \"0.8\"",
    );
    let after = MEMBER.replace(
        "thiserror = { workspace = true }",
        "serde = \"2\"\nrand = \"=0.8.5\"",
    );
    let found = check(&before, &after);
    assert_eq!(
        found.asks,
        [
            "crates/ledger-domain/Cargo.toml: `serde` moves to a new major version (\"1.0\" -> \"2\"). Major-version bumps are a human decision (H-16, ARC-12)."
        ]
    );
    assert_eq!(
        found.advice,
        [
            "ARC-12 crates/ledger-domain/Cargo.toml: `rand = \"=0.8.5\"` is an exact `=` pin.\n    fix: declare a caret floor at the oldest version whose API you use (`\"1.4\"`), never `=`, `~`, `*` or an upper bound."
        ]
    );
}

#[test]
fn an_invalid_before_skips_the_ratchet_and_the_watched_keys() {
    let found = check(
        "[package\n",
        &no_lints().replace("[dependencies]", "[lib]\ntest = false\n[dependencies]"),
    );
    assert_eq!(
        found.denies.len(),
        1,
        "every problem is new: {:?}",
        found.denies
    );
    assert_eq!(
        found.asks, [""; 0],
        "no watched comparison and no new-crate ask"
    );
}

#[test]
fn a_new_crate_without_a_name_is_named_by_its_path() {
    let found = check(
        "",
        "[package]\nversion = \"1.0.0\"\n[lints]\nworkspace = true\n",
    );
    assert!(
        found.asks.first().is_some_and(|ask| ask.starts_with(
            "crates/ledger-domain/Cargo.toml: new crate `crates/ledger-domain/Cargo.toml` in"
        )),
        "{:?}",
        found.asks
    );
    let numbered = check("", "[package]\nname = 7\n[lints]\nworkspace = true\n");
    assert!(
        numbered
            .asks
            .first()
            .is_some_and(|ask| ask.contains("new crate `7` in")),
        "String(7)"
    );
}
