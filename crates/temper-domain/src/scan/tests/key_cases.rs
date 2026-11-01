//! The 1.x `KEY_CASES` table for the regex rules (`tests/scan.test.mjs`): per key, positive
//! sources with the hits 1.x reports, and near misses that must not fire. A failure here is a
//! change in scanner output.

use super::{CodeKind, hits_of};

/// Each source's hits for `key` equal the listed `id:severity@line`s.
fn assert_hits(key: &str, cases: &[(CodeKind, &str, &[&str])]) {
    for &(kind, src, want) in cases {
        assert_eq!(hits_of(src, key, kind), want, "{kind:?}: {src}");
    }
}

/// No source has a hit for `key`.
fn assert_misses(key: &str, cases: &[(CodeKind, &str)]) {
    for &(kind, src) in cases {
        assert_eq!(
            hits_of(src, key, kind),
            Vec::<String>::new(),
            "{kind:?}: {src}"
        );
    }
}

#[test]
fn leak_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "leak",
        &[
            (
                CodeKind::Prod,
                "pub fn f(b: Box<u8>) -> &'static u8 {\n    Box::leak(b)\n}",
                &["H-07:hard@2"],
            ),
            (
                CodeKind::Test,
                "fn f(v: Vec<u8>) { std::mem::forget(v); }",
                &["H-07:hard@1"],
            ),
            (CodeKind::Bin, "static mut N: u8 = 0;", &["H-07:hard@1"]),
        ],
    );
}

#[test]
fn leak_near_misses_do_not_fire() {
    assert_misses(
        "leak",
        &[
            (
                CodeKind::Prod,
                "fn f() -> &'static str { \"Box::leak and mem::forget\" } // transmute",
            ),
            (
                CodeKind::Prod,
                "fn f(x: u8) -> u8 { core::mem::transmute_copy(&x) }",
            ),
        ],
    );
}

#[test]
fn catch_unwind_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "catch-unwind",
        &[
            (
                CodeKind::Prod,
                "pub fn f() { let _r = std::panic::catch_unwind(|| 1); }",
                &["H-03:hard@1"],
            ),
            (
                CodeKind::Test,
                "fn f() { let _r = std::panic::catch_unwind(|| 1); }",
                &["H-03:soft@1"],
            ),
        ],
    );
}

#[test]
fn catch_unwind_near_misses_do_not_fire() {
    assert_misses(
        "catch-unwind",
        &[
            (
                CodeKind::Prod,
                "fn f() -> &'static str { \"catch_unwind\" } // catch_unwind",
            ),
            (CodeKind::Prod, "fn catch_unwinding() {}"),
        ],
    );
}

#[test]
fn exit_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "exit",
        &[
            (
                CodeKind::Prod,
                "pub fn f() {\n    std::process::exit(2);\n}",
                &["ARC-14:hard@2"],
            ),
            (
                CodeKind::Test,
                "fn f() { process::exit(0) }",
                &["ARC-14:soft@1"],
            ),
            (
                CodeKind::Bin,
                "#[expect(clippy::exit, reason = \"fatal startup error\")]\nfn f() { process::exit(1) }",
                &["ARC-14:ask@2"],
            ),
        ],
    );
}

#[test]
fn exit_near_misses_do_not_fire() {
    assert_misses(
        "exit",
        &[
            (
                CodeKind::Bin,
                "fn main() -> std::process::ExitCode { std::process::ExitCode::FAILURE }",
            ),
            (
                CodeKind::Prod,
                "fn f() -> &'static str { \"process::exit(1)\" } // process::exit(1)",
            ),
        ],
    );
}

#[test]
fn let_underscore_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "let-underscore",
        &[(
            CodeKind::Prod,
            "fn f() {\n    let _ = std::fs::remove_file(\"x\");\n}",
            &["H-10:soft@2"],
        )],
    );
}

#[test]
fn let_underscore_near_misses_do_not_fire() {
    assert_misses(
        "let-underscore",
        &[
            (
                CodeKind::Test,
                "fn f() { let _ = std::fs::remove_file(\"x\"); }",
            ),
            (
                CodeKind::Prod,
                "fn f() { let _guard = lock(); let _x = 1; }",
            ),
        ],
    );
}

#[test]
fn lazy_static_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "lazy-static",
        &[
            (
                CodeKind::Prod,
                "lazy_static! {\n    static ref N: u8 = 1;\n}",
                &["MOD-03:hard@1"],
            ),
            (
                CodeKind::Test,
                "use once_cell::sync::Lazy;",
                &["MOD-03:hard@1"],
            ),
        ],
    );
}

#[test]
fn lazy_static_near_misses_do_not_fire() {
    assert_misses(
        "lazy-static",
        &[
            (
                CodeKind::Prod,
                "use std::sync::LazyLock;\nstatic N: LazyLock<u8> = LazyLock::new(|| 1);",
            ),
            (
                CodeKind::Prod,
                "fn f() -> &'static str { \"lazy_static!\" }",
            ),
        ],
    );
}

#[test]
fn path_attr_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "path-attr",
        &[
            (
                CodeKind::Prod,
                "#[path = \"other.rs\"]\nmod other;",
                &["ARC-06:hard@1"],
            ),
            (
                CodeKind::Test,
                "#[path = \"helpers/mod.rs\"] mod helpers;",
                &["ARC-06:hard@1"],
            ),
        ],
    );
}

#[test]
fn path_attr_near_misses_do_not_fire() {
    assert_misses(
        "path-attr",
        &[
            (
                CodeKind::Prod,
                "mod other;\n#[doc = \"path\"]\npub fn f() {}",
            ),
            (CodeKind::Prod, "// #[path = \"other.rs\"]\nmod other;"),
        ],
    );
}

#[test]
fn async_trait_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "async-trait",
        &[
            (
                CodeKind::Prod,
                "#[async_trait]\npub trait Store { async fn get(&self) -> u8; }",
                &["MOD-04:soft@1"],
            ),
            (
                CodeKind::Test,
                "#[async_trait::async_trait]\ntrait Store { async fn get(&self) -> u8; }",
                &["MOD-04:soft@1"],
            ),
        ],
    );
}

#[test]
fn async_trait_near_misses_do_not_fire() {
    assert_misses(
        "async-trait",
        &[
            (
                CodeKind::Prod,
                "pub trait Store { async fn get(&self) -> u8; }",
            ),
            (CodeKind::Prod, "#[async_trait_ext]\npub trait Store {}"),
        ],
    );
}

#[test]
fn shared_mut_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "shared-mut",
        &[
            (
                CodeKind::Prod,
                "pub struct S {\n    inner: Rc<RefCell<u8>>,\n}",
                &["OWN-11:soft@2"],
            ),
            (
                CodeKind::Bin,
                "fn f(m: Arc<RwLock<Vec<u8>>>) {}",
                &["OWN-11:soft@1"],
            ),
        ],
    );
}

#[test]
fn shared_mut_near_misses_do_not_fire() {
    assert_misses(
        "shared-mut",
        &[
            (CodeKind::Test, "fn f(m: Arc<Mutex<u8>>) {}"),
            (
                CodeKind::Prod,
                "pub struct S { inner: Arc<Vec<u8>>, cell: RefCell<u8> }",
            ),
        ],
    );
}

#[test]
fn global_mut_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "global-mut",
        &[
            (
                CodeKind::Prod,
                "static COUNTER: AtomicUsize = AtomicUsize::new(0);",
                &["ARC-09:soft@1"],
            ),
            (
                CodeKind::Test,
                "static CACHE: Mutex<Vec<u8>> = Mutex::new(Vec::new());",
                &["ARC-09:soft@1"],
            ),
        ],
    );
}

#[test]
fn global_mut_near_misses_do_not_fire() {
    assert_misses(
        "global-mut",
        &[
            (CodeKind::Prod, "static NAME: &str = \"Mutex\";"),
            (
                CodeKind::Prod,
                "const LIMIT: AtomicUsize = AtomicUsize::new(0);",
            ),
        ],
    );
}

#[test]
fn deref_inherit_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "deref-inherit",
        &[
            (
                CodeKind::Prod,
                "pub struct Meters(f64);\nimpl std::ops::Deref for Meters {\n    type Target = f64;\n    fn deref(&self) -> &f64 { &self.0 }\n}",
                &["API-19:soft@2"],
            ),
            (
                CodeKind::Prod,
                "impl<T> DerefMut for W<T> {}",
                &["API-19:soft@1"],
            ),
        ],
    );
}

#[test]
fn deref_inherit_near_misses_do_not_fire() {
    assert_misses(
        "deref-inherit",
        &[
            (
                CodeKind::Test,
                "impl Deref for Meters { type Target = f64; }",
            ),
            (
                CodeKind::Prod,
                "impl AsRef<f64> for Meters { fn as_ref(&self) -> &f64 { &self.0 } }",
            ),
        ],
    );
}

#[test]
fn sleep_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "sleep",
        &[
            (
                CodeKind::Prod,
                "pub fn f(d: Duration) { std::thread::sleep(d); }",
                &["TST-10:soft@1"],
            ),
            (
                CodeKind::Test,
                "fn f() { thread::sleep(Duration::from_millis(5)); }",
                &["TST-10:soft@1"],
            ),
        ],
    );
}

#[test]
fn sleep_near_misses_do_not_fire() {
    assert_misses(
        "sleep",
        &[
            (
                CodeKind::Example,
                "fn main() { std::thread::sleep(Duration::from_millis(5)); }",
            ),
            (
                CodeKind::Prod,
                "pub async fn f(d: Duration) { tokio::time::sleep(d).await; }",
            ),
        ],
    );
}

#[test]
fn unbounded_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "unbounded",
        &[
            (
                CodeKind::Prod,
                "pub fn f() { let (_tx, _rx) = mpsc::unbounded_channel::<u8>(); }",
                &["ASY-07:soft@1"],
            ),
            (
                CodeKind::Bin,
                "fn f() { let (_tx, _rx) = crossbeam_channel::unbounded(); }",
                &["ASY-07:soft@1"],
            ),
        ],
    );
}

#[test]
fn unbounded_near_misses_do_not_fire() {
    assert_misses(
        "unbounded",
        &[
            (
                CodeKind::Test,
                "fn f() { let (_tx, _rx) = mpsc::unbounded_channel::<u8>(); }",
            ),
            (
                CodeKind::Prod,
                "pub fn f() { let (_tx, _rx) = mpsc::channel::<u8>(8); }",
            ),
        ],
    );
}

#[test]
fn downcast_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "downcast",
        &[
            (
                CodeKind::Prod,
                "pub fn f(e: &dyn Error) -> bool { e.downcast_ref::<io::Error>().is_some() }",
                &["MOD-11:soft@1"],
            ),
            (
                CodeKind::Prod,
                "pub fn f(x: &dyn Shape) -> &dyn Any { x.as_any() }",
                &["MOD-11:soft@1"],
            ),
        ],
    );
}

#[test]
fn downcast_near_misses_do_not_fire() {
    assert_misses(
        "downcast",
        &[
            (
                CodeKind::Test,
                "fn f(e: &dyn Error) -> bool { e.downcast_ref::<io::Error>().is_some() }",
            ),
            (
                CodeKind::Prod,
                "pub fn downcasting_count(as_any_count: u8) -> u8 { as_any_count }",
            ),
        ],
    );
}

#[test]
fn env_var_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "env-var",
        &[
            (
                CodeKind::Prod,
                "pub fn f() -> Option<String> { std::env::var(\"HOME\").ok() }",
                &["ARC-11:soft@1"],
            ),
            (
                CodeKind::Test,
                "fn f() { let _v = env::var_os(\"HOME\"); }",
                &["ARC-11:soft@1"],
            ),
        ],
    );
}

#[test]
fn env_var_near_misses_do_not_fire() {
    assert_misses(
        "env-var",
        &[
            (
                CodeKind::Bin,
                "fn main() { let _v = std::env::var(\"HOME\"); }",
            ),
            (
                CodeKind::Prod,
                "pub fn f() -> Option<String> { std::env::args().next() }",
            ),
        ],
    );
}

#[test]
fn should_panic_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "should-panic",
        &[
            (
                CodeKind::Test,
                "#[test]\n#[should_panic]\nfn f() { g(); }",
                &["TST-05:soft@2"],
            ),
            (
                CodeKind::Prod,
                "pub fn g() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    #[should_panic(expected = \"boom\")]\n    fn f() { super::g(); }\n}",
                &["TST-05:soft@5"],
            ),
        ],
    );
}

#[test]
fn should_panic_near_misses_do_not_fire() {
    assert_misses(
        "should-panic",
        &[
            (CodeKind::Prod, "#[should_panic]\npub fn g() {}"),
            (
                CodeKind::Test,
                "#[test]\nfn f() { assert_eq!(g(), Err(E::Boom)); } // #[should_panic]",
            ),
        ],
    );
}

#[test]
fn weak_assert_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "weak-assert",
        &[
            (
                CodeKind::Test,
                "#[test]\nfn f() {\n    assert!(parse(\"1\").is_ok());\n}",
                &["TST-05:soft@3"],
            ),
            (
                CodeKind::Test,
                "fn f() { assert!(map.get(&k).is_none()); }",
                &["TST-05:soft@1"],
            ),
        ],
    );
}

#[test]
fn weak_assert_near_misses_do_not_fire() {
    assert_misses(
        "weak-assert",
        &[
            (
                CodeKind::Prod,
                "pub fn f(r: Result<u8, E>) { assert!(r.is_ok()); }",
            ),
            (
                CodeKind::Test,
                "fn f() { assert_eq!(parse(\"1\"), Ok(1)); assert!(r.is_ok() && done); }",
            ),
        ],
    );
}

#[test]
fn ambient_now_positive_cases_pin_id_severity_and_line() {
    assert_hits(
        "ambient-now",
        &[
            (
                CodeKind::Prod,
                "pub fn stamp() -> SystemTime {\n    SystemTime::now()\n}",
                &["OUT-04:soft@2"],
            ),
            (
                CodeKind::Prod,
                "pub fn id() -> Uuid { Uuid::new_v4() }",
                &["OUT-04:soft@1"],
            ),
            (
                CodeKind::Prod,
                "pub fn t() -> DateTime<Utc> { chrono::Utc::now() }",
                &["OUT-04:soft@1"],
            ),
            (
                CodeKind::Prod,
                "pub fn t() -> OffsetDateTime { OffsetDateTime::now_utc() }",
                &["OUT-04:soft@1"],
            ),
        ],
    );
}

#[test]
fn ambient_now_near_misses_do_not_fire() {
    assert_misses(
        "ambient-now",
        &[
            (
                CodeKind::Bin,
                "fn main() { let _t = std::time::SystemTime::now(); }",
            ),
            (
                CodeKind::Prod,
                "impl Clock for SystemClock {\n    fn now(&self) -> SystemTime { SystemTime::now() }\n}",
            ),
            (
                CodeKind::Prod,
                "pub fn elapsed() -> Duration { let t = Instant::now(); t.elapsed() }",
            ),
            (CodeKind::Test, "fn f() { let _id = Uuid::new_v4(); }"),
            (
                CodeKind::Prod,
                "pub fn f(clock: &impl Clock) -> u64 { clock.now() } // SystemTime::now()",
            ),
        ],
    );
}

#[test]
fn ambient_now_trait_impls() {
    assert_hits(
        "ambient-now",
        &[(
            CodeKind::Prod,
            "impl<T> for<'a> X {}\nimpl Clock for Sys { fn now(&self) -> SystemTime { SystemTime::now() } }\nimpl Sys { fn t() { SystemTime::now(); } }\nimpl<F: Fn() -> u8> Clock for F { fn now(&self) { Utc::now(); } }",
            &["OUT-04:soft@3"],
        )],
    );
}
