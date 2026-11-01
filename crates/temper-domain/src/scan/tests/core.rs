//! 1.x tests 2 to 8: the hard rules, test regions and `#[expect]` exception requests.

use super::{TEST_REGIONS_SRC, hit, keys, scan, summary};
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
