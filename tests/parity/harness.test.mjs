// Pure tests of the parity harness's library (tests/parity/lib/). No temper binary needed:
// engines are stubs (`node -e` scripts) or injected functions. Run in both modes of tests/ci.sh.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { HarnessError } from './lib/error.mjs';
import { firstDifference, hitDiff } from './lib/diff.mjs';
import { hermeticEnv, runEngine, rustEngine } from './lib/engines.mjs';
import { SCAN_FIXTURE, freshCopy, largeFile, snapshotTree, writeTree } from './lib/fixtures.mjs';
import { runInFreshCopy } from './lib/cases.mjs';
import { listRsFiles, overBudget, registryDirs, renderSummary, runCorpus } from './lib/corpus.mjs';
import { UsageError, parseArgs } from './corpus.mjs';
import { dropLoneSurrogateTail, maskRoots, sortHitLines } from './lib/normalise.mjs';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

const hit = (line, key = 'unwrap') => ({ key, id: 'H-02', severity: 'hard', line, text: 'v.unwrap()', file: 'src/lib.rs' });
const json = (hits) => JSON.stringify(hits, null, 2) + '\n';

test('firstDifference: equal buffers, a changed byte, a strict prefix', () => {
  assert.equal(firstDifference(Buffer.from('ab\ncdef'), Buffer.from('ab\ncdef')), null);
  const d = firstDifference(Buffer.from('ab\ncdef'), Buffer.from('ab\ncdXf'));
  assert.deepEqual({ offset: d.offset, line: d.line, a: d.a, b: d.b }, { offset: 5, line: 2, a: 'ab\ncdef', b: 'ab\ncdXf' });
  const p = firstDifference(Buffer.from('abc'), Buffer.from('abcdef'));
  assert.deepEqual({ offset: p.offset, line: p.line }, { offset: 3, line: 1 });
});

test('firstDifference: excerpts are 80 bytes around the offset, decoded lossily', () => {
  const a = Buffer.concat([Buffer.alloc(100, 'x'), Buffer.from([0xff]), Buffer.alloc(100, 'y')]);
  const b = Buffer.concat([Buffer.alloc(100, 'x'), Buffer.from('Z'), Buffer.alloc(100, 'y')]);
  const d = firstDifference(a, b);
  assert.equal(d.offset, 100);
  assert.equal(d.a, `${'x'.repeat(40)}�${'y'.repeat(39)}`);
  assert.equal(d.b, `${'x'.repeat(40)}Z${'y'.repeat(39)}`);
});

test('hitDiff: equal, an extra duplicate, another order, invalid JSON', () => {
  assert.deepEqual(hitDiff(json([hit(1), hit(2)]), json([hit(1), hit(2)])),
    { onlyNode: [], onlyRust: [], onlyNodeTotal: 0, onlyRustTotal: 0, orderOnly: false });
  assert.deepEqual(hitDiff(json([hit(1)]), json([hit(1), hit(1)])),
    { onlyNode: [], onlyRust: [hit(1)], onlyNodeTotal: 0, onlyRustTotal: 1, orderOnly: false });
  assert.deepEqual(hitDiff(json([hit(1), hit(2)]), json([hit(2), hit(1)])),
    { onlyNode: [], onlyRust: [], onlyNodeTotal: 0, onlyRustTotal: 0, orderOnly: true });
  assert.throws(() => hitDiff('[', json([])), HarnessError);
  assert.throws(() => hitDiff(json([]), '{}'), HarnessError);
});

test('hitDiff: key order inside a hit does not matter, and each list is capped at 50', () => {
  const reordered = { file: 'src/lib.rs', text: 'v.unwrap()', line: 1, severity: 'hard', id: 'H-02', key: 'unwrap' };
  assert.equal(hitDiff(json([hit(1)]), JSON.stringify([reordered])).orderOnly, true);
  const many = Array.from({ length: 60 }, (_, i) => hit(i + 1));
  const d = hitDiff(json(many), json([]));
  assert.deepEqual([d.onlyNode.length, d.onlyNodeTotal, d.onlyRust.length], [50, 60, 0]);
});

test('maskRoots: both roots and their realpaths, the longer root first', (t) => {
  const outer = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-mask-'));
  t.after(() => fs.rmSync(outer, { recursive: true, force: true }));
  const inner = path.join(outer, 'inner');
  fs.mkdirSync(inner);
  const link = path.join(outer, 'link');
  fs.symlinkSync(inner, link, 'dir');
  const real = fs.realpathSync.native(inner);
  const text = `a ${inner}/x b ${outer}/y c ${real}/z`;
  assert.equal(maskRoots(text, [outer, inner]), 'a <TMP>/x b <TMP>/y c <TMP>/z');
  assert.equal(maskRoots(`at ${real}/q`, [link]), 'at <TMP>/q');
});

test('maskRoots: on win32 both separator spellings and the JSON-escaped form', () => {
  const root = 'C:\\Users\\ci\\tmp\\temper-parity-1';
  const text = `${root}\\a.rs C:/Users/ci/tmp/temper-parity-1/b.rs "C:\\\\Users\\\\ci\\\\tmp\\\\temper-parity-1\\\\c.rs"`;
  assert.equal(maskRoots(text, [root], 'win32'), '<TMP>\\a.rs <TMP>/b.rs "<TMP>\\\\c.rs"');
  assert.equal(maskRoots(text, [root], 'linux'), text.replace(root, '<TMP>'));
});

test('sortHitLines (D16(l)): case-only path order is normalised; other differences stay', () => {
  const summary = '\n2 blocking, 0 exception request(s), 1 advisory\n';
  const node = `[hard] src/a.rs:2 x\n[hard] src/B.rs:2 y\n[soft] src/c.rs:1 z\n${summary}`;
  const rust = `[hard] src/B.rs:2 y\n[hard] src/a.rs:2 x\n[soft] src/c.rs:1 z\n${summary}`;
  assert.equal(sortHitLines(node), sortHitLines(rust));
  assert.notEqual(sortHitLines(node), sortHitLines(rust.replace('src/c.rs:1', 'src/c.rs:3')));
  assert.ok(sortHitLines(node).endsWith(summary));
  // A hit's indented continuation lines (`    fix: ..`) move with it.
  const fixA = '[hard] src/a.rs:2 x\n    fix: A\n';
  const fixB = '[hard] src/B.rs:2 y\n    fix: B\n';
  assert.equal(sortHitLines(`${fixA}${fixB}${summary}`), sortHitLines(`${fixB}${fixA}${summary}`));
  assert.equal(sortHitLines(`${fixB}${fixA}${summary}`), `${fixB}${fixA}${summary}`);
  // A run ends at a severity change: an ask line never moves above a hard one.
  assert.equal(sortHitLines('[hard] b\n[ask] a\n[hard] a\n'), '[hard] b\n[ask] a\n[hard] a\n');
});

test('dropLoneSurrogateTail (D16(e)): only a lone high surrogate ending a string value', () => {
  assert.deepEqual(dropLoneSurrogateTail('  "text": "ab\\ud83d",\n'), { text: '  "text": "ab",\n', count: 1 });
  assert.deepEqual(dropLoneSurrogateTail('  "text": "ab\\uD83D"\n}'), { text: '  "text": "ab"\n}', count: 1 });
  const paired = '  "text": "ab😀",\n';
  assert.deepEqual(dropLoneSurrogateTail(paired), { text: paired, count: 0 });
  const mid = '  "text": "a\\ud83db",\n';
  assert.deepEqual(dropLoneSurrogateTail(mid), { text: mid, count: 0 });
  const low = '  "text": "ab\\ude00",\n';
  assert.deepEqual(dropLoneSurrogateTail(low), { text: low, count: 0 });
});

test('hermeticEnv: keeps what the engines need, drops TEMPER_*, CLAUDE* and git redirects', () => {
  const env = { PATH: '/bin', HOME: '/h', LANG: 'C.UTF-8', TEMPER_BASE: 'abc', CLAUDE_PLUGIN_ROOT: '/p',
    GIT_DIR: '/g', GIT_WORK_TREE: '/w', RUSTFLAGS: '-D x', SystemRoot: 'C:\\Windows' };
  assert.deepEqual(hermeticEnv(env, 'linux'), { PATH: '/bin', HOME: '/h', LANG: 'C.UTF-8', GIT_CONFIG_NOSYSTEM: '1' });
  const win = { Path: 'C:\\bin', SystemRoot: 'C:\\Windows', PATHEXT: '.EXE', ComSpec: 'cmd.exe', TEMPER_X: '1' };
  assert.deepEqual(hermeticEnv(win, 'win32'),
    { Path: 'C:\\bin', SystemRoot: 'C:\\Windows', PATHEXT: '.EXE', ComSpec: 'cmd.exe', GIT_CONFIG_NOSYSTEM: '1' });
});

const stub = (script) => ({ name: 'stub', cmd: process.execPath, pre: ['-e', script] });

test('runEngine: any exit code is data, with the exact stdout and stderr bytes', () => {
  const r = runEngine(stub('process.stdout.write(Buffer.from([0x61, 0xff])); process.stderr.write("e\\n"); process.exitCode = 3'),
    [], os.tmpdir());
  assert.deepEqual({ exit: r.exit, stdout: [...r.stdout], stderr: r.stderr.toString() }, { exit: 3, stdout: [0x61, 0xff], stderr: 'e\n' });
  assert.equal(typeof r.ms, 'number');
});

test('runEngine: the engine sees only the hermetic environment, plus the git ceiling', () => {
  const r = runEngine(stub('process.stdout.write(JSON.stringify(process.env))'), [], os.tmpdir(), { ceiling: '/c' });
  const seen = JSON.parse(r.stdout.toString());
  assert.equal(seen.GIT_CEILING_DIRECTORIES, '/c');
  assert.equal(seen.GIT_CONFIG_NOSYSTEM, '1');
  assert.equal(Object.keys(seen).some((k) => k.startsWith('TEMPER_') || k.startsWith('CLAUDE')), false);
});

test('runEngine: a missing command, a signal, a timeout and an overflow are harness errors', () => {
  assert.throws(() => runEngine({ name: 'gone', cmd: path.join(os.tmpdir(), 'no-such-engine'), pre: [] }, [], os.tmpdir()),
    HarnessError);
  if (process.platform !== 'win32') {
    assert.throws(() => runEngine(stub('process.kill(process.pid, "SIGKILL")'), [], os.tmpdir()), /signal SIGKILL/);
  }
  assert.throws(() => runEngine(stub('setTimeout(() => {}, 60000)'), [], os.tmpdir(), { timeoutMs: 200 }), HarnessError);
  assert.throws(() => runEngine(stub('process.stdout.write("x".repeat(4096))'), [], os.tmpdir(), { maxBuffer: 1024 }), HarnessError);
});

test('rustEngine: a TEMPER_BIN that is not a file fails with the build hint', () => {
  assert.throws(() => rustEngine(path.join(os.tmpdir(), 'no-such-temper')), /cargo build -p temper-cli/);
  assert.throws(() => rustEngine(os.tmpdir()), HarnessError);
});

// 1.x's construction, rebuilt from the literal block in tests/scan.test.mjs's large-file test.
function largeFileFrom1x() {
  const src = fs.readFileSync(path.join(REPO, 'tests', 'scan.test.mjs'), 'utf8');
  const start = src.indexOf("test('large file:");
  const body = src.slice(src.indexOf('const block = [', start), src.indexOf('];', start));
  const block = [...body.matchAll(/^\s+'(.*)',$/gm)].map((m) => m[1]);
  assert.equal(block.length, 11, 'the 1.x block has 11 lines');
  const lines = [];
  for (let i = 0; lines.length < 4990; i += 1) lines.push(...block.map((l) => l.replace('add_N', `add_${i}`)));
  lines.push('pub fn seeded(v: Option<u8>) -> u8 {', '    v.unwrap()', '}', '');
  return lines.join('\n');
}

test('largeFile: 1.x construction, byte for byte, seeded at line 4996', () => {
  const { src, seedLine } = largeFile();
  assert.equal(seedLine, 4996);
  assert.equal(src.split('\n')[seedLine - 1], '    v.unwrap()');
  assert.equal(src, largeFileFrom1x());
});

test('SCAN_FIXTURE: the 1d.2 tree plus BOM, Latin-1 and long-line files', () => {
  assert.equal(Object.keys(SCAN_FIXTURE).length, 21);
  assert.ok(SCAN_FIXTURE['crates/core/src/bom.rs'].startsWith('﻿'));
  assert.ok(Buffer.isBuffer(SCAN_FIXTURE['crates/core/src/latin1.rs']));
  assert.ok(SCAN_FIXTURE['crates/core/src/long.rs'].split('\n').some((l) => l.length === 170 && l.includes('.unwrap()')));
});

test('writeTree, freshCopy and snapshotTree: a copy has the same snapshot; skipped dirs stay out', (t) => {
  const src = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-tree-'));
  t.after(() => fs.rmSync(src, { recursive: true, force: true }));
  writeTree(src, { 'a/b.rs': 'x\r\n', 'c.bin': Buffer.from([0, 0xff]), 'target/t.rs': 't' });
  const snap = snapshotTree(src);
  assert.deepEqual(snap.map(([p]) => p), ['a/b.rs', 'c.bin', 'target/t.rs']);
  const full = freshCopy(src, { skip: [] });
  const trimmed = freshCopy(src);
  t.after(() => [full, trimmed].forEach((d) => fs.rmSync(d, { recursive: true, force: true })));
  assert.deepEqual(snapshotTree(full), snap);
  assert.deepEqual(snapshotTree(trimmed).map(([p]) => p), ['a/b.rs', 'c.bin']);
  assert.notEqual(full, src);
});

test('runInFreshCopy: an engine that writes into the tree is caught (C14 wiring)', (t) => {
  const src = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-tree-'));
  t.after(() => fs.rmSync(src, { recursive: true, force: true }));
  writeTree(src, { 'Cargo.toml': '', 'sub/x.rs': '' });
  const writer = stub('require("node:fs").writeFileSync("x.tmp", ""); process.stdout.write(process.cwd())');
  const r = runInFreshCopy(writer, src, { args: [], cwd: 'sub' });
  assert.deepEqual(r.wrote, ['sub/x.tmp']);
  assert.equal(r.stdout, '<TMP>/sub'.replaceAll('/', path.sep));
  const quiet = runInFreshCopy(stub('process.exitCode = 1'), src, { args: [] });
  assert.deepEqual({ exit: quiet.exit, wrote: quiet.wrote, stdout: quiet.stdout, stderr: quiet.stderr },
    { exit: 1, wrote: [], stdout: '', stderr: '' });
  const git = runInFreshCopy(stub('process.stdout.write(String(require("node:fs").existsSync(".git")))'), src, { args: [], git: true });
  assert.equal(git.stdout, 'true');
});

test('registryDirs: registry and sparse packages only, deduplicated and sorted', () => {
  const pkg = (name, source, dir = `/reg/${name}`) => ({ name, source, manifest_path: `${dir}/Cargo.toml` });
  const metadata = { packages: [
    pkg('member', null, '/ws/crates/member'),
    pkg('pathdep', null, '/ws/vendor/pathdep'),
    pkg('gitdep', 'git+https://github.com/x/gitdep?rev=1#abc', '/git/checkouts/gitdep'),
    pkg('zeta-1.0.0', 'registry+https://github.com/rust-lang/crates.io-index'),
    pkg('alpha-2.0.0', 'registry+https://github.com/rust-lang/crates.io-index'),
    pkg('alpha-2.0.0', 'registry+https://github.com/rust-lang/crates.io-index'),
    pkg('mid-0.1.0', 'sparse+https://index.crates.io/'),
  ] };
  assert.deepEqual(registryDirs(metadata), ['/reg/alpha-2.0.0', '/reg/mid-0.1.0', '/reg/zeta-1.0.0']);
});

test('overBudget: the bound fails at ms >= budget, the complement of 1.x ms < 10_000', () => {
  const t = [{ file: 'a', ms: 9_999 }, { file: 'b', ms: 10_000 }, { file: 'c', ms: 10_001 }];
  assert.deepEqual(overBudget(t, 10_000).map((e) => e.file), ['b', 'c']);
});

test('listRsFiles: 1.x walk rules (target, .git, node_modules and .temper skipped)', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-walk-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  writeTree(root, { 'src/a.rs': '', 'src/b.txt': '', 'target/x.rs': '', '.git/y.rs': '', 'node_modules/z.rs': '',
    '.temper/w.rs': '', 'deep/er/c.rs': '' });
  assert.deepEqual(listRsFiles(root).map((f) => path.relative(root, f).split(path.sep).join('/')).sort(),
    ['deep/er/c.rs', 'src/a.rs']);
});

// A fake engine table: `out[engine][cwd]` for whole-root runs, `out[engine][cwd + ':' + file]` for one file.
function fakeRun(out, calls = []) {
  return (engine, args, cwd) => {
    calls.push([engine, ...args, cwd]);
    const file = args[1] === '--all' ? '' : `:${args[1]}`;
    const r = out[engine][cwd + file];
    if (!r) throw new Error(`no fake for ${engine} ${args.join(' ')} in ${cwd}`);
    return { exit: r.exit ?? 0, stdout: Buffer.from(r.stdout ?? '[]\n'), stderr: Buffer.from(r.stderr ?? ''), ms: r.ms ?? 1 };
  };
}
const files = () => ['src/lib.rs'];
const roots = [{ dir: '/a', label: 'a' }];
const same = (r) => ({ '1.x': { '/a': r }, Rust: { '/a': r } });

test('runCorpus: equal outputs match; stdout, exit or stderr alone makes a mismatch', () => {
  const out = json([hit(1)]);
  const ok = runCorpus({ roots, run: fakeRun(same({ stdout: out, exit: 1 })), listFiles: files });
  assert.equal(ok.ok, true);
  assert.deepEqual([ok.roots[0].ok, ok.roots[0].hits, ok.roots[0].files, ok.files, ok.hits], [true, 1, 1, 1, 1]);

  const stdout = runCorpus({ roots, listFiles: files,
    run: fakeRun({ '1.x': { '/a': { stdout: out } }, Rust: { '/a': { stdout: json([hit(1), hit(2)]) } } }) });
  assert.equal(stdout.ok, false);
  assert.deepEqual(stdout.roots[0].hitDiff.onlyRust, [hit(2)]);
  assert.equal(stdout.roots[0].stdout.offset > 0, true);

  const exit = runCorpus({ roots, listFiles: files, run: fakeRun({ '1.x': { '/a': { exit: 1 } }, Rust: { '/a': { exit: 0 } } }) });
  assert.deepEqual([exit.ok, exit.roots[0].hitDiff, exit.roots[0].exit], [false, null, { node: 1, rust: 0 }]);

  const stderr = runCorpus({ roots, listFiles: files, run: fakeRun({ '1.x': { '/a': { stderr: 'w\n' } }, Rust: { '/a': {} } }) });
  assert.deepEqual([stderr.ok, stderr.roots[0].stderr.offset], [false, 0]);
});

test('runCorpus: an empty root or a 1.x output that is not JSON is a harness error', () => {
  assert.throws(() => runCorpus({ roots, listFiles: () => [], run: fakeRun(same({})) }), /no \.rs file/);
  assert.throws(() => runCorpus({ roots, listFiles: files, run: fakeRun(same({ stdout: 'oops' })) }), HarnessError);
});

test('runCorpus: a root at or over the bound is timed file by file; one under it is not', () => {
  const two = [{ dir: '/a', label: 'a' }, { dir: '/b', label: 'b' }];
  const calls = [];
  const run = fakeRun({
    '1.x': { '/a': {}, '/b': {} },
    Rust: { '/a': { ms: 12_000 }, '/a:src/x.rs': { ms: 3 }, '/a:src/y.rs': { ms: 10_500 }, '/b': { ms: 9_000 } },
  }, calls);
  const r = runCorpus({ roots: two, run, budgetMs: 10_000,
    listFiles: (d) => (d === '/a' ? ['/a/src/x.rs', '/a/src/y.rs'] : ['/b/src/z.rs']) });
  assert.equal(r.ok, false);
  assert.deepEqual(r.overBudget, [{ label: 'a', file: 'src/y.rs', ms: 10_500 }]);
  assert.deepEqual(calls.filter(([e, , a]) => e === 'Rust' && a !== '--all').map((c) => c.slice(2)),
    [['src/x.rs', '--json', '/a'], ['src/y.rs', '--json', '/a']]);
  assert.deepEqual(r.roots.map((x) => x.drilled), [true, false]);
});

test('runCorpus: the large file must give exactly its seeded hit, whatever 1.x says', () => {
  const large = { dir: '/l', label: 'large file', seedLine: 4996 };
  const at = (line) => json([{ key: 'unwrap', id: 'H-02', severity: 'hard', line }]);
  const both = (stdout) => ({ '1.x': { '/l': { stdout, exit: 1 } }, Rust: { '/l': { stdout, exit: 1 } } });
  const bad = runCorpus({ roots: [], large, run: fakeRun(both(at(4995))), listFiles: () => ['/l/src/lib.rs'] });
  assert.equal(bad.ok, false);
  assert.equal(bad.roots[0].ok, true);
  assert.match(bad.large.message, /^large file: expected unwrap:H-02:hard@4996, got unwrap:H-02:hard@4995$/);
  const good = runCorpus({ roots: [], large, run: fakeRun(both(at(4996))), listFiles: () => ['/l/src/lib.rs'] });
  assert.deepEqual([good.ok, good.large], [true, { ok: true, message: 'large file: unwrap:H-02:hard@4996' }]);
});

test('runCorpus: D16(e) turns a 1.x-only lone-surrogate tail into a match, and counts it', () => {
  const node = '[\n  {\n    "text": "ab\\ud83d"\n  }\n]\n';
  const rust = '[\n  {\n    "text": "ab"\n  }\n]\n';
  const r = runCorpus({ roots, listFiles: files, run: fakeRun({ '1.x': { '/a': { stdout: node } }, Rust: { '/a': { stdout: rust } } }) });
  assert.deepEqual([r.ok, r.normalised, r.roots[0].normalised], [true, 1, 1]);
  const flipped = runCorpus({ roots, listFiles: files, run: fakeRun({ '1.x': { '/a': { stdout: rust } }, Rust: { '/a': { stdout: node } } }) });
  assert.equal(flipped.ok, false, 'the normaliser applies to 1.x only');
});

test('renderSummary: totals, slowest roots, a mismatch, a file over the bound (golden)', () => {
  const result = runCorpus({
    roots: [{ dir: '/a', label: 'a' }, { dir: '/b', label: 'b' }],
    listFiles: (d) => (d === '/a' ? ['/a/src/x.rs', '/a/src/y.rs'] : ['/b/src/z.rs']),
    run: fakeRun({
      '1.x': { '/a': { ms: 3_000 }, '/b': { stdout: json([hit(1)]), exit: 1, ms: 40 } },
      Rust: { '/a': { ms: 12_000 }, '/a:src/x.rs': { ms: 3 }, '/a:src/y.rs': { ms: 10_500 },
        '/b': { stdout: json([hit(2)]), exit: 1, ms: 9.6 } },
    }),
  });
  assert.equal(renderSummary(result), `## Scanner corpus

2 roots, 3 files, 1 hit. 1.x 3.0 s, Rust 12.0 s. D16(e) normalised: 0.

**Result: 1 mismatch, 1 file at or over the 10000 ms bound.**

### Slowest Rust roots

| Root | Files | Rust ms | 1.x ms |
|---|---:|---:|---:|
| a | 2 | 12000 | 3000 |
| b | 1 | 10 | 40 |

### Mismatches

#### b

- exit: 1.x 1, Rust 1
- stdout: first difference at byte 81, line 6
  - 1.x: \`"2\\",\\n    \\"severity\\": \\"hard\\",\\n    \\"line\\": 1,\\n    \\"text\\": \\"v.unwrap()\\",\\n    \\"file\\":"\`
  - Rust: \`"2\\",\\n    \\"severity\\": \\"hard\\",\\n    \\"line\\": 2,\\n    \\"text\\": \\"v.unwrap()\\",\\n    \\"file\\":"\`
- hits only in 1.x: 1, only in Rust: 1
  - 1.x: \`{"file":"src/lib.rs","id":"H-02","key":"unwrap","line":1,"severity":"hard","text":"v.unwrap()"}\`
  - Rust: \`{"file":"src/lib.rs","id":"H-02","key":"unwrap","line":2,"severity":"hard","text":"v.unwrap()"}\`

### Files at or over the bound

| Root | File | Rust ms |
|---|---|---:|
| a | src/y.rs | 10500 |
`);
});

test('corpus CLI parseArgs: --bin is required, repeats keep their order, the budget is a positive integer', () => {
  assert.throws(() => parseArgs([]), UsageError);
  assert.equal(new UsageError('x').exitCode, 2);
  const a = parseArgs(['--bin', 't', '--lock', 'A', '--root', 'r1', '--lock', 'B', '--root', 'r2']);
  assert.deepEqual(a, { bin: 't', nodeCli: path.join(REPO, 'scripts', 'temper.mjs'), locks: ['A', 'B'], roots: ['r1', 'r2'],
    budgetMs: 10_000, report: null });
  assert.equal(parseArgs(['--bin', 't', '--budget-ms', '250', '--report', 'out']).budgetMs, 250);
  for (const bad of ['0', '-1', 'ten', '1.5', '']) assert.throws(() => parseArgs(['--bin', 't', '--budget-ms', bad]), UsageError, bad);
  assert.throws(() => parseArgs(['--bin']), UsageError);
  assert.throws(() => parseArgs(['--bin', 't', '--bogus']), UsageError);
});

test('corpus CLI: a usage error exits 2 with corpus: on stderr', () => {
  const r = spawnSync(process.execPath, [path.join(REPO, 'tests', 'parity', 'corpus.mjs')], { encoding: 'utf8' });
  assert.equal(r.status, 2);
  assert.match(r.stderr, /^corpus: --bin is required/);
});

// D1: the corpus job's skill blocks. PATH is emptied, so any cargo (or other) process would fail.
test('doc_examples.py --extract writes the block modules without cargo; it rejects --fetch', (t) => {
  const py = ['python3', 'python'].map((c) => spawnSync(c, ['-c', 'import sys; print(sys.executable)'], { encoding: 'utf8' }))
    .find((r) => r.status === 0)?.stdout.trim();
  assert.ok(py, 'python 3.11+ is needed (python3 or python on PATH)');
  const out = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-blocks-'));
  t.after(() => fs.rmSync(out, { recursive: true, force: true }));
  const script = path.join(REPO, 'tests', 'doc_examples.py');
  const r = spawnSync(py, ['-I', script, '--extract', out], { encoding: 'utf8', env: { PATH: '' } });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, /^doc_examples: extracted \d+ blocks to /m);
  const mods = (c) => fs.readdirSync(path.join(out, 'crates', c, 'src')).filter((f) => f.endsWith('.rs') && f !== 'lib.rs');
  assert.ok(mods('doc-examples').length > 50);
  assert.ok(mods('dont-examples').length > 10);
  assert.ok(fs.existsSync(path.join(out, 'Cargo.toml')));
  for (const extra of ['--fetch', '--keep']) {
    assert.equal(spawnSync(py, ['-I', script, '--extract', out, extra], { encoding: 'utf8' }).status, 2, extra);
  }
});
