//! `KEY_CASES` for the escape-hatch rules: leaks, `catch_unwind`, `exit`, `let _`, lazy statics,
//! `#[path]` and `async_trait`.

use super::{assert_hits, assert_misses};
use crate::scan::CodeKind;

#[test]
fn leak_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "leak",
        &[
            (
                CodeKind::Prod,
                "pub fn f(b: Box<u8>) -> &'static u8 {\n    Box::leak(b)\n}",
                &["H-07:hard@2"],
            ),
            (
                CodeKind::Test,
                "fn f(v: Vec<u8>) { std::mem::forget(v); }",
                &["H-07:hard@1"],
            ),
            (CodeKind::Bin, "static mut N: u8 = 0;", &["H-07:hard@1"]),
        ],
    );
}

#[test]
fn leak_near_misses_do_not_fire() {
    assert_misses(
        "leak",
        &[
            (
                CodeKind::Prod,
                "fn f() -> &'static str { \"Box::leak and mem::forget\" } // transmute",
            ),
            (
                CodeKind::Prod,
                "fn f(x: u8) -> u8 { core::mem::transmute_copy(&x) }",
            ),
        ],
    );
}

#[test]
fn catch_unwind_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "catch-unwind",
        &[
            (
                CodeKind::Prod,
                "pub fn f() { let _r = std::panic::catch_unwind(|| 1); }",
                &["H-03:hard@1"],
            ),
            (
                CodeKind::Test,
                "fn f() { let _r = std::panic::catch_unwind(|| 1); }",
                &["H-03:soft@1"],
            ),
        ],
    );
}

#[test]
fn catch_unwind_near_misses_do_not_fire() {
    assert_misses(
        "catch-unwind",
        &[
            (
                CodeKind::Prod,
                "fn f() -> &'static str { \"catch_unwind\" } // catch_unwind",
            ),
            (CodeKind::Prod, "fn catch_unwinding() {}"),
        ],
    );
}

#[test]
fn exit_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "exit",
        &[
            (
                CodeKind::Prod,
                "pub fn f() {\n    std::process::exit(2);\n}",
                &["ARC-14:hard@2"],
            ),
            (
                CodeKind::Test,
                "fn f() { process::exit(0) }",
                &["ARC-14:soft@1"],
            ),
            (
                CodeKind::Bin,
                "#[expect(clippy::exit, reason = \"fatal startup error\")]\nfn f() { process::exit(1) }",
                &["ARC-14:ask@2"],
            ),
        ],
    );
}

#[test]
fn exit_near_misses_do_not_fire() {
    assert_misses(
        "exit",
        &[
            (
                CodeKind::Bin,
                "fn main() -> std::process::ExitCode { std::process::ExitCode::FAILURE }",
            ),
            (
                CodeKind::Prod,
                "fn f() -> &'static str { \"process::exit(1)\" } // process::exit(1)",
            ),
        ],
    );
}

#[test]
fn let_underscore_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "let-underscore",
        &[(
            CodeKind::Prod,
            "fn f() {\n    let _ = std::fs::remove_file(\"x\");\n}",
            &["H-10:soft@2"],
        )],
    );
}

#[test]
fn let_underscore_near_misses_do_not_fire() {
    assert_misses(
        "let-underscore",
        &[
            (
                CodeKind::Test,
                "fn f() { let _ = std::fs::remove_file(\"x\"); }",
            ),
            (
                CodeKind::Prod,
                "fn f() { let _guard = lock(); let _x = 1; }",
            ),
        ],
    );
}

#[test]
fn lazy_static_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "lazy-static",
        &[
            (
                CodeKind::Prod,
                "lazy_static! {\n    static ref N: u8 = 1;\n}",
                &["MOD-03:hard@1"],
            ),
            (
                CodeKind::Test,
                "use once_cell::sync::Lazy;",
                &["MOD-03:hard@1"],
            ),
        ],
    );
}

#[test]
fn lazy_static_near_misses_do_not_fire() {
    assert_misses(
        "lazy-static",
        &[
            (
                CodeKind::Prod,
                "use std::sync::LazyLock;\nstatic N: LazyLock<u8> = LazyLock::new(|| 1);",
            ),
            (
                CodeKind::Prod,
                "fn f() -> &'static str { \"lazy_static!\" }",
            ),
        ],
    );
}

#[test]
fn path_attr_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "path-attr",
        &[
            (
                CodeKind::Prod,
                "#[path = \"other.rs\"]\nmod other;",
                &["ARC-06:hard@1"],
            ),
            (
                CodeKind::Test,
                "#[path = \"helpers/mod.rs\"] mod helpers;",
                &["ARC-06:hard@1"],
            ),
        ],
    );
}

#[test]
fn path_attr_near_misses_do_not_fire() {
    assert_misses(
        "path-attr",
        &[
            (
                CodeKind::Prod,
                "mod other;\n#[doc = \"path\"]\npub fn f() {}",
            ),
            (CodeKind::Prod, "// #[path = \"other.rs\"]\nmod other;"),
        ],
    );
}

#[test]
fn async_trait_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "async-trait",
        &[
            (
                CodeKind::Prod,
                "#[async_trait]\npub trait Store { async fn get(&self) -> u8; }",
                &["MOD-04:soft@1"],
            ),
            (
                CodeKind::Test,
                "#[async_trait::async_trait]\ntrait Store { async fn get(&self) -> u8; }",
                &["MOD-04:soft@1"],
            ),
        ],
    );
}

#[test]
fn async_trait_near_misses_do_not_fire() {
    assert_misses(
        "async-trait",
        &[
            (
                CodeKind::Prod,
                "pub trait Store { async fn get(&self) -> u8; }",
            ),
            (CodeKind::Prod, "#[async_trait_ext]\npub trait Store {}"),
        ],
    );
}
