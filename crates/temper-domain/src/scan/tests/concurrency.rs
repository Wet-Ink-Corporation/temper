//! 1.x tests 19 to 23: threads, fences, map indexing, `Result` filters and atomics.

use super::{has, hits_of, scan, sev_of};
use crate::scan::CodeKind;

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
fn filter_map_ok_and_parse_unwrap_or_are_advisory() {
    let filter = |src| sev_of(src, "filter-map-ok", CodeKind::Prod);
    let parse = |src| sev_of(src, "parse-unwrap-or", CodeKind::Prod);
    let both = scan(
        "fn f(s: &str) -> u16 { s.parse::<u16>().unwrap_or_default() }",
        CodeKind::Prod,
    );
    let both: Vec<&str> = both.hits().iter().map(|h| h.key().as_str()).collect();

    assert_eq!(
        filter(
            "fn f(xs: &[&str]) -> Vec<u32> { xs.iter().filter_map(|s| s.parse::<u32>().ok()).collect() }"
        ),
        ["soft"],
        "a closure ending in .ok()"
    );
    assert_eq!(
        filter("fn f(d: ReadDir) -> Vec<DirEntry> { d.filter_map(Result::ok).collect() }"),
        ["soft"],
        "Result::ok"
    );
    assert_eq!(
        filter("fn f(xs: &[u8]) -> usize { xs.iter().filter_map(|x| x.checked_add(1)).count() }"),
        [""; 0],
        "an Option-returning closure"
    );
    assert_eq!(
        parse("fn f(s: &str) -> u16 { s.parse().unwrap_or(8080) }"),
        ["soft"],
        "a parse failure replaced by a default"
    );
    assert!(
        both.contains(&"parse-unwrap-or") && !both.contains(&"unwrap-or-default"),
        "parse-unwrap-or, not unwrap-or-default: {both:?}"
    );
    // ledger-app lib.rs doctest helper and money.rs parser
    assert_eq!(
        parse(
            "fn load(&self, a: &AccountId) -> io::Result<Vec<Recorded>> { Ok(self.0.get(a).cloned().unwrap_or_default()) }"
        ),
        [""; 0],
        "no parse"
    );
    assert_eq!(
        parse("fn f(s: &str) -> Result<u16, ParseIntError> { let n = s.parse::<u16>()?; Ok(n) }"),
        [""; 0],
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
