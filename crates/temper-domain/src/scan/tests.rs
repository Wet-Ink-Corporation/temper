//! Scanner-level tests: the port of `tests/scan.test.mjs` for the 1a rules, in its order, plus
//! the scan property.

use std::sync::LazyLock;

use proptest::prelude::*;

use super::strategies::token_soup;
use super::{CodeKind, FileRole, Hit, ScanOptions, ScanReport, Scanner, Severity, is_build_script};

const ALL_KINDS: [CodeKind; 4] = [
    CodeKind::Prod,
    CodeKind::Bin,
    CodeKind::Example,
    CodeKind::Test,
];

/// One scanner for every test in this binary, as the composition root builds one per process.
static SCANNER: LazyLock<Scanner> =
    LazyLock::new(|| Scanner::new().expect("every built-in pattern compiles"));

fn scanner() -> &'static Scanner {
    &SCANNER
}

fn scan_as(src: &str, kind: CodeKind, file: FileRole) -> ScanReport {
    scanner().scan(src, &ScanOptions { kind, file })
}

fn scan(src: &str, kind: CodeKind) -> ScanReport {
    scan_as(src, kind, FileRole::Source)
}

/// `key:severity` for each hit, as the 1.x tests' `keys` helper prints them.
fn keys(report: &ScanReport) -> Vec<String> {
    report
        .hits()
        .iter()
        .map(|h| format!("{}:{}", h.key(), severity_name(h.severity())))
        .collect()
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Hard => "hard",
        Severity::Ask => "ask",
        Severity::Soft => "soft",
    }
}

/// `(key, severity, line)` for each hit.
fn summary(report: &ScanReport) -> Vec<(&'static str, Severity, usize)> {
    report
        .hits()
        .iter()
        .map(|h| (h.key().as_str(), h.severity(), h.line().get()))
        .collect()
}

fn hit<'r>(report: &'r ScanReport, key: &str) -> &'r Hit {
    report
        .hits()
        .iter()
        .find(|h| h.key().as_str() == key)
        .expect("the scan reports a hit for the key")
}

const TEST_REGIONS_SRC: &str = "\npub fn ok() -> u8 { 1 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { assert_eq!(Some(ok()).unwrap(), 1); dbg!(1); }\n    #[test]\n    fn u() { unsafe { } }\n}";
const IGNORE_SRC: &str = "#[test]\n#[ignore = \"slow\"]\nfn a() {}\n#[test]\n#[cfg_attr(miri, ignore = \"spawns a process\")]\nfn b() {}\n#[test]\n#[cfg_attr(windows, ignore = \"unix sockets\")]\nfn c() {}\n#[test]\n#[ignore]\nfn d() {}";

#[test]
fn scanner_new_compiles_every_pattern() {
    let failed = Scanner::new().err().map(|error| error.rule());

    assert_eq!(failed, None, "no built-in pattern fails to compile");
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
fn print_build_scripts_get_the_writeln_fix_text() {
    let src = "fn main() {\n    println!(\"cargo::rerun-if-changed=schema.json\");\n}";

    let bin = scan_as(src, CodeKind::Bin, FileRole::Source);
    let build = scan_as(src, CodeKind::Bin, FileRole::BuildScript);

    let (bin, build) = (hit(&bin, "print"), hit(&build, "print"));
    assert_eq!(
        (bin.severity(), build.severity()),
        (Severity::Hard, Severity::Hard),
        "print is hard in binaries and build scripts"
    );
    assert_eq!(
        build.fix(),
        "Build scripts emit directives with `writeln!(io::stdout().lock(), \"cargo::rerun-if-changed=..\")?` from a `main` that returns `Result` (ARC-17, H-06).",
        "build scripts get the 1.x fixBuild text"
    );
    assert!(
        !bin.fix().contains("cargo::"),
        "other binaries get the plain fix"
    );
    assert!(
        is_build_script("build.rs")
            && !is_build_script("src/build.rs")
            && !is_build_script("src\\bin\\build.rs"),
        "only build.rs at the crate root is a build script"
    );
}

#[test]
fn scan_reports_ignore_count() {
    assert_eq!(
        scan(IGNORE_SRC, CodeKind::Test).ignores(),
        3,
        "cfg_attr(miri, ignore) is not counted"
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

fn assert_lines_in_range(src: &str) -> Result<(), TestCaseError> {
    let lines = src.split('\n').count();
    let scanner = scanner();
    for kind in ALL_KINDS {
        let report = scanner.scan(
            src,
            &ScanOptions {
                kind,
                file: FileRole::Source,
            },
        );
        for h in report.hits() {
            prop_assert!(
                (1..=lines).contains(&h.line().get()),
                "line {} outside 1..={}",
                h.line(),
                lines
            );
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn scan_never_panics_and_lines_are_in_range(src in any::<String>()) {
        assert_lines_in_range(&src)?;
    }

    #[test]
    fn scan_never_panics_and_lines_are_in_range_on_token_soup(src in token_soup()) {
        assert_lines_in_range(&src)?;
    }
}
