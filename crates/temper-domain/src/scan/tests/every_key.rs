//! 1.x's last test, "every RULES key is reported by at least one scan in this file", as a table:
//! for each key in table order, the first source in `scan.test.mjs` that reports it (its
//! witness), with that hit's severity and line, computed by running 1.x (decision 1d-8).

use super::{TEST_REGIONS_SRC, lints, options, scanner};
use crate::scan::rules::RULES;
use crate::scan::{CodeKind, Severity};

/// One key's witness: the key, the file's kind, the crate's forbidden lints, the source, and
/// the severity and line of the key's first hit.
type Witness = (
    &'static str,
    CodeKind,
    Option<&'static [&'static str]>,
    &'static str,
    Severity,
    usize,
);

const SRC_UNWRAP: &str = "\npub fn a(v: &Vec<u8>) -> u32 {\n    let x = v.first().unwrap();\n    let y = std::env::args().next().expect(\"argv\");\n    if *x > 3 { panic!(\"big\") }\n    dbg!(x);\n    println!(\"{y}\");\n    u32::from(*x) + (*x as u32)\n}";
const SRC_ALLOW_ATTR: &str = "\n#[allow(dead_code)]\npub fn load(p: &str) -> Result<String, String> { std::fs::read_to_string(p).map_err(|e| e.to_string()) }\npub fn run() -> anyhow::Result<()> { Ok(()) }\nfn quiet() { std::fs::remove_file(\"x\").ok(); }\npub(crate) fn inner() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }\n";
const SRC_LET_UNDERSCORE: &str =
    "fn f(c: &AtomicU64) { let _ = c.fetch_update(Relaxed, Relaxed, |n| n.checked_add(1)); }";
const SRC_SHARED_MUT: &str =
    "impl Service {\n    pub fn state(&self) -> Arc<Mutex<State>> { Arc::clone(&self.state) }\n}";
const SRC_SEQCST: &str = "fn f() { atomic::compiler_fence(Ordering::SeqCst); }";

/// The witnesses, in `RULES` order.
const WITNESSES: &[Witness] = {
    use CodeKind::{Prod, Test};
    use Severity::{Ask, Hard, Soft};
    &[
        ("unsafe", Prod, None, TEST_REGIONS_SRC, Hard, 9),
        ("unwrap", Prod, None, SRC_UNWRAP, Hard, 3),
        ("panic", Prod, None, SRC_UNWRAP, Hard, 5),
        (
            "assert-in-prod",
            Prod,
            None,
            "pub fn f(x: u8) { assert!(x > 0, \"x must be positive\"); }",
            Soft,
            1,
        ),
        ("allow-attr", Prod, None, SRC_ALLOW_ATTR, Hard, 2),
        ("dbg", Prod, None, SRC_UNWRAP, Hard, 6),
        ("print", Prod, None, SRC_UNWRAP, Hard, 7),
        (
            "leak",
            Prod,
            None,
            "pub fn f(b: Box<u8>) -> &'static u8 {\n    Box::leak(b)\n}",
            Hard,
            2,
        ),
        (
            "catch-unwind",
            Prod,
            None,
            "pub fn f() { let _r = std::panic::catch_unwind(|| 1); }",
            Hard,
            1,
        ),
        (
            "exit",
            Prod,
            None,
            "pub fn f() {\n    std::process::exit(2);\n}",
            Hard,
            2,
        ),
        ("discard-ok", Prod, None, SRC_ALLOW_ATTR, Hard, 5),
        (
            "empty-err-arm",
            Prod,
            None,
            "fn f() {\n    match a() {\n        Ok(v) => match b(v) { Err(e) => log(e), Ok(()) => {} },\n        Err(_) => {}\n    }\n}",
            Hard,
            4,
        ),
        (
            "empty-if-let-err",
            Prod,
            None,
            "fn f() { if let Err(_) = retry(|| { work() }) {} }",
            Hard,
            1,
        ),
        ("let-underscore", Prod, None, SRC_LET_UNDERSCORE, Soft, 1),
        (
            "filter-map-ok",
            Prod,
            None,
            "fn f(xs: &[&str]) -> Vec<u32> { xs.iter().filter_map(|s| s.parse::<u32>().ok()).collect() }",
            Soft,
            1,
        ),
        (
            "parse-unwrap-or",
            Prod,
            None,
            "fn f(s: &str) -> u16 { s.parse().unwrap_or(8080) }",
            Soft,
            1,
        ),
        ("as-cast", Prod, None, SRC_UNWRAP, Hard, 8),
        ("ptr-param", Prod, None, SRC_UNWRAP, Hard, 2),
        (
            "lazy-static",
            Prod,
            None,
            "lazy_static! {\n    static ref N: u8 = 1;\n}",
            Hard,
            1,
        ),
        (
            "path-attr",
            Prod,
            None,
            "#[path = \"other.rs\"]\nmod other;",
            Hard,
            1,
        ),
        (
            "ignore-no-reason",
            Test,
            None,
            "#[ignore]\n#[test]\nfn slow() {}",
            Hard,
            1,
        ),
        (
            "async-trait",
            Prod,
            None,
            "#[async_trait]\npub trait Store { async fn get(&self) -> u8; }",
            Soft,
            1,
        ),
        ("shared-mut", Prod, None, SRC_SHARED_MUT, Soft, 2),
        (
            "mutex-unit",
            Prod,
            None,
            "struct Device {\n    lock: Mutex<()>,\n    port: Port,\n}",
            Soft,
            2,
        ),
        (
            "global-mut",
            Prod,
            None,
            "static COUNTER: AtomicUsize = AtomicUsize::new(0);",
            Soft,
            1,
        ),
        (
            "deref-inherit",
            Prod,
            None,
            "pub struct Meters(f64);\nimpl std::ops::Deref for Meters {\n    type Target = f64;\n    fn deref(&self) -> &f64 { &self.0 }\n}",
            Soft,
            2,
        ),
        (
            "sleep",
            Prod,
            None,
            "pub fn f(d: Duration) { std::thread::sleep(d); }",
            Soft,
            1,
        ),
        (
            "thread-spawn",
            Prod,
            None,
            "fn run() { let h = thread::spawn(move || work()); }",
            Hard,
            1,
        ),
        (
            "detached-spawn",
            Prod,
            None,
            "fn f() {\n    tokio::spawn(async move { sync_forever(db).await });\n}",
            Soft,
            2,
        ),
        (
            "unbounded",
            Prod,
            None,
            "pub fn f() { let (_tx, _rx) = mpsc::unbounded_channel::<u8>(); }",
            Soft,
            1,
        ),
        (
            "spin-lock",
            Prod,
            None,
            "pub fn lock(&self) {\n    while self.locked.swap(true, Ordering::Acquire) {\n        std::hint::spin_loop();\n    }\n}",
            Soft,
            2,
        ),
        ("seqcst", Prod, None, SRC_SEQCST, Soft, 1),
        ("fetch-update", Prod, None, SRC_LET_UNDERSCORE, Soft, 1),
        ("compiler-fence", Prod, None, SRC_SEQCST, Hard, 1),
        (
            "downcast",
            Prod,
            None,
            "pub fn f(e: &dyn Error) -> bool { e.downcast_ref::<io::Error>().is_some() }",
            Soft,
            1,
        ),
        (
            "unwrap-or-default",
            Prod,
            None,
            "fn load(&self, a: &AccountId) -> io::Result<Vec<Recorded>> { Ok(self.0.get(a).cloned().unwrap_or_default()) }",
            Soft,
            1,
        ),
        (
            "map-index",
            Prod,
            None,
            "fn f() { if let Err(_) = map[&k].send(1) {   } }",
            Soft,
            1,
        ),
        (
            "env-var",
            Prod,
            None,
            "pub fn f() -> Option<String> { std::env::var(\"HOME\").ok() }",
            Soft,
            1,
        ),
        (
            "ambient-now",
            Prod,
            None,
            "pub fn stamp() -> SystemTime {\n    SystemTime::now()\n}",
            Soft,
            2,
        ),
        (
            "serde-hashmap",
            Prod,
            None,
            "use std::collections::HashMap;\n#[derive(Debug, Serialize)]\npub struct Report {\n    counts: HashMap<String, u64>,\n}",
            Soft,
            4,
        ),
        (
            "test-branch",
            Prod,
            None,
            "pub fn timeout_secs() -> u64 {\n    if cfg!(test) { 1 } else { 30 }\n}",
            Hard,
            2,
        ),
        (
            "global-allocator",
            Prod,
            None,
            "#[global_allocator]\nstatic ALLOC: Counting = Counting;",
            Hard,
            1,
        ),
        (
            "should-panic",
            Test,
            None,
            "#[test]\n#[should_panic]\nfn f() { g(); }",
            Soft,
            2,
        ),
        (
            "weak-assert",
            Test,
            None,
            "#[test]\nfn f() {\n    assert!(parse(\"1\").is_ok());\n}",
            Soft,
            3,
        ),
        ("extern-crate", Prod, None, "extern crate serde;", Soft, 1),
        (
            "crate-lint-attr",
            Prod,
            None,
            "#![deny(warnings)]\n//! Crate.",
            Soft,
            1,
        ),
        (
            "pub-glob-reexport",
            Prod,
            None,
            "pub use inner::*;",
            Soft,
            1,
        ),
        (
            "prelude-mod",
            Prod,
            None,
            "pub mod prelude {\n    pub use crate::Ledger;\n}",
            Soft,
            1,
        ),
        ("pub-smart-ptr", Prod, None, SRC_SHARED_MUT, Soft, 2),
        (
            "ops-impl",
            Prod,
            None,
            "impl std::ops::Add for Cents {\n    type Output = Self;\n    fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }\n}",
            Soft,
            1,
        ),
        (
            "type-alias-primitive",
            Prod,
            None,
            "pub type StoreId = u32;",
            Soft,
            1,
        ),
        (
            "get-prefix",
            Prod,
            None,
            "impl Account {\n    pub fn get_name(&self) -> &str { &self.name }\n}",
            Soft,
            2,
        ),
        (
            "struct-bounds",
            Prod,
            None,
            "pub struct Lookup<S: Store + Clone + std::fmt::Debug> {\n    store: S,\n}",
            Soft,
            1,
        ),
        (
            "doc-unwrap",
            Prod,
            None,
            "\n/// Parses an id.\n///\n/// ```\n/// let id: AccountId = \"acc-42\".parse().unwrap();\n/// # let other: AccountId = \"acc-7\".parse().expect(\"valid\");\n/// ```\npub fn parse() {}\n",
            Hard,
            5,
        ),
        (
            "doc-arg-sections",
            Prod,
            None,
            "/// Adds two amounts.\n///\n/// # Arguments\n///\n/// * `a` - the first amount\n///\n/// # Returns\n///\n/// The sum.\npub fn add(a: u64, b: u64) -> u64 { a + b }",
            Soft,
            3,
        ),
        (
            "expect-attr",
            Prod,
            None,
            "\n#[expect(clippy::unwrap_used, reason = \"value is validated by the parser above\")]\npub fn a(v: Option<u8>) -> u8 { v.unwrap() }\n",
            Ask,
            2,
        ),
        (
            "forbid-override",
            Prod,
            Some(&["unsafe_code"]),
            "#[allow(unsafe_code)]\npub fn f() {}\n#[expect(clippy::unwrap_used, reason = \"checked\")]\npub fn g() {}\n",
            Hard,
            1,
        ),
        ("erased-error", Prod, None, SRC_ALLOW_ATTR, Hard, 3),
        (
            "todo-comment",
            Prod,
            None,
            "pub fn f() {}\n// TODO: handle overflow",
            Soft,
            2,
        ),
        (
            "commented-code",
            Prod,
            None,
            "// struct variants. Closed carries no data, so it is encoded as Closed {}.\n// for each account we replay the events in order.\n// let x = compute(y);\nfn a() {}\n",
            Soft,
            3,
        ),
    ]
};

#[test]
fn every_rules_key_is_reported_by_its_1x_witness() {
    let witness_keys: Vec<&str> = WITNESSES.iter().map(|w| w.0).collect();
    let rule_keys: Vec<&str> = RULES.iter().map(|spec| spec.key).collect();
    assert_eq!(
        witness_keys, rule_keys,
        "one witness per key, in table order"
    );

    let wrong: Vec<String> = WITNESSES
        .iter()
        .filter_map(|&(key, kind, forbid, src, severity, line)| {
            let forbid = forbid.map(lints);
            let report = scanner().scan(src, &options(kind, forbid.as_ref()));
            let first = report
                .hits()
                .iter()
                .find(|h| h.key().as_str() == key)
                .map(|h| (h.severity(), h.line().get()));
            (first != Some((severity, line))).then(|| format!("{key}: {first:?}"))
        })
        .collect();

    assert_eq!(
        wrong,
        Vec::<String>::new(),
        "each key's first hit on its witness has the 1.x severity and line"
    );
}
