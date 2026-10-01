// The lint baseline and the ratchet: diff parsing, changed lines from real git,
// the ratchet rules, tighten, the committed file format, and the gate in
// ratchet mode.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import {
  BASELINE_PATH, readBaseline, writeBaseline, renderBaseline, baselineFrom, tighten, parseDiff, changedLines, ratchet,
  renderRatchet, debtLine, diffTotals,
} from '../scripts/lib/baseline.mjs';
import { discover, fingerprint, BASELINE_FILE, safeOutputPath, relTo } from '../scripts/lib/project.mjs';
import { runGate, renderGate } from '../scripts/lib/gate.mjs';
import { collectFindings } from '../scripts/lib/findings.mjs';
import { canSymlink } from './fixtures/can-symlink.mjs';

const hasGit = spawnSync('git', ['--version']).status === 0;
const hasCargo = spawnSync('cargo', ['--version']).status === 0;

const F = (crate, lint, file, line, extra = {}) => ({
  tool: lint.startsWith('scan:') ? 'scan' : lint.startsWith('rustdoc::') ? 'rustdoc' : 'clippy',
  lint, crate, file, line, end_line: line, message: `${lint} here`, rules: [], severity: 'unmapped', ...extra,
});
const collection = (findings, extra = {}) => ({
  temper_findings: 1, at: '2026-01-01T00:00:00.000Z', commit: 'c0ffee', toolchain: '1.98.1',
  sources: ['clippy', 'rustdoc', 'scan'], crates: [], findings, errors: [], ...extra,
});
const lines = (obj) => new Map(Object.entries(obj).map(([f, v]) => [f, v === 'all' ? 'all' : new Set(v)]));

const LEGACY = [
  F('dom', 'clippy::unwrap_used', 'crates/dom/src/lib.rs', 10),
  F('dom', 'clippy::unwrap_used', 'crates/dom/src/lib.rs', 20),
  F('dom', 'clippy::indexing_slicing', 'crates/dom/src/lib.rs', 30),
  F('dom', 'missing_docs', 'crates/dom/src/lib.rs', 40),
  F('app', 'clippy::as_conversions', 'crates/app/src/main.rs', 5),
  F('app', 'rustdoc::broken_intra_doc_links', 'crates/app/src/main.rs', 1),
];
const BASE = baselineFrom(collection(LEGACY));

// ------------------------------------------------------------ diff parsing

test('parseDiff: added and changed lines count, deletions and pure renames add nothing', () => {
  const diff = [
    'diff --git a/src/a.rs b/src/a.rs',
    'index 1..2 100644',
    '--- a/src/a.rs',
    '+++ b/src/a.rs',
    '@@ -3 +3 @@ fn a() {',
    '-    old',
    '+    new',
    '@@ -10,0 +11,2 @@',
    '+added',
    '+added',
    '@@ -20,3 +21,0 @@',
    '-gone',
    '-gone',
    '-gone',
    '\\ No newline at end of file',
    'diff --git a/src/old.rs b/src/new.rs',
    'similarity index 100%',
    'rename from src/old.rs',
    'rename to src/new.rs',
    'diff --git a/src/dead.rs b/src/dead.rs',
    'deleted file mode 100644',
    '--- a/src/dead.rs',
    '+++ /dev/null',
    '@@ -1,2 +0,0 @@',
    '-x',
    '-y',
    'diff --git "a/src/sp\\303\\251c ial.rs" "b/src/sp\\303\\251c ial.rs"',
    '--- "a/src/sp\\303\\251c ial.rs"',
    '+++ "b/src/sp\\303\\251c ial.rs"\t',
    '@@ -1 +1 @@',
    '-a',
    '+b',
    'diff --git a/src/ops.rs b/src/ops.rs',
    '--- a/src/ops.rs',
    '+++ b/src/ops.rs',
    '@@ -4,2 +4,2 @@',
    '--- old comment body',
    '-x',
    '+++ not a header',
    '+y',
  ].join('\r\n');
  const got = parseDiff(diff);
  assert.deepEqual([...got.keys()], ['src/a.rs', 'src/spéc ial.rs', 'src/ops.rs']);
  assert.deepEqual([...got.get('src/ops.rs')], [4, 5], 'hunk bodies that look like headers are body lines');
  assert.deepEqual([...got.get('src/a.rs')].sort((a, b) => a - b), [3, 11, 12]);
  assert.deepEqual([...got.get('src/spéc ial.rs')], [1]);
});

function repo() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-baseline-'));
  const g = (...args) => {
    const r = spawnSync('git', args, { cwd: dir, encoding: 'utf8' });
    assert.equal(r.status, 0, `git ${args.join(' ')}: ${r.stderr}`);
    return r.stdout.trim();
  };
  const put = (rel, text) => { const p = path.join(dir, ...rel.split('/')); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, text); };
  g('init', '-q');
  g('config', 'user.email', 't@example.com');
  g('config', 'user.name', 't');
  g('config', 'core.autocrlf', 'false');
  put('Cargo.toml', '[workspace]\nmembers = ["crates/*"]\n');
  return { dir, g, put, commit: (m) => { g('add', '-A'); g('commit', '-qm', m); return g('rev-parse', 'HEAD'); } };
}

const numbered = (n, f = (i) => `line ${i}`) => Array.from({ length: n }, (_, i) => f(i + 1)).join('\n') + '\n';

test('changedLines against git: edits, insertions, deletions, renames, untracked files, merge base', { skip: !hasGit && 'git not installed' }, () => {
  const r = repo();
  try {
    r.put('crates/dom/src/lib.rs', numbered(40));
    r.put('crates/dom/src/moved.rs', numbered(30, (i) => `moved ${i}`));
    r.put('crates/dom/src/edited_move.rs', numbered(30, (i) => `em ${i}`));
    const base = r.commit('base');
    const proj = discover(r.dir);
    assert.equal(changedLines(proj, null), null, 'no base: totals only');
    assert.equal(changedLines({ ...proj, gitRoot: null }, base), null, 'no git: totals only');
    assert.throws(() => changedLines(proj, 'no-such-ref'), /not a commit/);
    // edit line 5, insert after 10, delete 30-32
    const src = fs.readFileSync(path.join(r.dir, 'crates/dom/src/lib.rs'), 'utf8').split('\n');
    src[4] = 'line 5 changed';
    src.splice(10, 0, 'inserted a', 'inserted b');
    src.splice(31, 3);
    r.put('crates/dom/src/lib.rs', src.join('\n'));
    r.g('mv', 'crates/dom/src/moved.rs', 'crates/dom/src/renamed.rs');
    r.g('mv', 'crates/dom/src/edited_move.rs', 'crates/dom/src/edited_renamed.rs');
    const em = fs.readFileSync(path.join(r.dir, 'crates/dom/src/edited_renamed.rs'), 'utf8').replace('em 7\n', 'em seven\n');
    r.put('crates/dom/src/edited_renamed.rs', em);
    r.put('crates/dom/src/new.rs', 'fn fresh() {}\n');
    const got = changedLines(proj, base);
    assert.deepEqual([...got.get('crates/dom/src/lib.rs')].sort((a, b) => a - b), [5, 11, 12]);
    assert.ok(!got.has('crates/dom/src/renamed.rs') || got.get('crates/dom/src/renamed.rs').size === 0, 'a pure rename touches no line');
    assert.deepEqual([...got.get('crates/dom/src/edited_renamed.rs')], [7]);
    assert.deepEqual([...got.get('crates/dom/src/new.rs')], [1], 'an untracked file: every line is new');
    // a commit on top of the base: the merge base is still the base
    r.commit('work');
    r.put('crates/dom/src/lib.rs', `${fs.readFileSync(path.join(r.dir, 'crates/dom/src/lib.rs'), 'utf8')}tail\n`);
    const later = changedLines(proj, base);
    assert.ok(later.get('crates/dom/src/lib.rs').has(5), 'committed changes since the base still count');
    assert.equal(later.get('crates/dom/src/new.rs').size, 1, 'a file committed since the base is diffed, not untracked');
  } finally { fs.rmSync(r.dir, { recursive: true, force: true }); }
});

test('changedLines ignores user diff config (mnemonicPrefix, noprefix, interHunkContext) and sees unstaged renames', { skip: !hasGit && 'git not installed' }, () => {
  const r = repo();
  try {
    r.put('.gitignore', 'target/\n.temper/\n');
    r.put('crates/dom/src/lib.rs', numbered(40));
    r.put('crates/dom/src/moved.rs', numbered(30, (i) => `moved ${i}`));
    r.put('crates/app/src/main.rs', numbered(20, (i) => `app ${i}`));
    const base = r.commit('base');
    for (const [k, v] of [['diff.mnemonicPrefix', 'true'], ['diff.interHunkContext', '8'], ['diff.srcPrefix', 'x/'], ['diff.dstPrefix', 'y/'], ['diff.renames', 'false'], ['color.diff', 'always']]) r.g('config', k, v);
    const proj = discover(r.dir);
    const src = fs.readFileSync(path.join(r.dir, 'crates/dom/src/lib.rs'), 'utf8').split('\n');
    src[4] = 'line 5 changed';
    src[9] = 'line 10 changed'; // five lines apart: interHunkContext=8 would merge the hunks and count 6-9
    r.put('crates/dom/src/lib.rs', src.join('\n'));
    // an unstaged move (plain mv, nothing added to the index) and an unstaged crate-directory move
    fs.renameSync(path.join(r.dir, 'crates/dom/src/moved.rs'), path.join(r.dir, 'crates/dom/src/renamed.rs'));
    fs.renameSync(path.join(r.dir, 'crates/app'), path.join(r.dir, 'crates/app2'));
    fs.mkdirSync(path.join(r.dir, 'target'), { recursive: true });
    fs.writeFileSync(path.join(r.dir, 'target', 'junk.rs'), 'fn x() {}\n');
    const got = changedLines(proj, base);
    assert.deepEqual([...got.get('crates/dom/src/lib.rs')].sort((a, b) => a - b), [5, 10]);
    assert.ok(!got.get('crates/dom/src/renamed.rs')?.size, 'an unstaged rename touches no line');
    assert.ok(!got.get('crates/app2/src/main.rs')?.size, 'an unstaged directory move touches no line');
    assert.ok(![...got.keys()].some((f) => f.startsWith('target/')), 'ignored files are not diffed');
    assert.equal(r.g('status', '--porcelain', '--', 'crates/dom/src/renamed.rs'), '?? crates/dom/src/renamed.rs', 'the real index is untouched');
  } finally { fs.rmSync(r.dir, { recursive: true, force: true }); }
});

test('ratchet with a base compares the baseline at the merge base: a raise is flagged, not failed', { skip: !hasGit && 'git not installed' }, () => {
  const r = repo();
  try {
    r.put('crates/dom/src/lib.rs', numbered(40));
    const crates = [{ name: 'dom', dir: 'crates/dom' }, { name: 'app', dir: 'crates/app' }];
    const first = r.commit('code');
    const proj = discover(r.dir);
    // the baseline is new since the base: every count is a raise from zero
    writeBaseline(r.dir, BASE);
    const intro = ratchet(collection(LEGACY, { crates }), BASE, { proj, base: first });
    assert.equal(intro.ok, true);
    assert.equal(intro.raised.length, 5, 'one per (crate, lint)');
    assert.match(intro.notes.join('\n'), /the baseline is new since/);
    const base = r.commit('baseline');
    const raisedBaseline = { ...BASE, totals: { ...BASE.totals, dom: { ...BASE.totals.dom, 'clippy::unwrap_used': 3 } } };
    writeBaseline(r.dir, raisedBaseline);
    const more = [...LEGACY, F('dom', 'clippy::unwrap_used', 'crates/dom/src/lib.rs', 25)];
    const res = ratchet(collection(more, { crates }), raisedBaseline, { proj, base });
    assert.deepEqual(res.raised, [{ crate: 'dom', lint: 'clippy::unwrap_used', from: 2, to: 3 }]);
    assert.equal(res.ok, true, 'a human-approved raise still merges');
    assert.match(renderRatchet(res), /BASELINE RAISED since [0-9a-f]{12}: 1 count\(s\) higher[\s\S]*dom clippy::unwrap_used: 2 -> 3/);
    const same = ratchet(collection(LEGACY, { crates }), BASE, { proj, base: 'HEAD' });
    assert.deepEqual(same.raised, []);
  } finally { fs.rmSync(r.dir, { recursive: true, force: true }); }
});

test('a crate whose directory and name both changed keeps its counts (staged, unstaged or committed)', { skip: !hasGit && 'git not installed' }, () => {
  const r = repo();
  try {
    r.put('crates/dom/Cargo.toml', '[package]\nname = "dom"\n');
    r.put('crates/dom/src/lib.rs', numbered(40));
    const base = r.commit('code');
    const proj = discover(r.dir);
    const b = baselineFrom(collection(LEGACY, { commit: base, crates: [{ name: 'dom', dir: 'crates/dom' }, { name: 'app', dir: 'crates/app' }] }));
    const moved = LEGACY.map((f) => (f.crate === 'dom' ? { ...f, crate: 'dom-core', file: f.file.replace('crates/dom/', 'crates/dom-core/') } : f));
    const now = collection(moved, { crates: [{ name: 'dom-core', dir: 'crates/dom-core' }, { name: 'app', dir: 'crates/app' }] });
    // unstaged: plain mv plus the new name
    fs.renameSync(path.join(r.dir, 'crates/dom'), path.join(r.dir, 'crates/dom-core'));
    r.put('crates/dom-core/Cargo.toml', '[package]\nname = "dom-core"\n');
    const res = ratchet(now, b, { proj, base });
    assert.equal(res.ok, true, renderRatchet(res));
    assert.match(res.notes.join('\n'), /crate `dom` is now `dom-core` \(moved\)/);
    // committed, with the base after the rename (a later PR): renames since the baseline's commit
    const later = r.commit('rename');
    const res2 = ratchet(now, b, { proj, base: later });
    assert.equal(res2.ok, true, renderRatchet(res2));
    // no git history to follow: the counts do not carry over
    assert.equal(ratchet(now, { ...b, commit: null }, { changed: null }).ok, false);
    // tighten follows the move too
    const fewer = { ...now, findings: moved.filter((f) => f.line !== 10) };
    assert.equal(tighten(b, fewer, { proj }).totals.dom['clippy::unwrap_used'], 1);
  } finally { fs.rmSync(r.dir, { recursive: true, force: true }); }
});

test('the temporary index writes no untracked file content into .git/objects', { skip: !hasGit && 'git not installed' }, () => {
  const r = repo();
  try {
    r.put('crates/dom/src/lib.rs', numbered(10));
    const base = r.commit('code');
    const objects = () => spawnSync('git', ['count-objects', '-v'], { cwd: r.dir, encoding: 'utf8' }).stdout.match(/^size: (\d+)/m)[1] * 1 + spawnSync('git', ['count-objects', '-v'], { cwd: r.dir, encoding: 'utf8' }).stdout.match(/^size-pack: (\d+)/m)[1] * 1;
    const before = objects();
    fs.writeFileSync(path.join(r.dir, 'data.csv'), 'x,y\n'.repeat(500000)); // 2 MB of untracked data
    r.put('crates/dom/src/new.rs', numbered(5));
    const got = changedLines(discover(r.dir), base);
    assert.deepEqual([...got.get('crates/dom/src/new.rs')], [1, 2, 3, 4, 5]);
    assert.ok(!got.has('data.csv'), 'only Rust sources and manifests are diffed');
    assert.ok(objects() - before < 64, `.git/objects grew by ${objects() - before} KB`);
  } finally { fs.rmSync(r.dir, { recursive: true, force: true }); }
});

test('relTo: one directory spelled with an 8.3 short name and a long name is the same root', (t) => {
  if (process.platform !== 'win32') { t.skip('Windows only'); return; }
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-long-directory-name-'));
  try {
    fs.mkdirSync(path.join(root, 'src'));
    fs.writeFileSync(path.join(root, 'src', 'lib.rs'), '');
    const short = spawnSync('cmd', ['/d', '/c', `for %I in ("${root}") do @echo %~sI`], { encoding: 'utf8', windowsVerbatimArguments: true }).stdout.trim();
    if (!short || short.toLowerCase() === root.toLowerCase()) { t.skip('8.3 names are off on this volume'); return; }
    assert.equal(relTo(root, path.join(short, 'src', 'lib.rs')), 'src/lib.rs');
    assert.equal(relTo(short, path.join(root, 'src', 'lib.rs')), 'src/lib.rs');
    assert.equal(relTo(root, path.join(short, 'src', 'new.rs')), 'src/new.rs', 'a file not yet on disk');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('output paths: under target/ or .temper/ after resolving symlinks, never onto one', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-safe-'));
  try {
    fs.mkdirSync(path.join(root, 'crates', 'a', 'src'), { recursive: true });
    fs.mkdirSync(path.join(root, '.github'));
    const p = (rel) => path.join(root, rel);
    assert.equal(safeOutputPath(root, p('target/temper/f.json')), p('target/temper/f.json'), 'target/ need not exist yet');
    assert.equal(safeOutputPath(root, p('.temper/f.json')), p('.temper/f.json'));
    assert.equal(safeOutputPath(root, p('target')), null, 'a file must be below target/');
    assert.equal(safeOutputPath(root, p('target'), { dir: true }), p('target'));
    if (!canSymlink) return;
    fs.mkdirSync(p('target'));
    fs.symlinkSync(p('crates/a/src'), p('target/src'));
    fs.symlinkSync(p('.github'), p('target/gh'));
    fs.symlinkSync(p('crates/a/src/lib.rs'), p('target/f.json'));
    for (const bad of ['target/src/lib.rs', 'target/gh/workflows/x.yml', 'target/src/deep/new/f.json', 'target/f.json', 'crates/a/src/lib.rs', 'target/../x.json']) {
      assert.equal(safeOutputPath(root, p(bad)), null, bad);
    }
    assert.equal(safeOutputPath(root, p('target/src'), { dir: true, files: ['report.json'] }), null, 'a symlinked report dir');
    // target/ itself may be a symlink (a build dir on another disk)
    const elsewhere = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-safe-target-'));
    try {
      fs.rmSync(p('target'), { recursive: true, force: true });
      fs.symlinkSync(elsewhere, p('target'));
      assert.equal(safeOutputPath(root, p('target/temper/f.json')), p('target/temper/f.json'));
    } finally { fs.rmSync(elsewhere, { recursive: true, force: true }); }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

// ------------------------------------------------------------ ratchet

test('ratchet: legacy debt passes; a finding on a new line fails', () => {
  const same = ratchet(collection(LEGACY), BASE, { changed: lines({}) });
  assert.equal(same.ok, true);
  assert.equal(same.legacy_total, 6);
  assert.equal(same.baseline_total, 6);
  assert.deepEqual([same.new_code, same.increases, same.fixed], [[], [], []]);
  assert.equal(debtLine(same), 'legacy: 6 finding(s) (baseline 6)');
  const added = F('dom', 'clippy::unwrap_used', 'crates/dom/src/lib.rs', 50);
  const res = ratchet(collection([...LEGACY, added]), BASE, { changed: lines({ 'crates/dom/src/lib.rs': [50] }), base: 'HEAD' });
  assert.equal(res.ok, false);
  assert.deepEqual(res.new_code, [added]);
  assert.deepEqual(res.increases, [{ crate: 'dom', lint: 'clippy::unwrap_used', baseline: 2, current: 3 }]);
  assert.match(renderRatchet(res), /crates\/dom\/src\/lib\.rs:50 clippy::unwrap_used/);
  assert.doesNotMatch(renderRatchet(res), /above the baseline/, 'an increase the new finding explains is not repeated');
});

test('ratchet: a swap in the same crate (fix one, add one) fails although the count is unchanged', () => {
  const swapped = [...LEGACY.filter((f) => f.line !== 20), F('dom', 'clippy::unwrap_used', 'crates/dom/src/lib.rs', 21)];
  const res = ratchet(collection(swapped), BASE, { changed: lines({ 'crates/dom/src/lib.rs': [21] }) });
  assert.equal(res.ok, false);
  assert.deepEqual(res.increases, []);
  assert.equal(res.new_code.length, 1);
  // without a base only the totals are compared, and a swap passes
  const noBase = ratchet(collection(swapped), BASE, { changed: null });
  assert.equal(noBase.ok, true);
  assert.equal(noBase.mode, 'totals');
  assert.match(renderRatchet(noBase), /only the totals were compared/);
});

test('ratchet: a legacy finding on a touched line fails (CRF-08); long spans count by their first line', () => {
  const touched = ratchet(collection(LEGACY), BASE, { changed: lines({ 'crates/dom/src/lib.rs': [30] }) });
  assert.equal(touched.ok, false);
  assert.equal(touched.new_code[0].lint, 'clippy::indexing_slicing');
  const shortSpan = [...LEGACY.slice(0, 3), { ...LEGACY[3], end_line: 42 }, ...LEGACY.slice(4)];
  assert.equal(ratchet(collection(shortSpan), BASE, { changed: lines({ 'crates/dom/src/lib.rs': [42] }) }).ok, false);
  const crateDocs = [...LEGACY.slice(0, 3), { ...LEGACY[3], line: 1, end_line: 400 }, ...LEGACY.slice(4)];
  assert.equal(ratchet(collection(crateDocs), baselineFrom(collection(crateDocs)), { changed: lines({ 'crates/dom/src/lib.rs': [200] }) }).ok, true);
});

test('ratchet: a rename with an unchanged count passes', () => {
  const moved = LEGACY.map((f) => (f.crate === 'app' ? { ...f, file: 'crates/app/src/cli.rs' } : f));
  const res = ratchet(collection(moved), BASE, { changed: lines({ 'crates/app/src/cli.rs': [] }) });
  assert.equal(res.ok, true, renderRatchet(res));
});

test('ratchet: a count increase on an untouched line fails (non-local effect)', () => {
  const more = [...LEGACY, F('app', 'clippy::as_conversions', 'crates/app/src/main.rs', 9)];
  const res = ratchet(collection(more), BASE, { changed: lines({ 'crates/app/src/other.rs': [1] }) });
  assert.equal(res.ok, false);
  assert.deepEqual(res.new_code, []);
  assert.deepEqual(res.increases, [{ crate: 'app', lint: 'clippy::as_conversions', baseline: 1, current: 2 }]);
  assert.match(renderRatchet(res), /1 count\(s\) above the baseline on lines this change did not touch[\s\S]*app clippy::as_conversions: baseline 1, now 2/);
  // a lint the baseline never had in that crate is an increase from zero
  const other = ratchet(collection([...LEGACY, F('dom', 'clippy::dbg_macro', 'crates/dom/src/lib.rs', 3)]), BASE, { changed: null });
  assert.deepEqual(other.increases, [{ crate: 'dom', lint: 'clippy::dbg_macro', baseline: 0, current: 1 }]);
});

test('ratchet: fixes are reported, compile errors fail, exceptions and uncollected sources do not count', () => {
  const fewer = LEGACY.filter((f) => f.line !== 10);
  const res = ratchet(collection(fewer), BASE, { changed: lines({ 'crates/dom/src/lib.rs': [10] }) });
  assert.equal(res.ok, true);
  assert.deepEqual(res.fixed, [{ crate: 'dom', lint: 'clippy::unwrap_used', baseline: 2, current: 1 }]);
  assert.equal(res.fixed_count, 1);
  assert.equal(debtLine(res), 'legacy: 5 finding(s) (baseline 6); 1 fixed since the baseline: run temper baseline --tighten');
  const broken = ratchet(collection(LEGACY, { errors: [{ tool: 'clippy', code: 'E0308', file: 'crates/dom/src/lib.rs', line: 3, message: 'mismatched types' }] }), BASE, { changed: null });
  assert.equal(broken.ok, false);
  assert.match(renderRatchet(broken), /1 compile error\(s\)[\s\S]*crates\/dom\/src\/lib\.rs:3 E0308 mismatched types/);
  const exception = F('dom', 'scan:unwrap', 'crates/dom/src/lib.rs', 60, { exception: 'clippy::unwrap_used' });
  assert.equal(ratchet(collection([...LEGACY, exception]), BASE, { changed: lines({ 'crates/dom/src/lib.rs': [60] }) }).ok, true, 'an approved #[expect] is not debt');
  const noDoc = ratchet(collection(LEGACY.filter((f) => f.tool !== 'rustdoc'), { sources: ['clippy', 'scan'] }), BASE, { changed: null });
  assert.deepEqual(noDoc.fixed, [], 'rustdoc lints are not "fixed" when rustdoc did not run');
  assert.equal(noDoc.baseline_total, 5);
  const onlyClippy = ratchet(collection(LEGACY), BASE, { changed: null, sources: ['clippy'] });
  assert.equal(onlyClippy.baseline_total, 5);
});

test('ratchet: a renamed crate (same directory) keeps its baseline counts; --tighten is suggested only on a pass', () => {
  const crates = [{ name: 'dom', dir: 'crates/dom' }, { name: 'app', dir: 'crates/app' }];
  const base = baselineFrom(collection(LEGACY, { crates }));
  const renamed = LEGACY.map((f) => (f.crate === 'dom' ? { ...f, crate: 'dom-core' } : f));
  const res = ratchet(collection(renamed, { crates: [{ name: 'dom-core', dir: 'crates/dom' }, crates[1]] }), base, { changed: lines({}) });
  assert.equal(res.ok, true, renderRatchet(res));
  assert.match(res.notes.join('\n'), /crate `dom` is now `dom-core` \(same directory\)/);
  // a new crate in a new directory is not a rename
  const moved = ratchet(collection(renamed, { crates: [{ name: 'dom-core', dir: 'crates/core' }, crates[1]] }), base, { changed: lines({}) });
  assert.equal(moved.ok, false);
  // tighten follows the rename too
  const fewer = renamed.filter((f) => f.line !== 10);
  const next = tighten(base, collection(fewer, { crates: [{ name: 'dom-core', dir: 'crates/dom' }, crates[1]] }));
  assert.equal(next.totals.dom['clippy::unwrap_used'], 1);
  // fixed plus an increase: the debt line does not suggest --tighten (it cannot help)
  const mixed = ratchet(collection([...LEGACY.filter((f) => f.line !== 10), F('app', 'clippy::as_conversions', 'crates/app/src/main.rs', 9)], { crates }), base, { changed: null });
  assert.equal(mixed.ok, false);
  assert.equal(mixed.fixed_count, 1);
  assert.doesNotMatch(renderRatchet(mixed), /--tighten/);
});

test('ratchet without a baseline is strict: every finding is new', () => {
  const res = ratchet(collection(LEGACY), null, { changed: lines({}) });
  assert.equal(res.ok, false);
  assert.equal(res.mode, 'strict');
  assert.equal(res.new_code.length, LEGACY.length);
  assert.deepEqual([res.increases, res.fixed, res.raised, res.legacy_total, res.baseline_total], [[], [], [], 0, 0]);
  assert.match(renderRatchet(res), /6 finding\(s\); without a baseline every warning fails:[\s\S]*no baseline: zero findings required \(6 found\)/);
  const clean = ratchet(collection([F('dom', 'scan:expect-attr', 'a.rs', 1, { exception: 'clippy::unwrap_used' })]), null, {});
  assert.equal(clean.ok, true, 'approved exceptions are not findings');
});

test('ratchet: a forbid override (E0453 stand-in) always fails and is never baselined', () => {
  const override = F('dom', 'scan:forbid-override', 'crates/dom/src/lib.rs', 2);
  const b = baselineFrom(collection([...LEGACY, override]));
  assert.equal(b.totals.dom['scan:forbid-override'], undefined);
  const res = ratchet(collection([...LEGACY, override], { sources: ['clippy', 'rustdoc'] }), b, { changed: null });
  assert.equal(res.ok, false);
  assert.deepEqual(res.new_code, [override]);
});

test('ratchet: sources scope findings by the tool that reported them', () => {
  // a missing_docs only rustdoc reports (a #[cfg(doc)] item) counts when rustdoc is in scope
  const docOnly = F('dom', 'missing_docs', 'crates/dom/src/lib.rs', 50, { tool: 'rustdoc' });
  const all = ratchet(collection([...LEGACY, docOnly]), BASE, { changed: null });
  assert.deepEqual(all.increases, [{ crate: 'dom', lint: 'missing_docs', baseline: 1, current: 2 }]);
  const clippyOnly = ratchet(collection([...LEGACY, docOnly]), BASE, { changed: null, sources: ['clippy', 'scan'] });
  assert.deepEqual(clippyOnly.increases, []);
});

test('ratchet: at most 30 new-code findings are listed', () => {
  const many = Array.from({ length: 35 }, (_, i) => F('dom', 'clippy::unwrap_used', 'crates/dom/src/new.rs', i + 1));
  const text = renderRatchet(ratchet(collection([...LEGACY, ...many]), BASE, { changed: lines({ 'crates/dom/src/new.rs': 'all' }) }));
  assert.equal(text.split('\n').filter((l) => l.includes('crates/dom/src/new.rs:')).length, 30);
  assert.match(text, /\.\.\. 5 more/);
});

test('ratchet: a baseline from an older toolchain says why counts rose', () => {
  const res = ratchet(collection(LEGACY, { toolchain: '1.99.0' }), BASE, { changed: null });
  assert.match(res.notes.join('\n'), /recorded on Rust 1\.98\.1 and this run used 1\.99\.0/);
});

// ------------------------------------------------------------ tighten and the file

test('tighten never raises a count or adds an entry, and drops zeros', () => {
  const now = [
    ...LEGACY.filter((f) => f.lint !== 'clippy::indexing_slicing' && f.line !== 10), // one unwrap fixed, indexing gone
    F('app', 'clippy::as_conversions', 'crates/app/src/main.rs', 9), // raised: stays at the baseline
    F('dom', 'clippy::dbg_macro', 'crates/dom/src/lib.rs', 3), // new: not added
  ];
  const next = tighten(BASE, collection(now));
  assert.deepEqual(next.totals, {
    app: { 'clippy::as_conversions': 1, 'rustdoc::broken_intra_doc_links': 1 },
    dom: { 'clippy::unwrap_used': 1, missing_docs: 1 },
  });
  assert.equal(next.files, undefined, 'the per-file table is retired');
  assert.deepEqual(diffTotals(BASE, next), [
    { crate: 'dom', lint: 'clippy::indexing_slicing', from: 1, to: 0 },
    { crate: 'dom', lint: 'clippy::unwrap_used', from: 2, to: 1 },
  ]);
  for (const [c, lints] of Object.entries(next.totals)) for (const [l, n] of Object.entries(lints)) assert.ok(n <= (BASE.totals[c]?.[l] ?? 0));
  assert.equal(next.created, BASE.created);
  // a source that did not run keeps its counts
  const noDoc = tighten(BASE, collection([], { sources: ['clippy', 'scan'] }));
  assert.deepEqual(noDoc.totals, { app: { 'rustdoc::broken_intra_doc_links': 1 } });
  assert.throws(() => tighten(BASE, collection([], { errors: [{ message: 'E0308' }] })), /does not build/);
});

test('the baseline file: sorted keys, 2-space JSON, trailing newline, no zeros; read back', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-bl-'));
  try {
    assert.equal(readBaseline(dir), null);
    const b = baselineFrom(collection([...LEGACY].reverse(), { commit: 'abc', host: 'x86_64-unknown-linux-gnu', crates: [{ name: 'dom', dir: 'crates/dom' }, { name: 'app', dir: 'crates/app' }] }), { created: '2025-01-01T00:00:00.000Z' });
    b.totals.dom['clippy::zero'] = 0;
    const p = writeBaseline(dir, b);
    assert.equal(p, path.join(dir, '.claude', 'temper-baseline.json'));
    assert.equal(BASELINE_PATH, '.claude/temper-baseline.json');
    assert.equal(BASELINE_PATH, BASELINE_FILE);
    const text = fs.readFileSync(p, 'utf8');
    assert.ok(text.endsWith('}\n') && text.startsWith('{\n  "temper_baseline": 1,\n  "created": "2025-01-01T00:00:00.000Z",'));
    assert.doesNotMatch(text, /clippy::zero|: 0[,\n]/);
    const back = readBaseline(dir);
    assert.deepEqual(Object.keys(back.totals), ['app', 'dom']);
    assert.deepEqual(Object.keys(back.totals.dom), ['clippy::indexing_slicing', 'clippy::unwrap_used', 'missing_docs']);
    assert.deepEqual(Object.keys(back), ['temper_baseline', 'created', 'updated', 'commit', 'toolchain', 'host', 'crates', 'totals']);
    assert.deepEqual(back.crates, { app: 'crates/app', dom: 'crates/dom' });
    assert.equal(back.host, 'x86_64-unknown-linux-gnu');
    // a 1.2.0-draft baseline with a files table still reads; the table is dropped on rewrite
    fs.writeFileSync(p, JSON.stringify({ ...JSON.parse(text), files: { 'x.rs': { a: 1 } } }));
    assert.equal(readBaseline(dir).files, undefined);
    fs.writeFileSync(p, text);
    assert.equal(renderBaseline(back), text, 'stable round trip');
    fs.writeFileSync(p, '{"totals": {}}');
    assert.throws(() => readBaseline(dir), /not a temper baseline/);
    fs.writeFileSync(p, '{');
    assert.throws(() => readBaseline(dir), /not valid JSON/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('the baseline is part of the fingerprint, tracked or untracked', { skip: !hasGit && 'git not installed' }, () => {
  const r = repo();
  try {
    r.put('crates/a/src/lib.rs', '//! A.\n');
    const base = r.commit('base');
    const proj = discover(r.dir);
    const clean = fingerprint(proj, base);
    r.put(BASELINE_FILE, '{"temper_baseline":1,"totals":{}}\n');
    const untracked = fingerprint(proj, base);
    assert.notEqual(untracked, clean);
    r.commit('baseline');
    r.put(BASELINE_FILE, '{"temper_baseline":1,"totals":{"a":{"x":1}}}\n');
    assert.notEqual(fingerprint(proj, base), untracked);
    r.put('notes.md', 'unrelated\n');
    const withNotes = fingerprint(proj, base);
    fs.rmSync(path.join(r.dir, 'notes.md'));
    assert.equal(fingerprint(proj, base), withNotes, 'other files still do not count');
  } finally { fs.rmSync(r.dir, { recursive: true, force: true }); }
});

// ------------------------------------------------------------ the gate in ratchet mode

const LINTS = `[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
version = "0.1.0"
publish = false

[workspace.lints.rust]
missing_docs = "warn"

[workspace.lints.clippy]
unwrap_used = "deny"
indexing_slicing = "deny"
`;
const MEMBER = '[package]\nname = "dom"\nedition.workspace = true\nversion.workspace = true\npublish.workspace = true\n\n[lints]\nworkspace = true\n';
const LIB = `//! Legacy domain.

/// First element.
#[must_use]
pub fn first(v: &[u8]) -> u8 {
    v[0]
}

/// Parses.
#[must_use]
pub fn parse(s: &str) -> u8 {
    s.parse().unwrap()
}
`;

test('gate: ratchet mode with a baseline (legacy passes, new code fails, fixes are reported); none without one', { skip: (!hasGit || !hasCargo) && 'git or cargo not installed' }, () => {
  const r = repo();
  const savedBase = process.env.TEMPER_BASE;
  delete process.env.TEMPER_BASE;
  try {
    r.put('Cargo.toml', LINTS);
    r.put('.gitignore', 'target/\n');
    r.put('.claude/temper.json', JSON.stringify({ gate: { doc: false, scan: false } }));
    r.put('crates/dom/Cargo.toml', MEMBER);
    r.put('crates/dom/src/lib.rs', LIB);
    const base = r.commit('legacy');
    const proj = discover(r.dir);
    const strict = runGate(proj, { base, noCache: true });
    assert.equal(strict.ok, false, 'no baseline: clippy -D warnings as before');
    assert.match(renderGate(strict), /clippy FAILED/);
    assert.ok(!strict.steps.some((s) => s.ratchet));

    const c = collectFindings(proj, { cfg: proj.config });
    assert.deepEqual(c.errors, []);
    assert.deepEqual(c.sources, ['clippy'], 'gate.doc and gate.scan are off');
    writeBaseline(r.dir, baselineFrom(c));
    const green = runGate(proj, { base, noCache: true });
    assert.equal(green.ok, true, renderGate(green));
    const text = renderGate(green);
    assert.match(text, /clippy: no new findings and no counts above the baseline \(clippy\)/);
    assert.match(text, /ratchet against the baseline: legacy: \d+ finding\(s\) \(baseline \d+\)/);
    assert.ok(fs.existsSync(path.join(proj.stateDir, `findings-${green.fp}.json`)), 'the complete collection is cached');

    r.put('crates/dom/src/lib.rs', `${LIB}\n/// Second.\n#[must_use]\npub fn second(v: &[u8]) -> u8 {\n    v[1]\n}\n`);
    const red = runGate(proj, { base, noCache: true });
    assert.equal(red.ok, false);
    assert.match(renderGate(red), /crates\/dom\/src\/lib\.rs:18 clippy::indexing_slicing indexing may panic/);

    r.put('crates/dom/src/lib.rs', LIB.replace('    s.parse().unwrap()', '    s.parse().unwrap_or(0)'));
    const fixed = runGate(proj, { base, noCache: true });
    assert.equal(fixed.ok, true, renderGate(fixed));
    assert.match(renderGate(fixed), /1 fixed since the baseline: run temper baseline --tighten/);

    r.put('.claude/temper.json', JSON.stringify({ gate: { doc: false, scan: false, steps: [{ name: 'custom', cmd: ['cargo', '--version'] }] } }));
    const custom = runGate(discover(r.dir), { base, noCache: true });
    assert.deepEqual(custom.steps.map((s) => s.name), ['fmt', 'manifests', 'custom'], 'a gate.steps override wins');
  } finally {
    if (savedBase !== undefined) process.env.TEMPER_BASE = savedBase;
    fs.rmSync(r.dir, { recursive: true, force: true });
  }
});

// ------------------------------------------------------------ the CLI (no cargo needed)

const CLI = fileURLToPath(new URL('../scripts/temper.mjs', import.meta.url));
const cli = (cwd, args, cliPath = CLI, extraEnv = {}) => {
  const env = { ...process.env };
  delete env.TEMPER_BASE;
  delete env.CLAUDECODE;
  delete env.TEMPER_ALLOW_RAISE;
  const r = spawnSync(process.execPath, [cliPath, ...args], { cwd, encoding: 'utf8', env: { ...env, ...extraEnv } });
  return { code: r.status, out: `${r.stdout}${r.stderr}` };
};

test('CLI: usage errors, a bad --base or --findings-out, and raises inside Claude Code exit 2 before any cargo run', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-cli-'));
  try {
    fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[workspace]\nmembers = []\n');
    for (const args of [['--bogus'], ['--init', '--update'], ['--tighten', '--base', 'HEAD'], ['--check', '--base'], ['--findings-out', 'x.json']]) {
      const r = cli(dir, ['baseline', ...args]);
      assert.equal(r.code, 2, args.join(' '));
      assert.match(r.out, /usage: temper baseline/);
    }
    const tight = cli(dir, ['baseline', '--tighten']);
    assert.equal(tight.code, 2);
    assert.match(tight.out, /no \.claude\/temper-baseline\.json/);
    for (const out of ['crates/a/src/lib.rs', '.github/workflows/rust.yml', '.claude/settings.json', 'target/../src/x.json', '../elsewhere/target/f.json', 'target']) {
      const r = cli(dir, ['baseline', '--check', '--findings-out', out]);
      assert.equal(r.code, 2, out);
      assert.match(r.out, /--findings-out .* refused: write findings under target\/ or \.temper\//, out);
    }
    if (canSymlink) {
      fs.mkdirSync(path.join(dir, 'target'));
      fs.symlinkSync(path.join(dir, 'Cargo.toml'), path.join(dir, 'target', 'link.json'));
      assert.equal(cli(dir, ['baseline', '--check', '--findings-out', 'target/link.json']).code, 2, 'not onto a symlink');
    }
    assert.equal(cli(dir, ['baseline', '--check', '--any-host']).code, 2, '--any-host is for --tighten only');
    const badBase = cli(dir, ['baseline', '--check', '--base', 'origin/mian']);
    assert.equal(badBase.code, 2);
    assert.match(badBase.out, /--base origin\/mian is not a commit in this repository/);
    for (const mode of ['--init', '--update']) {
      const r = cli(dir, ['baseline', mode], CLI, { CLAUDECODE: '1' });
      assert.equal(r.code, 2);
      assert.match(r.out, /refused inside a Claude Code session[\s\S]*TEMPER_ALLOW_RAISE=1/);
    }
    writeBaseline(dir, BASE);
    const again = cli(dir, ['baseline', '--init']);
    assert.equal(again.code, 2);
    assert.match(again.out, /already exists/);
    fs.writeFileSync(path.join(dir, '.claude', 'temper-baseline.json'), '[]');
    assert.equal(cli(dir, ['baseline', '--check']).code, 2);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('CLI: --init says plainly that the workspace is below the git root, before any fmt check', { skip: !hasGit && 'git not installed' }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-nested-'));
  try {
    fs.mkdirSync(path.join(dir, 'rust', 'src'), { recursive: true });
    fs.writeFileSync(path.join(dir, 'rust', 'Cargo.toml'), '[package]\nname = "n"\nversion = "0.1.0"\nedition = "2024"\n');
    fs.writeFileSync(path.join(dir, 'rust', 'src', 'lib.rs'), 'pub fn f() {\n        }\n');
    spawnSync('git', ['init', '-q'], { cwd: dir });
    const r = cli(path.join(dir, 'rust'), ['baseline', '--init']);
    assert.equal(r.code, 2, r.out);
    assert.match(r.out, /temper needs the Cargo workspace at the git root: .* \(the workspace is at rust\)/);
    assert.doesNotMatch(r.out, /cargo fmt/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('CLI: --init refuses unformatted code', { skip: !hasCargo && 'cargo not installed' }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-fmt-'));
  try {
    fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[package]\nname = "f"\nversion = "0.1.0"\nedition = "2024"\n');
    fs.mkdirSync(path.join(dir, 'src'));
    fs.writeFileSync(path.join(dir, 'src', 'lib.rs'), '//! F.\n\n/// One.\npub fn one() -> u8 {\n        1\n}\n');
    const r = cli(dir, ['baseline', '--init']);
    assert.equal(r.code, 1, r.out);
    assert.match(r.out, /`cargo fmt --all -- --check` fails \(1 file\(s\), e\.g\. src\/lib\.rs\)\. Run `cargo fmt --all` and commit the formatting on its own first/);
    assert.ok(!fs.existsSync(path.join(dir, '.claude', 'temper-baseline.json')));
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('CLI: report and setup say so when this copy lacks report.mjs or vendor.mjs', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-nomod-'));
  try {
    const plugin = path.dirname(path.dirname(CLI));
    fs.cpSync(path.join(plugin, 'scripts'), path.join(dir, 'scripts'), { recursive: true });
    fs.cpSync(path.join(plugin, 'templates', 'workspace'), path.join(dir, 'templates', 'workspace'), { recursive: true });
    fs.rmSync(path.join(dir, 'scripts', 'lib', 'report.mjs'), { force: true });
    fs.rmSync(path.join(dir, 'scripts', 'lib', 'vendor.mjs'), { force: true });
    const ws = path.join(dir, 'ws');
    fs.mkdirSync(ws);
    fs.writeFileSync(path.join(ws, 'Cargo.toml'), '[workspace]\nmembers = []\n');
    const copy = path.join(dir, 'scripts', 'temper.mjs');
    const report = cli(ws, ['report'], copy);
    assert.equal(report.code, 2);
    assert.match(report.out, /`temper report` is not available in this copy of temper \(scripts\/lib\/report\.mjs is missing\)/);
    const setup = cli(ws, ['setup', '--dry-run'], copy);
    assert.equal(setup.code, 0, setup.out);
    assert.match(setup.out, /skip {3}\.claude\/temper\/ \(this temper copy cannot vendor itself: scripts\/lib\/vendor\.mjs is missing\)/);
    const usage = cli(ws, [], copy);
    assert.match(usage.out, /baseline \[--init \| --update \| --tighten \| --check/);
    assert.match(usage.out, /report \[--findings FILE\]/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
