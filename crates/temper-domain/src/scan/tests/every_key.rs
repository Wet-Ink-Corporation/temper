//! 1.x's last test, "every RULES key is reported by at least one scan in this file", as a table:
//! for each key in table order, the first source in `scan.test.mjs` that reports it (its
//! witness), with that hit's severity and line, computed by running 1.x (decision 1d-8).

use super::{TEST_REGIONS_SRC, lints, options, scanner};
use crate::scan::rules::RULES;
use crate::scan::{CodeKind, Severity};

/// One key's witness: a source that reports the key, and the severity and line of its first hit.
struct Witness {
    /// The `RULES` key.
    key: &'static str,
    /// The file's kind.
    kind: CodeKind,
    /// The crate's forbidden lints, when the witness needs some.
    forbid: Option<&'static [&'static str]>,
    /// The scanned source.
    src: &'static str,
    /// The severity of the key's first hit.
    severity: Severity,
    /// The 1-based line of the key's first hit.
    line: usize,
}

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
        Witness {
            key: "unsafe",
            kind: Prod,
            forbid: None,
            src: TEST_REGIONS_SRC,
            severity: Hard,
            line: 9,
        },
        Witness {
            key: "unwrap",
            kind: Prod,
            forbid: None,
            src: SRC_UNWRAP,
            severity: Hard,
            line: 3,
        },
        Witness {
            key: "panic",
            kind: Prod,
            forbid: None,
            src: SRC_UNWRAP,
            severity: Hard,
            line: 5,
        },
        Witness {
            key: "assert-in-prod",
            kind: Prod,
            forbid: None,
            src: "pub fn f(x: u8) { assert!(x > 0, \"x must be positive\"); }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "allow-attr",
            kind: Prod,
            forbid: None,
            src: SRC_ALLOW_ATTR,
            severity: Hard,
            line: 2,
        },
        Witness {
            key: "dbg",
            kind: Prod,
            forbid: None,
            src: SRC_UNWRAP,
            severity: Hard,
            line: 6,
        },
        Witness {
            key: "print",
            kind: Prod,
            forbid: None,
            src: SRC_UNWRAP,
            severity: Hard,
            line: 7,
        },
        Witness {
            key: "leak",
            kind: Prod,
            forbid: None,
            src: "pub fn f(b: Box<u8>) -> &'static u8 {\n    Box::leak(b)\n}",
            severity: Hard,
            line: 2,
        },
        Witness {
            key: "catch-unwind",
            kind: Prod,
            forbid: None,
            src: "pub fn f() { let _r = std::panic::catch_unwind(|| 1); }",
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "exit",
            kind: Prod,
            forbid: None,
            src: "pub fn f() {\n    std::process::exit(2);\n}",
            severity: Hard,
            line: 2,
        },
        Witness {
            key: "discard-ok",
            kind: Prod,
            forbid: None,
            src: SRC_ALLOW_ATTR,
            severity: Hard,
            line: 5,
        },
        Witness {
            key: "empty-err-arm",
            kind: Prod,
            forbid: None,
            src: "fn f() {\n    match a() {\n        Ok(v) => match b(v) { Err(e) => log(e), Ok(()) => {} },\n        Err(_) => {}\n    }\n}",
            severity: Hard,
            line: 4,
        },
        Witness {
            key: "empty-if-let-err",
            kind: Prod,
            forbid: None,
            src: "fn f() { if let Err(_) = retry(|| { work() }) {} }",
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "let-underscore",
            kind: Prod,
            forbid: None,
            src: SRC_LET_UNDERSCORE,
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "filter-map-ok",
            kind: Prod,
            forbid: None,
            src: "fn f(xs: &[&str]) -> Vec<u32> { xs.iter().filter_map(|s| s.parse::<u32>().ok()).collect() }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "parse-unwrap-or",
            kind: Prod,
            forbid: None,
            src: "fn f(s: &str) -> u16 { s.parse().unwrap_or(8080) }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "as-cast",
            kind: Prod,
            forbid: None,
            src: SRC_UNWRAP,
            severity: Hard,
            line: 8,
        },
        Witness {
            key: "ptr-param",
            kind: Prod,
            forbid: None,
            src: SRC_UNWRAP,
            severity: Hard,
            line: 2,
        },
        Witness {
            key: "lazy-static",
            kind: Prod,
            forbid: None,
            src: "lazy_static! {\n    static ref N: u8 = 1;\n}",
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "path-attr",
            kind: Prod,
            forbid: None,
            src: "#[path = \"other.rs\"]\nmod other;",
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "ignore-no-reason",
            kind: Test,
            forbid: None,
            src: "#[ignore]\n#[test]\nfn slow() {}",
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "async-trait",
            kind: Prod,
            forbid: None,
            src: "#[async_trait]\npub trait Store { async fn get(&self) -> u8; }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "shared-mut",
            kind: Prod,
            forbid: None,
            src: SRC_SHARED_MUT,
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "mutex-unit",
            kind: Prod,
            forbid: None,
            src: "struct Device {\n    lock: Mutex<()>,\n    port: Port,\n}",
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "global-mut",
            kind: Prod,
            forbid: None,
            src: "static COUNTER: AtomicUsize = AtomicUsize::new(0);",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "deref-inherit",
            kind: Prod,
            forbid: None,
            src: "pub struct Meters(f64);\nimpl std::ops::Deref for Meters {\n    type Target = f64;\n    fn deref(&self) -> &f64 { &self.0 }\n}",
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "sleep",
            kind: Prod,
            forbid: None,
            src: "pub fn f(d: Duration) { std::thread::sleep(d); }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "thread-spawn",
            kind: Prod,
            forbid: None,
            src: "fn run() { let h = thread::spawn(move || work()); }",
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "detached-spawn",
            kind: Prod,
            forbid: None,
            src: "fn f() {\n    tokio::spawn(async move { sync_forever(db).await });\n}",
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "unbounded",
            kind: Prod,
            forbid: None,
            src: "pub fn f() { let (_tx, _rx) = mpsc::unbounded_channel::<u8>(); }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "spin-lock",
            kind: Prod,
            forbid: None,
            src: "pub fn lock(&self) {\n    while self.locked.swap(true, Ordering::Acquire) {\n        std::hint::spin_loop();\n    }\n}",
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "seqcst",
            kind: Prod,
            forbid: None,
            src: SRC_SEQCST,
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "fetch-update",
            kind: Prod,
            forbid: None,
            src: SRC_LET_UNDERSCORE,
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "compiler-fence",
            kind: Prod,
            forbid: None,
            src: SRC_SEQCST,
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "downcast",
            kind: Prod,
            forbid: None,
            src: "pub fn f(e: &dyn Error) -> bool { e.downcast_ref::<io::Error>().is_some() }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "unwrap-or-default",
            kind: Prod,
            forbid: None,
            src: "fn load(&self, a: &AccountId) -> io::Result<Vec<Recorded>> { Ok(self.0.get(a).cloned().unwrap_or_default()) }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "map-index",
            kind: Prod,
            forbid: None,
            src: "fn f() { if let Err(_) = map[&k].send(1) {   } }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "env-var",
            kind: Prod,
            forbid: None,
            src: "pub fn f() -> Option<String> { std::env::var(\"HOME\").ok() }",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "ambient-now",
            kind: Prod,
            forbid: None,
            src: "pub fn stamp() -> SystemTime {\n    SystemTime::now()\n}",
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "serde-hashmap",
            kind: Prod,
            forbid: None,
            src: "use std::collections::HashMap;\n#[derive(Debug, Serialize)]\npub struct Report {\n    counts: HashMap<String, u64>,\n}",
            severity: Soft,
            line: 4,
        },
        Witness {
            key: "test-branch",
            kind: Prod,
            forbid: None,
            src: "pub fn timeout_secs() -> u64 {\n    if cfg!(test) { 1 } else { 30 }\n}",
            severity: Hard,
            line: 2,
        },
        Witness {
            key: "global-allocator",
            kind: Prod,
            forbid: None,
            src: "#[global_allocator]\nstatic ALLOC: Counting = Counting;",
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "should-panic",
            kind: Test,
            forbid: None,
            src: "#[test]\n#[should_panic]\nfn f() { g(); }",
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "weak-assert",
            kind: Test,
            forbid: None,
            src: "#[test]\nfn f() {\n    assert!(parse(\"1\").is_ok());\n}",
            severity: Soft,
            line: 3,
        },
        Witness {
            key: "extern-crate",
            kind: Prod,
            forbid: None,
            src: "extern crate serde;",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "crate-lint-attr",
            kind: Prod,
            forbid: None,
            src: "#![deny(warnings)]\n//! Crate.",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "pub-glob-reexport",
            kind: Prod,
            forbid: None,
            src: "pub use inner::*;",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "prelude-mod",
            kind: Prod,
            forbid: None,
            src: "pub mod prelude {\n    pub use crate::Ledger;\n}",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "pub-smart-ptr",
            kind: Prod,
            forbid: None,
            src: SRC_SHARED_MUT,
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "ops-impl",
            kind: Prod,
            forbid: None,
            src: "impl std::ops::Add for Cents {\n    type Output = Self;\n    fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }\n}",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "type-alias-primitive",
            kind: Prod,
            forbid: None,
            src: "pub type StoreId = u32;",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "get-prefix",
            kind: Prod,
            forbid: None,
            src: "impl Account {\n    pub fn get_name(&self) -> &str { &self.name }\n}",
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "struct-bounds",
            kind: Prod,
            forbid: None,
            src: "pub struct Lookup<S: Store + Clone + std::fmt::Debug> {\n    store: S,\n}",
            severity: Soft,
            line: 1,
        },
        Witness {
            key: "doc-unwrap",
            kind: Prod,
            forbid: None,
            src: "\n/// Parses an id.\n///\n/// ```\n/// let id: AccountId = \"acc-42\".parse().unwrap();\n/// # let other: AccountId = \"acc-7\".parse().expect(\"valid\");\n/// ```\npub fn parse() {}\n",
            severity: Hard,
            line: 5,
        },
        Witness {
            key: "doc-arg-sections",
            kind: Prod,
            forbid: None,
            src: "/// Adds two amounts.\n///\n/// # Arguments\n///\n/// * `a` - the first amount\n///\n/// # Returns\n///\n/// The sum.\npub fn add(a: u64, b: u64) -> u64 { a + b }",
            severity: Soft,
            line: 3,
        },
        Witness {
            key: "expect-attr",
            kind: Prod,
            forbid: None,
            src: "\n#[expect(clippy::unwrap_used, reason = \"value is validated by the parser above\")]\npub fn a(v: Option<u8>) -> u8 { v.unwrap() }\n",
            severity: Ask,
            line: 2,
        },
        Witness {
            key: "forbid-override",
            kind: Prod,
            forbid: Some(&["unsafe_code"]),
            src: "#[allow(unsafe_code)]\npub fn f() {}\n#[expect(clippy::unwrap_used, reason = \"checked\")]\npub fn g() {}\n",
            severity: Hard,
            line: 1,
        },
        Witness {
            key: "erased-error",
            kind: Prod,
            forbid: None,
            src: SRC_ALLOW_ATTR,
            severity: Hard,
            line: 3,
        },
        Witness {
            key: "todo-comment",
            kind: Prod,
            forbid: None,
            src: "pub fn f() {}\n// TODO: handle overflow",
            severity: Soft,
            line: 2,
        },
        Witness {
            key: "commented-code",
            kind: Prod,
            forbid: None,
            src: "// struct variants. Closed carries no data, so it is encoded as Closed {}.\n// for each account we replay the events in order.\n// let x = compute(y);\nfn a() {}\n",
            severity: Soft,
            line: 3,
        },
    ]
};

#[test]
fn every_rules_key_is_reported_by_its_1x_witness() {
    let witness_keys: Vec<&str> = WITNESSES.iter().map(|witness| witness.key).collect();
    let rule_keys: Vec<&str> = RULES.iter().map(|spec| spec.key).collect();
    assert_eq!(
        witness_keys, rule_keys,
        "one witness per key, in table order"
    );

    let wrong: Vec<String> = WITNESSES
        .iter()
        .filter_map(|witness| {
            let forbid = witness.forbid.map(lints);
            let report = scanner().scan(witness.src, &options(witness.kind, forbid.as_ref()));
            let first = report
                .hits()
                .iter()
                .find(|h| h.key().as_str() == witness.key)
                .map(|h| (h.severity(), h.line().get()));
            (first != Some((witness.severity, witness.line)))
                .then(|| format!("{}: {first:?}", witness.key))
        })
        .collect();

    assert_eq!(
        wrong,
        Vec::<String>::new(),
        "each key's first hit on its witness has the 1.x severity and line"
    );
}
