//! 1.x tests 24 to 30: the API-design rules (crate lint attributes, re-exports, smart pointers,
//! operator impls, type aliases, getters) and detached spawns.

use super::{has, hits_of, sev_of};
use crate::scan::CodeKind;

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
fn struct_bounds_bounds_belong_on_impls() {
    let key = "struct-bounds";
    // ledger-app/src/ledger.rs and the API-21 exceptions in api-design.md
    let good = "\npub struct Ledger<S, C> {\n    store: S,\n    clock: C,\n}\nimpl<S: EventStore, C: Clock> Ledger<S, C> {\n    pub fn new(store: S, clock: C) -> Self { Self { store, clock } }\n}\nstruct Cursor<S: Store> { last: Option<S::Record> }\nstruct Typed<S: Store> { last: Option<<S as Store>::Record> }\nstruct Framed<W: ?Sized> { inner: Box<W> }\nstruct Journal<W: Write> { out: W }\nimpl<W: Write> Drop for Journal<W> { fn drop(&mut self) { let _flushed = self.out.flush(); } }\nstruct Borrowed<'a, T: 'a> { items: &'a [T] }\nstruct Buf<const N: usize> { bytes: [u8; N] }\npub struct Id<T> { raw: u64, marker: PhantomData<fn() -> T> }\nenum Either<L, R> { Left(L), Right(R) }";

    assert_eq!(
        sev_of(
            "pub struct Lookup<S: Store + Clone + std::fmt::Debug> {\n    store: S,\n}",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "an inline bound"
    );
    assert_eq!(
        sev_of(
            "pub struct Repo<S>\nwhere\n    S: Store,\n{\n    store: S,\n}",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "a `where` bound"
    );
    assert_eq!(
        sev_of(
            "pub enum Reply<T: Serialize> { Ok(T), Err(String) }",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "an enum's bound"
    );
    assert_eq!(
        sev_of("struct Pair<T>(T, T) where T: Clone;", key, CodeKind::Prod),
        ["soft"],
        "a tuple struct's `where` bound"
    );
    assert_eq!(
        has(good, key, CodeKind::Prod),
        Vec::new(),
        "unbounded types and the API-21 exceptions"
    );
}

#[test]
fn struct_bounds_like_1x() {
    let quiet = [
        "struct A<T: X { t: T }",
        "struct C<T> where T: Iterator { n: T::Item }",
        "struct D<T> where Vec<T>: X { v: <Vec<T> as X>::Y }",
        "struct E<T> where Vec<T>: X { v: Vec<T>::Y }",
        "struct G<T: X> { t: T }\nimpl<T: X> Drop for G<T> { fn drop(&mut self) {} }",
        "struct H<T: X> { t: T }\nimpl<T: X> Drop for Drop for H<T> {}",
        "struct J<T: X>(T::Out);",
        "struct L<'a, T: 'a + ?Sized, const N: usize = 3, U = u8> { t: &'a T }",
        "struct Q<T: X>\n{ t: <T as X>::Y }",
        "struct U<T: X> { t: <  T  as X>::Y }",
    ];
    let reported = [
        "struct B<T> where T::Item: Copy { t: T }",
        "struct F<T: X> { t: T }\nimpl<T: X> Drop for FBar<T> { fn drop(&mut self) {} }",
        "struct I<T: X> { t: T }\nimpl<T: X> Clone for I<T> { } Drop for I<T> {}",
        "struct K<T: X>(T) where T: Y;",
        "struct M<T: ?Sized + X> { t: Box<T> }",
        "struct N<T> where  : X { t: T }",
        "struct R<T: X> { t: Rs::T::Y }",
        "struct S<T: X> { t: xT::Y }",
    ];

    for src in quiet {
        assert_eq!(
            hits_of(src, "struct-bounds", CodeKind::Prod),
            Vec::<String>::new(),
            "{src}"
        );
    }
    for src in reported {
        assert_eq!(
            hits_of(src, "struct-bounds", CodeKind::Prod),
            ["API-21:soft@1"],
            "{src}"
        );
    }
    assert_eq!(
        hits_of(
            "enum O<T: X> { A(T) }\nstruct P<F: Fn(u8) -> u8> { f: F }",
            "struct-bounds",
            CodeKind::Prod
        ),
        ["API-21:soft@1", "API-21:soft@2"],
        "an enum, and `->` inside the parameter list"
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
