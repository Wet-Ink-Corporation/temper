import { test } from 'node:test';
import assert from 'node:assert/strict';
import { scanSource, newHits, classifyPath, isBuildScript, isRustFence, RULES } from '../scripts/lib/scan.mjs';
import { maskRust, countTests, countIgnored } from '../scripts/lib/lexer.mjs';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const keys = (src, kind = 'prod', sev) => scanSource(src, { kind }).hits.filter((h) => !sev || h.severity === sev).map((h) => `${h.key}:${h.severity}`);

test('masks strings, raw strings, chars and comments but keeps lifetimes', () => {
  const src = `fn f<'a>(x: &'a str) -> char { let s = "x.unwrap()"; let r = r#"panic!("no")"#; // .unwrap()\n let c = '"'; /* unsafe { } */ 'q' }`;
  const { code } = maskRust(src);
  assert.ok(!code.includes('unwrap'));
  assert.ok(!code.includes('panic'));
  assert.ok(!code.includes('unsafe'));
  assert.ok(code.includes("<'a>"));
  assert.equal(code.length, src.length);
});

test('hard hits in production code', () => {
  const src = `
pub fn a(v: &Vec<u8>) -> u32 {
    let x = v.first().unwrap();
    let y = std::env::args().next().expect("argv");
    if *x > 3 { panic!("big") }
    dbg!(x);
    println!("{y}");
    u32::from(*x) + (*x as u32)
}`;
  const hard = keys(src, 'prod', 'hard');
  for (const k of ['ptr-param:hard', 'unwrap:hard', 'panic:hard', 'dbg:hard', 'print:hard', 'as-cast:hard']) assert.ok(hard.includes(k), `missing ${k} in ${hard}`);
  assert.equal(hard.filter((k) => k === 'unwrap:hard').length, 2);
});

test('test regions allow unwrap/panic but not dbg or unsafe', () => {
  const src = `
pub fn ok() -> u8 { 1 }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn t() { assert_eq!(Some(ok()).unwrap(), 1); dbg!(1); }
    #[test]
    fn u() { unsafe { } }
}`;
  const k = keys(src, 'prod');
  assert.ok(!k.some((x) => x.startsWith('unwrap')));
  assert.ok(k.includes('dbg:hard'));
  assert.ok(k.includes('unsafe:hard'));
  assert.equal(countTests(maskRust(src).code), 2);
});

test('#[expect] with a reason converts a hard hit into an exception request', () => {
  const src = `
#[expect(clippy::unwrap_used, reason = "value is validated by the parser above")]
pub fn a(v: Option<u8>) -> u8 { v.unwrap() }
`;
  const hits = scanSource(src, { kind: 'prod' }).hits;
  assert.ok(hits.some((h) => h.key === 'unwrap' && h.severity === 'ask'));
  assert.ok(hits.some((h) => h.key === 'expect-attr' && h.severity === 'ask'));
  const noReason = scanSource('#[expect(clippy::unwrap_used)]\npub fn a() {}', { kind: 'prod' }).hits;
  assert.ok(noReason.some((h) => h.key === 'expect-attr' && h.severity === 'hard'));
});

test('allow attribute, erased errors in pub signatures, .ok(); discard', () => {
  const src = `
#[allow(dead_code)]
pub fn load(p: &str) -> Result<String, String> { std::fs::read_to_string(p).map_err(|e| e.to_string()) }
pub fn run() -> anyhow::Result<()> { Ok(()) }
fn quiet() { std::fs::remove_file("x").ok(); }
pub(crate) fn inner() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }
`;
  const k = keys(src, 'prod', 'hard');
  assert.ok(k.includes('allow-attr:hard'));
  assert.equal(k.filter((x) => x === 'erased-error:hard').length, 2);
  assert.ok(k.includes('discard-ok:hard'));
  // binaries may use anyhow
  assert.ok(!keys(src, 'bin', 'hard').includes('erased-error:hard'));
});

test('newHits reports only introduced violations', () => {
  const before = scanSource('fn a() { let x = f().unwrap(); }', { kind: 'prod' }).hits;
  const after = scanSource('fn a() { let x = f().unwrap(); }\nfn b() { g().unwrap(); }', { kind: 'prod' }).hits;
  const fresh = newHits(before, after);
  assert.equal(fresh.length, 1);
  assert.equal(fresh[0].line, 2);
});

test('path classification', () => {
  assert.equal(classifyPath('src/lib.rs', true), 'prod');
  assert.equal(classifyPath('src/main.rs', true), 'bin');
  assert.equal(classifyPath('src/cli/args.rs', false), 'bin');
  assert.equal(classifyPath('tests/it/main.rs', true), 'test');
  assert.equal(classifyPath('src/parser/tests.rs', true), 'test');
  assert.equal(classifyPath('examples/demo.rs', true), 'example');
  assert.equal(classifyPath('benches/b.rs', true), 'example');
});

test('no false positives on idiomatic code', () => {
  const src = `
//! Crate docs.
use std::borrow::Cow;
/// Normalizes.
#[must_use]
pub fn normalize(s: &str) -> Cow<'_, str> {
    if s.contains(' ') { Cow::Owned(s.replace(' ', "_")) } else { Cow::Borrowed(s) }
}
/// Parses a port.
///
/// # Errors
/// Returns an error when the input is not a port number.
pub fn port(s: &str) -> Result<u16, std::num::ParseIntError> { s.parse() }
/// Widening is lossless.
#[must_use]
pub fn widen(x: u32) -> u64 { u64::from(x) }
fn labels() { 'outer: for _ in 0..1 { break 'outer; } }
const MSG: &str = "don't .unwrap() here; panic!() is banned";
`;
  const hits = scanSource(src, { kind: 'prod' }).hits.filter((h) => h.severity !== 'soft');
  assert.deepEqual(hits, []);
});

// ------------------------------------------------------------------ 1.1 rules
// Negatives are taken from the example workspace and the rust-craft references.

const has = (src, key, kind = 'prod') => scanSource(src, { kind }).hits.filter((h) => h.key === key);
const sevOf = (src, key, kind = 'prod') => has(src, key, kind).map((h) => h.severity);

test('rule table: unique keys, rule ids, four contexts, fix text', () => {
  const seen = new Set();
  for (const r of RULES) {
    assert.ok(!seen.has(r.key), `duplicate key ${r.key}`);
    seen.add(r.key);
    assert.match(r.id, /^[A-Z]{1,4}-\d{2}$/, r.key);
    assert.deepEqual(Object.keys(r.ctx).sort(), ['bin', 'example', 'prod', 'test'], r.key);
    assert.ok(r.fix && r.msg, r.key);
    assert.ok(Boolean(r.re) !== Boolean(r.find), `${r.key} needs exactly one of re/find`);
  }
  // rules added or changed in 1.1 name their rule in the fix text
  const v11 = ['assert-in-prod', 'ignore-no-reason', 'extern-crate', 'doc-unwrap', 'empty-err-arm', 'empty-if-let-err', 'thread-spawn', 'compiler-fence', 'global-allocator', 'map-index', 'filter-map-ok', 'parse-unwrap-or', 'seqcst', 'fetch-update', 'crate-lint-attr', 'pub-glob-reexport', 'prelude-mod', 'pub-smart-ptr', 'struct-bounds', 'ops-impl', 'type-alias-primitive', 'get-prefix', 'doc-arg-sections', 'mutex-unit', 'detached-spawn'];
  for (const key of v11) {
    const r = RULES.find((x) => x.key === key);
    assert.ok(r, key);
    assert.ok(r.fix.includes(r.id), `${key}: fix text should cite ${r.id}`);
  }
});

test('assert-in-prod: compile-time assertions are exempt', () => {
  assert.deepEqual(sevOf('pub fn f(x: u8) { assert!(x > 0, "x must be positive"); }', 'assert-in-prod'), ['soft']);
  // PERF-05 size pin and MOD-10 inline const block
  assert.deepEqual(has('const _: () = assert!(size_of::<Event>() <= 128, "Event grew past two cache lines");', 'assert-in-prod'), []);
  assert.deepEqual(has('pub fn chunk<const N: usize>(xs: &[u8]) -> usize {\n    const { assert!(N > 0, "N must be positive") };\n    xs.len() / N\n}', 'assert-in-prod'), []);
  // a const generic parameter is not a const context
  assert.deepEqual(sevOf('impl<const N: usize> Ring<N> {\n    pub fn push(&mut self) { assert!(self.len < N, "full"); }\n}', 'assert-in-prod'), ['soft']);
});

test('ignore-no-reason: also catches cfg_attr(.., ignore)', () => {
  for (const s of ['#[ignore]\n#[test]\nfn slow() {}', '#[cfg_attr(miri, ignore)]\n#[test]\nfn spawns() {}', '#[cfg_attr(any(miri, all(unix, not(target_os = "linux"))), ignore)]\n#[test]\nfn t() {}']) {
    assert.deepEqual(sevOf(s, 'ignore-no-reason', 'test'), ['hard'], s);
  }
  for (const s of ['#[cfg_attr(miri, ignore = "Miri cannot spawn processes")]\n#[test]\nfn cli() {}', '#[ignore = "needs a network"]\n#[test]\nfn net() {}', '#[derive(Debug)]\n#[cfg_attr(feature = "educe", educe(Debug, ignore))]\nstruct S;']) {
    assert.deepEqual(has(s, 'ignore-no-reason', 'test'), [], s);
  }
});

test('extern-crate: alloc and std in a no_std crate are exempt', () => {
  assert.deepEqual(sevOf('extern crate serde;', 'extern-crate'), ['soft']);
  assert.deepEqual(has('#![no_std]\nextern crate alloc;\n#[cfg(test)]\nextern crate std;\nextern crate self as ledger;', 'extern-crate'), []);
});

test('print: build scripts get the writeln! fix text', () => {
  const src = 'fn main() {\n    println!("cargo::rerun-if-changed=schema.json");\n}';
  const [bin] = scanSource(src, { kind: 'bin' }).hits.filter((h) => h.key === 'print');
  const [build] = scanSource(src, { kind: 'bin', build: true }).hits.filter((h) => h.key === 'print');
  assert.equal(bin.severity, 'hard');
  assert.equal(build.severity, 'hard');
  assert.match(build.fix, /writeln!\(io::stdout\(\)\.lock\(\), "cargo::/);
  assert.doesNotMatch(bin.fix, /cargo::/);
  assert.ok(isBuildScript('build.rs') && !isBuildScript('src/build.rs') && !isBuildScript('src/bin/build.rs'));
});

test('doc-unwrap: unwrap/expect/panic in doctest fences', () => {
  const bad = `
/// Parses an id.
///
/// \`\`\`
/// let id: AccountId = "acc-42".parse().unwrap();
/// # let other: AccountId = "acc-7".parse().expect("valid");
/// \`\`\`
pub fn parse() {}
`;
  const hits = has(bad, 'doc-unwrap');
  assert.deepEqual(hits.map((h) => [h.severity, h.line]), [['hard', 5], ['hard', 6]]);
  assert.deepEqual(sevOf('//! ```no_run\n//! if bad { panic!("boom") }\n//! ```\n', 'doc-unwrap'), ['hard']);
  assert.deepEqual(sevOf('/**\n * ```\n * let v = f().unwrap();\n * ```\n */\npub fn g() {}', 'doc-unwrap'), ['hard']);
  assert.deepEqual(sevOf('#[cfg(test)]\nmod tests {\n    /// ```\n    /// x.unwrap();\n    /// ```\n    fn helper() {}\n}', 'doc-unwrap'), ['soft']);
  // rustdoc runs no doctests in binaries or examples
  assert.deepEqual(sevOf('//! ```\n//! let cfg = Config::load().unwrap();\n//! ```\nfn main() {}', 'doc-unwrap', 'bin'), ['soft']);
  assert.deepEqual(sevOf('//! ```\n//! let cfg = Config::load().unwrap();\n//! ```\nfn main() {}', 'doc-unwrap', 'example'), ['soft']);
  // a should_panic fence shows a panic on purpose
  assert.deepEqual(has('/// ```should_panic\n/// let v: Vec<u8> = Vec::new();\n/// v.first().unwrap();\n/// ```\npub fn f() {}', 'doc-unwrap'), []);
  // CRLF sources
  assert.deepEqual(has(bad.replace(/\n/g, '\r\n'), 'doc-unwrap').map((h) => h.line), [5, 6]);
});

test('doc-unwrap: idiomatic doctests and non-Rust fences are clean', () => {
  // ledger-domain/src/account_id.rs
  const good = `
/// # Examples
///
/// \`\`\`
/// # fn main() -> Result<(), ledger_domain::ParseAccountIdError> {
/// let id: ledger_domain::AccountId = "acc-42".parse()?;
/// assert_eq!(id.as_str(), "acc-42");
/// # Ok(())
/// # }
/// \`\`\`
///
/// Never call \`.unwrap()\` here; \`panic!\` is banned too.
///
/// \`\`\`text
/// thread 'main' panicked at src/main.rs: called \`Option::unwrap()\` on a \`None\` value
/// \`\`\`
///
/// \`\`\`compile_fail
/// let x: u8 = None.unwrap();
/// \`\`\`
///
/// \`\`\`rust,ignore
/// let x = y.unwrap();
/// \`\`\`
///
/// \`\`\`toml
/// expect = "x.unwrap()"
/// \`\`\`
///
/// \`\`\`
/// let s = "call .unwrap() and panic!() elsewhere";
/// # Ok::<(), std::io::Error>(())
/// \`\`\`
#[derive(Debug)]
pub struct AccountId(String);
`;
  assert.deepEqual(has(good, 'doc-unwrap'), []);
  assert.ok(isRustFence('') && isRustFence('rust') && isRustFence('no_run') && isRustFence('rust,edition2024'));
  assert.ok(!isRustFence('text') && !isRustFence('ignore') && !isRustFence('rust,ignore') && !isRustFence('compile_fail') && !isRustFence('sh'));
});

test('empty-if-let-err finds the body brace by depth; empty-err-arm allows keep-first-error', () => {
  assert.deepEqual(has('fn f() { if let Err(_) = tx.try_send(Event::Tick {}) { tracing::warn!("queue full"); } }', 'empty-if-let-err'), []);
  assert.deepEqual(has('fn f() -> Result<(), E> { if let Err(_) = retry(|| {}) { return Err(E::Gave); } Ok(()) }', 'empty-if-let-err'), []);
  assert.deepEqual(sevOf('fn f() { if let Err(_) = retry(|| { work() }) {} }', 'empty-if-let-err'), ['hard']);
  assert.deepEqual(sevOf('fn f() { if let Err(_) = map[&k].send(1) {   } }', 'empty-if-let-err'), ['hard']);
  const firstError = `
fn parse_all(items: &[&str]) -> Result<(), ParseError> {
    let mut first = None;
    for item in items {
        match parse(item) {
            Ok(v) => keep(v),
            Err(e) if first.is_none() => first = Some(e),
            Err(_) => {}
        }
    }
    first.map_or(Ok(()), Err)
}`;
  assert.deepEqual(has(firstError, 'empty-err-arm'), []);
  // a binding in a nested match does not excuse the outer arm
  const nested = 'fn f() {\n    match a() {\n        Ok(v) => match b(v) { Err(e) => log(e), Ok(()) => {} },\n        Err(_) => {}\n    }\n}';
  assert.deepEqual(sevOf(nested, 'empty-err-arm'), ['hard']);
});

test('lexer: escaped quote char literal; ignore counting', () => {
  const { code } = maskRust("let q = ['\\'', ',', 'a']; let s = \".unwrap()\";");
  assert.equal(code, "let q = ['  ', ' ', ' ']; let s = \"         \";");
  const src = '#[test]\n#[ignore = "slow"]\nfn a() {}\n#[test]\n#[cfg_attr(miri, ignore = "spawns a process")]\nfn b() {}\n#[test]\n#[cfg_attr(windows, ignore = "unix sockets")]\nfn c() {}\n#[test]\n#[ignore]\nfn d() {}';
  assert.equal(countIgnored(maskRust(src).code), 3);
  assert.equal(scanSource(src, { kind: 'test' }).ignores, 3);
});

test('empty-err-arm and empty-if-let-err: silent discards', () => {
  assert.deepEqual(sevOf('fn a(r: Result<u8, E>) { match r { Ok(v) => keep(v), Err(_) => {} } }', 'empty-err-arm'), ['hard']);
  assert.deepEqual(sevOf('fn a() { match f() { Ok(()) => (), Err(_e) => (), } }', 'empty-err-arm'), ['hard']);
  assert.deepEqual(sevOf('fn a() {\n    match rx.try_recv() {\n        Ok(v) => keep(v),\n        Err(..) => { /* nothing to do */ }\n    }\n}', 'empty-err-arm'), ['hard']);
  assert.deepEqual(sevOf('fn a() { if let Err(_) = tx.send(1) {} }', 'empty-if-let-err'), ['hard']);
  assert.deepEqual(sevOf('fn a() { if let Err(_e) = tx.send(1) {\n    // receiver gone\n} }', 'empty-if-let-err'), ['hard']);
  assert.deepEqual(sevOf('#[test]\nfn t() { if let Err(_) = tx.send(1) {} }', 'empty-if-let-err'), ['soft']);
  // ledger-store-jsonl/src/store.rs
  const good = `
fn load(path: PathBuf) -> Result<Vec<Recorded>, StoreError> {
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(StoreError::Read { path, source }),
    };
    if let Err(source) = file.lock_shared() {
        return Err(StoreError::Read { path, source });
    }
    if let Err(_) = probe() {} else { warm_up(); }
    Ok(Vec::new())
}`;
  assert.deepEqual(has(good, 'empty-err-arm'), []);
  assert.deepEqual(has(good, 'empty-if-let-err'), []);
});

test('thread-spawn: std::thread::spawn is refused outside tests', () => {
  assert.deepEqual(sevOf('fn run() { let h = thread::spawn(move || work()); }', 'thread-spawn'), ['hard']);
  assert.deepEqual(sevOf('fn run() { let h = std::thread::spawn(work); }', 'thread-spawn', 'bin'), ['hard']);
  assert.deepEqual(sevOf('#[test]\nfn t() { let h = thread::spawn(work); }', 'thread-spawn'), ['soft']);
  const excepted = '#[expect(clippy::disallowed_methods, reason = "startup thread; the process exits if it cannot start")]\nfn run() { let h = thread::spawn(work); }';
  assert.deepEqual(sevOf(excepted, 'thread-spawn'), ['ask']);
  // async-concurrency.md ASY-15 Do
  const good = `
pub fn start_audit(log: AuditLog) -> io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().name("audit".into()).spawn(move || log.run())
}
pub fn parallel_sum(parts: &[Vec<u64>]) -> io::Result<u64> {
    thread::scope(|s| {
        let handles = parts.iter().map(|p| thread::Builder::new().spawn_scoped(s, move || p.iter().sum::<u64>()));
        handles.count();
        Ok(0)
    })
}`;
  assert.deepEqual(has(good, 'thread-spawn'), []);
});

test('compiler-fence and global-allocator', () => {
  assert.deepEqual(sevOf('fn f() { atomic::compiler_fence(Ordering::SeqCst); }', 'compiler-fence'), ['hard']);
  assert.deepEqual(sevOf('use std::sync::atomic::{compiler_fence, Ordering};\nfn f() { compiler_fence(Ordering::SeqCst); }', 'compiler-fence'), ['hard']);
  assert.deepEqual(sevOf('fn f() { core::sync::atomic::compiler_fence(Ordering::Release); }', 'compiler-fence'), ['hard']);
  assert.deepEqual(has('fn f() { fence(Ordering::Acquire); }', 'compiler-fence'), []);
  // a user function of the same name (divan's time/fence.rs shape)
  assert.deepEqual(has('/// Keeps the timer reads ordered.\npub fn compiler_fence() { black_box(()); }\nfn g(t: &Timer) { compiler_fence(); t.fence.compiler_fence(); }', 'compiler-fence'), []);
  const alloc = '#[global_allocator]\nstatic ALLOC: Counting = Counting;';
  assert.deepEqual(sevOf(alloc, 'global-allocator'), ['hard']);
  assert.deepEqual(has(alloc, 'global-allocator', 'bin'), []);
  // performance.md dhat entry point: a top-level fn main marks a binary root
  const dhat = '#[cfg(feature = "dhat-heap")]\n#[global_allocator]\nstatic ALLOC: dhat::Alloc = dhat::Alloc;\n\nfn main() {\n    #[cfg(feature = "dhat-heap")]\n    let _profiler = dhat::Profiler::new_heap();\n}';
  assert.deepEqual(has(dhat, 'global-allocator'), []);
  assert.deepEqual(has('#[cfg(test)]\nmod tests {\n    #[global_allocator]\n    static A: dhat::Alloc = dhat::Alloc;\n}', 'global-allocator'), []);
  // a bodiless #[cfg(test)] item is a test region too (a counting allocator for unit tests)
  const counting = '#[cfg(test)]\n#[global_allocator]\nstatic COUNTING: CountingAlloc = CountingAlloc::new();\n\npub fn f() -> u8 { 1 }\n#[global_allocator]\nstatic REAL: System = System;';
  assert.deepEqual(has(counting, 'global-allocator').map((h) => h.line), [6]);
});

test('map-index: map[&key] is advisory; slices of references are not indexing', () => {
  assert.deepEqual(sevOf('fn f(&self, id: &AccountId) -> Cents { self.balances[&id.0] }', 'map-index'), ['soft']);
  assert.deepEqual(sevOf('fn f(m: &HashMap<K, V>, k: K) -> &V { &lookup()[&k] }', 'map-index'), ['soft']);
  // ledger-cli args.rs and tests/it/support.rs
  const good = `
fn parse_request(words: &[&str]) -> Result<Request, UsageError> { todo_free(words) }
pub(crate) fn ledger_all(store: &Path, commands: &[&[&str]]) -> TestResult { run(store, commands) }
fn g(a: &u8, b: &u8) {
    ledger_all(store.path(), &[&["open", "acc-1", "Ada Lovelace"]]);
    let pair = [&a, &b];
    let v = vec![&a];
    for x in [&a, &b] { use_it(x); }
    let [&first, ..] = pair else { return };
    let _m = matches!(pair, [&1, ..]);
    let t: Box<[&str]> = Box::new([]);
}`;
  assert.deepEqual(has(good, 'map-index'), []);
  assert.deepEqual(has('#[test]\nfn t() { assert_eq!(m[&k], 1); }', 'map-index'), []);
});

test('filter-map-ok and parse-unwrap-or: masked failures are advisory', () => {
  assert.deepEqual(sevOf('fn f(xs: &[&str]) -> Vec<u32> { xs.iter().filter_map(|s| s.parse::<u32>().ok()).collect() }', 'filter-map-ok'), ['soft']);
  assert.deepEqual(sevOf('fn f(d: ReadDir) -> Vec<DirEntry> { d.filter_map(Result::ok).collect() }', 'filter-map-ok'), ['soft']);
  assert.deepEqual(has('fn f(xs: &[u8]) -> usize { xs.iter().filter_map(|x| x.checked_add(1)).count() }', 'filter-map-ok'), []);
  assert.deepEqual(sevOf('fn f(s: &str) -> u16 { s.parse().unwrap_or(8080) }', 'parse-unwrap-or'), ['soft']);
  const both = scanSource('fn f(s: &str) -> u16 { s.parse::<u16>().unwrap_or_default() }', { kind: 'prod' }).hits.map((h) => h.key);
  assert.ok(both.includes('parse-unwrap-or') && !both.includes('unwrap-or-default'), both.join());
  // ledger-app lib.rs doctest helper and money.rs parser
  assert.deepEqual(has('fn load(&self, a: &AccountId) -> io::Result<Vec<Recorded>> { Ok(self.0.get(a).cloned().unwrap_or_default()) }', 'parse-unwrap-or'), []);
  assert.deepEqual(has('fn f(s: &str) -> Result<u16, ParseIntError> { let n = s.parse::<u16>()?; Ok(n) }', 'parse-unwrap-or'), []);
});

test('seqcst and fetch-update: atomics advisories', () => {
  // async-concurrency.md ASY-13 Don't
  assert.deepEqual(sevOf('fn next_id(next: &AtomicU64) -> u64 {\n    next.fetch_add(1, Ordering::SeqCst) // "to be safe": pairs with nothing\n}', 'seqcst'), ['soft']);
  const justified = 'fn arrive(&self) -> bool {\n    // SeqCst: the store of our flag and the load of theirs need one total order (Dekker).\n    self.mine.store(true, Ordering::SeqCst);\n    !self.theirs.load(Ordering::SeqCst) // same total order as the store above\n}';
  assert.deepEqual(has(justified, 'seqcst'), []);
  assert.deepEqual(has('use std::sync::atomic::Ordering::{Acquire, SeqCst};\nfn f(c: &AtomicU64) -> u64 { c.load(Ordering::Relaxed) }', 'seqcst'), []);
  assert.deepEqual(has('#[test]\nfn t() { assert_eq!(counter.load(Ordering::SeqCst), 1); }', 'seqcst'), []);
  assert.deepEqual(sevOf('fn f(c: &AtomicU64) { let _ = c.fetch_update(Relaxed, Relaxed, |n| n.checked_add(1)); }', 'fetch-update'), ['soft']);
  assert.deepEqual(has('fn f(c: &AtomicU64) -> Result<u64, u64> { c.try_update(Relaxed, Relaxed, |n| n.checked_add(1)) }', 'fetch-update'), []);
});

test('crate-lint-attr, pub-glob-reexport, prelude-mod', () => {
  assert.deepEqual(sevOf('#![deny(warnings)]\n//! Crate.', 'crate-lint-attr'), ['soft']);
  assert.deepEqual(sevOf('#![forbid(unsafe_code)]', 'crate-lint-attr', 'bin'), ['soft']);
  assert.deepEqual(has('#![doc(test(attr(deny(warnings))))]\n#![cfg_attr(docsrs, feature(doc_cfg))]\n#[must_use]\npub fn f() -> u8 { 1 }', 'crate-lint-attr'), []);
  assert.deepEqual(sevOf('pub use inner::*;', 'pub-glob-reexport'), ['soft']);
  // ledger-app/src/lib.rs
  const good = `mod clock;
mod event_store;
pub use clock::{Clock, Timestamp};
pub use event_store::{EventStore, Recorded, Version};
pub(crate) use util::*;
#[cfg(unix)]
pub use unix::*;
#[cfg(test)]
mod tests {
    use super::*;
}`;
  assert.deepEqual(has(good, 'pub-glob-reexport'), []);
  assert.deepEqual(has(good, 'pub-glob-reexport', 'bin'), []);
  assert.deepEqual(sevOf('pub mod prelude {\n    pub use crate::Ledger;\n}', 'prelude-mod'), ['soft']);
  assert.deepEqual(has('pub mod account;\nmod prelude_support;', 'prelude-mod'), []);
});

test('pub-smart-ptr: shared mutable state and dyn in public signatures', () => {
  assert.deepEqual(sevOf('impl Service {\n    pub fn state(&self) -> Arc<Mutex<State>> { Arc::clone(&self.state) }\n}', 'pub-smart-ptr'), ['soft']);
  assert.deepEqual(sevOf('pub fn build_stamper(clock: Arc<dyn Clock>) -> Stamper<Arc<dyn Clock>> { Stamper::new(clock) }', 'pub-smart-ptr'), ['soft']);
  assert.deepEqual(sevOf('pub fn share(cfg: Rc<RefCell<Config>>) {}', 'pub-smart-ptr'), ['soft']);
  // async-concurrency.md, modern-rust.md, ownership.md Do examples
  const good = `
impl Live {
    pub fn snapshot(&self) -> Arc<Config> { Arc::clone(&self.current) }
    pub async fn get_or_fetch(&self, store: &impl Store, key: &str) -> Result<Option<Arc<str>>, StoreError> { Ok(None) }
}
pub fn cached_or_fill(cache: &Mutex<Option<u64>>, fill: impl FnOnce() -> u64) -> Result<u64, Poisoned> { Ok(fill()) }
pub fn apply(snapshot: &mut Arc<Totals>, cents: u64) { Arc::make_mut(snapshot).add(cents); }
pub(crate) fn internal() -> Arc<Mutex<u8>> { Arc::default() }
pub struct DynClock(Arc<dyn Clock>);`;
  assert.deepEqual(has(good, 'pub-smart-ptr'), []);
});

test('struct-bounds: bounds belong on impls', () => {
  assert.deepEqual(sevOf('pub struct Lookup<S: Store + Clone + std::fmt::Debug> {\n    store: S,\n}', 'struct-bounds'), ['soft']);
  assert.deepEqual(sevOf('pub struct Repo<S>\nwhere\n    S: Store,\n{\n    store: S,\n}', 'struct-bounds'), ['soft']);
  assert.deepEqual(sevOf('pub enum Reply<T: Serialize> { Ok(T), Err(String) }', 'struct-bounds'), ['soft']);
  assert.deepEqual(sevOf('struct Pair<T>(T, T) where T: Clone;', 'struct-bounds'), ['soft']);
  // ledger-app/src/ledger.rs and the API-21 exceptions in api-design.md
  const good = `
pub struct Ledger<S, C> {
    store: S,
    clock: C,
}
impl<S: EventStore, C: Clock> Ledger<S, C> {
    pub fn new(store: S, clock: C) -> Self { Self { store, clock } }
}
struct Cursor<S: Store> { last: Option<S::Record> }
struct Typed<S: Store> { last: Option<<S as Store>::Record> }
struct Framed<W: ?Sized> { inner: Box<W> }
struct Journal<W: Write> { out: W }
impl<W: Write> Drop for Journal<W> { fn drop(&mut self) { let _flushed = self.out.flush(); } }
struct Borrowed<'a, T: 'a> { items: &'a [T] }
struct Buf<const N: usize> { bytes: [u8; N] }
pub struct Id<T> { raw: u64, marker: PhantomData<fn() -> T> }
enum Either<L, R> { Left(L), Right(R) }`;
  assert.deepEqual(has(good, 'struct-bounds'), []);
});

test('ops-impl: operators must not overflow silently', () => {
  // api-design.md API-22 Don't
  const bad = 'impl std::ops::Add for Cents {\n    type Output = Self;\n    fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }\n}';
  assert.deepEqual(sevOf(bad, 'ops-impl'), ['soft']);
  assert.deepEqual(sevOf("impl<'a> Sub<&'a Cents> for &'a Cents { type Output = Cents; fn sub(self, rhs: &Cents) -> Cents { Cents(self.0 - rhs.0) } }", 'ops-impl'), ['soft']);
  const good = `
impl Add for Cents {
    type Output = Option<Self>;
    fn add(self, rhs: Self) -> Option<Self> { self.0.checked_add(rhs.0).map(Self) }
}
impl Addressable for Cents {}
impl fmt::Display for Cents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}
impl Cents {
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) { Some(v) => Some(Self(v)), None => None }
    }
}`;
  assert.deepEqual(has(good, 'ops-impl'), []);
});

test('type-alias-primitive, get-prefix, mutex-unit', () => {
  assert.deepEqual(sevOf('pub type StoreId = u32;', 'type-alias-primitive'), ['soft']);
  assert.deepEqual(sevOf("pub(crate) type Name = String;\ntype Tag = &'static str;", 'type-alias-primitive'), ['soft', 'soft']);
  const aliases = `pub(crate) type TestResult = Result<(), Box<dyn Error>>;
type Result<T> = std::result::Result<T, Error>;
impl TryFrom<&str> for AccountId {
    type Error = AccountIdError;
}
impl Add for Meters {
    type Output = u64;
}`;
  assert.deepEqual(has(aliases, 'type-alias-primitive'), []);
  assert.deepEqual(sevOf('impl Account {\n    pub fn get_name(&self) -> &str { &self.name }\n}', 'get-prefix'), ['soft']);
  const getters = `
impl<K: Ord, V> Index<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> { self.map.get(key) }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> { self.map.get_mut(key) }
    pub fn get_or_insert(&mut self, key: K) -> &mut V { self.map.entry(key).or_default() }
    pub fn name(&self) -> &str { &self.name }
}
impl Named for Account {
    fn get_name(&self) -> &str { &self.name }
}`;
  assert.deepEqual(has(getters, 'get-prefix'), []);
  // ownership.md OWN-12 Don't
  assert.deepEqual(sevOf('struct Device {\n    lock: Mutex<()>,\n    port: Port,\n}', 'mutex-unit'), ['soft']);
  assert.deepEqual(has('struct Device {\n    port: Mutex<Port>,\n    cache: Mutex<Option<u64>>,\n}', 'mutex-unit'), []);
});

test('doc-arg-sections: no # Arguments / # Returns', () => {
  const bad = '/// Adds two amounts.\n///\n/// # Arguments\n///\n/// * `a` - the first amount\n///\n/// # Returns\n///\n/// The sum.\npub fn add(a: u64, b: u64) -> u64 { a + b }';
  assert.deepEqual(has(bad, 'doc-arg-sections').map((h) => h.line), [3, 7]);
  // ledger-domain/src/account.rs
  const good = '/// Decides a command.\n///\n/// # Errors\n///\n/// Returns a [`Rejection`] when the command would break an account rule.\n///\n/// ```text\n/// # Returns\n/// ```\npub fn decide() {}';
  assert.deepEqual(has(good, 'doc-arg-sections'), []);
});

test('detached-spawn: std threads and statement-level spawns', () => {
  assert.deepEqual(sevOf('fn f() {\n    tokio::spawn(async move { sync_forever(db).await });\n}', 'detached-spawn'), ['soft']);
  assert.deepEqual(sevOf('fn f() -> io::Result<()> {\n    thread::Builder::new()\n        .name("audit".into())\n        .spawn(move || audit())?;\n    Ok(())\n}', 'detached-spawn'), ['soft']);
  const owned = `
fn start() -> io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().name("worker".into()).spawn(work)
}
async fn run(tasks: &mut JoinSet<()>) -> io::Result<()> {
    let handle = tokio::spawn(async move { poll().await });
    tasks.spawn(async move { poll().await });
    tokio::spawn(async move { poll().await }).await?;
    thread::Builder::new().spawn(work)?.join();
    thread::scope(|s| {
        s.spawn(|| work());
    });
    handle.await?;
    Ok(())
}`;
  assert.deepEqual(has(owned, 'detached-spawn'), []);
});

test('no new-rule hits on an idiomatic ledger module', () => {
  // Condensed from ledger-domain/src/money.rs and ledger-app/src/ledger.rs.
  const src = `
//! Money and the ledger service.
use std::fmt;
use std::str::FromStr;

/// An amount of money in cents.
///
/// \`\`\`
/// # fn main() -> Result<(), ledger_domain::ParseCentsError> {
/// let c: ledger_domain::Cents = "12.34".parse()?;
/// assert_eq!(c.get(), 1234);
/// # Ok(())
/// # }
/// \`\`\`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Cents(u64);

impl Cents {
    /// The amount in cents.
    #[must_use]
    pub const fn get(self) -> u64 { self.0 }

    /// Adds, or \`None\` on overflow.
    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }
}

impl fmt::Display for Cents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

impl FromStr for Cents {
    type Err = ParseCentsError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (units, fraction) = s.split_once('.').unwrap_or((s, "0"));
        let units: u64 = units.parse().map_err(|_| ParseCentsError)?;
        units.checked_mul(100).and_then(|u| u.checked_add(decimal(fraction)?)).map(Self).ok_or(ParseCentsError)
    }
}

/// Runs commands against the store.
pub struct Ledger<S, C> {
    store: S,
    clock: C,
}

impl<S: EventStore, C: Clock> Ledger<S, C> {
    /// Builds a ledger.
    pub fn new(store: S, clock: C) -> Self { Self { store, clock } }

    /// Summarizes an account.
    ///
    /// # Errors
    ///
    /// Returns the store's error when loading fails.
    pub fn summary(&self, account: &AccountId) -> Result<AccountSummary, S::Error> {
        let events = self.store.load(account)?;
        Ok(AccountSummary::from_events(&events))
    }
}
`;
  const newKeys = new Set(['doc-unwrap', 'empty-err-arm', 'empty-if-let-err', 'thread-spawn', 'compiler-fence', 'global-allocator', 'map-index', 'filter-map-ok', 'parse-unwrap-or', 'seqcst', 'fetch-update', 'crate-lint-attr', 'pub-glob-reexport', 'prelude-mod', 'pub-smart-ptr', 'struct-bounds', 'ops-impl', 'type-alias-primitive', 'get-prefix', 'doc-arg-sections', 'mutex-unit', 'detached-spawn']);
  const hits = scanSource(src, { kind: 'prod' }).hits.filter((h) => newKeys.has(h.key));
  assert.deepEqual(hits, []);
});

test('commented-out code heuristic ignores prose', () => {
  const src = `// struct variants. Closed carries no data, so it is encoded as Closed {}.
// for each account we replay the events in order.
// let x = compute(y);
fn a() {}
`;
  const hits = scanSource(src, { kind: 'prod' }).hits.filter((h) => h.key === 'commented-code');
  assert.equal(hits.length, 1);
  assert.equal(hits[0].line, 3);
});

// One rule's hits as `id:severity@line`, so a case pins the rule ID, the
// severity and the line the scanner reports today.
const hitsOf = (src, key, kind = 'prod', opts = {}) =>
  scanSource(src, { kind, ...opts }).hits.filter((h) => h.key === key).map((h) => `${h.id}:${h.severity}@${h.line}`);

// Per key: positive sources with the hits expected today, one or more contexts
// each, and near-miss sources that must not fire. The severities are the `ctx`
// entries of the RULES table in scripts/lib/scan.mjs, so a failure here is a
// change in scanner output.
const KEY_CASES = {
  'leak': {
    hit: [
      ['prod', 'pub fn f(b: Box<u8>) -> &\'static u8 {\n    Box::leak(b)\n}', ['H-07:hard@2']],
      ['test', 'fn f(v: Vec<u8>) { std::mem::forget(v); }', ['H-07:hard@1']],
      ['bin', 'static mut N: u8 = 0;', ['H-07:hard@1']],
    ],
    miss: [
      ['prod', 'fn f() -> &\'static str { "Box::leak and mem::forget" } // transmute'],
      ['prod', 'fn f(x: u8) -> u8 { core::mem::transmute_copy(&x) }'],
    ],
  },
  'catch-unwind': {
    hit: [
      ['prod', 'pub fn f() { let _r = std::panic::catch_unwind(|| 1); }', ['H-03:hard@1']],
      ['test', 'fn f() { let _r = std::panic::catch_unwind(|| 1); }', ['H-03:soft@1']],
    ],
    miss: [
      ['prod', 'fn f() -> &\'static str { "catch_unwind" } // catch_unwind'],
      ['prod', 'fn catch_unwinding() {}'],
    ],
  },
  'exit': {
    hit: [
      ['prod', 'pub fn f() {\n    std::process::exit(2);\n}', ['ARC-14:hard@2']],
      ['test', 'fn f() { process::exit(0) }', ['ARC-14:soft@1']],
      ['bin', '#[expect(clippy::exit, reason = "fatal startup error")]\nfn f() { process::exit(1) }', ['ARC-14:ask@2']],
    ],
    miss: [
      ['bin', 'fn main() -> std::process::ExitCode { std::process::ExitCode::FAILURE }'],
      ['prod', 'fn f() -> &\'static str { "process::exit(1)" } // process::exit(1)'],
    ],
  },
  'let-underscore': {
    hit: [
      ['prod', 'fn f() {\n    let _ = std::fs::remove_file("x");\n}', ['H-10:soft@2']],
    ],
    miss: [
      ['test', 'fn f() { let _ = std::fs::remove_file("x"); }'],
      ['prod', 'fn f() { let _guard = lock(); let _x = 1; }'],
    ],
  },
  'lazy-static': {
    hit: [
      ['prod', 'lazy_static! {\n    static ref N: u8 = 1;\n}', ['MOD-03:hard@1']],
      ['test', 'use once_cell::sync::Lazy;', ['MOD-03:hard@1']],
    ],
    miss: [
      ['prod', 'use std::sync::LazyLock;\nstatic N: LazyLock<u8> = LazyLock::new(|| 1);'],
      ['prod', 'fn f() -> &\'static str { "lazy_static!" }'],
    ],
  },
  'path-attr': {
    hit: [
      ['prod', '#[path = "other.rs"]\nmod other;', ['ARC-06:hard@1']],
      ['test', '#[path = "helpers/mod.rs"] mod helpers;', ['ARC-06:hard@1']],
    ],
    miss: [
      ['prod', 'mod other;\n#[doc = "path"]\npub fn f() {}'],
      ['prod', '// #[path = "other.rs"]\nmod other;'],
    ],
  },
  'async-trait': {
    hit: [
      ['prod', '#[async_trait]\npub trait Store { async fn get(&self) -> u8; }', ['MOD-04:soft@1']],
      ['test', '#[async_trait::async_trait]\ntrait Store { async fn get(&self) -> u8; }', ['MOD-04:soft@1']],
    ],
    miss: [
      ['prod', 'pub trait Store { async fn get(&self) -> u8; }'],
      ['prod', '#[async_trait_ext]\npub trait Store {}'],
    ],
  },
  'shared-mut': {
    hit: [
      ['prod', 'pub struct S {\n    inner: Rc<RefCell<u8>>,\n}', ['OWN-11:soft@2']],
      ['bin', 'fn f(m: Arc<RwLock<Vec<u8>>>) {}', ['OWN-11:soft@1']],
    ],
    miss: [
      ['test', 'fn f(m: Arc<Mutex<u8>>) {}'],
      ['prod', 'pub struct S { inner: Arc<Vec<u8>>, cell: RefCell<u8> }'],
    ],
  },
  'global-mut': {
    hit: [
      ['prod', 'static COUNTER: AtomicUsize = AtomicUsize::new(0);', ['ARC-09:soft@1']],
      ['test', 'static CACHE: Mutex<Vec<u8>> = Mutex::new(Vec::new());', ['ARC-09:soft@1']],
    ],
    miss: [
      ['prod', 'static NAME: &str = "Mutex";'],
      ['prod', 'const LIMIT: AtomicUsize = AtomicUsize::new(0);'],
    ],
  },
  'deref-inherit': {
    hit: [
      ['prod', 'pub struct Meters(f64);\nimpl std::ops::Deref for Meters {\n    type Target = f64;\n    fn deref(&self) -> &f64 { &self.0 }\n}', ['API-19:soft@2']],
      ['prod', 'impl<T> DerefMut for W<T> {}', ['API-19:soft@1']],
    ],
    miss: [
      ['test', 'impl Deref for Meters { type Target = f64; }'],
      ['prod', 'impl AsRef<f64> for Meters { fn as_ref(&self) -> &f64 { &self.0 } }'],
    ],
  },
  'sleep': {
    hit: [
      ['prod', 'pub fn f(d: Duration) { std::thread::sleep(d); }', ['TST-10:soft@1']],
      ['test', 'fn f() { thread::sleep(Duration::from_millis(5)); }', ['TST-10:soft@1']],
    ],
    miss: [
      ['example', 'fn main() { std::thread::sleep(Duration::from_millis(5)); }'],
      ['prod', 'pub async fn f(d: Duration) { tokio::time::sleep(d).await; }'],
    ],
  },
  'unbounded': {
    hit: [
      ['prod', 'pub fn f() { let (_tx, _rx) = mpsc::unbounded_channel::<u8>(); }', ['ASY-07:soft@1']],
      ['bin', 'fn f() { let (_tx, _rx) = crossbeam_channel::unbounded(); }', ['ASY-07:soft@1']],
    ],
    miss: [
      ['test', 'fn f() { let (_tx, _rx) = mpsc::unbounded_channel::<u8>(); }'],
      ['prod', 'pub fn f() { let (_tx, _rx) = mpsc::channel::<u8>(8); }'],
    ],
  },
  'downcast': {
    hit: [
      ['prod', 'pub fn f(e: &dyn Error) -> bool { e.downcast_ref::<io::Error>().is_some() }', ['MOD-11:soft@1']],
      ['prod', 'pub fn f(x: &dyn Shape) -> &dyn Any { x.as_any() }', ['MOD-11:soft@1']],
    ],
    miss: [
      ['test', 'fn f(e: &dyn Error) -> bool { e.downcast_ref::<io::Error>().is_some() }'],
      ['prod', 'pub fn downcasting_count(as_any_count: u8) -> u8 { as_any_count }'],
    ],
  },
  'env-var': {
    hit: [
      ['prod', 'pub fn f() -> Option<String> { std::env::var("HOME").ok() }', ['ARC-11:soft@1']],
      ['test', 'fn f() { let _v = env::var_os("HOME"); }', ['ARC-11:soft@1']],
    ],
    miss: [
      ['bin', 'fn main() { let _v = std::env::var("HOME"); }'],
      ['prod', 'pub fn f() -> Option<String> { std::env::args().next() }'],
    ],
  },
  'should-panic': {
    hit: [
      ['test', '#[test]\n#[should_panic]\nfn f() { g(); }', ['TST-05:soft@2']],
      ['prod', 'pub fn g() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    #[should_panic(expected = "boom")]\n    fn f() { super::g(); }\n}', ['TST-05:soft@5']],
    ],
    miss: [
      ['prod', '#[should_panic]\npub fn g() {}'],
      ['test', '#[test]\nfn f() { assert_eq!(g(), Err(E::Boom)); } // #[should_panic]'],
    ],
  },
  'weak-assert': {
    hit: [
      ['test', '#[test]\nfn f() {\n    assert!(parse("1").is_ok());\n}', ['TST-05:soft@3']],
      ['test', 'fn f() { assert!(map.get(&k).is_none()); }', ['TST-05:soft@1']],
    ],
    miss: [
      ['prod', 'pub fn f(r: Result<u8, E>) { assert!(r.is_ok()); }'],
      ['test', 'fn f() { assert_eq!(parse("1"), Ok(1)); assert!(r.is_ok() && done); }'],
    ],
  },
  'todo-comment': {
    hit: [
      ['prod', 'pub fn f() {}\n// TODO: handle overflow', ['CRF-04:soft@2']],
      ['test', 'fn f() {} /* FIXME later */', ['CRF-04:soft@1']],
    ],
    miss: [
      ['prod', '// TODO(#123): handle overflow\n// FIXME see https://example.com/i/4\n// HACK until PROJ-12 lands\npub fn f() {}'],
      ['prod', '/// TODO in a doc comment\npub fn f() -> &\'static str { "TODO" }'],
    ],
  },
};

for (const [key, { hit, miss }] of Object.entries(KEY_CASES)) {
  test(`${key}: positive cases pin the current id, severity and line`, () => {
    for (const [kind, src, want] of hit) assert.deepEqual(hitsOf(src, key, kind), want, `${kind}: ${src}`);
  });
  test(`${key}: near-miss cases do not fire`, () => {
    for (const [kind, src] of miss) assert.deepEqual(hitsOf(src, key, kind), [], `${kind}: ${src}`);
  });
}

test('forbid-override: fires only for a lint the crate forbids', () => {
  const src = '#[allow(unsafe_code)]\npub fn f() {}\n#[expect(clippy::unwrap_used, reason = "checked")]\npub fn g() {}\n';
  const forbid = new Set(['unsafe_code']);
  assert.deepEqual(hitsOf(src, 'forbid-override', 'prod', { forbid }), ['H-05:hard@1']);
  assert.deepEqual(hitsOf(src, 'forbid-override', 'test', { forbid }), ['H-05:hard@1']);
  // Near misses: no forbid set, or a forbid set that does not name the lint.
  assert.deepEqual(hitsOf(src, 'forbid-override', 'prod'), []);
  assert.deepEqual(hitsOf(src, 'forbid-override', 'prod', { forbid: new Set(['clippy::panic']) }), []);
});

test('every RULES key is named in this file', () => {
  const self = readFileSync(fileURLToPath(import.meta.url), 'utf8');
  const named = (k) => new RegExp(`(?:^|[^a-z-])${k}(?:[^a-z-]|$)`, 'm').test(self);
  assert.deepEqual(RULES.map((r) => r.key).filter((k) => !named(k)), []);
});

test('large file: a seeded hit near the end of ~5000 lines is reported in bounded time', () => {
  const block = [
    '/// Adds two values.',
    '///',
    '/// # Errors',
    '///',
    '/// Returns `None` on overflow.',
    'pub fn add_N(a: u32, b: u32) -> Option<u32> {',
    '    let sum = a.checked_add(b)?;',
    '    if sum > 10 { return Some(sum - 1); }',
    '    Some(sum)',
    '}',
    '',
  ];
  const lines = [];
  for (let i = 0; lines.length < 4990; i += 1) lines.push(...block.map((l) => l.replace('add_N', `add_${i}`)));
  const seed = lines.length + 2;
  lines.push('pub fn seeded(v: Option<u8>) -> u8 {', '    v.unwrap()', '}', '');
  const src = lines.join('\n');
  const t0 = performance.now();
  const { hits } = scanSource(src, { kind: 'prod' });
  const ms = performance.now() - t0;
  assert.deepEqual(hits.map((h) => `${h.key}:${h.id}:${h.severity}@${h.line}`), [`unwrap:H-02:hard@${seed}`]);
  // Generous bound: far below the 30 s pre-edit hook timeout (hooks/hooks.json)
  // and loose enough that a slow Windows CI runner does not flake.
  assert.ok(ms < 10_000, `scan of ${lines.length} lines took ${ms.toFixed(0)} ms`);
});
