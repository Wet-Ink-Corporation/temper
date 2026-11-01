//! `KEY_CASES` for the test rules and the finders: `should_panic`, weak asserts, test branches,
//! serialized hash maps and ambient clocks.

use super::{assert_hits, assert_misses};
use crate::scan::CodeKind;

#[test]
fn should_panic_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "should-panic",
        &[
            (
                CodeKind::Test,
                "#[test]\n#[should_panic]\nfn f() { g(); }",
                &["TST-05:soft@2"],
            ),
            (
                CodeKind::Prod,
                "pub fn g() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    #[should_panic(expected = \"boom\")]\n    fn f() { super::g(); }\n}",
                &["TST-05:soft@5"],
            ),
        ],
    );
}

#[test]
fn should_panic_near_misses_do_not_fire() {
    assert_misses(
        "should-panic",
        &[
            (CodeKind::Prod, "#[should_panic]\npub fn g() {}"),
            (
                CodeKind::Test,
                "#[test]\nfn f() { assert_eq!(g(), Err(E::Boom)); } // #[should_panic]",
            ),
        ],
    );
}

#[test]
fn weak_assert_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "weak-assert",
        &[
            (
                CodeKind::Test,
                "#[test]\nfn f() {\n    assert!(parse(\"1\").is_ok());\n}",
                &["TST-05:soft@3"],
            ),
            (
                CodeKind::Test,
                "fn f() { assert!(map.get(&k).is_none()); }",
                &["TST-05:soft@1"],
            ),
        ],
    );
}

#[test]
fn weak_assert_near_misses_do_not_fire() {
    assert_misses(
        "weak-assert",
        &[
            (
                CodeKind::Prod,
                "pub fn f(r: Result<u8, E>) { assert!(r.is_ok()); }",
            ),
            (
                CodeKind::Test,
                "fn f() { assert_eq!(parse(\"1\"), Ok(1)); assert!(r.is_ok() && done); }",
            ),
        ],
    );
}

#[test]
fn test_branch_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "test-branch",
        &[
            (
                CodeKind::Prod,
                "pub fn timeout_secs() -> u64 {\n    if cfg!(test) { 1 } else { 30 }\n}",
                &["TST-09:hard@2"],
            ),
            (
                CodeKind::Prod,
                "pub fn f() -> u8 {\n    #[cfg(test)]\n    return 0;\n    1\n}",
                &["TST-09:hard@2"],
            ),
            (
                CodeKind::Bin,
                "fn main() {\n    if cfg!(not(test)) { run(); }\n}",
                &["TST-09:hard@2"],
            ),
            (
                CodeKind::Prod,
                "impl Store {\n    pub fn open() -> Self {\n        let dir = if cfg!(any(test, feature = \"x\")) { \"t\" } else { \"p\" };\n        Self(dir)\n    }\n}",
                &["TST-09:hard@3"],
            ),
        ],
    );
}

#[test]
fn test_branch_near_misses_do_not_fire() {
    assert_misses(
        "test-branch",
        &[
            (
                CodeKind::Prod,
                "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    fn helper() -> bool { cfg!(test) }\n}",
            ),
            (
                CodeKind::Prod,
                "#[cfg(any(test, feature = \"test-util\"))]\npub mod fakes;\n#[cfg(test)]\nfn fixture() -> u8 { 1 }\npub fn f() -> bool { cfg!(feature = \"test\") || cfg!(debug_assertions) }",
            ),
            (CodeKind::Test, "fn helper() -> bool { cfg!(test) }"),
            (
                CodeKind::Prod,
                "pub fn f() -> &'static str { \"cfg!(test)\" } // #[cfg(test)]",
            ),
            (
                CodeKind::Prod,
                "pub trait Port {\n    #[cfg(test)]\n    fn probe(&self);\n}",
            ),
        ],
    );
}

#[test]
fn test_branch_like_1x() {
    let cases: [(&str, &[&str]); 4] = [
        (
            "fn decl();\nconst C: bool = cfg!(test);\npub fn f() { if cfg!(test) {} }",
            &["TST-09:hard@3"],
        ),
        (
            "fn f() { #[cfg(all(unix, test))] g(); }\n#[test]\nfn t() { if cfg!(test) {} }",
            &["TST-09:hard@1"],
        ),
        ("fn r#match() { if cfg!(test) {} }", &["TST-09:hard@1"]),
        (
            "fn f() { let a = [cfg!(test)]; } fn g() { cfg!(testing); cfg!(tes",
            &["TST-09:hard@1"],
        ),
    ];

    for (src, want) in cases {
        assert_hits(
            "test-branch",
            &[(CodeKind::Prod, src, want), (CodeKind::Example, src, want)],
        );
        assert_misses("test-branch", &[(CodeKind::Test, src)]);
    }
}

#[test]
fn serde_hashmap_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "serde-hashmap",
        &[
            (
                CodeKind::Prod,
                "use std::collections::HashMap;\n#[derive(Debug, Serialize)]\npub struct Report {\n    counts: HashMap<String, u64>,\n}",
                &["OUT-04:soft@4"],
            ),
            (
                CodeKind::Bin,
                "#[derive(serde::Serialize)]\nenum Event {\n    Seen { ids: std::collections::HashSet<u64> },\n}",
                &["OUT-04:soft@3"],
            ),
            (
                CodeKind::Prod,
                "#[cfg_attr(feature = \"serde\", derive(Serialize))]\npub struct Tally(pub HashMap<u8, u8>);",
                &["OUT-04:soft@2"],
            ),
        ],
    );
}

#[test]
fn serde_hashmap_near_misses_do_not_fire() {
    assert_misses(
        "serde-hashmap",
        &[
            (
                CodeKind::Prod,
                "#[derive(Debug, Deserialize)]\npub struct Input {\n    counts: HashMap<String, u64>,\n}",
            ),
            (
                CodeKind::Prod,
                "#[derive(Serialize)]\npub struct Report {\n    counts: BTreeMap<String, u64>,\n    #[serde(serialize_with = \"sorted\", default)]\n    extra: HashMap<String, u64>,\n    #[serde(skip)]\n    cache: HashMap<u8, u8>,\n}",
            ),
            (
                CodeKind::Prod,
                "#[derive(Serialize)]\npub struct A { x: u8 }\npub struct B { m: HashMap<u8, u8> }",
            ),
            (
                CodeKind::Test,
                "#[derive(Serialize)]\nstruct Fixture { m: HashMap<u8, u8> }",
            ),
        ],
    );
}

#[test]
fn serde_hashmap_like_1x() {
    assert_hits(
        "serde-hashmap",
        &[
            (
                CodeKind::Prod,
                "#[derive(Serialize)]\n#[serde(rename_all = \"x\")]\npub(crate) struct A<T: Into<u8>> {\n    a: u8,\n    b: Vec<HashMap<u8, T>>,\n    #[serde(with = \"m\")] c: HashSet<u8>,\n    d: (HashMap<u8, u8>, u8),\n}",
                &["OUT-04:soft@5", "OUT-04:soft@7"],
            ),
            (
                CodeKind::Prod,
                "#[derive(Serialize)]\nenum C { V(HashMap<u8,u8>), W { s: HashSet<u8> } }",
                &["OUT-04:soft@2", "OUT-04:soft@2"],
            ),
            (
                CodeKind::Prod,
                "#[derive(Serialize)]\nstruct E(#[serde(skip_serializing)] HashMap<u8,u8>, HashSet<u8>);",
                &["OUT-04:soft@2"],
            ),
            (
                CodeKind::Prod,
                "#[derive(Serialize)]\nstruct F { é: u8, m: HashMap<u8, u8> }",
                &["OUT-04:soft@2"],
            ),
            // 1.x's `matchAngle(..) + 1` is 0 when the list does not close: the walk restarts
            // at the start of the file.
            (
                CodeKind::Prod,
                "struct Pre { m: HashMap<u8,u8> }\n#[derive(Serialize)]\nstruct B<T: X {",
                &["OUT-04:soft@1"],
            ),
        ],
    );
    assert_misses(
        "serde-hashmap",
        &[
            (
                CodeKind::Prod,
                "#[derive(Serialize)]\nstruct D;\nstruct Z { n: HashMap<u8, u8> }",
            ),
            (
                CodeKind::Prod,
                "#[derive(Serialize)] fn f() {}\nstruct Z { n: HashMap<u8, u8> }",
            ),
            (
                CodeKind::Prod,
                "#[derive(Serialize)]\nstruct B<T: X {\n    m: HashMap<u8, u8>,\n}\nstruct Z { n: HashMap<u8, u8> }",
            ),
        ],
    );
}

#[test]
fn ambient_now_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "ambient-now",
        &[
            (
                CodeKind::Prod,
                "pub fn stamp() -> SystemTime {\n    SystemTime::now()\n}",
                &["OUT-04:soft@2"],
            ),
            (
                CodeKind::Prod,
                "pub fn id() -> Uuid { Uuid::new_v4() }",
                &["OUT-04:soft@1"],
            ),
            (
                CodeKind::Prod,
                "pub fn t() -> DateTime<Utc> { chrono::Utc::now() }",
                &["OUT-04:soft@1"],
            ),
            (
                CodeKind::Prod,
                "pub fn t() -> OffsetDateTime { OffsetDateTime::now_utc() }",
                &["OUT-04:soft@1"],
            ),
        ],
    );
}

#[test]
fn ambient_now_near_misses_do_not_fire() {
    assert_misses(
        "ambient-now",
        &[
            (
                CodeKind::Bin,
                "fn main() { let _t = std::time::SystemTime::now(); }",
            ),
            (
                CodeKind::Prod,
                "impl Clock for SystemClock {\n    fn now(&self) -> SystemTime { SystemTime::now() }\n}",
            ),
            (
                CodeKind::Prod,
                "pub fn elapsed() -> Duration { let t = Instant::now(); t.elapsed() }",
            ),
            (CodeKind::Test, "fn f() { let _id = Uuid::new_v4(); }"),
            (
                CodeKind::Prod,
                "pub fn f(clock: &impl Clock) -> u64 { clock.now() } // SystemTime::now()",
            ),
        ],
    );
}

#[test]
fn ambient_now_trait_impls() {
    assert_hits(
        "ambient-now",
        &[(
            CodeKind::Prod,
            "impl<T> for<'a> X {}\nimpl Clock for Sys { fn now(&self) -> SystemTime { SystemTime::now() } }\nimpl Sys { fn t() { SystemTime::now(); } }\nimpl<F: Fn() -> u8> Clock for F { fn now(&self) { Utc::now(); } }",
            &["OUT-04:soft@3"],
        )],
    );
}
