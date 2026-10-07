// Review verdicts (scripts/lib/verdict.mjs): extraction from the reviewer's
// final message, validation, and storage keyed by tree fingerprint.
//
// Audit finding engine-1-12 (docs/history/2026-10-audit/findings.json): a
// files_read entry covers a changed file only by whole path components (a bare
// suffix such as "lib.rs" does not), and every verdict rates at least
// MIN_APPLICABLE_SCORES dimensions with an integer score.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { DIMENSIONS, MIN_APPLICABLE_SCORES, extractVerdict, validateVerdict, recordVerdict, verdictFor, summarizeFindings } from '../scripts/lib/verdict.mjs';
import { discover, rustChanges } from '../scripts/lib/project.mjs';

const FP = '0123456789abcdef';
const noGit = spawnSync('git', ['--version'], { encoding: 'utf8' }).status === 0 ? false : 'git is not installed';

/** A well-formed approval; `over` replaces top-level fields. */
const verdict = (over = {}) => ({
  temper_verdict: 1,
  verdict: 'approved',
  fingerprint: FP,
  findings: [],
  scores: Object.fromEntries(DIMENSIONS.map((d) => [d, 4])),
  gate: { fmt: 'pass', clippy: 'pass', test: 'pass', doc: 'pass' },
  evidence: { files_read: ['crates/core/src/lib.rs'], commands_run: ['node temper.mjs gate'] },
  ...over,
});
const finding = (over = {}) => ({ rule: 'OWN-08', severity: 'minor', file: 'crates/core/src/lib.rs', line: 3, evidence: 'x.clone()', fix: 'borrow it', ...over });
const has = (problems, re) => assert.ok(problems.some((p) => re.test(p)), `expected a problem matching ${re}, got:\n${problems.join('\n')}`);

// ------------------------------------------------------------ extractVerdict

test('extractVerdict: null without text or without a temper_verdict block', () => {
  assert.equal(extractVerdict(undefined), null);
  assert.equal(extractVerdict(''), null);
  assert.equal(extractVerdict('All good.\n```json\n{"verdict":"approved"}\n```\n'), null);
});

test('extractVerdict: parses a fenced json block, with or without the language tag', () => {
  const v = verdict();
  assert.deepEqual(extractVerdict(`Review done.\n\n\`\`\`json\n${JSON.stringify(v, null, 2)}\n\`\`\`\n`), v);
  assert.deepEqual(extractVerdict(`\`\`\`\n${JSON.stringify(v)}\n\`\`\``), v);
});

test('extractVerdict: the last block containing temper_verdict wins', () => {
  const first = verdict({ verdict: 'changes-requested' });
  const last = verdict();
  const text = `Draft:\n\`\`\`json\n${JSON.stringify(first)}\n\`\`\`\nFinal:\n\`\`\`json\n${JSON.stringify(last)}\n\`\`\`\nOther:\n\`\`\`json\n{"x":1}\n\`\`\`\n`;
  assert.equal(extractVerdict(text).verdict, 'approved');
});

test('extractVerdict: JSON that does not parse is reported, not thrown', () => {
  const v = extractVerdict('```json\n{"temper_verdict": 1, "verdict": }\n```\n');
  assert.equal(typeof v.__parseError, 'string');
  const problems = validateVerdict(v);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /^verdict JSON does not parse: /);
});

// ----------------------------------------------------------- validateVerdict

test('validateVerdict: a well-formed approval has no problems', () => {
  assert.deepEqual(validateVerdict(verdict()), []);
  assert.deepEqual(validateVerdict(verdict(), { changedRust: ['crates/core/src/lib.rs'], gate: { ok: true } }), []);
  // minor and nit findings, a "none" nit, and an "n/a" dimension or gate entry are consistent with approval
  const scores = { ...verdict().scores, concurrency: 'n/a' };
  const v = verdict({ scores, gate: { fmt: 'pass', doc: 'n/a' }, findings: [finding(), finding({ rule: 'none', severity: 'nit' })] });
  assert.deepEqual(validateVerdict(v), []);
  // a changes-requested verdict may carry blockers and low scores
  const cr = verdict({ verdict: 'changes-requested', findings: [finding({ severity: 'blocker' })], scores: { ...verdict().scores, errors: 1 }, gate: { clippy: 'fail' } });
  assert.deepEqual(validateVerdict(cr, { gate: { ok: false } }), []);
});

test('validateVerdict: no verdict block', () => {
  const problems = validateVerdict(null);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /^no verdict block found: end the final message with a ```json block containing "temper_verdict": 1$/);
});

test('validateVerdict: header fields', () => {
  has(validateVerdict(verdict({ temper_verdict: 2 })), /^"temper_verdict" must be 1$/);
  has(validateVerdict(verdict({ verdict: 'lgtm' })), /^"verdict" must be "approved" or "changes-requested"$/);
  for (const fingerprint of [undefined, 'abc', '0123456789ABCDEF', '0123456789abcdef0', 1234]) {
    has(validateVerdict(verdict({ fingerprint })), /^"fingerprint" must be the 16-hex fingerprint/);
  }
  has(validateVerdict(verdict({ findings: 'none' })), /^"findings" must be an array \(empty if none\)$/);
});

test('validateVerdict: each finding needs a rule ID, a severity, a file, evidence and a fix', () => {
  // changes-requested: an approved verdict with a null finding throws in the approval checks (not pinned here)
  has(validateVerdict(verdict({ verdict: 'changes-requested', findings: [null] })), /^findings\[0\] must be an object$/);
  has(validateVerdict(verdict({ findings: [finding({ rule: 'own-8' })] })), /^findings\[0\]\.rule must be a rule ID like OWN-08/);
  has(validateVerdict(verdict({ findings: [finding({ rule: 'none' })] })), /^findings\[0\]\.rule must be a rule ID/);
  has(validateVerdict(verdict({ findings: [finding({ severity: 'critical' })] })), /^findings\[0\]\.severity must be one of blocker\/major\/minor\/nit$/);
  has(validateVerdict(verdict({ findings: [finding({ file: '' })] })), /^findings\[0\]\.file is required$/);
  has(validateVerdict(verdict({ findings: [finding(), finding({ evidence: undefined })] })), /^findings\[1\]\.evidence is required \(quote the code\)$/);
  has(validateVerdict(verdict({ findings: [finding({ fix: '' })] })), /^findings\[0\]\.fix is required$/);
});

test('validateVerdict: every dimension needs a score of 0-4 or "n/a"', () => {
  has(validateVerdict(verdict({ scores: undefined })), /^"scores" object is required$/);
  const { docs: _docs, ...missing } = verdict().scores;
  has(validateVerdict(verdict({ scores: missing })), /^scores\.docs is missing \(0-4 or "n\/a"\)$/);
  for (const bad of [5, -1, 2.5, '4', 'na']) {
    has(validateVerdict(verdict({ scores: { ...verdict().scores, api: bad } })), /^scores\.api must be 0-4 or "n\/a"$/);
  }
});

test('validateVerdict: evidence lists the files read and the commands run', () => {
  has(validateVerdict(verdict({ evidence: { files_read: [], commands_run: ['gate'] } })), /^evidence\.files_read must list the files you actually read$/);
  has(validateVerdict(verdict({ evidence: { files_read: ['a.rs'], commands_run: [] } })), /^evidence\.commands_run must list the commands you ran \(at least the gate\)$/);
  const problems = validateVerdict(verdict({ evidence: undefined }));
  has(problems, /^evidence\.files_read/);
  has(problems, /^evidence\.commands_run/);
});

test('validateVerdict: a changed .rs file missing from files_read is reported', () => {
  const ctx = { changedRust: ['crates/core/src/lib.rs', 'crates/core/src/new.rs', 'crates/core/Cargo.toml'] };
  const problems = validateVerdict(verdict(), ctx);
  assert.deepEqual(problems, ['changed files not reviewed: crates/core/src/new.rs']);
  // Windows separators in files_read compare equal
  const win = verdict({ evidence: { files_read: ['crates\\core\\src\\lib.rs', 'crates/core/src/new.rs'], commands_run: ['gate'] } });
  assert.deepEqual(validateVerdict(win, ctx), []);
});

test('validateVerdict: files_read covers a changed file by whole path components, not by a bare suffix', () => {
  const ctx = { changedRust: ['crates/core/src/lib.rs'] };
  const read = (...files_read) => validateVerdict(verdict({ evidence: { files_read, commands_run: ['gate'] } }), ctx);
  // refused: a suffix of the changed path, or a longer path that does not split at a "/"
  for (const entry of ['s', 'lib.rs', 'src/lib.rs', 'core/src/lib.rs', 'ore/src/lib.rs', 'xcrates/core/src/lib.rs', 'crates/core/src/lib', 'crates/core/src/lib.rs/', '']) {
    assert.deepEqual(read(entry), ['changed files not reviewed: crates/core/src/lib.rs'], `files_read ${JSON.stringify(entry)}`);
  }
  // accepted: the path itself (either separator, with or without ./), or a longer path ending in /<the path>
  for (const entry of [
    'crates/core/src/lib.rs', './crates/core/src/lib.rs', '.\\crates\\core\\src\\lib.rs', ' crates/core/src/lib.rs ',
    '/home/dev/proj/crates/core/src/lib.rs', 'C:\\Users\\dev\\proj\\crates\\core\\src\\lib.rs', 'C:/Users/dev/proj/crates/core/src/lib.rs',
    '\\\\?\\C:\\proj\\crates\\core\\src\\lib.rs', 'proj/crates/core/src/lib.rs',
  ]) {
    assert.deepEqual(read(entry), [], `files_read ${JSON.stringify(entry)}`);
  }
  // a crate at the repository root: the short path is the whole changed path
  const root = { changedRust: ['src/main.rs', 'build.rs'] };
  assert.deepEqual(validateVerdict(verdict({ evidence: { files_read: ['src/main.rs', '/repo/build.rs'], commands_run: ['gate'] } }), root), []);
  assert.deepEqual(validateVerdict(verdict({ evidence: { files_read: ['main.rs', 'build.rs'], commands_run: ['gate'] } }), root), ['changed files not reviewed: src/main.rs']);
  // one entry covers only its own file: "lib.rs" no longer covers every lib.rs in the workspace
  const two = { changedRust: ['crates/a/src/lib.rs', 'crates/b/src/lib.rs'] };
  assert.deepEqual(validateVerdict(verdict({ evidence: { files_read: ['crates/a/src/lib.rs', 'lib.rs'], commands_run: ['gate'] } }), two), ['changed files not reviewed: crates/b/src/lib.rs']);
});

test('validateVerdict: the paths SubagentStart lists, as given or absolute, cover the changed files', { skip: noGit }, (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-verdict-repo-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const put = (rel, text) => { const f = path.join(dir, ...rel.split('/')); fs.mkdirSync(path.dirname(f), { recursive: true }); fs.writeFileSync(f, text); };
  const git = (...args) => execFileSync('git', ['-c', 'user.email=t@example.com', '-c', 'user.name=t', '-c', 'commit.gpgsign=false', ...args], { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  put('Cargo.toml', '[workspace]\nmembers = ["crates/*"]\nresolver = "3"\n');
  put('crates/core/Cargo.toml', '[package]\nname = "core"\nversion = "0.1.0"\nedition = "2024"\n');
  put('crates/core/src/lib.rs', '//! Core.\n');
  git('init', '-q');
  git('add', '-A');
  git('commit', '-qm', 'base');
  put('crates/core/src/lib.rs', '//! Core, changed.\n');
  put('crates/core/src/new.rs', '//! New.\n');
  const proj = discover(dir);
  const changedRust = rustChanges(proj, git('rev-parse', 'HEAD')).filter((f) => f.endsWith('.rs'));
  assert.deepEqual([...changedRust].sort(), ['crates/core/src/lib.rs', 'crates/core/src/new.rs']);
  const check = (files_read) => validateVerdict(verdict({ evidence: { files_read, commands_run: ['gate'] } }), { changedRust });
  // the repo-relative paths SubagentStart prints, and the same files as native absolute paths
  assert.deepEqual(check(changedRust), []);
  assert.deepEqual(check(changedRust.map((f) => path.join(proj.root, ...f.split('/')))), []);
  assert.deepEqual(check(changedRust.map((f) => path.join(dir, ...f.split('/')))), []);
  has(check(['lib.rs', 'new.rs']), /^changed files not reviewed: crates\/core\/src\/(lib|new)\.rs, crates\/core\/src\/(lib|new)\.rs$/);
});

test(`validateVerdict: every verdict rates at least ${MIN_APPLICABLE_SCORES} dimensions with an integer score`, () => {
  assert.equal(MIN_APPLICABLE_SCORES, 3);
  const scored = (n, over = {}) => verdict({ scores: Object.fromEntries(DIMENSIONS.map((d, i) => [d, i < n ? 3 : 'n/a'])), ...over });
  const msg = (n) => `scores must rate at least ${MIN_APPLICABLE_SCORES} dimensions 0-4 ("n/a" only where a dimension does not apply); ${n} rated`;
  // refused: an approval with every score "n/a", or with fewer than the minimum rated
  assert.deepEqual(validateVerdict(scored(0)), [msg(0)]);
  assert.deepEqual(validateVerdict(scored(MIN_APPLICABLE_SCORES - 1)), [msg(MIN_APPLICABLE_SCORES - 1)]);
  // refused too for changes-requested: the scores are the review's evidence either way
  assert.deepEqual(validateVerdict(scored(0, { verdict: 'changes-requested' })), [msg(0)]);
  // accepted: exactly the minimum, for either verdict, and a 0 counts as rated
  assert.deepEqual(validateVerdict(scored(MIN_APPLICABLE_SCORES)), []);
  assert.deepEqual(validateVerdict(scored(MIN_APPLICABLE_SCORES, { verdict: 'changes-requested' })), []);
  const zero = { ...scored(MIN_APPLICABLE_SCORES).scores, correctness: 0 };
  assert.deepEqual(validateVerdict(verdict({ verdict: 'changes-requested', scores: zero })), []);
  // a missing scores object reports that problem, not the minimum as well
  assert.ok(!validateVerdict(verdict({ scores: undefined })).some((p) => p.startsWith('scores must rate')));
});

test('validateVerdict: "approved" must agree with the findings, the scores and the gate', () => {
  has(validateVerdict(verdict({ findings: [finding({ severity: 'major' }), finding({ severity: 'blocker' })] })), /^"approved" is inconsistent with 2 blocker\/major finding\(s\): use "changes-requested"$/);
  has(validateVerdict(verdict({ scores: { ...verdict().scores, tests: 2, docs: 0 } })), /^"approved" requires every applicable dimension >= 3; low: tests, docs$/);
  has(validateVerdict(verdict(), { gate: { ok: false } }), /^"approved" is impossible: the gate is red for this fingerprint$/);
  has(validateVerdict(verdict({ gate: { fmt: 'pass', clippy: 'fail' } })), /^"approved" requires every gate entry to be "pass"$/);
  assert.deepEqual(validateVerdict(verdict(), { gate: null }), []);
});

// ------------------------------------------------------ storage and summary

test('recordVerdict and verdictFor round-trip by fingerprint', (t) => {
  const stateDir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-verdict-'));
  t.after(() => fs.rmSync(stateDir, { recursive: true, force: true }));
  const proj = { stateDir };
  assert.equal(verdictFor(proj, FP), null);
  const rec = recordVerdict(proj, verdict(), { agent_id: 'a1', stale: false });
  assert.equal(rec.agent_id, 'a1');
  assert.equal(rec.stale, false);
  assert.match(rec.recorded_at, /^\d{4}-\d\d-\d\dT/);
  assert.ok(fs.existsSync(path.join(stateDir, 'verdicts', `${FP}.json`)));
  assert.deepEqual(verdictFor(proj, FP), rec);
  assert.equal(verdictFor(proj, 'fedcba9876543210'), null);
  // an unreadable record reads as no verdict
  fs.writeFileSync(path.join(stateDir, 'verdicts', `${FP}.json`), '{');
  assert.equal(verdictFor(proj, FP), null);
});

test('summarizeFindings lists blockers and majors and counts the rest', () => {
  const v = verdict({ findings: [finding({ severity: 'major', rule: 'ERR-01', line: 7, fix: 'return a typed error' }), finding({ severity: 'blocker', rule: 'H-02', line: undefined, fix: 'propagate with ?' }), finding(), finding({ severity: 'nit', rule: 'none' })] });
  assert.equal(summarizeFindings(v), [
    '- [major] ERR-01 crates/core/src/lib.rs:7: return a typed error',
    '- [blocker] H-02 crates/core/src/lib.rs: propagate with ?',
    '(2 minor/nit finding(s) in the full verdict)',
  ].join('\n'));
  assert.equal(summarizeFindings(verdict()), '');
});
