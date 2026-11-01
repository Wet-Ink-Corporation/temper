//! Scanner-level tests: the port of `tests/scan.test.mjs` for the 1a rules, in its order, plus
//! the scan property.

use std::error::Error as _;
use std::sync::LazyLock;

use proptest::prelude::*;

use super::rules::RULES;
use super::scanner::compile;
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

/// One rule's hits as `id:severity@line` (1.x `hitsOf`).
fn hits_of(src: &str, key: &str, kind: CodeKind) -> Vec<String> {
    scan(src, kind)
        .hits()
        .iter()
        .filter(|h| h.key().as_str() == key)
        .map(|h| format!("{}:{}@{}", h.id(), severity_name(h.severity()), h.line()))
        .collect()
}

/// One rule's hits (1.x `has`).
fn has(src: &str, key: &str, kind: CodeKind) -> Vec<Hit> {
    scan(src, kind)
        .into_hits()
        .into_iter()
        .filter(|h| h.key().as_str() == key)
        .collect()
}

/// One rule's severities (1.x `sevOf`).
fn sev_of(src: &str, key: &str, kind: CodeKind) -> Vec<&'static str> {
    has(src, key, kind)
        .iter()
        .map(|h| severity_name(h.severity()))
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
fn assert_in_prod_compile_time_assertions_are_exempt() {
    let key = "assert-in-prod";

    assert_eq!(
        sev_of(
            "pub fn f(x: u8) { assert!(x > 0, \"x must be positive\"); }",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "a run-time assert is advisory"
    );
    assert_eq!(
        has(
            "const _: () = assert!(size_of::<Event>() <= 128, \"Event grew past two cache lines\");",
            key,
            CodeKind::Prod
        ),
        [],
        "PERF-05 size pin"
    );
    assert_eq!(
        has(
            "pub fn chunk<const N: usize>(xs: &[u8]) -> usize {\n    const { assert!(N > 0, \"N must be positive\") };\n    xs.len() / N\n}",
            key,
            CodeKind::Prod
        ),
        [],
        "MOD-10 inline const block"
    );
    assert_eq!(
        sev_of(
            "impl<const N: usize> Ring<N> {\n    pub fn push(&mut self) { assert!(self.len < N, \"full\"); }\n}",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "a const generic parameter is not a const context"
    );
}

#[test]
fn assert_in_prod_const_contexts() {
    let src = "const A: u8 = { assert!(true); 1 };\nfn f<const N: usize>() { assert!(N > 0); }\nfn g(a: u8,\n const B: u8) { assert!(a > 0); }\nconst fn h() { assert!(true); }";

    assert_eq!(
        hits_of(src, "assert-in-prod", CodeKind::Prod),
        ["H-03:soft@2", "H-03:soft@4", "H-03:soft@5"],
        "const items and blocks are exempt; const generics and const fn are not"
    );
}

#[test]
fn ignore_no_reason_also_catches_cfg_attr_ignore() {
    let key = "ignore-no-reason";
    let flagged = [
        "#[ignore]\n#[test]\nfn slow() {}",
        "#[cfg_attr(miri, ignore)]\n#[test]\nfn spawns() {}",
        "#[cfg_attr(any(miri, all(unix, not(target_os = \"linux\"))), ignore)]\n#[test]\nfn t() {}",
    ];
    let clean = [
        "#[cfg_attr(miri, ignore = \"Miri cannot spawn processes\")]\n#[test]\nfn cli() {}",
        "#[ignore = \"needs a network\"]\n#[test]\nfn net() {}",
        "#[derive(Debug)]\n#[cfg_attr(feature = \"educe\", educe(Debug, ignore))]\nstruct S;",
    ];

    for s in flagged {
        assert_eq!(sev_of(s, key, CodeKind::Test), ["hard"], "{s}");
    }
    for s in clean {
        assert_eq!(has(s, key, CodeKind::Test), [], "{s}");
    }
}

#[test]
fn ignore_no_reason_backtracks_like_js() {
    let key = "ignore-no-reason";

    assert_eq!(
        hits_of(
            "#[cfg_attr(x, ignorefoo, ignore)]\n#[test]\nfn t() {}",
            key,
            CodeKind::Test
        ),
        ["H-12:hard@1"],
        "JS backtracks the lazy `[^\\]]*?` past `ignorefoo`"
    );
    assert_eq!(
        hits_of(
            "#[ignore ]\n#[test]\nfn t() {}\n#[cfg_attr(miri, ignore )]\nfn u() {}",
            key,
            CodeKind::Test
        ),
        ["H-12:hard@1", "H-12:hard@4"],
        "spaces before the closer"
    );
}

#[test]
fn extern_crate_alloc_and_std_are_exempt() {
    let key = "extern-crate";

    assert_eq!(
        sev_of("extern crate serde;", key, CodeKind::Prod),
        ["soft"],
        "extern crate is unnecessary"
    );
    assert_eq!(
        has(
            "#![no_std]\nextern crate alloc;\n#[cfg(test)]\nextern crate std;\nextern crate self as ledger;",
            key,
            CodeKind::Prod
        ),
        [],
        "alloc and std in a no_std crate, and self as, are exempt"
    );
}

#[test]
fn extern_crate_lookahead_backtracks_like_js() {
    let key = "extern-crate";

    assert_eq!(
        hits_of(
            "extern crate  alloc;\nextern crate alloc;\nextern crate  self as y;\nextern crate\tstd ;",
            key,
            CodeKind::Prod
        ),
        ["MOD-01:soft@1", "MOD-01:soft@3"],
        "JS gives `\\s+` back one space, and the look-ahead then sees a space"
    );
    assert_eq!(
        hits_of(
            "extern crate\nalloc;\nextern crate\n\nstd;",
            key,
            CodeKind::Prod
        ),
        ["MOD-01:soft@3"],
        "the same over newlines"
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
            && !is_build_script("src/bin/build.rs")
            && !is_build_script("src\\bin\\build.rs"),
        "only build.rs at the crate root is a build script"
    );
}

#[test]
fn thread_spawn_is_refused_outside_tests() {
    let key = "thread-spawn";
    let excepted = "#[expect(clippy::disallowed_methods, reason = \"startup thread; the process exits if it cannot start\")]\nfn run() { let h = thread::spawn(work); }";
    // async-concurrency.md ASY-15 Do
    let good = "\npub fn start_audit(log: AuditLog) -> io::Result<thread::JoinHandle<()>> {\n    thread::Builder::new().name(\"audit\".into()).spawn(move || log.run())\n}\npub fn parallel_sum(parts: &[Vec<u64>]) -> io::Result<u64> {\n    thread::scope(|s| {\n        let handles = parts.iter().map(|p| thread::Builder::new().spawn_scoped(s, move || p.iter().sum::<u64>()));\n        handles.count();\n        Ok(0)\n    })\n}";

    assert_eq!(
        sev_of(
            "fn run() { let h = thread::spawn(move || work()); }",
            key,
            CodeKind::Prod
        ),
        ["hard"],
        "thread::spawn in a library"
    );
    assert_eq!(
        sev_of(
            "fn run() { let h = std::thread::spawn(work); }",
            key,
            CodeKind::Bin
        ),
        ["hard"],
        "and in a binary"
    );
    assert_eq!(
        sev_of(
            "#[test]\nfn t() { let h = thread::spawn(work); }",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "advisory in a test"
    );
    assert_eq!(
        sev_of(excepted, key, CodeKind::Prod),
        ["ask"],
        "a reasoned expect of clippy::disallowed_methods asks"
    );
    assert_eq!(has(good, key, CodeKind::Prod), [], "Builder and scope");
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
