// The `temper scan` parity cases (port plan §2.2 item 2, Phase 1e). Each case runs both engines,
// each in its own fresh copy of the fixture, and requires equal exit codes, stdout and stderr
// after the temp root is masked, and no file written. Needs TEMPER_BIN, the Rust binary:
//
//   cargo build -p temper-cli && TEMPER_BIN=target/debug/temper node --test tests/parity/scan.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { runInFreshCopy } from './lib/cases.mjs';
import { nodeEngine, rustEngine } from './lib/engines.mjs';
import { SCAN_FIXTURE, largeCrate, largeFile, writeTree } from './lib/fixtures.mjs';
import { sortHitLines } from './lib/normalise.mjs';

// Invariant 5 (H-12): without the binary the file fails; it never skips.
const bin = process.env.TEMPER_BIN;
if (!bin) {
  throw new Error('TEMPER_BIN is not set: build temper-cli (cargo build -p temper-cli) and point TEMPER_BIN at target/debug/temper');
}

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const engines = [nodeEngine(REPO), rustEngine(bin)];
const dirs = {};

before(() => {
  for (const [name, files] of [['fixture', SCAN_FIXTURE], ['large', largeCrate()], ['empty', {}]]) {
    dirs[name] = fs.mkdtempSync(path.join(os.tmpdir(), `temper-parity-${name}-`));
    writeTree(dirs[name], files);
  }
});
after(() => {
  for (const d of Object.values(dirs)) fs.rmSync(d, { recursive: true, force: true });
});

/**
 * Runs both engines on fresh copies of `src` and asserts they agree and wrote nothing. Returns
 * the agreed `{ exit, stdout, stderr }`, so a case can also pin what it is about.
 */
function parity(src, { args, cwd = '.', git = false, skip = [], normalise = (s) => s }) {
  const [node, rust] = engines.map((engine) => {
    const r = runInFreshCopy(engine, src, { args, cwd, git, skip });
    assert.deepStrictEqual(r.wrote, [], `${engine.name} wrote into the tree`);
    return { exit: r.exit, stdout: normalise(r.stdout), stderr: normalise(r.stderr) };
  });
  assert.deepStrictEqual(rust, node);
  return node;
}

const files = (stdout) => [...new Set(JSON.parse(stdout).map((h) => h.file))];

test('S1 scan --all --json: walk order, kinds, skipped dirs, CRLF, BOM, lossy decoding, truncation', () => {
  const r = parity(dirs.fixture, { args: ['scan', '--all', '--json'] });
  assert.equal(r.exit, 1);
  const seen = files(r.stdout);
  for (const f of ['crates/core/src/crlf.rs', 'crates/core/src/bom.rs', 'crates/core/src/latin1.rs', 'scripts/gen.rs']) {
    assert.ok(seen.includes(f), f);
  }
  assert.equal(seen.some((f) => /^(target|\.temper|node_modules)\//.test(f) || f.endsWith('.txt')), false);
  assert.ok(JSON.parse(r.stdout).some((h) => h.file === 'crates/core/src/latin1.rs' && h.text.includes('�')));
});

test('S2 scan --all --json from a subdirectory of a git repository: the root is the top level', () => {
  const r = parity(dirs.fixture, { args: ['scan', '--all', '--json'], cwd: 'crates/core', git: true });
  assert.ok(files(r.stdout).includes('crates/tool/src/main.rs'));
});

test('S3 scan --all, text form, through the D16(l) sort (B.rs and a.rs)', () => {
  const r = parity(dirs.fixture, { args: ['scan', '--all'], normalise: sortHitLines });
  assert.match(r.stdout, /\n\d+ blocking, \d+ exception request\(s\), \d+ advisory\n$/);
});

test('S4 the example workspace, text and JSON, compared exactly', () => {
  const ws = path.join(REPO, 'templates', 'example-workspace');
  parity(ws, { args: ['scan', '--all'], skip: ['target', '.git'] });
  const r = parity(ws, { args: ['scan', '--all', '--json'], skip: ['target', '.git'] });
  assert.equal(r.exit, 0);
});

test('S5 scan one file --json', () => {
  const r = parity(dirs.fixture, { args: ['scan', 'crates/core/src/lib.rs', '--json'] });
  assert.deepEqual(files(r.stdout), ['crates/core/src/lib.rs']);
});

test('S6 a missing file is skipped; a non-.rs file is scanned', () => {
  const r = parity(dirs.fixture, { args: ['scan', 'crates/core/src/a.rs', 'nope.rs', 'notes.txt', '--json'] });
  assert.deepEqual(files(r.stdout), ['crates/core/src/a.rs', 'notes.txt']);
});

test('S7 relative paths from a subdirectory, and the test kind', () => {
  const r = parity(dirs.fixture, { args: ['scan', '../tests/it.rs', 'a.rs', '--json'], cwd: 'crates/core/src' });
  assert.deepEqual(files(r.stdout), ['crates/core/tests/it.rs', 'crates/core/src/a.rs']);
});

test('S8 a file outside any crate is prod, relative to the root', () => {
  const r = parity(dirs.fixture, { args: ['scan', 'scripts/gen.rs', '--json'] });
  assert.deepEqual(files(r.stdout), ['scripts/gen.rs']);
});

test('S9 exit 0 when clean, and with only ask or soft hits', () => {
  assert.equal(parity(dirs.fixture, { args: ['scan', 'crates/core/src/clean.rs'] }).exit, 0);
  const asks = parity(dirs.fixture, { args: ['scan', 'crates/core/src/asks.rs'] });
  assert.equal(asks.exit, 0);
  assert.match(asks.stdout, /^\[ask\] /);
});

test('S10 outside a Rust project: exit 2 and the message', () => {
  const r = parity(dirs.empty, { args: ['scan', '--all'] });
  assert.deepEqual(r, { exit: 2, stdout: '', stderr: 'temper: not inside a Rust project (no Cargo.toml found)\n' });
});

test('S11 unknown flags are ignored', () => {
  const r = parity(dirs.fixture, { args: ['scan', '--all', '--json', '--bogus'] });
  assert.equal(r.exit, 1);
});

test('S12 the large file through the CLI: exactly its seeded hit', () => {
  const r = parity(dirs.large, { args: ['scan', '--all', '--json'] });
  assert.deepEqual(JSON.parse(r.stdout).map((h) => `${h.key}:${h.id}:${h.severity}@${h.line}`),
    [`unwrap:H-02:hard@${largeFile().seedLine}`]);
});

if (process.platform === 'win32') {
  test('S13 a backslash argument (D16(m) drive-relative forms stay out)', () => {
    const r = parity(dirs.fixture, { args: ['scan', 'crates\\core\\src\\lib.rs', '--json'] });
    assert.deepEqual(files(r.stdout), ['crates/core/src/lib.rs']);
  });
}
