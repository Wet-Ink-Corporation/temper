import { test } from 'node:test';
import assert from 'node:assert/strict';
import { scanSource, newHits, classifyPath } from '../scripts/lib/scan.mjs';
import { maskRust, countTests } from '../scripts/lib/lexer.mjs';

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
