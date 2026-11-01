//! 1.x tests 2 to 8: the hard rules, test regions and `#[expect]` exception requests.

use super::{TEST_REGIONS_SRC, full, hit, hits_of, keys, msgs_of, scan, summary};
use crate::scan::{CodeKind, Hit, Severity};

#[test]
fn hard_hits_in_production_code() {
    let src = "\npub fn a(v: &Vec<u8>) -> u32 {\n    let x = v.first().unwrap();\n    let y = std::env::args().next().expect(\"argv\");\n    if *x > 3 { panic!(\"big\") }\n    dbg!(x);\n    println!(\"{y}\");\n    u32::from(*x) + (*x as u32)\n}";

    let report = scan(src, CodeKind::Prod);
    let hard: Vec<String> = report
        .hits()
        .iter()
        .filter(|h| h.severity() == Severity::Hard)
        .map(|h| format!("{} {}@{}", h.key(), h.id(), h.line()))
        .collect();

    assert_eq!(
        hard,
        [
            "unwrap H-02@3",
            "unwrap H-02@4",
            "panic H-03@5",
            "dbg H-06@6",
            "print H-06@7",
            "as-cast H-11@8",
            "ptr-param OWN-03@2",
        ],
        "the 1.x hard hits, in rule-table order"
    );
}

#[test]
fn test_regions_allow_unwrap_panic_but_not_dbg_or_unsafe() {
    let report = scan(TEST_REGIONS_SRC, CodeKind::Prod);
    let k = keys(&report);

    assert!(
        !k.iter().any(|x| x.starts_with("unwrap")),
        "unwrap in a test region is allowed: {k:?}"
    );
    assert!(
        k.contains(&"dbg:hard".to_owned()),
        "dbg is hard everywhere: {k:?}"
    );
    assert!(
        k.contains(&"unsafe:hard".to_owned()),
        "unsafe is hard everywhere: {k:?}"
    );
    assert_eq!(report.tests(), 2, "two #[test] fns");
    let full: Vec<_> = report
        .hits()
        .iter()
        .map(|h| {
            (
                h.key().as_str(),
                h.id().as_str(),
                h.severity(),
                h.line().get(),
                h.text(),
            )
        })
        .collect();
    assert_eq!(
        full,
        [
            ("unsafe", "H-01", Severity::Hard, 9, "fn u() { unsafe { } }"),
            (
                "dbg",
                "H-06",
                Severity::Hard,
                7,
                "fn t() { assert_eq!(Some(ok()).unwrap(), 1); dbg!(1); }"
            ),
        ],
        "the 1.x hits, in rule-table order"
    );
}

#[test]
fn expect_with_reason_turns_hard_hit_into_exception_request() {
    let src = "\n#[expect(clippy::unwrap_used, reason = \"value is validated by the parser above\")]\npub fn a(v: Option<u8>) -> u8 { v.unwrap() }\n";

    let report = scan(src, CodeKind::Prod);
    let no_reason = scan(
        "#[expect(clippy::unwrap_used)]\npub fn a() {}",
        CodeKind::Prod,
    );

    let unwrap = hit(&report, "unwrap");
    assert_eq!(
        (unwrap.severity(), unwrap.line().get(), unwrap.exception()),
        (Severity::Ask, 3, Some("clippy::unwrap_used")),
        "the unwrap becomes an exception request"
    );
    let attr = hit(&report, "expect-attr");
    assert_eq!(
        (
            attr.severity(),
            attr.line().get(),
            attr.msg(),
            attr.exception()
        ),
        (
            Severity::Ask,
            2,
            "lint exception requested for `clippy::unwrap_used`",
            Some("clippy::unwrap_used")
        ),
        "the attribute itself asks"
    );
    let bare = hit(&no_reason, "expect-attr");
    assert_eq!(
        (bare.severity(), bare.msg(), bare.fix()),
        (
            Severity::Hard,
            "`#[expect]` without a reason",
            "Add `reason = \"...\"` explaining why the lint does not apply. A human will review it."
        ),
        "an expect without a reason is refused"
    );
}

#[test]
fn expect_window_is_twelve_lines() {
    let unwrap_at = |blank_lines: usize| {
        let src = format!(
            "#[expect(clippy::unwrap_used, reason = \"r\")]\n{}fn f() {{ x.unwrap() }}",
            "\n".repeat(blank_lines)
        );
        let report = scan(&src, CodeKind::Prod);
        let unwrap = hit(&report, "unwrap");
        (unwrap.severity(), unwrap.line().get())
    };

    assert_eq!(
        [11, 12, 13].map(unwrap_at),
        [
            (Severity::Ask, 13),
            (Severity::Hard, 14),
            (Severity::Hard, 15)
        ],
        "the attribute must start within the 12 lines above the hit"
    );
}

#[test]
fn last_matching_expect_in_window_wins() {
    let src = "#[expect(clippy::expect_used, reason = \"a\")]\n#[expect(clippy::unwrap_used, reason = \"b\")]\nfn f() { x.unwrap() }";

    let report = scan(src, CodeKind::Prod);

    let unwrap = hit(&report, "unwrap");
    assert_eq!(
        (unwrap.severity(), unwrap.exception()),
        (Severity::Ask, Some("clippy::unwrap_used")),
        "the nearer attribute names the lint"
    );
}

#[test]
fn forced_expect_attr_severity_ignores_test_regions() {
    let src = "#[cfg(test)]\nmod t {\n#[expect(clippy::unwrap_used)]\nfn a() {}\n}";

    let report = scan(src, CodeKind::Prod);

    assert_eq!(
        summary(&report),
        [("expect-attr", Severity::Hard, 3)],
        "the finder's severity wins over the test context"
    );
}

#[test]
fn allow_attribute_erased_errors_in_pub_signatures_ok_discard() {
    let src = "\n#[allow(dead_code)]\npub fn load(p: &str) -> Result<String, String> { std::fs::read_to_string(p).map_err(|e| e.to_string()) }\npub fn run() -> anyhow::Result<()> { Ok(()) }\nfn quiet() { std::fs::remove_file(\"x\").ok(); }\npub(crate) fn inner() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }\n";
    let hard = |kind| -> Vec<String> {
        let all = keys(&scan(src, kind));
        all.into_iter().filter(|k| k.ends_with(":hard")).collect()
    };

    let k = hard(CodeKind::Prod);

    assert!(
        k.iter().any(|x| x == "allow-attr:hard"),
        "`#[allow]` is hard: {k:?}"
    );
    assert_eq!(
        k.iter().filter(|x| *x == "erased-error:hard").count(),
        2,
        "the `String` and `anyhow` signatures: {k:?}"
    );
    assert!(
        k.iter().any(|x| x == "discard-ok:hard"),
        "`.ok();` is hard: {k:?}"
    );
    let bin = hard(CodeKind::Bin);
    assert!(
        !bin.iter().any(|x| x == "erased-error:hard"),
        "binaries may use anyhow: {bin:?}"
    );
    assert_eq!(
        (full(src, CodeKind::Prod), full(src, CodeKind::Bin)),
        (
            vec![
                "allow-attr H-05 hard@2".to_owned(),
                "discard-ok H-10 hard@5".to_owned(),
                "erased-error H-09 hard@3".to_owned(),
                "erased-error H-09 hard@4".to_owned(),
            ],
            vec![
                "allow-attr H-05 hard@2".to_owned(),
                "discard-ok H-10 hard@5".to_owned(),
            ]
        ),
        "the oracle's whole reports"
    );
}

#[test]
fn erased_error_labels_in_1x_order() {
    let src = "pub const async fn a() -> Result<Box<dyn Error>, String> {}\npub fn b() -> eyre::Result<()>;\npub fn c() -> Box<dyn core::error::Error + Send> {}\npub fn d(x: Result<u8, String>) {}\npub fn e() -> u8 { anyhow::Result }\npub async fn f() -> anyhow::Error {}";
    let label = |line: u8, what: &str| format!("{line}:public library signature returns {what}");

    assert_eq!(
        msgs_of(src, "erased-error", CodeKind::Prod),
        [
            label(1, "Result<_, String>"),
            label(1, "Box<dyn Error>"),
            label(2, "eyre"),
            label(3, "Box<dyn Error>"),
            label(4, "Result<_, String>"),
            label(6, "anyhow"),
        ],
        "each label in 1.x order, over the signature up to its `{{` or `;`"
    );
    assert_eq!(
        msgs_of(src, "erased-error", CodeKind::Bin),
        Vec::<String>::new(),
        "binaries are exempt"
    );
}

#[test]
fn allow_attr_and_discard_ok_forms() {
    let allows = "#![allow(x)]\n# ! [ allow (y)]\n#[cfg_attr(a, allow(b))]\n#[allowed(z)]";

    assert_eq!(
        hits_of(allows, "allow-attr", CodeKind::Test),
        ["H-05:hard@1", "H-05:hard@2"],
        "inner and spaced forms; not inside `cfg_attr` or a longer name"
    );
    assert_eq!(
        hits_of(
            "fn f() { a.ok(); b . ok ( ) ; c.ok()?; d.ok() }",
            "discard-ok",
            CodeKind::Test
        ),
        ["H-10:soft@1", "H-10:soft@1"],
        "only `.ok()` ending a statement"
    );
    assert_eq!(
        hits_of("fn f() { a.ok(); }", "discard-ok", CodeKind::Bin),
        ["H-10:hard@1"],
        "hard in a binary"
    );
}

/// 1.x test 8, ported last in 1b so it runs against the whole 1b table (decision 1b-10).
#[test]
fn no_false_positives_on_idiomatic_code() {
    let src = "\n//! Crate docs.\nuse std::borrow::Cow;\n/// Normalizes.\n#[must_use]\npub fn normalize(s: &str) -> Cow<'_, str> {\n    if s.contains(' ') { Cow::Owned(s.replace(' ', \"_\")) } else { Cow::Borrowed(s) }\n}\n/// Parses a port.\n///\n/// # Errors\n/// Returns an error when the input is not a port number.\npub fn port(s: &str) -> Result<u16, std::num::ParseIntError> { s.parse() }\n/// Widening is lossless.\n#[must_use]\npub fn widen(x: u32) -> u64 { u64::from(x) }\nfn labels() { 'outer: for _ in 0..1 { break 'outer; } }\nconst MSG: &str = \"don't .unwrap() here; panic!() is banned\";\n";

    let report = scan(src, CodeKind::Prod);
    let blocking: Vec<&Hit> = report
        .hits()
        .iter()
        .filter(|h| h.severity() != Severity::Soft)
        .collect();

    assert_eq!(blocking, Vec::<&Hit>::new(), "no hard or ask hit");
}
