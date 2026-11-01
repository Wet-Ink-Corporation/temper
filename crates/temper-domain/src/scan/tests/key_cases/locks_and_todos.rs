//! `KEY_CASES` for spin locks and untracked `TODO`s.

use super::{assert_hits, assert_misses};
use crate::scan::CodeKind;

#[test]
fn spin_lock_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "spin-lock",
        &[
            (
                CodeKind::Prod,
                "pub fn lock(&self) {\n    while self.locked.swap(true, Ordering::Acquire) {\n        std::hint::spin_loop();\n    }\n}",
                &["ASY-17:soft@2", "ASY-17:soft@3"],
            ),
            (
                CodeKind::Prod,
                "pub fn take(&self) {\n    loop {\n        if self.flag.compare_exchange_weak(false, true, Acquire, Relaxed).is_ok() { return; }\n    }\n}",
                &["ASY-17:soft@3"],
            ),
        ],
    );
}

#[test]
fn spin_lock_near_misses_do_not_fire() {
    assert_misses(
        "spin-lock",
        &[
            (
                CodeKind::Prod,
                "pub fn first(&self) -> bool { !self.done.swap(true, Ordering::AcqRel) }",
            ),
            (
                CodeKind::Test,
                "fn f() { loop { std::hint::spin_loop(); } }",
            ),
            (
                CodeKind::Prod,
                "pub fn f(v: &mut [u8]) { for i in 0..2 { v.swap(i, 1); } }",
            ),
            (
                CodeKind::Prod,
                "pub fn f(flag: &AtomicBool) -> bool { flag.compare_exchange(false, true, AcqRel, Acquire).is_ok() }",
            ),
        ],
    );
}

#[test]
fn spin_lock_like_1x() {
    assert_hits(
        "spin-lock",
        &[
            (
                CodeKind::Prod,
                "fn f() {\n    for<'a> x;\n    impl X for Y { fn g() { a.swap(true, R); } }\n    while a.compare_exchange(1, 2, A, B).is_err() {}\n    for i in v { b.swap(true, R); c.swap(true, R);\n    }\n    std::hint::spin_loop(); std::hint::spin_loop();\n}",
                &["ASY-17:soft@4", "ASY-17:soft@5", "ASY-17:soft@7"],
            ),
            (
                CodeKind::Prod,
                "fn f() { for x in [a.swap(true, R)] {} }",
                &["ASY-17:soft@1"],
            ),
        ],
    );
    assert_misses(
        "spin-lock",
        &[(CodeKind::Prod, "fn f() { loop; a.swap(true, R); }")],
    );
}

#[test]
fn todo_comment_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "todo-comment",
        &[
            (
                CodeKind::Prod,
                "pub fn f() {}\n// TODO: handle overflow",
                &["CRF-04:soft@2"],
            ),
            (
                CodeKind::Test,
                "fn f() {} /* FIXME later */",
                &["CRF-04:soft@1"],
            ),
        ],
    );
}

#[test]
fn todo_comment_near_misses_do_not_fire() {
    assert_misses(
        "todo-comment",
        &[
            (
                CodeKind::Prod,
                "// TODO(#123): handle overflow\n// FIXME see https://example.com/i/4\n// HACK until PROJ-12 lands\npub fn f() {}",
            ),
            (
                CodeKind::Prod,
                "/// TODO in a doc comment\npub fn f() -> &'static str { \"TODO\" }",
            ),
        ],
    );
}

#[test]
fn todo_comment_like_1x() {
    assert_hits(
        "todo-comment",
        &[(
            CodeKind::Prod,
            "/* TODO */\n// TODO(#1)\n/// TODO\n// todo lowercase\n// XXX: JIRA-12 not a ref? \n// HACK: see ABC-1\n/* FIXME http://x */\n//TODO:x\n// TODOS\n/**/\n// FIXME A1-2",
            &["CRF-04:soft@1", "CRF-04:soft@8"],
        )],
    );
}
