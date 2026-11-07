// The `temper status` parity cases (port Phase 3b: the text form; 3c: `--json`). Each case in
// cases/cli/status.json builds its tree with setup actions (git commits with fixed dates, writes,
// removals) in a fresh copy of its fixture for each engine, runs its CLI steps there, and requires
// equal exit codes, stdout and stderr after the roots are masked, and no written file. The cases
// are the fingerprint corpus: every tree shape the fingerprint reads. Needs TEMPER_BIN:
//
//   cargo build -p temper-cli && TEMPER_BIN=target/debug/temper node --test tests/parity/status.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { runHookCase } from './lib/cases.mjs';
import { nodeEngine, rustEngine } from './lib/engines.mjs';
import { HOOK_FIXTURES, writeTree } from './lib/fixtures.mjs';

// Invariant 5 (H-12): without the binary the file fails; it never skips.
const bin = process.env.TEMPER_BIN;
if (!bin) {
  throw new Error('TEMPER_BIN is not set: build temper-cli (cargo build -p temper-cli) and point TEMPER_BIN at target/debug/temper');
}

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '..', '..');
const engines = [nodeEngine(REPO), rustEngine(bin)];
// OC-4 A: the absolute budget per Rust step.
const BUDGET_MS = 2_000;

// D16(a): the detail of a JSON parse error is serde_json's wording, not V8's, so it is masked:
// at the end of a text line, and inside a JSON string of `status --json` (port 3c).
const d16a = (text) => text
  .replace(/ is not valid JSON \(.*\)$/gm, ' is not valid JSON (<detail>)')
  .replace(/ is not valid JSON \((?:[^"\\]|\\.)*\)"/g, ' is not valid JSON (<detail>)"');

// Port plan 3-13: `status --json` reports TEMPER_PLUGIN_VERSION, which the launcher sets to the
// plugin's version; 1.x reads the same version from plugin.json. Both engines get it.
const VERSION = JSON.parse(fs.readFileSync(path.join(REPO, '.claude-plugin', 'plugin.json'), 'utf8')).version;

const cases = JSON.parse(fs.readFileSync(path.join(HERE, 'cases', 'cli', 'status.json'), 'utf8'));

const dirs = {};
before(() => {
  for (const name of new Set(cases.map((c) => c.fixture))) {
    dirs[name] = fs.mkdtempSync(path.join(os.tmpdir(), `temper-parity-${name}-`));
    writeTree(dirs[name], HOOK_FIXTURES[name]);
  }
});
after(() => {
  for (const d of Object.values(dirs)) fs.rmSync(d, { recursive: true, force: true });
});

test('the status cases: unique names, known fixtures, CLI steps only, and the 3c floor of 60', () => {
  const names = cases.map((c) => c.name);
  assert.deepEqual(names.filter((n, i) => names.indexOf(n) !== i), [], 'duplicate names');
  assert.deepEqual(cases.filter((c) => !(c.fixture in HOOK_FIXTURES)).map((c) => c.name), [], 'unknown fixtures');
  assert.deepEqual(cases.filter((c) => !c.steps.every((s) => Array.isArray(s.args))).map((c) => c.name), [], 'CLI steps');
  assert.ok(cases.length >= 60, `${cases.length} cases`);
  assert.ok(cases.filter((c) => c.steps.some((s) => s.args.includes('--json'))).length >= 20, 'the --json cases');
});

for (const c of cases) {
  test(`status: ${c.name}`, () => {
    const now = Date.now();
    const [node, rust] = engines.map((engine) => runHookCase(engine, dirs[c.fixture], {
      steps: c.steps, cwd: c.cwd, git: c.git, setup: c.setup, env: { TEMPER_PLUGIN_VERSION: VERSION, ...c.env }, now,
    }));
    const out = (r) => r.steps.map((s) => ({ exit: s.exit, stdout: d16a(s.stdout), stderr: s.stderr }));
    assert.deepStrictEqual(out(rust), out(node), 'exit, stdout and stderr per step');
    assert.deepStrictEqual(rust.files, node.files, 'written files');
    if (c.stdout) assert.deepEqual(out(node).map((s) => s.stdout), c.stdout, 'the pinned stdout');
    const slow = rust.steps.map((s) => s.ms).filter((ms) => ms >= BUDGET_MS);
    assert.deepEqual(slow, [], `a Rust step took ${BUDGET_MS} ms or more (OC-4 A)`);
  });
}
