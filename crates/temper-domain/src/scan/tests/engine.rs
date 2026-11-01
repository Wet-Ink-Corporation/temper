//! The scanner engine: pattern compilation, the rule table's order, the counts and the hit
//! fields every rule shares.

use std::error::Error as _;

use super::{hit, scan};
use crate::scan::rules::RULES;
use crate::scan::scanner::compile;
use crate::scan::{CodeKind, Scanner};

#[test]
fn scanner_new_compiles_every_pattern() {
    let failed = Scanner::new().err().map(|error| error.rule());

    assert_eq!(failed, None, "no built-in pattern fails to compile");
}

#[test]
fn compile_reports_an_unsupported_construct_with_the_rule_name() {
    let error = compile("t", r"\A").err();

    let reported = error.map(|e| (e.rule(), e.source().map(ToString::to_string)));

    assert_eq!(
        reported,
        Some((
            "t",
            Some("JS regex construct `\\A` at byte 0 has no faithful translation".to_owned())
        )),
        "the rule and the construct are named"
    );
}

/// The 60 1.x rule keys in `scan.mjs` table order.
const KEYS_1X: [&str; 60] = [
    "unsafe",
    "unwrap",
    "panic",
    "assert-in-prod",
    "allow-attr",
    "dbg",
    "print",
    "leak",
    "catch-unwind",
    "exit",
    "discard-ok",
    "empty-err-arm",
    "empty-if-let-err",
    "let-underscore",
    "filter-map-ok",
    "parse-unwrap-or",
    "as-cast",
    "ptr-param",
    "lazy-static",
    "path-attr",
    "ignore-no-reason",
    "async-trait",
    "shared-mut",
    "mutex-unit",
    "global-mut",
    "deref-inherit",
    "sleep",
    "thread-spawn",
    "detached-spawn",
    "unbounded",
    "spin-lock",
    "seqcst",
    "fetch-update",
    "compiler-fence",
    "downcast",
    "unwrap-or-default",
    "map-index",
    "env-var",
    "ambient-now",
    "serde-hashmap",
    "test-branch",
    "global-allocator",
    "should-panic",
    "weak-assert",
    "extern-crate",
    "crate-lint-attr",
    "pub-glob-reexport",
    "prelude-mod",
    "pub-smart-ptr",
    "ops-impl",
    "type-alias-primitive",
    "get-prefix",
    "struct-bounds",
    "doc-unwrap",
    "doc-arg-sections",
    "expect-attr",
    "forbid-override",
    "erased-error",
    "todo-comment",
    "commented-code",
];

#[test]
fn rules_keep_1x_table_order() {
    let mut oracle = KEYS_1X.iter();

    let out_of_order: Vec<&str> = RULES
        .iter()
        .map(|spec| spec.key)
        .filter(|key| !oracle.any(|k| k == key))
        .collect();

    assert_eq!(
        out_of_order,
        Vec::<&str>::new(),
        "every key is a 1.x key, in 1.x order"
    );
}

#[test]
fn scan_reports_clone_count() {
    assert_eq!(
        scan("fn f(a: &A) -> A { a.clone() }", CodeKind::Prod).clones(),
        1,
        "one clone call"
    );
}

#[test]
fn hit_text_is_js_trimmed_and_cut_at_160_utf16_units() {
    let pair = format!(
        "fn f() {{\n\u{A0}  let s = {}😀tail.unwrap();\n}}",
        "x".repeat(150)
    );
    let split = format!("let s = {}😀z.unwrap();", "y".repeat(151));

    let texts = [pair, split].map(|src| {
        let report = scan(&src, CodeKind::Prod);
        hit(&report, "unwrap").text().to_owned()
    });

    assert_eq!(
        texts,
        [
            format!("let s = {}😀", "x".repeat(150)),
            format!("let s = {}", "y".repeat(151)),
        ],
        "NBSP is trimmed and a surrogate pair is never split (D16(e))"
    );
}

#[test]
fn into_hits_hands_back_the_hits() {
    let report = scan("fn f() { dbg!(1); }", CodeKind::Prod);
    let expected = report.hits().to_vec();

    assert_eq!(report.into_hits(), expected, "the same hits, owned");
}
