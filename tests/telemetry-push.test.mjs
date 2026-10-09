// Telemetry that outlives the machine: with `telemetry.branch` set, Stop pushes the
// session's rows to that branch on origin (a Claude Code cloud session's container,
// and .git/temper/ with it, is discarded when the session ends), and
// `temper stats --branch` reads every pushed session back. The push must leave the
// clone's working tree, index, HEAD and branches as they were.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { telemetry, pushTelemetry, fetchTelemetryBranch, discover } from '../scripts/lib/project.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const CLI = path.join(here, '..', 'scripts', 'temper.mjs');
const hasGit = spawnSync('git', ['--version'], { encoding: 'utf8' }).status === 0;
const noGit = !hasGit && 'git not installed';

const saved = {};
before(() => {
  for (const k of ['CLAUDE_CODE_REMOTE_SESSION_ID', 'GIT_INDEX_FILE']) { saved[k] = process.env[k]; delete process.env[k]; }
});
after(() => {
  for (const [k, v] of Object.entries(saved)) { if (v === undefined) delete process.env[k]; else process.env[k] = v; }
});

const git = (cwd, ...args) => execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();

/** A bare origin and a clone of it holding a Cargo workspace with `config` as .claude/temper.json. */
function workspace(t, config = { telemetry: { branch: 'temper-telemetry' } }) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-tpush-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const origin = path.join(dir, 'origin.git');
  const ws = path.join(dir, 'ws');
  git(dir, 'init', '-q', '--bare', origin);
  git(dir, 'init', '-q', ws);
  for (const [k, v] of [['user.name', 't'], ['user.email', 't@example.com'], ['commit.gpgsign', 'false']]) git(ws, 'config', k, v);
  fs.writeFileSync(path.join(ws, 'Cargo.toml'), '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2024"\n');
  fs.mkdirSync(path.join(ws, '.claude'));
  fs.writeFileSync(path.join(ws, '.claude', 'temper.json'), JSON.stringify(config));
  git(ws, 'add', '-A');
  git(ws, 'commit', '-q', '-m', 'init');
  git(ws, 'remote', 'add', 'origin', origin);
  return { ws, origin, proj: discover(ws) };
}

const log = (proj, sid, event, detail = {}) => telemetry(proj, { session_id: sid }, event, detail);
const branchFiles = (origin) => git(origin, 'ls-tree', '-r', '--name-only', 'temper-telemetry').split('\n').filter(Boolean);

test('push: off unless telemetry.branch is set', { skip: noGit }, (t) => {
  const { proj } = workspace(t, {});
  log(proj, 's1', 'session-start');
  assert.deepEqual(pushTelemetry(proj, 's1'), { pushed: false, why: 'telemetry.branch is not set' });
});

test('push: creates the branch with only this session\'s rows, and leaves the clone as it was', { skip: noGit }, (t) => {
  const { ws, origin, proj } = workspace(t);
  fs.writeFileSync(path.join(ws, 'staged.txt'), 'x');
  git(ws, 'add', 'staged.txt');
  fs.writeFileSync(path.join(ws, 'dirty.txt'), 'y');
  const before = { head: git(ws, 'rev-parse', 'HEAD'), status: git(ws, 'status', '--porcelain'), branches: git(ws, 'branch', '--list') };
  log(proj, 's1', 'session-start');
  log(proj, 'other', 'session-start');
  log(proj, 's1', 'stop-green', { fp: 'abc', ms: 1200, cached: false });
  const res = pushTelemetry(proj, 's1');
  assert.equal(res.pushed, true, res.why);
  assert.equal(res.rows, 2);
  assert.match(res.file, /^sessions\/\d{4}-\d{2}-\d{2}-s1\.jsonl$/);
  assert.deepEqual(branchFiles(origin), [res.file]);
  const rows = git(origin, 'show', `temper-telemetry:${res.file}`).split('\n').map((l) => JSON.parse(l));
  assert.deepEqual(rows.map((r) => r.event), ['session-start', 'stop-green']);
  assert.match(git(origin, 'log', '-1', '--format=%s', 'temper-telemetry'), /\[skip ci\]/);
  assert.deepEqual({ head: git(ws, 'rev-parse', 'HEAD'), status: git(ws, 'status', '--porcelain'), branches: git(ws, 'branch', '--list') }, before);
});

test('push: unchanged rows are not pushed again; new rows rewrite the session file', { skip: noGit }, (t) => {
  const { origin, proj } = workspace(t);
  log(proj, 's1', 'session-start');
  assert.equal(pushTelemetry(proj, 's1').pushed, true);
  assert.deepEqual(pushTelemetry(proj, 's1'), { pushed: false, why: 'unchanged since the last push' });
  log(proj, 's1', 'verdict', { verdict: 'approved' });
  const res = pushTelemetry(proj, 's1');
  assert.equal(res.pushed, true, res.why);
  assert.equal(git(origin, 'show', `temper-telemetry:${res.file}`).split('\n').length, 2);
  assert.equal(git(origin, 'rev-list', '--count', 'temper-telemetry'), '2');
});

test('push: sessions from separate clones land side by side; a cloud session is named by its cloud id', { skip: noGit }, (t) => {
  const a = workspace(t);
  const b = path.join(path.dirname(a.ws), 'ws2');
  git(path.dirname(a.ws), 'clone', '-q', a.origin, b);
  // the clone has no commits on origin yet; give it the same project files
  fs.cpSync(path.join(a.ws, '.claude'), path.join(b, '.claude'), { recursive: true });
  fs.copyFileSync(path.join(a.ws, 'Cargo.toml'), path.join(b, 'Cargo.toml'));
  const projB = discover(b);
  log(a.proj, 's1', 'session-start');
  assert.equal(pushTelemetry(a.proj, 's1').pushed, true);
  process.env.CLAUDE_CODE_REMOTE_SESSION_ID = 'cse_01ABC';
  t.after(() => { delete process.env.CLAUDE_CODE_REMOTE_SESSION_ID; });
  log(projB, 's2', 'session-start');
  // b fetches the tip a pushed and adds its own file next to it
  const res = pushTelemetry(projB, 's2');
  assert.equal(res.pushed, true, res.why);
  assert.match(res.file, /-cse_01ABC\.jsonl$/);
  assert.equal(branchFiles(a.origin).length, 2);
  const rows = fetchTelemetryBranch(a.proj, 'temper-telemetry');
  assert.deepEqual(rows.map((r) => r.sid).sort(), ['s1', 's2']);
  assert.equal(rows.find((r) => r.sid === 's2').ccr, 'cse_01ABC');
});

test('push: an unreachable origin reports why and records nothing as pushed', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  git(ws, 'remote', 'set-url', 'origin', path.join(ws, 'no-such-remote.git'));
  log(proj, 's1', 'session-start');
  const res = pushTelemetry(proj, 's1', { attempts: 1 });
  assert.equal(res.pushed, false);
  assert.ok(res.why.length > 0);
  // fixing the remote later pushes, because nothing was marked as pushed
  git(ws, 'remote', 'set-url', 'origin', path.join(path.dirname(ws), 'origin.git'));
  assert.equal(pushTelemetry(proj, 's1').pushed, true);
});

test('stats --branch: sessions, gate latency, cut-short gates and review rounds across pushed sessions', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  log(proj, 's1', 'session-start');
  log(proj, 's1', 'gate-start');
  log(proj, 's1', 'stop-blocked', { reason: 'no-review', fp: 'a', ms: 40000, cached: false });
  log(proj, 's1', 'verdict', { verdict: 'changes-requested' });
  log(proj, 's1', 'verdict', { verdict: 'approved' });
  log(proj, 's1', 'gate-start');
  log(proj, 's1', 'stop-green', { fp: 'a', ms: 5, cached: true });
  log(proj, 's2', 'session-start');
  log(proj, 's2', 'gate-start'); // killed: no outcome follows
  log(proj, 's2', 'gate-start');
  log(proj, 's2', 'stop-green', { fp: 'b', ms: 90000, cached: false });
  log(proj, 's2', 'verdict', { verdict: 'approved' });
  assert.equal(pushTelemetry(proj, 's1').pushed, true);
  assert.equal(pushTelemetry(proj, 's2').pushed, true);
  // a fresh clone with no local telemetry sees both sessions through the branch
  fs.rmSync(path.join(proj.stateDir, 'telemetry.jsonl'));
  const r = spawnSync(process.execPath, [CLI, 'stats', '--branch'], { cwd: ws, encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /^sessions: 2$/m);
  assert.match(r.stdout, /gate at Stop: 2 run\(s\), 1 cached; p50 90\.0 s, p90 90\.0 s, max 90\.0 s; 1 cut short/);
  assert.match(r.stdout, /review loop: 2 session\(s\) with verdicts; rounds per session p50 2, max 2; approved on the first verdict in 1/);
});

test('telemetry push CLI: usage without a session, a reason when off', { skip: noGit }, (t) => {
  const { ws } = workspace(t, {});
  const usage = spawnSync(process.execPath, [CLI, 'telemetry', 'push'], { cwd: ws, encoding: 'utf8' });
  assert.equal(usage.status, 2);
  const off = spawnSync(process.execPath, [CLI, 'telemetry', 'push', '--session', 's1'], { cwd: ws, encoding: 'utf8' });
  assert.equal(off.status, 0);
  assert.match(off.stdout, /not pushed: telemetry\.branch is not set/);
});

test('hook stop: hands the session\'s telemetry to a detached push that lands on the branch', { skip: noGit }, async (t) => {
  const { ws, origin } = workspace(t);
  const plug = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-plugdata-'));
  t.after(() => fs.rmSync(plug, { recursive: true, force: true }));
  const env = { ...process.env, CLAUDE_PLUGIN_DATA: plug, CLAUDE_ENV_FILE: '' };
  const hook = (event, input) => spawnSync(process.execPath, [CLI, 'hook', event], { cwd: ws, env, input: JSON.stringify({ cwd: ws, session_id: 'h1', ...input }), encoding: 'utf8' });
  assert.equal(hook('session-start', { source: 'startup', hook_event_name: 'SessionStart' }).status, 0);
  assert.equal(hook('stop', { hook_event_name: 'Stop' }).status, 0);
  let files = [];
  for (let i = 0; i < 100 && !files.length; i += 1) {
    await new Promise((done) => { setTimeout(done, 100); });
    try { files = branchFiles(origin); } catch { /* the branch is not there yet */ }
  }
  assert.equal(files.length, 1);
  assert.match(git(origin, 'show', `temper-telemetry:${files[0]}`), /"event":"session-start"/);
});
