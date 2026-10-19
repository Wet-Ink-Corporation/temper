// `temper status --json` and `temper stats --json`: the versioned documents a reader such as
// temper lens uses instead of parsing the state dir. The text output of both commands is
// covered elsewhere and must not change.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { discover, fingerprint, headRev, sessionDir, telemetry } from '../scripts/lib/project.mjs';
import { temperVersion } from '../scripts/lib/vendor.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const CLI = path.join(here, '..', 'scripts', 'temper.mjs');
const hasGit = spawnSync('git', ['--version'], { encoding: 'utf8' }).status === 0;
const noGit = !hasGit && 'git not installed';

const git = (cwd, ...args) => execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();

/** A git repository holding one crate, with src/lib.rs changed after the first commit. */
function workspace(t) {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-status-'));
  t.after(() => fs.rmSync(ws, { recursive: true, force: true }));
  git(ws, 'init', '-q');
  for (const [k, v] of [['user.name', 't'], ['user.email', 't@example.com'], ['commit.gpgsign', 'false']]) git(ws, 'config', k, v);
  fs.writeFileSync(path.join(ws, 'Cargo.toml'), '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2024"\n');
  fs.mkdirSync(path.join(ws, 'src'));
  fs.writeFileSync(path.join(ws, 'src', 'lib.rs'), 'pub fn a() {}\n');
  git(ws, 'add', '-A');
  git(ws, 'commit', '-q', '-m', 'init');
  fs.writeFileSync(path.join(ws, 'src', 'lib.rs'), 'pub fn a() {}\npub fn b() {}\n');
  return { ws, proj: discover(ws) };
}

/** Runs the CLI in `cwd` with no TEMPER_BASE or TEMPER_DISABLE from the test's own environment. */
function cli(cwd, args, env = {}) {
  const base = { ...process.env };
  delete base.TEMPER_BASE;
  delete base.TEMPER_DISABLE;
  return spawnSync(process.execPath, [CLI, ...args], { cwd, encoding: 'utf8', env: { ...base, ...env } });
}

function json(cwd, args, env) {
  const r = cli(cwd, args, env);
  assert.equal(r.status, 0, r.stderr);
  return JSON.parse(r.stdout);
}

const put = (proj, rel, value) => {
  const p = path.join(proj.stateDir, ...rel.split('/'));
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, typeof value === 'string' ? value : JSON.stringify(value));
};

const SCORES = {
  correctness: 4, escape_hatches: 4, ownership: 3, errors: 3, api: 3, architecture: 'n/a',
  tests: 3, performance: 'n/a', concurrency: 'n/a', observability: 'n/a', craft: 4, docs: 3,
};

test('status --json: a green gate and an approved verdict for the current tree', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  const fp = fingerprint(proj, headRev(proj));
  put(proj, `gate/${fp}.json`, {
    ok: true, fp, at: '2026-10-11T10:00:00.000Z',
    steps: [{ name: 'fmt', ok: true, ms: 120, summary: 'applied' }, { name: 'clippy', ok: true, ms: 9000, summary: 'clean', log: '/x/clippy.log' }],
  });
  put(proj, `verdicts/${fp}.json`, {
    temper_verdict: 1, verdict: 'approved', fingerprint: fp, scores: SCORES, recorded_at: '2026-10-11T10:05:00.000Z', stale: false,
    findings: [{ severity: 'nit', rule: 'API-01', file: 'src/lib.rs', line: 2, evidence: 'pub fn b() {}', fix: 'document b' }],
    evidence: { files_read: ['src/lib.rs'], commands_run: ['temper gate'] },
  });
  const doc = json(ws, ['status', '--json']);
  assert.deepEqual(doc, {
    temper_status: 1,
    version: temperVersion(),
    root: proj.root,
    stateDir: proj.stateDir,
    base: headRev(proj),
    fingerprint: fp,
    changed: ['src/lib.rs'],
    gate: {
      ok: true, unavailable: false, at: '2026-10-11T10:00:00.000Z',
      steps: [{ name: 'fmt', ok: true, ms: 120, summary: 'applied' }, { name: 'clippy', ok: true, ms: 9000, summary: 'clean' }],
      running: null,
    },
    review: {
      verdict: 'approved', stale: false, fingerprint: fp, recorded_at: '2026-10-11T10:05:00.000Z', scores: SCORES,
      findings: [{ severity: 'nit', rule: 'API-01', file: 'src/lib.rs', line: 2, fix: 'document b' }],
    },
    baseline: null,
    config: { review: proj.config.review, skillGate: proj.config.skillGate },
  });
});

test('status --json: a red gate, a stale verdict and a readable baseline', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  fs.mkdirSync(path.join(ws, '.claude'), { recursive: true });
  fs.writeFileSync(path.join(ws, '.claude', 'temper-baseline.json'), JSON.stringify({ temper_baseline: 1, toolchain: '1.90.0', totals: { a: { 'clippy::x': 2, 'clippy::y': 3 } } }));
  const fp = fingerprint(proj, headRev(proj));
  put(proj, `gate/${fp}.json`, { ok: false, fp, at: '2026-10-11T10:00:00.000Z', steps: [{ name: 'clippy', ok: false, ms: 50, summary: 'error: unused' }] });
  put(proj, `verdicts/${fp}.json`, {
    temper_verdict: 1, verdict: 'changes-requested', fingerprint: 'ffffffffffffffff', scores: SCORES, recorded_at: '2026-10-11T10:05:00.000Z', stale: true,
    findings: [{ severity: 'major', rule: 'ERR-02', file: 'src/lib.rs', evidence: 'x', fix: 'type the error' }],
  });
  const doc = json(ws, ['status', '--json']);
  assert.equal(doc.gate.ok, false);
  assert.deepEqual(doc.gate.steps, [{ name: 'clippy', ok: false, ms: 50, summary: 'error: unused' }]);
  assert.equal(doc.review.verdict, 'changes-requested');
  assert.equal(doc.review.stale, true);
  assert.equal(doc.review.fingerprint, 'ffffffffffffffff');
  assert.deepEqual(doc.review.findings, [{ severity: 'major', rule: 'ERR-02', file: 'src/lib.rs', line: null, fix: 'type the error' }]);
  assert.deepEqual(doc.baseline, { path: '.claude/temper-baseline.json', total: 5, toolchain: '1.90.0' });
});

test('status --json: no gate and no review for the tree are null; an unreadable baseline is an error object', { skip: noGit }, (t) => {
  const { ws } = workspace(t);
  fs.mkdirSync(path.join(ws, '.claude'), { recursive: true });
  fs.writeFileSync(path.join(ws, '.claude', 'temper-baseline.json'), '{ not json');
  const doc = json(ws, ['status', '--json']);
  assert.equal(doc.gate, null);
  assert.equal(doc.review, null);
  assert.equal(typeof doc.baseline.error, 'string');
  assert.equal('disabled' in doc, false);
});

test('status --json: malformed gate and verdict files read as none, or lose their bad entries', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  const fp = fingerprint(proj, headRev(proj));
  put(proj, `gate/${fp}.json`, [1]);
  put(proj, `verdicts/${fp}.json`, 7);
  let doc = json(ws, ['status', '--json']);
  assert.equal(doc.gate, null);
  assert.equal(doc.review, null);
  put(proj, `gate/${fp}.json`, { ok: false, steps: [null, { name: 'clippy', ok: false, ms: 1, summary: 'x' }] });
  put(proj, `verdicts/${fp}.json`, { verdict: 'approved', findings: [null, 3] });
  doc = json(ws, ['status', '--json']);
  assert.deepEqual(doc.gate.steps, [{ name: 'clippy', ok: false, ms: 1, summary: 'x' }]);
  assert.deepEqual(doc.review.findings, []);
  put(proj, `gate/${fp}.json`, { ok: true, steps: {} });
  assert.deepEqual(json(ws, ['status', '--json']).gate.steps, []);
});

test('status --json: outside a Rust project it says project: null and exits 0', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-norust-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const r = cli(dir, ['status', '--json']);
  assert.equal(r.status, 0);
  assert.equal(r.stderr, '');
  assert.deepEqual(JSON.parse(r.stdout), { temper_status: 1, version: temperVersion(), project: null });
  // The text form keeps its error and exit code.
  assert.equal(cli(dir, ['status']).status, 2);
});

test('status --json: TEMPER_DISABLE=1 marks the document disabled', { skip: noGit }, (t) => {
  const { ws } = workspace(t);
  const doc = json(ws, ['status', '--json'], { TEMPER_DISABLE: '1' });
  assert.equal(doc.disabled, true);
  assert.equal(doc.temper_status, 1);
});

test('status --json --session: the session\'s recorded base wins over HEAD', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  const first = headRev(proj);
  git(ws, 'add', '-A');
  git(ws, 'commit', '-q', '-m', 'b');
  const head = headRev(proj);
  put(proj, path.relative(proj.stateDir, path.join(sessionDir(proj, 's1'), 'base')).split(path.sep).join('/'), first);
  const plain = json(ws, ['status', '--json']);
  assert.equal(plain.base, head);
  assert.deepEqual(plain.changed, []);
  const session = json(ws, ['status', '--json', '--session', 's1']);
  assert.equal(session.base, first);
  assert.deepEqual(session.changed, ['src/lib.rs']);
  assert.equal(session.fingerprint, fingerprint(proj, first));
  // The text form takes --session too; an unknown session falls back to HEAD.
  assert.match(cli(ws, ['status', '--session', 's1']).stdout, new RegExp(`^base: +${first}$`, 'm'));
  assert.equal(json(ws, ['status', '--json', '--session', 'nobody']).base, head);
  assert.equal(cli(ws, ['status', '--session']).status, 2);
});

test('status: the text output keeps its lines', { skip: noGit }, (t) => {
  const { ws } = workspace(t);
  const r = cli(ws, ['status']);
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(r.stdout.split('\n').filter(Boolean).map((l) => l.split(':')[0]),
    ['root', 'state', 'base', 'fingerprint', 'changed', 'gate', 'review', 'baseline', 'config']);
});

test('stats --json: the counts stats prints, and with --session that session\'s rows', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  const log = (sid, event, detail = {}, agent) => telemetry(proj, { session_id: sid, ...(agent ? { agent_type: agent } : {}) }, event, detail);
  log('s1', 'session-start');
  log('s1', 'deny-scan', { file: 'src/lib.rs', rules: ['ERR-01', 'UNS-01'] });
  log('s1', 'deny-scan', { file: 'src/lib.rs', rules: ['ERR-01'] });
  log('s1', 'gate-start');
  log('s1', 'stop-green', { ms: 3000, cached: false });
  log('s1', 'verdict', { via: 'handback', verdict: 'approved' }, 'temper:rust-reviewer');
  log('s2', 'session-start');
  log('s2', 'gate-start');
  const doc = json(ws, ['stats', '--json', '--session', 's1']);
  assert.equal(doc.temper_stats, 1);
  assert.equal(doc.sessions, 2);
  assert.deepEqual(doc.events, { 'session-start': 2, 'deny-scan': 2, 'gate-start': 2, 'stop-green': 1, verdict: 1 });
  assert.deepEqual(Object.keys(doc.events).slice(0, 3), ['session-start', 'deny-scan', 'gate-start']);
  assert.deepEqual(doc.rules, { 'ERR-01': 2, 'UNS-01': 1 });
  assert.deepEqual(doc.gate, { runs: 1, cached: 0, cutShort: 1, p50Ms: 3000, p90Ms: 3000, maxMs: 3000, recentMs: [3000] });
  assert.deepEqual(doc.review, {
    sessions: 1, roundsP50: 1, roundsMax: 1, firstApproved: 1,
    changes: { count: 1, approved: 1, neverApproved: 0, firstRound: 1, roundsP50: 1, roundsMax: 1 },
  });
  assert.equal(doc.session.id, 's1');
  assert.deepEqual(doc.session.rows.map((r) => [r.event, r.agent, r.file, r.rules, r.via]), [
    ['session-start', 'main', null, [], null],
    ['deny-scan', 'main', 'src/lib.rs', ['ERR-01', 'UNS-01'], null],
    ['deny-scan', 'main', 'src/lib.rs', ['ERR-01'], null],
    ['gate-start', 'main', null, [], null],
    ['stop-green', 'main', null, [], null],
    ['verdict', 'temper:rust-reviewer', null, [], 'handback'],
  ]);
  assert.ok(doc.session.rows.every((r) => typeof r.t === 'string'));
  assert.equal('session' in json(ws, ['stats', '--json']), false);
  // The text summary is unchanged by the refactor.
  const text = cli(ws, ['stats']).stdout;
  assert.match(text, /^sessions: 2$/m);
  assert.match(text, /^ {2}ERR-01 {3}2$/m);
  assert.match(text, /^gate at Stop: 1 run\(s\), 0 cached; p50 3\.0 s, p90 3\.0 s, max 3\.0 s; 1 cut short \(hook timeout\?\)$/m);
  assert.match(text, /^review loop: 1 session\(s\) with verdicts; rounds per session p50 1, max 1; approved on the first verdict in 1$/m);
});

test('stats: a verdict recorded at hand-back and again at SubagentStop counts once', { skip: noGit }, (t) => {
  // In a cloud session the reviewer's verdict is recorded by the SubagentHandback hook and again
  // by SubagentStop, as two telemetry rows. They are one review round.
  const { ws, proj } = workspace(t);
  const log = (sid, detail) => telemetry(proj, { session_id: sid, agent_type: 'temper:rust-reviewer' }, 'verdict', detail);
  log('s1', { via: 'handback', verdict: 'changes-requested', fp: 'a' });
  log('s1', { via: 'subagent-stop', verdict: 'changes-requested', fp: 'a' });
  log('s1', { via: 'handback', verdict: 'approved', fp: 'b' });
  log('s1', { via: 'subagent-stop', verdict: 'approved', fp: 'b' });
  // A second review of the same tree with the same verdict, both at hand-back, is a second round.
  log('s1', { via: 'handback', verdict: 'approved', fp: 'b' });
  // A SubagentStop row with no hand-back before it (a terminal session) is a round of its own.
  log('s2', { via: 'subagent-stop', verdict: 'approved', fp: 'c' });
  // A SubagentStop row that differs from the hand-back before it in tree or verdict is kept.
  log('s3', { via: 'handback', verdict: 'approved', fp: 'd' });
  log('s3', { via: 'subagent-stop', verdict: 'approved', fp: 'e' });
  log('s3', { via: 'handback', verdict: 'approved', fp: 'e' });
  log('s3', { via: 'subagent-stop', verdict: 'changes-requested', fp: 'e' });
  const doc = json(ws, ['stats', '--json']);
  assert.equal(doc.events.verdict, 10);
  // Changes: s1 [a, b] approved in 2 (the repeat on b is not a new change); s2 [c] in 1; s3 [d] in
  // 1, [e] in 1, and the changes-requested on e after its approval opens a change never approved.
  assert.deepEqual(doc.review, {
    sessions: 3, roundsP50: 3, roundsMax: 4, firstApproved: 2,
    changes: { count: 5, approved: 4, neverApproved: 1, firstRound: 3, roundsP50: 1, roundsMax: 2 },
  });
  assert.match(cli(ws, ['stats']).stdout, /^review loop: 3 session\(s\) with verdicts; rounds per session p50 3, max 4; approved on the first verdict in 2$/m);
});

test('stats: review rounds per change, from the first verdict to the approval that closes it', { skip: noGit }, (t) => {
  // A change is a session's run of review rounds up to and including the first approval of a
  // current tree. A stale approval (recorded for a tree that has changed since) does not close it.
  const { ws, proj } = workspace(t);
  const log = (sid, verdict, fp, extra = {}) => telemetry(proj, { session_id: sid, agent_type: 'temper:rust-reviewer' }, 'verdict', { via: 'handback', verdict, fp, ...extra });
  log('s1', 'changes-requested', 'a');
  log('s1', 'changes-requested', 'b');
  log('s1', 'approved', 'c'); // change 1: 3 rounds
  log('s1', 'approved', 'c'); // the approved tree reviewed again: not a new change
  log('s1', 'changes-requested', 'd');
  log('s1', 'approved', 'e'); // change 2: 2 rounds
  log('s2', 'approved', 'f', { stale: true });
  log('s2', 'approved', 'g'); // change 3: 2 rounds, the stale approval did not close it
  log('s3', 'changes-requested', 'h');
  log('s3', 'changes-requested', 'i'); // change 4: never approved
  log('s4', 'approved', 'j');
  telemetry(proj, { session_id: 's4', agent_type: 'temper:rust-reviewer' }, 'verdict', { via: 'subagent-stop', verdict: 'approved', fp: 'j' });
  // change 5: 1 round; the SubagentStop echo of the hand-back is the same round
  telemetry(proj, { session_id: 's4' }, 'stop-green', { fp: 'j' });
  const doc = json(ws, ['stats', '--json']);
  assert.deepEqual(doc.review.changes, { count: 5, approved: 4, neverApproved: 1, firstRound: 1, roundsP50: 2, roundsMax: 3 });
  assert.match(cli(ws, ['stats']).stdout, /^review rounds per change: p50 2, max 3 \(4 approved, 1 on the first round\); 1 never approved$/m);
});

test('stats: review rounds per change when no change was approved', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  telemetry(proj, { session_id: 's1', agent_type: 'temper:rust-reviewer' }, 'verdict', { via: 'handback', verdict: 'changes-requested', fp: 'a' });
  const doc = json(ws, ['stats', '--json']);
  assert.deepEqual(doc.review.changes, { count: 1, approved: 0, neverApproved: 1, firstRound: 0, roundsP50: null, roundsMax: null });
  assert.match(cli(ws, ['stats']).stdout, /^review rounds per change: none approved; 1 never approved$/m);
});

test('stats --json: empty telemetry, and no Rust project', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-norust-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  assert.deepEqual(json(dir, ['stats', '--json']), { temper_stats: 1, project: null });
  if (!hasGit) return;
  const { ws } = workspace(t);
  assert.deepEqual(json(ws, ['stats', '--json']), {
    temper_stats: 1, sessions: 0, from: null, to: null, events: {}, rules: {},
    gate: { runs: 0, cached: 0, cutShort: 0, p50Ms: null, p90Ms: null, maxMs: null, recentMs: [] },
    review: {
      sessions: 0, roundsP50: null, roundsMax: null, firstApproved: 0,
      changes: { count: 0, approved: 0, neverApproved: 0, firstRound: 0, roundsP50: null, roundsMax: null },
    },
  });
  assert.doesNotMatch(cli(ws, ['stats']).stdout, /review rounds per change/);
  assert.equal(cli(ws, ['stats', '--session']).status, 2);
});
