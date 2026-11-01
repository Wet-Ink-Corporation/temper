//! Scanner-level tests: the port of `tests/scan.test.mjs` for the 1a rules, in its order, plus
//! the scan property.

mod key_cases;

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
fn compiler_fence_and_global_allocator() {
    let fence = "compiler-fence";
    let global = "global-allocator";
    let alloc = "#[global_allocator]\nstatic ALLOC: Counting = Counting;";
    // performance.md dhat entry point: a top-level fn main marks a binary root
    let dhat = "#[cfg(feature = \"dhat-heap\")]\n#[global_allocator]\nstatic ALLOC: dhat::Alloc = dhat::Alloc;\n\nfn main() {\n    #[cfg(feature = \"dhat-heap\")]\n    let _profiler = dhat::Profiler::new_heap();\n}";
    // a bodiless #[cfg(test)] item is a test region too (a counting allocator for unit tests)
    let counting = "#[cfg(test)]\n#[global_allocator]\nstatic COUNTING: CountingAlloc = CountingAlloc::new();\n\npub fn f() -> u8 { 1 }\n#[global_allocator]\nstatic REAL: System = System;";

    for s in [
        "fn f() { atomic::compiler_fence(Ordering::SeqCst); }",
        "use std::sync::atomic::{compiler_fence, Ordering};\nfn f() { compiler_fence(Ordering::SeqCst); }",
        "fn f() { core::sync::atomic::compiler_fence(Ordering::Release); }",
    ] {
        assert_eq!(sev_of(s, fence, CodeKind::Prod), ["hard"], "{s}");
    }
    assert_eq!(
        has(
            "fn f() { fence(Ordering::Acquire); }",
            fence,
            CodeKind::Prod
        ),
        [],
        "fence is not compiler_fence"
    );
    assert_eq!(
        has(
            "/// Keeps the timer reads ordered.\npub fn compiler_fence() { black_box(()); }\nfn g(t: &Timer) { compiler_fence(); t.fence.compiler_fence(); }",
            fence,
            CodeKind::Prod
        ),
        [],
        "a user function of the same name (divan's time/fence.rs shape)"
    );
    assert_eq!(
        sev_of(alloc, global, CodeKind::Prod),
        ["hard"],
        "a library sets the allocator"
    );
    assert_eq!(has(alloc, global, CodeKind::Bin), [], "a binary may");
    assert_eq!(
        has(dhat, global, CodeKind::Prod),
        [],
        "fn main marks a root"
    );
    assert_eq!(
        has(
            "#[cfg(test)]\nmod tests {\n    #[global_allocator]\n    static A: dhat::Alloc = dhat::Alloc;\n}",
            global,
            CodeKind::Prod
        ),
        [],
        "a test module may"
    );
    assert_eq!(
        hits_of(counting, global, CodeKind::Prod),
        ["ARC-14:hard@6"],
        "only the allocator outside the bodiless cfg(test) item"
    );
}

#[test]
fn compiler_fence_lookbehind_and_window() {
    let fence = "compiler-fence";
    let window = |filler: &str, n: usize| {
        let src = format!(
            "fn f() {{ atomic::/*{}*/compiler_fence(Ordering::SeqCst); }}",
            filler.repeat(n)
        );
        hits_of(&src, fence, CodeKind::Prod)
    };

    assert_eq!(
        hits_of(
            "use std::sync::atomic::compiler_fence;\nfn f() { x . compiler_fence(); y\n    compiler_fence(); fn  compiler_fence() {} }\nfn g() { compiler_fence(); }",
            fence,
            CodeKind::Prod
        ),
        ["ASY-13:hard@4"],
        "not after `.` or a word char, across spaces and newlines"
    );
    assert_eq!(
        [window("é", 28), window("é", 29)],
        [vec!["ASY-13:hard@1".to_owned()], vec![]],
        "the 40-unit window counts é as one unit"
    );
    assert_eq!(
        [window("😀", 14), window("😀", 15)],
        [vec!["ASY-13:hard@1".to_owned()], vec![]],
        "and 😀 as two"
    );
}

#[test]
fn global_allocator_sees_fn_main_after_any_js_line_terminator() {
    let sources = [
        "#[global_allocator]\r\nstatic A: S = S;\r\nfn main() {}",
        "#[global_allocator]\nstatic A: S = S;\rfn main() {}",
        "#[global_allocator]\nstatic A: S = S;\u{2028}pub fn main() {}",
    ];

    for s in sources {
        assert_eq!(
            has(s, "global-allocator", CodeKind::Prod),
            [],
            "JS `^` under m starts a line after \\r, U+2028 and U+2029: {s:?}"
        );
    }
}

#[test]
fn map_index_is_advisory_slices_of_references_are_not() {
    let key = "map-index";
    // ledger-cli args.rs and tests/it/support.rs
    let good = "\nfn parse_request(words: &[&str]) -> Result<Request, UsageError> { todo_free(words) }\npub(crate) fn ledger_all(store: &Path, commands: &[&[&str]]) -> TestResult { run(store, commands) }\nfn g(a: &u8, b: &u8) {\n    ledger_all(store.path(), &[&[\"open\", \"acc-1\", \"Ada Lovelace\"]]);\n    let pair = [&a, &b];\n    let v = vec![&a];\n    for x in [&a, &b] { use_it(x); }\n    let [&first, ..] = pair else { return };\n    let _m = matches!(pair, [&1, ..]);\n    let t: Box<[&str]> = Box::new([]);\n}";

    assert_eq!(
        sev_of(
            "fn f(&self, id: &AccountId) -> Cents { self.balances[&id.0] }",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "indexing a map by reference"
    );
    assert_eq!(
        sev_of(
            "fn f(m: &HashMap<K, V>, k: K) -> &V { &lookup()[&k] }",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "indexing a call's result"
    );
    assert_eq!(
        has(good, key, CodeKind::Prod),
        [],
        "slices and arrays of references"
    );
    assert_eq!(
        has(
            "#[test]\nfn t() { assert_eq!(m[&k], 1); }",
            key,
            CodeKind::Prod
        ),
        [],
        "tests may index"
    );
}

#[test]
fn map_index_skips_keywords() {
    assert_eq!(
        hits_of(
            "fn f() { let a = m[&k]; let b = in[&k]; let c = f()[&k]; let d = v[ &k]; }",
            "map-index",
            CodeKind::Prod
        ),
        ["H-04:soft@1", "H-04:soft@1", "H-04:soft@1"],
        "`in[&k]` is a keyword, not a map"
    );
}

#[test]
fn filter_map_ok_is_advisory() {
    let filter = "filter-map-ok";

    assert_eq!(
        sev_of(
            "fn f(xs: &[&str]) -> Vec<u32> { xs.iter().filter_map(|s| s.parse::<u32>().ok()).collect() }",
            filter,
            CodeKind::Prod
        ),
        ["soft"],
        "a closure ending in .ok()"
    );
    assert_eq!(
        sev_of(
            "fn f(d: ReadDir) -> Vec<DirEntry> { d.filter_map(Result::ok).collect() }",
            filter,
            CodeKind::Prod
        ),
        ["soft"],
        "Result::ok"
    );
    assert_eq!(
        has(
            "fn f(xs: &[u8]) -> usize { xs.iter().filter_map(|x| x.checked_add(1)).count() }",
            filter,
            CodeKind::Prod
        ),
        [],
        "an Option-returning closure"
    );
}

#[test]
fn parse_unwrap_or_is_advisory() {
    let parse = "parse-unwrap-or";
    let both = scan(
        "fn f(s: &str) -> u16 { s.parse::<u16>().unwrap_or_default() }",
        CodeKind::Prod,
    );
    let both: Vec<&str> = both.hits().iter().map(|h| h.key().as_str()).collect();

    assert_eq!(
        sev_of(
            "fn f(s: &str) -> u16 { s.parse().unwrap_or(8080) }",
            parse,
            CodeKind::Prod
        ),
        ["soft"],
        "a parse failure replaced by a default"
    );
    assert!(
        both.contains(&parse) && !both.contains(&"unwrap-or-default"),
        "parse-unwrap-or, not unwrap-or-default: {both:?}"
    );
    // ledger-app lib.rs doctest helper and money.rs parser
    assert_eq!(
        has(
            "fn load(&self, a: &AccountId) -> io::Result<Vec<Recorded>> { Ok(self.0.get(a).cloned().unwrap_or_default()) }",
            parse,
            CodeKind::Prod
        ),
        [],
        "no parse"
    );
    assert_eq!(
        has(
            "fn f(s: &str) -> Result<u16, ParseIntError> { let n = s.parse::<u16>()?; Ok(n) }",
            parse,
            CodeKind::Prod
        ),
        [],
        "a propagated parse"
    );
}

#[test]
fn unwrap_or_default_lookbehind() {
    assert_eq!(
        hits_of(
            "fn f() { a.parse::<u8>().unwrap_or_default(); b.parse ( ) . unwrap_or_default ( ); c.reparse().unwrap_or_default(); d.unwrap_or_default(); }",
            "unwrap-or-default",
            CodeKind::Prod
        ),
        ["H-10:soft@1", "H-10:soft@1"],
        "only after something other than a `parse()` call (here reparse and d)"
    );
}

#[test]
fn seqcst_and_fetch_update_are_atomics_advisories() {
    let seqcst = "seqcst";
    let update = "fetch-update";
    // async-concurrency.md ASY-13 Don't
    let unjustified = "fn next_id(next: &AtomicU64) -> u64 {\n    next.fetch_add(1, Ordering::SeqCst) // \"to be safe\": pairs with nothing\n}";
    let justified = "fn arrive(&self) -> bool {\n    // SeqCst: the store of our flag and the load of theirs need one total order (Dekker).\n    self.mine.store(true, Ordering::SeqCst);\n    !self.theirs.load(Ordering::SeqCst) // same total order as the store above\n}";

    assert_eq!(
        sev_of(unjustified, seqcst, CodeKind::Prod),
        ["soft"],
        "a comment that names no order"
    );
    assert_eq!(
        has(justified, seqcst, CodeKind::Prod),
        [],
        "comments that name the total order"
    );
    assert_eq!(
        has(
            "use std::sync::atomic::Ordering::{Acquire, SeqCst};\nfn f(c: &AtomicU64) -> u64 { c.load(Ordering::Relaxed) }",
            seqcst,
            CodeKind::Prod
        ),
        [],
        "an import"
    );
    assert_eq!(
        has(
            "#[test]\nfn t() { assert_eq!(counter.load(Ordering::SeqCst), 1); }",
            seqcst,
            CodeKind::Prod
        ),
        [],
        "a test"
    );
    assert_eq!(
        sev_of(
            "fn f(c: &AtomicU64) { let _ = c.fetch_update(Relaxed, Relaxed, |n| n.checked_add(1)); }",
            update,
            CodeKind::Prod
        ),
        ["soft"],
        "fetch_update is deprecated"
    );
    assert_eq!(
        has(
            "fn f(c: &AtomicU64) -> Result<u64, u64> { c.try_update(Relaxed, Relaxed, |n| n.checked_add(1)) }",
            update,
            CodeKind::Prod
        ),
        [],
        "try_update is its replacement"
    );
}

#[test]
fn seqcst_justification_like_js() {
    let key = "seqcst";
    let window = |n: usize| {
        let src = format!(
            "use std::sync::atomic::Ordering::{{/*{}*/SeqCst}};",
            "é".repeat(n)
        );
        hits_of(&src, key, CodeKind::Prod)
    };

    assert_eq!(
        hits_of(
            "fn f() {\n    x.load(Ordering::SeqCst); // seqcst ORDER needed\n\n\n    y.load(Ordering::SeqCst); // \u{17F}eqcst\n\n\n    z.load(Ordering::SeqCst); // Global  Order\n}",
            key,
            CodeKind::Prod
        ),
        ["ASY-13:soft@5"],
        "the note is case-insensitive in ASCII only (U+017F is not s)"
    );
    assert_eq!(
        has(
            "fn f() {\n    /* needs a single\n       total order */\n    x.load(Ordering::SeqCst);\n}",
            key,
            CodeKind::Prod
        ),
        [],
        "a block comment ending on the line above"
    );
    assert_eq!(
        [window(262), window(263)],
        [vec![], vec!["ASY-13:soft@1".to_owned()]],
        "the `use` must start within 300 UTF-16 units"
    );
}

#[test]
fn crate_lint_attr_pub_glob_reexport_prelude_mod() {
    let lint = "crate-lint-attr";
    let glob = "pub-glob-reexport";
    let prelude = "prelude-mod";
    // ledger-app/src/lib.rs
    let good = "mod clock;\nmod event_store;\npub use clock::{Clock, Timestamp};\npub use event_store::{EventStore, Recorded, Version};\npub(crate) use util::*;\n#[cfg(unix)]\npub use unix::*;\n#[cfg(test)]\nmod tests {\n    use super::*;\n}";

    assert_eq!(
        sev_of("#![deny(warnings)]\n//! Crate.", lint, CodeKind::Prod),
        ["soft"],
        "a crate-level deny"
    );
    assert_eq!(
        sev_of("#![forbid(unsafe_code)]", lint, CodeKind::Bin),
        ["soft"],
        "a crate-level forbid in a binary"
    );
    assert_eq!(
        has(
            "#![doc(test(attr(deny(warnings))))]\n#![cfg_attr(docsrs, feature(doc_cfg))]\n#[must_use]\npub fn f() -> u8 { 1 }",
            lint,
            CodeKind::Prod
        ),
        [],
        "doc and cfg_attr attributes"
    );
    assert_eq!(
        sev_of("pub use inner::*;", glob, CodeKind::Prod),
        ["soft"],
        "a public glob re-export"
    );
    assert_eq!(
        has(good, glob, CodeKind::Prod),
        [],
        "crate-visible and cfg-selected globs"
    );
    assert_eq!(has(good, glob, CodeKind::Bin), [], "binaries");
    assert_eq!(
        sev_of(
            "pub mod prelude {\n    pub use crate::Ledger;\n}",
            prelude,
            CodeKind::Prod
        ),
        ["soft"],
        "a public prelude"
    );
    assert_eq!(
        has(
            "pub mod account;\nmod prelude_support;",
            prelude,
            CodeKind::Prod
        ),
        [],
        "other modules"
    );
}

#[test]
fn pub_glob_reexport_cfg_above_walks_attribute_lines() {
    assert_eq!(
        hits_of(
            "#[cfg(unix)]\n#[doc(hidden)]\npub use a::*;\n#[doc(hidden)]\n\npub use b::*;\n  #[cfg(x)]\r\npub use c::*;",
            "pub-glob-reexport",
            CodeKind::Prod
        ),
        ["API-14:soft@6"],
        "a cfg among the attribute lines directly above; a blank line ends them"
    );
}

#[test]
fn pub_smart_ptr_shared_mutable_state_and_dyn() {
    let key = "pub-smart-ptr";
    // async-concurrency.md, modern-rust.md, ownership.md Do examples
    let good = "\nimpl Live {\n    pub fn snapshot(&self) -> Arc<Config> { Arc::clone(&self.current) }\n    pub async fn get_or_fetch(&self, store: &impl Store, key: &str) -> Result<Option<Arc<str>>, StoreError> { Ok(None) }\n}\npub fn cached_or_fill(cache: &Mutex<Option<u64>>, fill: impl FnOnce() -> u64) -> Result<u64, Poisoned> { Ok(fill()) }\npub fn apply(snapshot: &mut Arc<Totals>, cents: u64) { Arc::make_mut(snapshot).add(cents); }\npub(crate) fn internal() -> Arc<Mutex<u8>> { Arc::default() }\npub struct DynClock(Arc<dyn Clock>);";

    for s in [
        "impl Service {\n    pub fn state(&self) -> Arc<Mutex<State>> { Arc::clone(&self.state) }\n}",
        "pub fn build_stamper(clock: Arc<dyn Clock>) -> Stamper<Arc<dyn Clock>> { Stamper::new(clock) }",
        "pub fn share(cfg: Rc<RefCell<Config>>) {}",
    ] {
        assert_eq!(sev_of(s, key, CodeKind::Prod), ["soft"], "{s}");
    }
    assert_eq!(
        has(good, key, CodeKind::Prod),
        [],
        "snapshots, borrowed mutexes and private signatures"
    );
}

#[test]
fn ops_impl_operators_must_not_overflow_silently() {
    let key = "ops-impl";
    // api-design.md API-22 Don't
    let bad = "impl std::ops::Add for Cents {\n    type Output = Self;\n    fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }\n}";
    let good = "\nimpl Add for Cents {\n    type Output = Option<Self>;\n    fn add(self, rhs: Self) -> Option<Self> { self.0.checked_add(rhs.0).map(Self) }\n}\nimpl Addressable for Cents {}\nimpl fmt::Display for Cents {\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, \"{}\", self.0) }\n}\nimpl Cents {\n    pub const fn checked_add(self, other: Self) -> Option<Self> {\n        match self.0.checked_add(other.0) { Some(v) => Some(Self(v)), None => None }\n    }\n}";

    assert_eq!(
        sev_of(bad, key, CodeKind::Prod),
        ["soft"],
        "plain `+` in an Add impl"
    );
    assert_eq!(
        sev_of(
            "impl<'a> Sub<&'a Cents> for &'a Cents { type Output = Cents; fn sub(self, rhs: &Cents) -> Cents { Cents(self.0 - rhs.0) } }",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "a generic Sub impl"
    );
    assert_eq!(
        has(good, key, CodeKind::Prod),
        [],
        "checked arithmetic and other impls"
    );
}

#[test]
fn ops_impl_body_search_like_1x() {
    assert_eq!(
        hits_of(
            "impl Add for A { fn add(self, o: Self) -> Self { Self(self.0.wrapping_add(o.0)) } }\nimpl Mul for B;\nimpl Sub for C { fn sub(self, o: Self) -> Self { Self(self.0 - o.0) } }",
            "ops-impl",
            CodeKind::Prod
        ),
        ["API-22:soft@2", "API-22:soft@3"],
        "the body is the next `{{`, even in a later item"
    );
}

#[test]
fn type_alias_primitive_get_prefix_mutex_unit() {
    let alias = "type-alias-primitive";
    let getter = "get-prefix";
    let unit = "mutex-unit";
    let aliases = "pub(crate) type TestResult = Result<(), Box<dyn Error>>;\ntype Result<T> = std::result::Result<T, Error>;\nimpl TryFrom<&str> for AccountId {\n    type Error = AccountIdError;\n}\nimpl Add for Meters {\n    type Output = u64;\n}";
    let getters = "\nimpl<K: Ord, V> Index<K, V> {\n    pub fn get(&self, key: &K) -> Option<&V> { self.map.get(key) }\n    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> { self.map.get_mut(key) }\n    pub fn get_or_insert(&mut self, key: K) -> &mut V { self.map.entry(key).or_default() }\n    pub fn name(&self) -> &str { &self.name }\n}\nimpl Named for Account {\n    fn get_name(&self) -> &str { &self.name }\n}";

    assert_eq!(
        sev_of("pub type StoreId = u32;", alias, CodeKind::Prod),
        ["soft"],
        "an alias for u32"
    );
    assert_eq!(
        sev_of(
            "pub(crate) type Name = String;\ntype Tag = &'static str;",
            alias,
            CodeKind::Prod
        ),
        ["soft", "soft"],
        "String and &'static str"
    );
    assert_eq!(
        has(aliases, alias, CodeKind::Prod),
        [],
        "Result aliases and associated types"
    );
    assert_eq!(
        sev_of(
            "impl Account {\n    pub fn get_name(&self) -> &str { &self.name }\n}",
            getter,
            CodeKind::Prod
        ),
        ["soft"],
        "a get_ getter"
    );
    assert_eq!(
        has(getters, getter, CodeKind::Prod),
        [],
        "keyed lookups, get_mut and trait impls"
    );
    // ownership.md OWN-12 Don't
    assert_eq!(
        sev_of(
            "struct Device {\n    lock: Mutex<()>,\n    port: Port,\n}",
            unit,
            CodeKind::Prod
        ),
        ["soft"],
        "a Mutex guarding nothing"
    );
    assert_eq!(
        has(
            "struct Device {\n    port: Mutex<Port>,\n    cache: Mutex<Option<u64>>,\n}",
            unit,
            CodeKind::Prod
        ),
        [],
        "a Mutex that owns its data"
    );
}

#[test]
fn get_prefix_reserved_names() {
    assert_eq!(
        hits_of(
            "impl A {\n fn get_unchecked_mutx(&self) {}\n fn get_mut(&self) {}\n fn get_unchecked(&self) {}\n fn get_unchecked_mut(&mut self) {}\n fn get_refs(&self) {}\n}",
            "get-prefix",
            CodeKind::Prod
        ),
        ["API-01:soft@2", "API-01:soft@6"],
        "only the exact reserved names are exempt"
    );
}

#[test]
fn type_alias_after_js_line_terminators() {
    assert_eq!(
        hits_of(
            "x\u{2028}type A = u8;\ry\rtype B = u8;\npub type C = u8;",
            "type-alias-primitive",
            CodeKind::Prod
        ),
        ["API-02:soft@1", "API-02:soft@1", "API-02:soft@2"],
        "JS `^` under m starts a line after U+2028 and \\r"
    );
}

#[test]
fn detached_spawn_std_threads_and_statement_level_spawns() {
    let key = "detached-spawn";
    let owned = "\nfn start() -> io::Result<thread::JoinHandle<()>> {\n    thread::Builder::new().name(\"worker\".into()).spawn(work)\n}\nasync fn run(tasks: &mut JoinSet<()>) -> io::Result<()> {\n    let handle = tokio::spawn(async move { poll().await });\n    tasks.spawn(async move { poll().await });\n    tokio::spawn(async move { poll().await }).await?;\n    thread::Builder::new().spawn(work)?.join();\n    thread::scope(|s| {\n        s.spawn(|| work());\n    });\n    handle.await?;\n    Ok(())\n}";

    assert_eq!(
        sev_of(
            "fn f() {\n    tokio::spawn(async move { sync_forever(db).await });\n}",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "a statement-level tokio::spawn"
    );
    assert_eq!(
        sev_of(
            "fn f() -> io::Result<()> {\n    thread::Builder::new()\n        .name(\"audit\".into())\n        .spawn(move || audit())?;\n    Ok(())\n}",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "a Builder spawn whose handle is dropped"
    );
    assert_eq!(
        has(owned, key, CodeKind::Prod),
        [],
        "returned, bound, joined or awaited handles"
    );
}

#[test]
fn detached_spawn_like_1x() {
    let key = "detached-spawn";

    assert_eq!(
        hits_of(
            "fn f() {\n    thread::spawn(work).join();\n    std::thread::spawn(work)\n        ?.join();\n    tokio::task::spawn(x);\n}",
            key,
            CodeKind::Prod
        ),
        ["ASY-04:soft@5"],
        "joined handles are owned"
    );
    assert_eq!(
        hits_of(
            "fn f() {\n    thread::Builder::new().name(\"a\".into()).spawn(work).unwrap();\n    tokio::spawn(a); tokio::spawn(b);\n}",
            key,
            CodeKind::Prod
        ),
        ["ASY-04:soft@2", "ASY-04:soft@3"],
        "only a spawn at the start of a line"
    );
    assert_eq!(
        hits_of(
            "fn f() {\u{2028}    tokio::spawn(async {});\n}",
            key,
            CodeKind::Prod
        ),
        ["ASY-04:soft@1"],
        "a line starts after U+2028"
    );
}

#[test]
fn no_new_rule_hits_on_an_idiomatic_ledger_module() {
    // Condensed from ledger-domain/src/money.rs and ledger-app/src/ledger.rs.
    let src = "\n//! Money and the ledger service.\nuse std::fmt;\nuse std::str::FromStr;\n\n/// An amount of money in cents.\n///\n/// ```\n/// # fn main() -> Result<(), ledger_domain::ParseCentsError> {\n/// let c: ledger_domain::Cents = \"12.34\".parse()?;\n/// assert_eq!(c.get(), 1234);\n/// # Ok(())\n/// # }\n/// ```\n#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]\npub struct Cents(u64);\n\nimpl Cents {\n    /// The amount in cents.\n    #[must_use]\n    pub const fn get(self) -> u64 { self.0 }\n\n    /// Adds, or `None` on overflow.\n    #[must_use]\n    pub const fn checked_add(self, other: Self) -> Option<Self> {\n        match self.0.checked_add(other.0) {\n            Some(v) => Some(Self(v)),\n            None => None,\n        }\n    }\n}\n\nimpl fmt::Display for Cents {\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n        write!(f, \"{}.{:02}\", self.0 / 100, self.0 % 100)\n    }\n}\n\nimpl FromStr for Cents {\n    type Err = ParseCentsError;\n    fn from_str(s: &str) -> Result<Self, Self::Err> {\n        let (units, fraction) = s.split_once('.').unwrap_or((s, \"0\"));\n        let units: u64 = units.parse().map_err(|_| ParseCentsError)?;\n        units.checked_mul(100).and_then(|u| u.checked_add(decimal(fraction)?)).map(Self).ok_or(ParseCentsError)\n    }\n}\n\n/// Runs commands against the store.\npub struct Ledger<S, C> {\n    store: S,\n    clock: C,\n}\n\nimpl<S: EventStore, C: Clock> Ledger<S, C> {\n    /// Builds a ledger.\n    pub fn new(store: S, clock: C) -> Self { Self { store, clock } }\n\n    /// Summarizes an account.\n    ///\n    /// # Errors\n    ///\n    /// Returns the store's error when loading fails.\n    pub fn summary(&self, account: &AccountId) -> Result<AccountSummary, S::Error> {\n        let events = self.store.load(account)?;\n        Ok(AccountSummary::from_events(&events))\n    }\n}\n";
    let new_keys = [
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

    let report = scan(src, CodeKind::Prod);
    let hits: Vec<&Hit> = report
        .hits()
        .iter()
        .filter(|h| new_keys.contains(&h.key().as_str()))
        .collect();

    assert_eq!(hits, Vec::<&Hit>::new(), "no 1.1 rule fires");
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
