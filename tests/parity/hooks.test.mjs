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
import {
  maskConfigLine, maskD17Lines, maskFailureText, maskFoundMs, maskMarkerTime, maskStartedTime, maskTelemetryTimes, maskTreeAt,
  maskGitIndex, maskRecordedAt, maskTreeIds, maskVerdictParse,
} from './lib/normalise.mjs';

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

/**
 * A written file after §2.2's masks: telemetry row times (and a gate-lost row's `foundMs`), a skill
 * marker's time, a session's start time (port 3d), a tree record's time and the stat data of a git
 * index a case's git call rewrote (port 3e; only the stat data since the #210 review), and D16(w)'s
 * lines in an env file, which `counts.d17` counts (OC-6 A).
 */
function fileMask() {
  const counts = { d17: 0 };
  const mask = (rel, text, bytes) => {
    if (/(^|\/)telemetry(\.1)?\.jsonl$/.test(rel)) return maskFoundMs(maskTelemetryTimes(text));
    if (/\/skill-[^/]+$/.test(rel)) return maskMarkerTime(text);
    if (/\/sessions\/[^/]+\/started$/.test(rel)) return maskStartedTime(text);
    if (/\/sessions\/[^/]+\/tree-[^/]+\.json$/.test(rel)) return maskTreeAt(text);
    if (/\/verdicts\/[^/]+\.(json|history\.jsonl)$/.test(rel)) return maskRecordedAt(text);
    if (/^tree\/\.git\/(worktrees\/[^/]+\/)?index$/.test(rel)) return maskGitIndex(bytes);
    if (/(^|\/)env\.sh$/.test(rel)) {
      const m = maskD17Lines(text);
      counts.d17 += m.count;
      return m.text;
    }
    return text;
  };
  return { mask, counts };
}

/**
 * One engine's run of a case, masked; `failures` counts OC-3 (p)'s masked texts, `parse` D16(a)'s
 * verdict parse texts and `d17` D16(w)'s masked env-file lines. `engineFor` picks each step's
 * engine (port 3f's cross-read cases); `<FP>` is always 1.x's fingerprint.
 */
function run(engine, c, steps, now, engineFor = () => engine) {
  const { mask, counts } = fileMask();
  const r = runHookCase(engine, dirs[c.fixture], {
    steps, cwd: c.cwd, git: c.git, pluginData: c.pluginData, env: c.env, maskFile: mask, setup: c.setup, now, engineFor, fpEngine: engines[0],
  });
  let failures = 0;
  let parse = 0;
  const out = r.steps.map((s) => {
    let stdout = maskRecordedAt(s.stdout);
    const p = maskVerdictParse(stdout);
    parse += p.count;
    stdout = p.text;
    if (c.maskFailure) {
      const m = maskFailureText(stdout);
      failures += m.count;
      stdout = m.text;
    }
    // OC-5 A: only 2.0 says that the config is unreadable; a no-op elsewhere.
    return { exit: s.exit, stdout: maskConfigLine(stdout), stderr: s.stderr };
  });
  return { out, files: maskTreeIds(r.files), failures, parse, d17: counts.d17, ms: r.steps.map((s) => s.ms) };
}

/** OC-6 A (wi-168ff8): D17's two lines per env-file write on Rust's side, none on 1.x's. */
function assertD17(rust, node, writes) {
  assert.equal(node.d17, 0, 'D17: 1.x writes no TEMPER_BIN or PATH line');
  assert.equal(rust.d17, 2 * writes, `D17: two lines per env-file write (${writes})`);
}

test('the hook cases: unique names, known fixtures, and the 3f floor of 224 (protocol, post-tool and user-prompt: 34, pre-edit: 31 and 13 with git, pre-bash: 67, session-start: 27, subagent-start: 20, pre-handback: 16, subagent-stop: 11, verdict cross-reads: 5)', () => {
  const names = cases.map((c) => c.name);
  assert.deepEqual(names.filter((n, i) => names.indexOf(n) !== i), [], 'duplicate names');
  assert.deepEqual(cases.filter((c) => !(c.fixture in HOOK_FIXTURES)).map((c) => c.name), [], 'unknown fixtures');
  assert.ok(cases.length >= 224, `${cases.length} cases`);
});

for (const c of cases) {
  test(`${c.file}: ${c.name}`, () => {
    const steps = c.steps ?? [{ event: c.event, stdin: c.stdin }];
    // One `now` per case, so both engines see the same placeholder times.
    const now = Date.now();
    // A cross-read case (port 3f) runs its `engine: "node"` steps with 1.x on the Rust side too,
    // and is compared with the same case run by 1.x alone.
    const [nodeEngine1x, rustEngine2] = engines;
    const node = run(nodeEngine1x, c, steps, now);
    const rust = run(rustEngine2, c, steps, now, (step) => (step.engine === 'node' ? nodeEngine1x : rustEngine2));
    assert.deepStrictEqual(rust.out, node.out, 'exit, stdout and stderr per step');
    assert.deepStrictEqual(rust.files, node.files, 'written files');
    assert.equal(rust.failures, node.failures, 'OC-3 (p) masks on each side');
    assert.equal(rust.parse, node.parse, 'D16(a) verdict parse masks on each side');
    if (c.maskParse) assert.equal(rust.parse, c.maskParse, 'D16(a) masked texts');
    if (c.maskFailure) assert.equal(rust.failures, c.maskFailure, 'OC-3 (p) masked texts');
    assertD17(rust, node, c.envFileWrites ?? 0);
    if (c.stdout) assert.deepEqual(node.out.map((s) => s.stdout), c.stdout, 'the pinned stdout');
    if (c.wrote) assert.deepEqual(Object.keys(node.files).sort(), [...c.wrote].sort(), 'the pinned files');
    for (const [rel, parts] of Object.entries(c.has ?? {})) for (const p of parts) assert.ok(node.files[rel]?.includes(p), `${rel} has ${p}`);
    for (const [rel, parts] of Object.entries(c.lacks ?? {})) {
      assert.ok(rel in node.files, `${rel} was written`);
      for (const p of parts) assert.ok(!node.files[rel].includes(p), `${rel} lacks ${p}`);
    }
    const slow = rust.ms.filter((ms) => ms >= BUDGET_MS);
    assert.deepEqual(slow, [], `a Rust step took ${BUDGET_MS} ms or more (OC-4 A)`);
  });
}

// Ported from messages.test.mjs, "the CLI on a simulated Node 17: SessionStart runs in full and adds
// the warning" (port-phase-3 item 36): 1.x's CLI against launch.mjs with TEMPER_ENGINE=rust, both on
// a Node that reports 17.9.1, so the launcher passes the warning in TEMPER_RUNTIME_WARNINGS.
test('the CLI on a simulated Node 17: SessionStart runs in full and adds the warning, through launch.mjs as through 1.x', () => {
  const fake = 'data:text/javascript,Object.defineProperty(process.versions,"node",{value:"17.9.1"});';
  const onNode17 = (name, script) => ({ name, cmd: process.execPath, pre: ['--import', fake, path.join(REPO, 'scripts', script)] });
  const c = {
    fixture: 'workspace',
    git: true,
    pluginData: true,
    env: { CLAUDE_PLUGIN_ROOT: '/plugin', CLAUDE_ENV_FILE: '<ROOT>/.temper/env.sh', TEMPER_ENGINE: 'rust', TEMPER_BIN: path.resolve(bin) },
    setup: [{ git: ['add', '-A'] }, { git: ['commit', '-qm', 'init'] }, { write: { '.temper/env.sh': '' } }],
  };
  const steps = [{ event: 'session-start', stdin: '{"cwd":"<ROOT>","session_id":"s-node17","source":"startup","hook_event_name":"SessionStart"}' }];
  const now = Date.now();
  const [node, rust] = [onNode17('1.x', 'temper.mjs'), onNode17('launch', 'launch.mjs')].map((engine) => run(engine, c, steps, now));
  assert.deepStrictEqual(rust.out, node.out, 'exit, stdout and stderr');
  assert.deepStrictEqual(rust.files, node.files, 'written files');
  assertD17(rust, node, 1);
  const json = JSON.parse(node.out[0].stdout);
  assert.match(json.systemMessage, /^temper: Node 17\.9\.1 is older than the documented minimum, Node 18\.0\.0/);
  const context = json.hookSpecificOutput.additionalContext;
  assert.match(context, /governed by the temper plugin/);
  assert.match(context, /- Gate command: /);
  assert.ok(context.endsWith(`\n- ${json.systemMessage}`), context);
  assert.match(node.files['tree/.temper/env.sh'], /^export TEMPER_CLI="/);
  // The launcher did hand the hook to the engine: without one it fails as it cannot run it.
  const missing = run(onNode17('launch', 'launch.mjs'), { ...c, env: { ...c.env, TEMPER_BIN: path.join(REPO, 'target', 'no-such-temper') } }, steps, now);
  assert.match(missing.out[0].stderr, /^temper: cannot run the engine at /, 'routed to the engine');
});
