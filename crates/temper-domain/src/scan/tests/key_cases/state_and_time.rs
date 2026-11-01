//! `KEY_CASES` for shared and global state, `Deref` inheritance, sleeps, unbounded channels,
//! downcasts and environment reads.

use super::{assert_hits, assert_misses};
use crate::scan::CodeKind;

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
