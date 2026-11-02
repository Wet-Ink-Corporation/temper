// The `temper hook` parity cases (port plan §2.2 item 2, Phase 2). Each case in cases/hooks/*.json
// runs its steps with both engines, each in its own fresh copy of the fixture and its own scratch
// temp and plugin data dirs, and requires equal exit codes, stdout and stderr after the roots are
// masked, and equal written files after the masks below. Needs TEMPER_BIN, the Rust binary:
//
//   cargo build -p temper-cli && TEMPER_BIN=target/debug/temper node --test tests/parity/hooks.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { runHookCase } from './lib/cases.mjs';
import { nodeEngine, rustEngine } from './lib/engines.mjs';
import { HOOK_FIXTURES, writeTree } from './lib/fixtures.mjs';
import { maskFailureText, maskMarkerTime, maskTelemetryTimes } from './lib/normalise.mjs';

// Invariant 5 (H-12): without the binary the file fails; it never skips.
const bin = process.env.TEMPER_BIN;
if (!bin) {
  throw new Error('TEMPER_BIN is not set: build temper-cli (cargo build -p temper-cli) and point TEMPER_BIN at target/debug/temper');
}

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '..', '..');
const engines = [nodeEngine(REPO), rustEngine(bin)];
const CASES_DIR = path.join(HERE, 'cases', 'hooks');
// OC-4 A: the wall-clock bound of the 1.x tests, as an absolute budget per Rust step.
const BUDGET_MS = 2_000;

/** Every case, in file then list order, with its file's name. */
const cases = fs.readdirSync(CASES_DIR).filter((f) => f.endsWith('.json')).sort()
  .flatMap((f) => JSON.parse(fs.readFileSync(path.join(CASES_DIR, f), 'utf8')).map((c) => ({ ...c, file: f })));

const dirs = {};
before(() => {
  for (const [name, files] of Object.entries(HOOK_FIXTURES)) {
    dirs[name] = fs.mkdtempSync(path.join(os.tmpdir(), `temper-parity-${name}-`));
    writeTree(dirs[name], files);
  }
});
after(() => {
  for (const d of Object.values(dirs)) fs.rmSync(d, { recursive: true, force: true });
});

/** A written file after §2.2's masks: telemetry row times, and a skill marker's time. */
function maskFile(rel, text) {
  if (/(^|\/)telemetry(\.1)?\.jsonl$/.test(rel)) return maskTelemetryTimes(text);
  if (/\/skill-[^/]+$/.test(rel)) return maskMarkerTime(text);
  return text;
}

/** One engine's run of a case, masked; `failures` counts OC-3 (p)'s masked texts. */
function run(engine, c, steps) {
  const r = runHookCase(engine, dirs[c.fixture], { steps, cwd: c.cwd, git: c.git, pluginData: c.pluginData, env: c.env, maskFile });
  let failures = 0;
  const out = r.steps.map((s) => {
    let { stdout } = s;
    if (c.maskFailure) {
      const m = maskFailureText(stdout);
      failures += m.count;
      stdout = m.text;
    }
    return { exit: s.exit, stdout, stderr: s.stderr };
  });
  return { out, files: r.files, failures, ms: r.steps.map((s) => s.ms) };
}

test('the hook cases: unique names, known fixtures, and the 2e floor of 110 (protocol, post-tool and user-prompt: 31, pre-edit: 31, pre-bash: 48)', () => {
  const names = cases.map((c) => c.name);
  assert.deepEqual(names.filter((n, i) => names.indexOf(n) !== i), [], 'duplicate names');
  assert.deepEqual(cases.filter((c) => !(c.fixture in HOOK_FIXTURES)).map((c) => c.name), [], 'unknown fixtures');
  assert.ok(cases.length >= 110, `${cases.length} cases`);
});

for (const c of cases) {
  test(`${c.file}: ${c.name}`, () => {
    const steps = c.steps ?? [{ event: c.event, stdin: c.stdin }];
    const [node, rust] = engines.map((engine) => run(engine, c, steps));
    assert.deepStrictEqual(rust.out, node.out, 'exit, stdout and stderr per step');
    assert.deepStrictEqual(rust.files, node.files, 'written files');
    assert.equal(rust.failures, node.failures, 'OC-3 (p) masks on each side');
    if (c.maskFailure) assert.equal(rust.failures, c.maskFailure, 'OC-3 (p) masked texts');
    if (c.stdout) assert.deepEqual(node.out.map((s) => s.stdout), c.stdout, 'the pinned stdout');
    if (c.wrote) assert.deepEqual(Object.keys(node.files).sort(), [...c.wrote].sort(), 'the pinned files');
    const slow = rust.ms.filter((ms) => ms >= BUDGET_MS);
    assert.deepEqual(slow, [], `a Rust step took ${BUDGET_MS} ms or more (OC-4 A)`);
  });
}
