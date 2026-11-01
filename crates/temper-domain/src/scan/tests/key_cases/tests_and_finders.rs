//! `KEY_CASES` for the test rules and the finders: `should_panic`, weak asserts, ambient clocks.

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
