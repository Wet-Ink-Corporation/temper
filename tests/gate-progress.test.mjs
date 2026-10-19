// The gate's progress file: while the gate runs it keeps <stateDir>/gate-progress-<fp>.json with
// the steps done and the step it is on, `status --json` reports it as gate.running, and the file
// goes when the gate ends. A file left by a gate that is gone is stale: ignored and pruned.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { discover, fingerprint, headRev, gateProgressPath, pruneFingerprintState } from '../scripts/lib/project.mjs';
import { gateProgress } from '../scripts/lib/gate.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const CLI = path.join(here, '..', 'scripts', 'temper.mjs');
const hasGit = spawnSync('git', ['--version'], { encoding: 'utf8' }).status === 0;
const hasCargo = spawnSync('cargo', ['--version'], { encoding: 'utf8' }).status === 0;
const git = (cwd, ...args) => execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
const sleep = (ms) => new Promise((done) => { setTimeout(done, ms); });

/** A one-crate repository whose gate runs `steps` (a gate.steps override) after its scan and manifests. */
function workspace(t, steps) {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-progress-'));
  t.after(() => fs.rmSync(ws, { recursive: true, force: true }));
  git(ws, 'init', '-q');
  for (const [k, v] of [['user.name', 't'], ['user.email', 't@example.com'], ['commit.gpgsign', 'false']]) git(ws, 'config', k, v);
  fs.writeFileSync(path.join(ws, 'Cargo.toml'), '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2024"\n\n[lints.rust]\nunsafe_code = "forbid"\n');
  fs.mkdirSync(path.join(ws, 'src'));
  fs.writeFileSync(path.join(ws, 'src', 'lib.rs'), 'pub fn a() {}\n');
  fs.mkdirSync(path.join(ws, '.claude'));
  fs.writeFileSync(path.join(ws, '.claude', 'temper.json'), JSON.stringify({ gate: { steps } }));
  fs.writeFileSync(path.join(ws, '.gitignore'), 'target/\n');
  git(ws, 'add', '-A');
  git(ws, 'commit', '-q', '-m', 'init');
  return { ws, proj: discover(ws) };
}

const env = () => { const e = { ...process.env }; delete e.TEMPER_BASE; delete e.TEMPER_DISABLE; return e; };

test('gate progress: the file names the current step while it runs, status --json reports it, and it goes after', { skip: (!hasGit || !hasCargo) && 'git or cargo not installed' }, async (t) => {
  const release = path.join(os.tmpdir(), `temper-release-${process.pid}-${Date.now()}`);
  t.after(() => fs.rmSync(release, { force: true }));
  // A step that pauses until the test creates the release file (at most 60 s).
  const pause = `const fs=require('fs');const end=Date.now()+60000;while(!fs.existsSync(${JSON.stringify(release)})&&Date.now()<end){Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,50)}`;
  const { ws, proj } = workspace(t, [{ name: 'first', cmd: [process.execPath, '-e', ''] }, { name: 'pause', cmd: [process.execPath, '-e', pause] }]);
  const child = spawn(process.execPath, [CLI, 'gate', '--no-cache'], { cwd: ws, env: env(), stdio: 'ignore' });
  const exited = new Promise((done) => { child.on('exit', done); });
  t.after(() => { fs.writeFileSync(release, ''); child.kill(); });

  const fp = fingerprint(proj, headRev(proj));
  const file = gateProgressPath(proj, fp);
  let seen = null;
  for (let i = 0; i < 600 && seen?.current?.name !== 'pause'; i += 1) {
    await sleep(100);
    try { seen = JSON.parse(fs.readFileSync(file, 'utf8')); } catch { /* not written yet */ }
  }
  assert.equal(seen?.current?.name, 'pause', 'the progress file names the paused step');
  assert.equal(seen.fp, fp);
  assert.equal(seen.pid, child.pid);
  assert.ok(Date.parse(seen.startedAt) <= Date.parse(seen.current.startedAt));
  assert.deepEqual(seen.steps.map((s) => [s.name, s.ok, typeof s.ms]),
    [['fmt', true, 'number'], ['scan', true, 'number'], ['manifests', true, 'number'], ['first', true, 'number']]);

  const status = spawnSync(process.execPath, [CLI, 'status', '--json'], { cwd: ws, env: env(), encoding: 'utf8' });
  assert.equal(status.status, 0, status.stderr);
  const doc = JSON.parse(status.stdout);
  assert.equal(doc.fingerprint, fp);
  assert.equal(doc.gate.ok, null, 'no result yet for this tree');
  assert.deepEqual(doc.gate.steps, []);
  assert.equal(doc.gate.running.current.name, 'pause');
  assert.equal(doc.gate.running.steps.length, 4);

  // Another gate on the same fingerprint wrote the file last: this gate leaves it to that one.
  const other = { ...seen, pid: process.pid };
  fs.writeFileSync(file, JSON.stringify(other));
  fs.writeFileSync(release, '');
  assert.equal(await exited, 0);
  assert.deepEqual(JSON.parse(fs.readFileSync(file, 'utf8')), other, 'a gate removes only its own file');
  fs.rmSync(file);
  const after = JSON.parse(spawnSync(process.execPath, [CLI, 'status', '--json'], { cwd: ws, env: env(), encoding: 'utf8' }).stdout);
  assert.equal(after.gate.ok, true);
  assert.equal(after.gate.running, null);
  assert.deepEqual(after.gate.steps.map((s) => s.name), ['fmt', 'scan', 'manifests', 'first', 'pause']);
});

/** A state dir as discover() returns it, with the default step timeout. */
function state(t) {
  const stateDir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-pstate-'));
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  return { stateDir, config: { gate: { stepTimeoutSec: 900 } } };
}
const FP = '0123456789abcdef';
const write = (proj, value, fp = FP) => fs.writeFileSync(gateProgressPath(proj, fp), JSON.stringify(value));

/** A pid no process holds: above every platform's pid range. */
const deadPid = () => 2147483646;

test('gate progress: a file whose process is gone, or whose step is older than the step timeout, is stale', (t) => {
  const proj = state(t);
  const now = Date.parse('2026-10-11T12:00:00.000Z');
  const at = (msAgo) => new Date(now - msAgo).toISOString();
  write(proj, { fp: FP, pid: process.pid, startedAt: at(60000), steps: [{ name: 'fmt', ok: true, ms: 10 }], current: { name: 'clippy', startedAt: at(5000) } });
  assert.deepEqual(gateProgress(proj, FP, now), { startedAt: at(60000), steps: [{ name: 'fmt', ok: true, ms: 10 }], current: { name: 'clippy', startedAt: at(5000) } });
  assert.equal(gateProgress(proj, 'fedcba9876543210', now), null, 'another fingerprint has no gate running');
  write(proj, { fp: FP, pid: process.pid, startedAt: at(2000000), steps: [], current: { name: 'test', startedAt: at(1300000) } });
  assert.equal(gateProgress(proj, FP, now), null, 'the current step outlived the step timeout and its margin');
  write(proj, { fp: FP, pid: process.pid, startedAt: at(2000000), steps: [], current: { name: 'clippy', startedAt: at(1000000) } });
  assert.notEqual(gateProgress(proj, FP, now), null, 'a ratchet step past the step timeout but within the margin is still running');
  write(proj, { fp: FP, pid: deadPid(), startedAt: at(1000), steps: [], current: { name: 'test', startedAt: at(1000) } });
  assert.equal(gateProgress(proj, FP, now), null, 'the gate process is gone');
  fs.writeFileSync(gateProgressPath(proj, FP), '{ not json');
  assert.equal(gateProgress(proj, FP, now), null);
});

test('gate progress: pruning removes files over a day old, and keeps younger ones of gates that are gone for their session to read', (t) => {
  const proj = state(t);
  const live = '1111111111111111';
  const dead = '2222222222222222';
  const junk = '3333333333333333';
  const [oldLive, oldDead, oldJunk] = ['4444444444444444', '5555555555555555', '6666666666666666'];
  const now = new Date().toISOString();
  write(proj, { fp: live, pid: process.pid, startedAt: now, steps: [], current: null }, live);
  write(proj, { fp: dead, pid: deadPid(), startedAt: now, steps: [], current: null }, dead);
  fs.writeFileSync(gateProgressPath(proj, junk), 'x');
  write(proj, { fp: oldLive, pid: process.pid, startedAt: now, steps: [], current: null }, oldLive);
  write(proj, { fp: oldDead, pid: deadPid(), startedAt: now, steps: [], current: null }, oldDead);
  fs.writeFileSync(gateProgressPath(proj, oldJunk), 'x');
  const dayAgo = new Date(Date.now() - 86400000 - 60000);
  for (const fp of [oldLive, oldDead, oldJunk]) fs.utimesSync(gateProgressPath(proj, fp), dayAgo, dayAgo);
  pruneFingerprintState(proj, null);
  assert.deepEqual(fs.readdirSync(proj.stateDir).sort(), [live, dead, junk].map((fp) => `gate-progress-${fp}.json`));
});
