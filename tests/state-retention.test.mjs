// Retention of the state temper keeps per tree fingerprint (gate results,
// review verdicts, step logs) and of telemetry.jsonl. SessionStart on startup
// prunes the per-fingerprint records by age and count, and never the record of
// the tree the session starts on; telemetry rotates to telemetry.1.jsonl at a
// size cap, and `temper stats` reads both files.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { RETENTION, pruneFingerprintState, telemetry, readTelemetry, discover, fingerprint, sessionDir } from '../scripts/lib/project.mjs';
import { sessionStart } from '../scripts/lib/hooks.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const CLI = path.join(here, '..', 'scripts', 'temper.mjs');
const git = spawnSync('git', ['--version'], { encoding: 'utf8' }).status === 0;
const noGit = !git && 'git not installed';
const DAY = 86400000;

let plugData;
const saved = {};
before(() => {
  plugData = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-plugdata-'));
  for (const k of ['CLAUDE_PLUGIN_DATA', 'CLAUDE_ENV_FILE']) saved[k] = process.env[k];
  process.env.CLAUDE_PLUGIN_DATA = plugData;
  delete process.env.CLAUDE_ENV_FILE;
});
after(() => {
  for (const [k, v] of Object.entries(saved)) { if (v === undefined) delete process.env[k]; else process.env[k] = v; }
  fs.rmSync(plugData, { recursive: true, force: true });
});

/** A bare state dir, as discover() would return it. */
function state(t) {
  const stateDir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-state-'));
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  return { stateDir };
}

/** Write `rel` under the state dir with an mtime `ageDays` ago (minus `i` ms so ages are distinct). */
function put(proj, rel, ageDays, text = '{}', i = 0) {
  const p = path.join(proj.stateDir, ...rel.split('/'));
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, text);
  const when = new Date(Date.now() - ageDays * DAY - i);
  fs.utimesSync(p, when, when);
  return p;
}
const has = (proj, rel) => fs.existsSync(path.join(proj.stateDir, ...rel.split('/')));
const list = (proj, dir) => { try { return fs.readdirSync(path.join(proj.stateDir, dir)).sort(); } catch { return []; } };
const fp = (n) => n.toString(16).padStart(16, '0');

test('retention limits are stated and ordered: verdicts outlive gate results, whose logs go with them', () => {
  assert.deepEqual(RETENTION, {
    verdicts: { maxAgeDays: 60, keep: 300 },
    gate: { maxAgeDays: 30, keep: 300, maxBytes: 64 * 1024 * 1024 },
    telemetryBytes: 1024 * 1024,
    graceMinutes: 60,
  });
});

test('prune: gate results, verdicts and logs past their age go; fresher ones stay', (t) => {
  const proj = state(t);
  put(proj, `gate/${fp(1)}.json`, 31);
  put(proj, `gate/${fp(2)}.json`, 29);
  put(proj, `verdicts/${fp(1)}.json`, 61);
  put(proj, `verdicts/${fp(2)}.json`, 59);
  put(proj, `verdicts/${fp(3)}.json`, 31); // older than the gate limit, younger than the verdict limit
  put(proj, `logs/${fp(1)}-clippy.log`, 31);
  put(proj, `logs/${fp(2)}-test.log`, 29);
  put(proj, `logs/${fp(4)}-test.log`, 31); // a log whose gate result is gone
  put(proj, `logs/${fp(5)}-test.log`, 15); // ... and one younger than the gate limit
  pruneFingerprintState(proj, null);
  assert.deepEqual(list(proj, 'gate'), [`${fp(2)}.json`]);
  assert.deepEqual(list(proj, 'verdicts'), [`${fp(2)}.json`, `${fp(3)}.json`]);
  assert.deepEqual(list(proj, 'logs'), [`${fp(2)}-test.log`, `${fp(5)}-test.log`]);
});

const HOURS2 = 2 / 24; // past the grace period, well inside the age limits

test('prune: a busy repository keeps only the newest gate runs, each with all its logs', (t) => {
  const proj = state(t);
  const keep = RETENTION.gate.keep;
  for (let i = 0; i < keep + 5; i += 1) {
    put(proj, `gate/${fp(i)}.json`, HOURS2, '{}', i * 1000);
    for (const step of ['clippy', 'test', 'doctest', 'doc']) put(proj, `logs/${fp(i)}-${step}.log`, HOURS2, 'x', i * 1000);
  }
  for (let i = 0; i < keep + 3; i += 1) put(proj, `verdicts/${fp(i)}.json`, HOURS2, '{}', i * 1000);
  pruneFingerprintState(proj, null);
  const gates = list(proj, 'gate');
  assert.equal(gates.length, keep);
  // i = 0 is the newest (smallest offset); the five oldest went, logs and all.
  for (let i = keep; i < keep + 5; i += 1) assert.ok(!gates.includes(`${fp(i)}.json`), `gate ${i}`);
  assert.ok(gates.includes(`${fp(0)}.json`));
  assert.equal(list(proj, 'logs').length, keep * 4);
  assert.equal(list(proj, 'verdicts').length, RETENTION.verdicts.keep);
});

test('prune: a kept gate result keeps the logs it cites (a red result from hours ago, 40 gate runs since)', (t) => {
  const proj = state(t);
  const red = fp(0xbad);
  put(proj, `gate/${red}.json`, 0.2, JSON.stringify({ ok: false, steps: [{ name: 'test', ok: false, log: 'x' }] }));
  put(proj, `logs/${red}-clippy.log`, 0.2, 'x');
  put(proj, `logs/${red}-test.log`, 0.2, 'x');
  for (let i = 0; i < 40; i += 1) {
    put(proj, `gate/${fp(i)}.json`, 0, '{}', i * 1000);
    for (const step of ['clippy', 'test', 'doctest', 'doc', 'rustdoc']) put(proj, `logs/${fp(i)}-${step}.log`, 0, 'x', i * 1000);
  }
  pruneFingerprintState(proj, null);
  assert.equal(list(proj, 'gate').length, 41);
  assert.ok(has(proj, `logs/${red}-clippy.log`) && has(proj, `logs/${red}-test.log`), 'the red result still points at its logs');
  for (const g of list(proj, 'gate')) {
    const f = g.slice(0, -'.json'.length);
    assert.ok(list(proj, 'logs').some((l) => l.startsWith(`${f}-`)), `logs of ${f}`);
  }
});

test('prune: a gate run another worktree has in progress, its logs written within the grace period, is not touched', (t) => {
  const proj = state(t);
  const keep = RETENTION.gate.keep;
  const minute = 1 / 1440;
  // Another session's gate, for a fingerprint that is not this session's, has
  // written two step logs 30 minutes ago and is still running its next step: no
  // gate/<fp>.json yet. Since then, `keep` gate runs finished in other trees.
  const running = fp(0xfeed);
  put(proj, `logs/${running}-clippy.log`, 30 * minute, 'x');
  put(proj, `logs/${running}-test.log`, 30 * minute, 'x');
  for (let i = 0; i < keep; i += 1) {
    put(proj, `gate/${fp(i)}.json`, 10 * minute, '{}', i);
    put(proj, `logs/${fp(i)}-test.log`, 10 * minute, 'x', i);
  }
  // Older than the grace period and over the count: these go.
  put(proj, `gate/${fp(0x01d000)}.json`, HOURS2);
  put(proj, `logs/${fp(0x01d000)}-test.log`, HOURS2);
  // A verdict written just now, over the count, also stays.
  for (let i = 0; i < RETENTION.verdicts.keep; i += 1) put(proj, `verdicts/${fp(i)}.json`, 10 * minute, '{}', i);
  put(proj, `verdicts/${running}.json`, 30 * minute);
  pruneFingerprintState(proj, null);
  assert.ok(has(proj, `logs/${running}-clippy.log`) && has(proj, `logs/${running}-test.log`), 'the running gate keeps its logs');
  assert.ok(has(proj, `verdicts/${running}.json`));
  assert.ok(!has(proj, `gate/${fp(0x01d000)}.json`) && !has(proj, `logs/${fp(0x01d000)}-test.log`), 'a run past the grace period still goes');
  assert.equal(list(proj, 'gate').length, keep);
});

test('prune: past the byte budget the oldest gate runs go whole, result and logs together', (t) => {
  const proj = state(t);
  const big = 'x'.repeat(30 * 1024 * 1024);
  for (const i of [1, 2, 3]) {
    put(proj, `gate/${fp(i)}.json`, 4 - i);
    put(proj, `logs/${fp(i)}-test.log`, 4 - i, big);
  }
  pruneFingerprintState(proj, null);
  assert.deepEqual(list(proj, 'gate'), [`${fp(2)}.json`, `${fp(3)}.json`]);
  assert.deepEqual(list(proj, 'logs'), [`${fp(2)}-test.log`, `${fp(3)}-test.log`]);
});

test('prune: the newest gate run stays even when it alone is over the byte budget', (t) => {
  const proj = state(t);
  put(proj, `gate/${fp(1)}.json`, 0);
  put(proj, `logs/${fp(1)}-test.log`, 0, 'x'.repeat(RETENTION.gate.maxBytes + 1));
  pruneFingerprintState(proj, null);
  assert.deepEqual(list(proj, 'gate'), [`${fp(1)}.json`]);
  assert.deepEqual(list(proj, 'logs'), [`${fp(1)}-test.log`]);
});

test('prune: the records of the current fingerprint stay, however old, and count toward no cap', (t) => {
  const proj = state(t);
  const cur = fp(0xabc);
  put(proj, `gate/${cur}.json`, 400);
  put(proj, `verdicts/${cur}.json`, 400);
  put(proj, `logs/${cur}-clippy.log`, 400);
  put(proj, `logs/${cur}-test.log`, 400);
  put(proj, `verdicts/${fp(1)}.json`, 400);
  pruneFingerprintState(proj, cur);
  assert.deepEqual(list(proj, 'gate'), [`${cur}.json`]);
  assert.deepEqual(list(proj, 'verdicts'), [`${cur}.json`]);
  assert.deepEqual(list(proj, 'logs'), [`${cur}-clippy.log`, `${cur}-test.log`]);
});

test('prune: leaves sessions, the findings cache, telemetry and a missing state dir alone', (t) => {
  const proj = state(t);
  put(proj, 'sessions/s-old/base', 400, 'abc');
  put(proj, `findings-${fp(1)}.json`, 400);
  put(proj, 'telemetry.jsonl', 400, '{"event":"x"}\n');
  put(proj, 'telemetry.1.jsonl', 400, '{"event":"y"}\n');
  pruneFingerprintState(proj, null);
  assert.ok(has(proj, 'sessions/s-old/base'));
  assert.ok(has(proj, `findings-${fp(1)}.json`));
  assert.ok(has(proj, 'telemetry.jsonl'));
  assert.ok(has(proj, 'telemetry.1.jsonl'));
  assert.doesNotThrow(() => pruneFingerprintState({ stateDir: path.join(proj.stateDir, 'missing') }, null));
});

test('telemetry: below the cap it appends; at the cap it rotates to telemetry.1.jsonl, keeping one old file', (t) => {
  const proj = state(t);
  const cap = RETENTION.telemetryBytes;
  telemetry(proj, { session_id: 's1' }, 'first');
  telemetry(proj, { session_id: 's1' }, 'second');
  assert.equal(readTelemetry(proj).map((r) => r.event).join(','), 'first,second');
  assert.ok(!has(proj, 'telemetry.1.jsonl'), 'a small file is not rotated');

  const line = `${JSON.stringify({ t: 'x', sid: 'old', event: 'filler' })}\n`;
  fs.writeFileSync(path.join(proj.stateDir, 'telemetry.jsonl'), line.repeat(Math.ceil(cap / line.length)));
  telemetry(proj, { session_id: 's2' }, 'after-rotate');
  const rotated = fs.readFileSync(path.join(proj.stateDir, 'telemetry.1.jsonl'), 'utf8');
  assert.ok(rotated.length >= cap);
  assert.equal(fs.readFileSync(path.join(proj.stateDir, 'telemetry.jsonl'), 'utf8').trim().split('\n').length, 1);

  fs.writeFileSync(path.join(proj.stateDir, 'telemetry.jsonl'), `${JSON.stringify({ event: 'marker' })}\n`.padEnd(cap + 10, ' '));
  telemetry(proj, { session_id: 's3' }, 'third-file');
  const rows = readTelemetry(proj);
  assert.ok(!rows.some((r) => r.event === 'filler'), 'the oldest file is dropped on the second rotation');
  assert.deepEqual(rows.map((r) => r.event), ['marker', 'third-file'], 'old file first, then the current one');
});

/** A git repository holding a Cargo workspace, committed. */
function repo(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-retention-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const g = (...args) => execFileSync('git', ['-c', 'user.email=t@example.com', '-c', 'user.name=t', '-c', 'commit.gpgsign=false', ...args], { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[workspace]\nresolver = "3"\nmembers = []\n');
  g('init', '-q');
  g('config', 'core.autocrlf', 'false');
  g('add', '-A');
  g('commit', '-qm', 'base');
  return { dir, git: g, proj: () => discover(dir) };
}

test('temper stats counts the rotated file and the current one', { skip: noGit }, (t) => {
  const r = repo(t);
  const proj = r.proj();
  fs.mkdirSync(proj.stateDir, { recursive: true });
  fs.writeFileSync(path.join(proj.stateDir, 'telemetry.1.jsonl'), `${JSON.stringify({ sid: 'a', event: 'session-start' })}\n${JSON.stringify({ sid: 'a', event: 'deny-scan', rules: ['H-02'] })}\n`);
  fs.writeFileSync(path.join(proj.stateDir, 'telemetry.jsonl'), `${JSON.stringify({ sid: 'b', event: 'session-start' })}\nnot json\n`);
  const res = spawnSync(process.execPath, [CLI, 'stats'], { cwd: r.dir, encoding: 'utf8' });
  assert.equal(res.status, 0, res.stderr);
  assert.match(res.stdout, /^sessions: 2\n/);
  assert.match(res.stdout, /session-start\s+2\n/);
  assert.match(res.stdout, /H-02\s+1\n/);
});

test('temper stats with no telemetry at all reports zero sessions', { skip: noGit }, (t) => {
  const r = repo(t);
  const res = spawnSync(process.execPath, [CLI, 'stats'], { cwd: r.dir, encoding: 'utf8' });
  assert.equal(res.status, 0, res.stderr);
  assert.match(res.stdout, /^sessions: 0\n/);
});

test('SessionStart startup prunes old records but keeps the tree it starts on; resume prunes nothing', { skip: noGit }, (t) => {
  const r = repo(t);
  fs.writeFileSync(path.join(r.dir, 'lib.rs'), 'pub fn uncommitted() {}\n'); // work the session finds
  const proj = r.proj();
  const cur = fingerprint(proj, r.git('rev-parse', 'HEAD'));
  put(proj, `verdicts/${cur}.json`, 400);
  put(proj, `gate/${cur}.json`, 400);
  put(proj, `verdicts/${fp(1)}.json`, 400);
  put(proj, `gate/${fp(1)}.json`, 400);
  put(proj, `logs/${fp(1)}-test.log`, 400);

  sessionStart({ cwd: r.dir, session_id: 's-resume', source: 'resume' });
  assert.ok(has(proj, `verdicts/${fp(1)}.json`), 'resume does not prune');

  sessionStart({ cwd: r.dir, session_id: 's-start', source: 'startup' });
  assert.deepEqual(list(proj, 'verdicts'), [`${cur}.json`]);
  assert.deepEqual(list(proj, 'gate'), [`${cur}.json`]);
  assert.deepEqual(list(proj, 'logs'), []);
  assert.ok(fs.existsSync(sessionDir(proj, 's-start')));
});
