// What the telemetry rows carry beyond the event name (#137, #138): shell deny rows keep the
// command, clipped and with likely secrets redacted; verdict-invalid rows name the problem kinds;
// verdict rows carry the reviewer's agent_id; and a gate run whose hook was killed gets a
// gate-lost row from the next hook in the session, with why and the step it was on.
// `temper stats` reports these additively.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { preBash, preHandback, sessionStart, hookTimeoutMs, lostGateRow, openGateStarts, stepTimes, startHeartbeat, heartbeatPath } from '../scripts/lib/hooks.mjs';
import { discover, fingerprint, gateProgressPath, readTelemetry, redactCommand, sessionTelemetry, telemetry, writeAtomic } from '../scripts/lib/project.mjs';
import { DIMENSIONS, problemKinds, validateVerdict, verdictProblems } from '../scripts/lib/verdict.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const CLI = path.join(here, '..', 'scripts', 'temper.mjs');
const noGit = spawnSync('git', ['--version'], { encoding: 'utf8' }).status !== 0 && 'git not installed';

/** A git repository holding one crate, with src/lib.rs changed after the first commit. */
function workspace(t) {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-telemetry-'));
  t.after(() => fs.rmSync(ws, { recursive: true, force: true }));
  const git = (...args) => execFileSync('git', ['-c', 'user.email=t@example.com', '-c', 'user.name=t', '-c', 'commit.gpgsign=false', ...args], { cwd: ws, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  fs.writeFileSync(path.join(ws, 'Cargo.toml'), '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2024"\n');
  fs.mkdirSync(path.join(ws, 'src'));
  fs.writeFileSync(path.join(ws, 'src', 'lib.rs'), 'pub fn a() {}\n');
  git('init', '-q');
  git('add', '-A');
  git('commit', '-qm', 'init');
  fs.writeFileSync(path.join(ws, 'src', 'lib.rs'), 'pub fn a() {}\npub fn b() {}\n');
  const proj = discover(ws);
  return { ws, proj, fp: () => fingerprint(proj, git('rev-parse', 'HEAD')) };
}

/** The pid of a process that has ended. */
const deadPid = () => spawnSync(process.execPath, ['-e', '']).pid;

// ------------------------------------------------------------- redactCommand

test('redactCommand: likely secrets become ***, and the rest of the command stays readable', () => {
  const cases = [
    ['gh api --token ghp_abcdefghijklmnopqrstuvwx repos', 'gh api --token *** repos'],
    ['npm publish --password=hunter2 --otp 123', 'npm publish --password=*** --otp 123'],
    ['curl -H "Authorization: Bearer abc.def" https://x', 'curl -H "Authorization: ***" https://x'],
    ["curl -H 'X-Api-Key: k1' https://u:p4ss@host/x", "curl -H 'X-Api-Key: ***' https://***@host/x"],
    ['git push https://oauth2:tok@github.com/o/r', 'git push https://***@github.com/o/r'],
    ["GH_TOKEN=abc123 cmd; export API_KEY='s e'; PATH=/bin ls", 'GH_TOKEN=*** cmd; export API_KEY=***; PATH=/bin ls'],
    ['DB_PASSWORD="a b" AWS_SECRET_ACCESS_KEY=x run', 'DB_PASSWORD=*** AWS_SECRET_ACCESS_KEY=*** run'],
    ['curl -d {"api_key":"zz"} x', 'curl -d {"api_key":"***"} x'],
    ['echo sk-abcdefghijklmnopqrstuvwxyz0123 xoxb-1234567890-abc AKIAABCDEFGHIJKLMNOP', 'echo *** *** ***'],
    // Names that only contain a secret-looking word are left alone.
    ['GIT_AUTHOR_NAME=me git commit --author me -m x', 'GIT_AUTHOR_NAME=me git commit --author me -m x'],
    ['cat > src/lib.rs <<EOF\nfn a() {}\nEOF', 'cat > src/lib.rs <<EOF\nfn a() {}\nEOF'],
  ];
  for (const [cmd, want] of cases) assert.equal(redactCommand(cmd), want, cmd);
});

test('redactCommand: clipped to 200 characters after redaction, so a cut secret is still removed', () => {
  const long = `${'x'.repeat(180)} GH_TOKEN=${'s'.repeat(40)}`;
  assert.equal(redactCommand(long), `${'x'.repeat(180)} GH_TOKEN=***`);
  assert.equal(redactCommand('y'.repeat(300)), `${'y'.repeat(200)}…`);
  assert.equal(redactCommand('y'.repeat(300), 10), `${'y'.repeat(10)}…`);
  assert.equal(redactCommand(undefined), '');
});

// ---------------------------------------------------------- verdict problems

test('verdictProblems: a stable kind per problem, with the same messages validateVerdict gives', () => {
  const v = {
    temper_verdict: 1, verdict: 'approved', fingerprint: 'nothex', findings: [{ rule: 'X', severity: 'major' }],
    scores: { ...Object.fromEntries(DIMENSIONS.map((d) => [d, 4])), craft: 'n/a', docs: 7 },
    evidence: { files_read: [], commands_run: ['gate'] }, gate: { clippy: 'fail' },
  };
  const problems = verdictProblems(v, { changedRust: ['src/lib.rs'] });
  assert.deepEqual(problems.map((x) => x.message), validateVerdict(v, { changedRust: ['src/lib.rs'] }));
  assert.deepEqual(problemKinds(problems), [
    'fingerprint', 'finding-rule', 'finding-file', 'finding-evidence', 'finding-fix', 'score-range', 'score-na',
    'files-read', 'files-unreviewed', 'approved-with-findings', 'approved-gate-entry',
  ]);
  assert.deepEqual(problemKinds(verdictProblems(null)), ['no-block']);
  assert.deepEqual(problemKinds(verdictProblems({ __parseError: 'x' })), ['json-parse']);
  assert.deepEqual(problemKinds(verdictProblems({ ...v, findings: 'none', scores: null, gate: {} }, { gate: { ok: false } })),
    ['fingerprint', 'findings-array', 'scores-missing', 'files-read', 'approved-red-gate']);
});

// ------------------------------------------------------------------ gate-lost

test('hookTimeoutMs: the Stop and SubagentStop deadlines from hooks/hooks.json', () => {
  assert.equal(hookTimeoutMs('Stop'), 1200000);
  assert.equal(hookTimeoutMs('SubagentStop'), 1200000);
  assert.equal(hookTimeoutMs('NoSuchEvent'), null);
  assert.equal(hookTimeoutMs('Stop', os.tmpdir()), null);
});

test('lostGateRow: timeout when last seen alive near the deadline; interrupted when seen to stop short of it', () => {
  const t0 = Date.parse('2026-10-16T23:00:00.000Z');
  const at = (s) => new Date(t0 + s * 1000).toISOString();
  const start = { t: at(0), pid: 7, timeoutMs: 1200000 };
  const prog = { steps: [{ name: 'fmt', ok: true, ms: 900 }, { name: 'scan', ok: true, ms: 40 }], current: { name: 'clippy', startedAt: at(30) }, mtimeMs: t0 + 30000 };
  const beat = (s, pid = 7) => ({ pid, at: t0 + s * 1000 });
  // The heartbeat says how long the hook lived.
  assert.deepEqual(lostGateRow(start, prog, beat(1185), t0 + 5000000), {
    pid: 7, reason: 'timeout', ranMs: 1185000, foundMs: 5000000, timeoutMs: 1200000, heartbeat: true,
    progress: true, step: 'clippy', steps: { fmt: 900, scan: 40 },
  });
  const interrupted = lostGateRow(start, prog, beat(400), t0 + 5000000);
  assert.deepEqual([interrupted.reason, interrupted.ranMs], ['interrupted', 400000]);
  // Another process's heartbeat says nothing; then only the progress file and the time found count.
  const other = lostGateRow(start, prog, beat(1199, 8), t0 + 600000);
  assert.deepEqual([other.reason, other.ranMs, other.heartbeat], ['interrupted', 30000, false]);
  assert.equal(lostGateRow(start, prog, null, t0 + 1300000).reason, 'unknown');
  assert.equal(lostGateRow(start, { ...prog, mtimeMs: t0 + 1190000 }, null, t0 + 1300000).reason, 'timeout');
  // No progress file: it died formatting (or the file is gone); no deadline recorded: unknown.
  assert.deepEqual(lostGateRow(start, null, null, t0 + 1000), {
    pid: 7, reason: 'interrupted', ranMs: 0, foundMs: 1000, timeoutMs: 1200000, heartbeat: false, progress: false, step: null, steps: {},
  });
  assert.equal(lostGateRow({ t: at(0), pid: 7 }, null, beat(5), t0 + 1000).reason, 'unknown');
});

test('startHeartbeat: a worker thread keeps beating while the main thread waits on a child, and the file outlives a killed hook', { timeout: 30000 }, async (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-beat-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const proj = { stateDir: dir };
  const input = { session_id: 's-beat', agent_id: 'x1' };
  const script = path.join(dir, 'hook.mjs');
  fs.writeFileSync(script, [
    `import { spawnSync } from 'node:child_process';`,
    `import { startHeartbeat } from ${JSON.stringify(new URL('../scripts/lib/hooks.mjs', import.meta.url).href)};`,
    `startHeartbeat(${JSON.stringify(proj)}, ${JSON.stringify(input)}, 100);`,
    `spawnSync(process.execPath, ['-e', 'setTimeout(() => {}, 20000)']);`,
  ].join('\n'));
  const t0 = Date.now();
  const child = spawn(process.execPath, [script], { stdio: 'ignore' });
  const file = heartbeatPath(proj, input);
  assert.match(file, /gate-alive-agent-x1\.json$/);
  const read = () => { try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return null; } };
  while (!(read()?.at >= t0 + 1000) && Date.now() - t0 < 15000) await new Promise((r) => setTimeout(r, 100));
  const exited = new Promise((r) => child.on('exit', r));
  child.kill('SIGKILL');
  await exited;
  const killedAt = Date.now();
  const last = read();
  assert.equal(last?.pid, child.pid);
  assert.ok(last.at >= t0 + 1000 && last.at <= killedAt, `${last.at - t0}`);
  await new Promise((r) => setTimeout(r, 400));
  assert.equal(read().at, last.at, 'no beats after the process is gone');
});

test('stepTimes: a fresh gate\'s step times for its Stop row; none for a cached result', () => {
  const steps = [{ name: 'fmt', ok: true, ms: 800, summary: 'applied' }, { name: 'clippy', ok: false, ms: 64000, summary: 'x' }];
  assert.deepEqual(stepTimes({ ok: false, steps, cached: false }), { steps: { fmt: 800, clippy: 64000 } });
  assert.deepEqual(stepTimes({ ok: false, steps, cached: true }), {});
  assert.deepEqual(stepTimes({ unavailable: true }), {});
});

test('openGateStarts: a gate-start stays open until an outcome or gate-lost of the same agent and pid', () => {
  const rows = [
    { event: 'gate-start', agent: 'main', pid: 1 },
    { event: 'stop-green', agent: 'main', pid: 1 },
    { event: 'gate-start', agent: 'main', pid: 2 },
    { event: 'gate-start', agent: 'temper:rust-implementer', pid: 3 },
    { event: 'stop-blocked', agent: 'temper:rust-implementer', pid: 4 },
    { event: 'gate-start', agent: 'main', pid: 5 },
    { event: 'gate-lost', agent: 'main', pid: 5 },
    { event: 'gate-start', agent: 'main' },
    { event: 'stop-advisory-unreviewed', agent: 'main' },
  ];
  assert.deepEqual(openGateStarts(rows).map((r) => r.pid), [2, 3]);
});

test('SessionStart on resume logs gate-lost for the session\'s killed gates, once, with the step and heartbeat', { skip: noGit }, (t) => {
  const { ws, proj, fp } = workspace(t);
  const [main, sub] = [deadPid(), deadPid()];
  const t0 = Date.now() - 60000;
  const at = (ms) => new Date(t0 + ms).toISOString();
  const row = (r) => fs.appendFileSync(path.join(proj.stateDir, 'telemetry.jsonl'), `${JSON.stringify({ t: at(0), sid: 's-lost', agent: 'main', ...r })}\n`);
  fs.mkdirSync(proj.stateDir, { recursive: true });
  row({ event: 'gate-start', pid: main, timeoutMs: 1200000 });
  row({ event: 'gate-start', agent: 'temper:rust-implementer', agent_id: 'im1', pid: sub, timeoutMs: 1200000 });
  // A gate whose process still runs (this one) is not lost; nor is another session's.
  row({ event: 'gate-start', agent: 'temper:rust-implementer', agent_id: 'im2', pid: process.pid, timeoutMs: 1200000 });
  row({ event: 'gate-start', sid: 'other', pid: deadPid(), timeoutMs: 1200000 });
  // The main agent's gate left a progress file on the test step; the implementer's, a heartbeat.
  writeAtomic(gateProgressPath(proj, fp()), JSON.stringify({
    fp: fp(), pid: main, startedAt: at(0), steps: [{ name: 'fmt', ok: true, ms: 800 }], current: { name: 'test', startedAt: at(20000) },
  }));
  fs.utimesSync(gateProgressPath(proj, fp()), new Date(t0 + 20000), new Date(t0 + 20000));
  const beat = heartbeatPath(proj, { session_id: 's-lost', agent_id: 'im1' });
  writeAtomic(beat, JSON.stringify({ pid: sub, at: t0 + 45000 }));
  sessionStart({ cwd: ws, session_id: 's-lost', source: 'resume' });
  sessionStart({ cwd: ws, session_id: 's-lost', source: 'resume' });
  const lost = readTelemetry(proj).filter((r) => r.event === 'gate-lost');
  assert.deepEqual(lost.map((r) => [r.sid, r.agent, r.pid, r.reason, r.step, r.heartbeat, r.progress]), [
    ['s-lost', 'main', main, 'interrupted', 'test', false, true],
    ['s-lost', 'temper:rust-implementer', sub, 'interrupted', null, true, false],
  ]);
  assert.deepEqual(lost[0].steps, { fmt: 800 });
  assert.ok(lost[0].ranMs >= 19000 && lost[0].ranMs <= 21000, String(lost[0].ranMs));
  assert.equal(lost[1].ranMs, 45000);
  assert.deepEqual(sessionTelemetry(proj, 's-lost').map((r) => r.event),
    ['gate-start', 'gate-start', 'gate-start', 'session-start', 'gate-lost', 'gate-lost', 'session-start']);
  assert.deepEqual(sessionTelemetry(proj, undefined), []);
});

// ------------------------------------------------------- hook rows (#138)

test('shell deny rows carry the command, redacted and clipped', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  const bash = (command) => preBash({ tool_name: 'Bash', cwd: ws, session_id: 's-cmd', tool_input: { command } });
  const cmds = [
    `GH_TOKEN=s3cret cat > src/lib.rs <<'EOF'\npub fn x() {}\nEOF`,
    'echo x > Cargo.lock',
    `curl -H "Authorization: Bearer s3cret" https://example.com/${'p'.repeat(300)} > Cargo.toml`,
    'cargo test --ignore-rust-version',
  ];
  for (const c of cmds) assert.equal(bash(c).json?.hookSpecificOutput?.permissionDecision, 'deny', c);
  const rows = sessionTelemetry(proj, 's-cmd');
  assert.deepEqual(rows.map((r) => r.event), ['deny-shell-write', 'deny-lockfile', 'deny-shell-write', 'deny-ignore-rust-version']);
  assert.equal(rows[0].cmd, "GH_TOKEN=*** cat > src/lib.rs <<'EOF'\npub fn x() {}\nEOF");
  assert.equal(rows[1].cmd, 'echo x > Cargo.lock');
  assert.match(rows[2].cmd, /^curl -H "Authorization: \*\*\*" https:\/\/example\.com\/p+…$/);
  assert.equal(rows[2].cmd.length, 201);
  assert.equal(rows[3].cmd, 'cargo test --ignore-rust-version');
  assert.ok(rows.every((r) => !JSON.stringify(r).includes('s3cret')));
});

test('verdict-invalid rows name the problem kinds; verdict rows carry the reviewer\'s agent_id', { skip: noGit }, (t) => {
  const { ws, proj, fp } = workspace(t);
  const input = { cwd: ws, session_id: 's-kinds', tool_name: 'SubagentHandback', agent_type: 'temper:rust-reviewer', agent_id: 'rv7' };
  const verdict = (over = {}) => ({
    temper_verdict: 1, verdict: 'approved', fingerprint: fp(), findings: [],
    scores: Object.fromEntries(DIMENSIONS.map((d) => [d, 4])), gate: {},
    evidence: { files_read: ['src/lib.rs'], commands_run: ['gate'] }, ...over,
  });
  const say = (v) => preHandback({ ...input, tool_input: { message: `Done.\n\n\`\`\`json\n${JSON.stringify(v)}\n\`\`\`\n` } });
  const denied = say(verdict({ fingerprint: 'nothex', evidence: { files_read: [], commands_run: [] } }));
  // The deny message is the prose list, unchanged.
  assert.match(denied.json.hookSpecificOutput.permissionDecisionReason, /:\n- "fingerprint" must be the 16-hex fingerprint given in your review ticket\n- evidence\.files_read must list/);
  assert.deepEqual(say(verdict()), {});
  const rows = sessionTelemetry(proj, 's-kinds');
  const bad = rows.find((r) => r.event === 'verdict-invalid');
  assert.deepEqual([bad.problems, bad.kinds, bad.agent_id], [4, ['fingerprint', 'files-read', 'commands-run', 'files-unreviewed'], 'rv7']);
  assert.equal(rows.find((r) => r.event === 'verdict').agent_id, 'rv7');
});

// -------------------------------------------------------------- temper stats

test('stats: gates cut short by reason and step, step times, and verdicts once per reviewer and tree', { skip: noGit }, (t) => {
  const { ws, proj } = workspace(t);
  const log = (sid, event, detail = {}, agent) => telemetry(proj, { session_id: sid, ...(agent ? { agent_type: agent } : {}) }, event, detail);
  log('s1', 'gate-start', { pid: 1 });
  log('s1', 'gate-lost', { pid: 1, reason: 'timeout', step: 'test' });
  log('s1', 'gate-start', { pid: 2 });
  log('s1', 'gate-lost', { pid: 2, reason: 'interrupted', step: 'test' });
  log('s1', 'gate-start', { pid: 3 });
  log('s1', 'gate-lost', { pid: 3, reason: 'unknown', step: null, progress: false });
  log('s1', 'gate-start', { pid: 4 });
  log('s1', 'stop-green', { pid: 4, ms: 9000, cached: false, steps: { fmt: 1000, clippy: 5000, test: 3000 } });
  log('s1', 'stop-blocked', { pid: 5, ms: 4000, cached: false, steps: { fmt: 2000, clippy: 2000 } });
  log('s1', 'stop-green', { pid: 6, ms: 1, cached: true, steps: { fmt: 99999 } });
  // The same reviewer recording one verdict twice (hand-back, then SubagentStop) is one round,
  // even with a row between; a second reviewer on the same tree is another.
  const rv = 'temper:rust-reviewer';
  log('s1', 'verdict', { via: 'handback', agent_id: 'a1', fp: 'f1', verdict: 'changes-requested' }, rv);
  log('s1', 'subagent-stop', {}, rv);
  log('s1', 'verdict', { via: 'subagent-stop', agent_id: 'a1', fp: 'f1', verdict: 'changes-requested' }, rv);
  log('s1', 'verdict', { via: 'handback', agent_id: 'a2', fp: 'f1', verdict: 'approved' }, rv);
  const r = spawnSync(process.execPath, [CLI, 'stats', '--json'], { cwd: ws, encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  const doc = JSON.parse(r.stdout);
  assert.equal(doc.gate.cutShort, 3);
  assert.deepEqual(doc.gate.lost, { timeout: 1, interrupted: 1, unknown: 1 });
  assert.deepEqual(doc.gate.lostSteps, { test: 2, 'fmt or before': 1 });
  assert.deepEqual(doc.gate.stepP50Ms, { clippy: 5000, test: 3000, fmt: 2000 });
  assert.deepEqual(doc.review, { sessions: 1, roundsP50: 2, roundsMax: 2, firstApproved: 0 });
  const text = spawnSync(process.execPath, [CLI, 'stats'], { cwd: ws, encoding: 'utf8' }).stdout;
  assert.match(text, /; 3 cut short \(1 timeout, 1 interrupted, 1 unknown\)$/m);
  assert.match(text, /^gates cut short, by the step they were on:\n {2}test {23}2\n {2}fmt or before {14}1$/m);
  assert.match(text, /^gate step times at Stop \(p50\):\n {2}clippy {21}5\.0 s\n {2}test {23}3\.0 s\n {2}fmt {24}2\.0 s$/m);
});
