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
fn rules_are_the_60_1x_rules_in_order() {
    let keys: Vec<&str> = RULES.iter().map(|spec| spec.key).collect();

    assert_eq!(
        keys, KEYS_1X,
        "the table holds every 1.x rule, in 1.x order"
    );
}

/// Is `id` a rust-craft rule id: 1 to 4 ASCII uppercase letters, `-`, 2 ASCII digits (1.x
/// `/^[A-Z]{1,4}-\d{2}$/`)?
fn is_rule_id(id: &str) -> bool {
    id.split_once('-').is_some_and(|(letters, digits)| {
        (1..=4).contains(&letters.len())
            && letters.bytes().all(|b| b.is_ascii_uppercase())
            && digits.len() == 2
            && digits.bytes().all(|b| b.is_ascii_digit())
    })
}

/// 1.x `rule table: unique keys, rule ids, four contexts, fix text`. The four contexts and
/// "exactly one of re/find" hold by construction: `Contexts` has the four fields and `Matcher`
/// is either a pattern or a finder.
#[test]
fn rule_table_unique_keys_rule_ids_four_contexts_fix_text() {
    let mut seen = std::collections::BTreeSet::new();
    for spec in RULES {
        assert!(seen.insert(spec.key), "duplicate key {}", spec.key);
        assert!(is_rule_id(spec.id), "{}: id {}", spec.key, spec.id);
        assert!(
            !spec.fix.is_empty() && !spec.msg.is_empty(),
            "{}: msg and fix",
            spec.key
        );
    }
    // Rules added or changed in 1.1 name their rule in the fix text.
    let v11 = [
        "assert-in-prod",
        "ignore-no-reason",
        "extern-crate",
        "doc-unwrap",
        "empty-err-arm",
        "empty-if-let-err",
        "thread-spawn",
        "compiler-fence",
        "global-allocator",
        "map-index",
        "filter-map-ok",
        "parse-unwrap-or",
        "seqcst",
        "fetch-update",
        "crate-lint-attr",
        "pub-glob-reexport",
        "prelude-mod",
        "pub-smart-ptr",
        "struct-bounds",
        "ops-impl",
        "type-alias-primitive",
        "get-prefix",
        "doc-arg-sections",
        "mutex-unit",
        "detached-spawn",
    ];
    for key in v11 {
        let spec = RULES.iter().find(|spec| spec.key == key);
        assert!(spec.is_some(), "{key} is in the table");
        if let Some(spec) = spec {
            assert!(
                spec.fix.contains(spec.id),
                "{key}: fix text should cite {}",
                spec.id
            );
        }
    }
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

/// A requested `exception` (written before `msg`) comes from a rule's `lints`, and a finder's
/// own `msg` from a finder. Only regex rules list lints, so no hit has both, and the hit JSON's
/// two `exception` positions can be told apart from the hit alone (port plan 1d.2, Invariant 2).
#[test]
fn exception_requests_come_only_from_regex_rules() {
    use crate::scan::rules::Matcher;

    let finders_with_lints: Vec<&str> = RULES
        .iter()
        .filter(|rule| !rule.lints.is_empty() && !matches!(rule.matcher, Matcher::Regex(_)))
        .map(|rule| rule.key)
        .collect();

    assert_eq!(
        finders_with_lints,
        Vec::<&str>::new(),
        "finder rules that list lints"
    );
}

#[test]
fn into_hits_hands_back_the_hits() {
    let report = scan("fn f() { dbg!(1); }", CodeKind::Prod);
    let expected = report.hits().to_vec();

    assert_eq!(report.into_hits(), expected, "the same hits, owned");
}
