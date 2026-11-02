use super::toml;
use crate::manifest::{WatchedKey, benign_when_new, watched_keys};

fn ids(text: &str) -> Vec<(String, &'static str)> {
    watched_keys(&toml(text))
        .into_iter()
        .map(|(label, w)| (label, w.id))
        .collect()
}

fn values(text: &str) -> Vec<(String, String)> {
    watched_keys(&toml(text))
        .into_iter()
        .map(|(label, w)| (label, w.value))
        .collect()
}

fn pairs<'a>(want: &[(&'a str, &'a str)]) -> Vec<(String, &'a str)> {
    want.iter()
        .map(|&(label, id)| (label.to_owned(), id))
        .collect()
}

#[test]
fn watched_manifest_switches_carry_their_rule_id() {
    let got = ids(
        "[package]\nrust-version = \"1.96\"\nautobins = false\n[lib]\ndoctest = false\n[[example]]\nname = \"demo\"\nrequired-features = [\"cli\"]\n[patch.crates-io]\nfoo = { path = \"../foo\" }\n[features]\ndefault = [\"std\"]\n",
    );
    // 1.x's Map in insertion order; its test compares the same entries as an object.
    let want = [
        ("package.autobins", "H-12"),
        ("package.rust-version", "H-16"),
        ("features.default", "H-16"),
        ("patch", "H-16"),
        ("[lib] doctest", "H-12"),
        ("[[example]] demo required-features", "H-12"),
    ];
    assert_eq!(got, pairs(&want));
}

#[test]
fn watched_values_are_json_and_targets_are_named_or_numbered() {
    let got = values(
        "[[bin]]\ntest = false\nbench = false\n[[bin]]\nname = \"x\"\nharness = false\n[[bench]]\nname = \"b\"\nharness = false\ntest = true\n[workspace]\nresolver = \"3\"\n[workspace.package]\nedition = \"2024\"\n[package]\nedition = { workspace = true }\n",
    );
    let want = [
        ("package.edition", r#"{"workspace":true}"#),
        ("workspace.package.edition", r#""2024""#),
        ("workspace.resolver", r#""3""#),
        ("[[bin]] #1 test", "false"),
        ("[[bin]] x harness", "false"),
        ("[[bench]] b test", "true"),
    ];
    let want: Vec<_> = want
        .iter()
        .map(|&(l, v)| (l.to_owned(), v.to_owned()))
        .collect();
    assert_eq!(
        got, want,
        "`bench` on a bin and `harness` on a bench are not watched"
    );
}

#[test]
fn a_repeated_label_keeps_its_first_place_and_its_last_value() {
    let got = values(
        "[[bin]]\nname = \"x\"\ntest = false\n[[bin]]\nname = \"y\"\ntest = false\n[[bin]]\nname = \"x\"\ntest = true\n",
    );
    let want = [("[[bin]] x test", "true"), ("[[bin]] y test", "false")];
    let want: Vec<_> = want
        .iter()
        .map(|&(l, v)| (l.to_owned(), v.to_owned()))
        .collect();
    assert_eq!(got, want, "a JS Map's set");
}

#[test]
fn benign_when_new_allows_inheritance_the_standard_edition_and_default_features() {
    let key = |value: &str| WatchedKey {
        id: "H-16",
        why: "",
        value: value.to_owned(),
    };
    assert!(
        benign_when_new("package.rust-version", &key(r#"{"workspace":true}"#)),
        "inheritance"
    );
    assert!(
        benign_when_new("package.edition", &key(r#""2024""#)),
        "edition 2024"
    );
    assert!(
        !benign_when_new("package.edition", &key(r#""2021""#)),
        "edition 2021"
    );
    assert!(
        benign_when_new("workspace.resolver", &key(r#""3""#)),
        "resolver 3"
    );
    assert!(
        !benign_when_new("workspace.resolver", &key(r#""2""#)),
        "resolver 2"
    );
    assert!(
        benign_when_new("features.default", &key(r#"["std"]"#)),
        "default features"
    );
    assert!(
        !benign_when_new("package.build", &key(r#""build.rs""#)),
        "a build script"
    );
}
