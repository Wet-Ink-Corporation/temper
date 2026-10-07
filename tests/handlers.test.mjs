// Hook orchestration handlers called directly on throwaway git workspaces:
// SessionStart, UserPromptSubmit, the PostToolUse skill marker and the skill
// gate in PreToolUse, the Stop completion gate, SubagentStart, SubagentStop and
// the SubagentHandback check.
//
// The completion gate runs `cargo --version` and `cargo fmt --all` before it
// reads the cached gate record for the tree fingerprint. These tests seed that
// record (<stateDir>/gate/<fp>.json) instead of running clippy and the tests,
// and skip the cases that reach the gate when cargo or rustfmt is not
// installed. Every workspace is rustfmt-clean, so fmt leaves the fingerprint alone.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync, execFileSync } from 'node:child_process';
import { sessionStart, userPromptSubmit, postTool, preEdit, stop, subagentStop, subagentStart, preHandback } from '../scripts/lib/hooks.mjs';
import { discover, fingerprint, sessionDir, startFingerprint, writeAtomic } from '../scripts/lib/project.mjs';
import { DIMENSIONS, recordVerdict, verdictFor } from '../scripts/lib/verdict.mjs';

const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8' }).status === 0
  && spawnSync('cargo', ['fmt', '--version'], { encoding: 'utf8' }).status === 0;
const git = spawnSync('git', ['--version'], { encoding: 'utf8' }).status === 0;
const noGit = !git && 'git not installed';
const noCargo = (!git || !cargo) && 'cargo, rustfmt or git not installed';

const ROOT = '[workspace]\nresolver = "3"\nmembers = ["crates/*"]\n\n[workspace.package]\nedition = "2024"\nversion = "0.1.0"\npublish = false\n';
const MEMBER = '[package]\nname = "core"\nedition.workspace = true\nversion.workspace = true\npublish.workspace = true\n';
const LIB_RS = '//! Core.\n\n/// One.\n#[must_use]\npub fn one() -> u8 {\n    1\n}\n';
const LIB_RS_2 = `${LIB_RS}\n/// Two.\n#[must_use]\npub fn two() -> u8 {\n    2\n}\n`;
const LIB = 'crates/core/src/lib.rs';

let plugData;
const saved = {};
before(() => {
  plugData = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-plugdata-'));
  for (const k of ['CLAUDE_PLUGIN_DATA', 'CLAUDE_ENV_FILE']) saved[k] = process.env[k];
  process.env.CLAUDE_PLUGIN_DATA = plugData; // skill-load markers
  delete process.env.CLAUDE_ENV_FILE; // never append to a real session's env file
});
after(() => {
  for (const [k, v] of Object.entries(saved)) { if (v === undefined) delete process.env[k]; else process.env[k] = v; }
  fs.rmSync(plugData, { recursive: true, force: true });
});

/** A git repository holding a one-crate workspace, committed. Removed when the test ends. */
function repo(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-handlers-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const r = {
    dir,
    file: (rel) => path.join(dir, ...rel.split('/')),
    put: (rel, text) => { fs.mkdirSync(path.dirname(r.file(rel)), { recursive: true }); fs.writeFileSync(r.file(rel), text); },
    git: (...args) => execFileSync('git', ['-c', 'user.email=t@example.com', '-c', 'user.name=t', '-c', 'commit.gpgsign=false', ...args], { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim(),
    proj: () => discover(dir),
    /** Fingerprint of the tree against HEAD, as the handlers compute it for a session without a recorded base. */
    fp: () => fingerprint(r.proj(), r.git('rev-parse', 'HEAD')),
    /** Seed the cached gate record for the current tree. */
    gate: (ok) => {
      const fp = r.fp();
      const steps = [{ name: 'fmt', ok: true, ms: 0, summary: 'applied' }, { name: 'clippy', ok, ms: 0, summary: ok ? 'clean' : 'error: seeded failure' }];
      writeAtomic(path.join(r.proj().stateDir, 'gate', `${fp}.json`), JSON.stringify({ ok, fp, at: new Date().toISOString(), steps }));
      return fp;
    },
    config: (cfg) => r.put('.claude/temper.json', JSON.stringify(cfg)),
  };
  r.put('Cargo.toml', ROOT);
  r.put('crates/core/Cargo.toml', MEMBER);
  r.put(LIB, LIB_RS);
  r.git('init', '-q');
  r.git('config', 'core.autocrlf', 'false');
  r.git('add', '-A');
  r.git('commit', '-qm', 'base');
  return r;
}

const context = (res) => res.json?.hookSpecificOutput?.additionalContext ?? '';
const decision = (res) => res.json?.hookSpecificOutput?.permissionDecision ?? 'none';
const reason = (res) => res.json?.hookSpecificOutput?.permissionDecisionReason ?? '';
const blocked = (res) => (res.json?.decision === 'block' ? res.json.reason : null);
const message = (res) => res.json?.systemMessage ?? null;

/** A valid approving verdict for `fp` that read the changed files. */
const verdict = (fp, over = {}) => ({
  temper_verdict: 1, verdict: 'approved', fingerprint: fp, findings: [],
  scores: Object.fromEntries(DIMENSIONS.map((d) => [d, 4])),
  gate: { fmt: 'pass', clippy: 'pass', test: 'pass', doc: 'pass' },
  evidence: { files_read: [LIB], commands_run: ['node temper.mjs gate'] },
  ...over,
});
const fenced = (v) => `Review complete.\n\n\`\`\`json\n${JSON.stringify(v, null, 2)}\n\`\`\`\n`;

// --------------------------------------------------------------- SessionStart

test('SessionStart: records the session base and start fingerprint and describes the state', { skip: noGit }, (t) => {
  const r = repo(t);
  const head = r.git('rev-parse', 'HEAD');
  const res = sessionStart({ cwd: r.dir, session_id: 's-start', source: 'startup' });
  assert.equal(res.json.hookSpecificOutput.hookEventName, 'SessionStart');
  const text = context(res);
  assert.match(text, /governed by the temper plugin and the rust-craft engineering standard/);
  assert.match(text, /It has not been loaded in this session yet\./);
  assert.match(text, /- Iron rules \(production code\): H-01 no unsafe; /);
  assert.ok(text.includes(`- Session base revision: ${head}. Current state: no Rust changes since the session base.`), text);
  const proj = r.proj();
  const sd = sessionDir(proj, 's-start');
  assert.equal(fs.readFileSync(path.join(sd, 'base'), 'utf8'), head);
  assert.ok(fs.existsSync(path.join(sd, 'started')));
  assert.equal(startFingerprint(proj, 's-start'), fingerprint(proj, head));
});

test('SessionStart: resume keeps the base; startup, clear and fork reset it', { skip: noGit }, (t) => {
  const r = repo(t);
  const first = r.git('rev-parse', 'HEAD');
  sessionStart({ cwd: r.dir, session_id: 's-src', source: 'startup' });
  r.put(LIB, LIB_RS_2);
  r.git('commit', '-qam', 'two');
  const second = r.git('rev-parse', 'HEAD');
  const base = () => fs.readFileSync(path.join(sessionDir(r.proj(), 's-src'), 'base'), 'utf8');
  for (const source of ['resume', 'compact']) {
    const res = sessionStart({ cwd: r.dir, session_id: 's-src', source });
    assert.equal(base(), first, source);
    assert.match(context(res), new RegExp(`Session base revision: ${first}\\. Current state: 1 Rust file\\(s\\) changed; gate not run; review none \\(fingerprint [0-9a-f]{16}\\)\\.`));
  }
  for (const source of ['clear', 'fork', 'startup']) {
    sessionStart({ cwd: r.dir, session_id: 's-src', source });
    assert.equal(base(), second, source);
  }
});

test('SessionStart: reports the cached gate and the recorded review for the tree', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.gate(true);
  recordVerdict(r.proj(), verdict(fp, { verdict: 'changes-requested' }));
  const res = sessionStart({ cwd: r.dir, session_id: 's-state', source: 'startup' });
  assert.match(context(res), new RegExp(`Current state: 1 Rust file\\(s\\) changed; gate green; review changes-requested \\(fingerprint ${fp}\\)\\.`));
  r.gate(false);
  assert.match(context(sessionStart({ cwd: r.dir, session_id: 's-state', source: 'resume' })), /; gate red; /);
});

test('SessionStart: CLAUDE_ENV_FILE gains TEMPER_CLI and TEMPER_BASE', { skip: noGit }, (t) => {
  const r = repo(t);
  const env = path.join(r.dir, '.temper', 'env.sh');
  r.put('.temper/env.sh', '');
  process.env.CLAUDE_ENV_FILE = env;
  t.after(() => { delete process.env.CLAUDE_ENV_FILE; });
  sessionStart({ cwd: r.dir, session_id: 's-env', source: 'startup' });
  const lines = fs.readFileSync(env, 'utf8').split('\n');
  assert.match(lines[0], /^export TEMPER_CLI=".*\/scripts\/temper\.mjs"$/);
  assert.equal(lines[1], `export TEMPER_BASE="${r.git('rev-parse', 'HEAD')}"`);
});

test('SessionStart outside a Rust project does nothing', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-norust-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  assert.deepEqual(sessionStart({ cwd: dir, session_id: 's-none', source: 'startup' }), {});
});

// ------------------------------------- skill marker, UserPromptSubmit, skill gate

test('skill gate: a Rust edit is refused until rust-craft is loaded through the Skill tool', { skip: noGit }, (t) => {
  const r = repo(t);
  const sid = 's-skill';
  const edit = (content, agent = {}) => preEdit({ tool_name: 'Write', cwd: r.dir, session_id: sid, ...agent, tool_input: { file_path: r.file(LIB), content } });

  const nudge = userPromptSubmit({ cwd: r.dir, session_id: sid });
  assert.equal(nudge.json.hookSpecificOutput.hookEventName, 'UserPromptSubmit');
  assert.match(context(nudge), /rust-craft standard \(skill `temper:rust-craft`\) is not loaded in this session yet/);

  for (const agent of [{}, { agent_type: 'general-purpose', agent_id: 'gp1' }]) {
    const res = edit(LIB_RS_2, agent);
    assert.equal(decision(res), 'deny', JSON.stringify(agent));
    assert.match(reason(res), /^temper: the rust-craft standard is not loaded in this agent context, so this Rust edit was not applied\./);
    assert.match(reason(res), /Invoke the Skill tool with skill "temper:rust-craft"/);
  }
  const manifest = preEdit({ tool_name: 'Edit', cwd: r.dir, session_id: sid, tool_input: { file_path: r.file('crates/core/Cargo.toml'), old_string: 'publish.workspace = true\n', new_string: 'publish.workspace = true\ndescription = "Core"\n' } });
  assert.equal(decision(manifest), 'deny');
  // a non-Rust file is not gated
  assert.equal(decision(preEdit({ tool_name: 'Write', cwd: r.dir, session_id: sid, tool_input: { file_path: r.file('NOTES.md'), content: 'x' } })), 'none');
  // the preloaded subagents carry the skill
  assert.equal(decision(edit(LIB_RS_2, { agent_type: 'temper:rust-implementer', agent_id: 'impl1' })), 'none');

  // other skills and other files do not count
  postTool({ tool_name: 'Skill', cwd: r.dir, session_id: sid, tool_input: { skill: 'temper:review' } });
  postTool({ tool_name: 'Read', cwd: r.dir, session_id: sid, tool_input: { file_path: '/plugins/temper/skills/rust-craft/references/rules.md' } });
  assert.equal(decision(edit(LIB_RS_2)), 'deny');

  assert.deepEqual(postTool({ tool_name: 'Skill', cwd: r.dir, session_id: sid, tool_input: { skill: 'temper:rust-craft' } }), {});
  assert.equal(decision(edit(LIB_RS_2)), 'none');
  assert.equal(decision(edit(LIB_RS_2.replace('    2\n', '    "2".parse().unwrap()\n'))), 'deny', 'the escape-hatch scan still applies');
  // MultiEdit applies every edit before the scan
  const multi = (second) => preEdit({ tool_name: 'MultiEdit', cwd: r.dir, session_id: sid, tool_input: { file_path: r.file(LIB), edits: [{ old_string: '/// One.', new_string: '/// The number one.' }, { old_string: '    1\n', new_string: second }] } });
  assert.equal(decision(multi('    1_u8\n')), 'none');
  assert.equal(decision(multi('    "1".parse().unwrap()\n')), 'deny');
  assert.deepEqual(userPromptSubmit({ cwd: r.dir, session_id: sid }), {});
  // the marker is per agent context: the general-purpose subagent is still gated
  assert.equal(decision(edit(LIB_RS_2, { agent_type: 'general-purpose', agent_id: 'gp1' })), 'deny');
  // and per session
  assert.equal(decision(preEdit({ tool_name: 'Write', cwd: r.dir, session_id: 's-other', tool_input: { file_path: r.file(LIB), content: LIB_RS_2 } })), 'deny');
});

test('skill gate: reading the rust-craft SKILL.md also loads it, with either path separator', { skip: noGit }, (t) => {
  const r = repo(t);
  const edit = (sid, agent = {}) => preEdit({ tool_name: 'Write', cwd: r.dir, session_id: sid, ...agent, tool_input: { file_path: r.file(LIB), content: LIB_RS_2 } });
  postTool({ tool_name: 'Read', cwd: r.dir, session_id: 's-read', tool_input: { file_path: '/home/u/.claude/plugins/temper/skills/rust-craft/SKILL.md' } });
  assert.equal(decision(edit('s-read')), 'none');
  postTool({ tool_name: 'Read', cwd: r.dir, session_id: 's-read-win', agent_id: 'w1', tool_input: { file_path: 'C:\\Users\\u\\.claude\\plugins\\temper\\skills\\rust-craft\\SKILL.md' } });
  assert.equal(decision(edit('s-read-win', { agent_type: 'general-purpose', agent_id: 'w1' })), 'none');
  // the Skill tool's `name` field works too
  postTool({ tool_name: 'Skill', cwd: r.dir, session_id: 's-name', tool_input: { name: 'rust-craft' } });
  assert.equal(decision(edit('s-name')), 'none');
});

test('skill gate: "skillGate": false in .claude/temper.json turns it off', { skip: noGit }, (t) => {
  const r = repo(t);
  r.config({ skillGate: false });
  const res = preEdit({ tool_name: 'Write', cwd: r.dir, session_id: 's-nogate', tool_input: { file_path: r.file(LIB), content: LIB_RS_2 } });
  assert.equal(decision(res), 'none');
  assert.deepEqual(userPromptSubmit({ cwd: r.dir, session_id: 's-nogate' }), {});
});

// ------------------------------------------------- Stop: before the gate runs

test('Stop: no Rust change, plan mode, or a tree the session started with ends the turn silently', { skip: noGit }, (t) => {
  const r = repo(t);
  assert.deepEqual(stop({ cwd: r.dir, session_id: 's-clean' }), {});
  r.put(LIB, LIB_RS_2);
  assert.deepEqual(stop({ cwd: r.dir, session_id: 's-plan', permission_mode: 'plan' }), {});
  // the change predates the session (start fingerprint equals the current tree)
  sessionStart({ cwd: r.dir, session_id: 's-preexisting', source: 'startup' });
  assert.deepEqual(stop({ cwd: r.dir, session_id: 's-preexisting' }), {});
  // a change to a non-Rust file is not a Rust change
  const r2 = repo(t);
  r2.put('NOTES.md', 'notes\n');
  assert.deepEqual(stop({ cwd: r2.dir, session_id: 's-notes' }), {});
});

test('Stop: a final "Status: WIP" or "Status: BLOCKED" line pauses without the gate', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const at = (msg) => message(stop({ cwd: r.dir, session_id: 's-status', last_assistant_message: msg }));
  const wip = 'temper: Rust changes left in progress; the gate and review were not enforced for this turn.';
  const blockedMsg = 'temper: Rust changes left blocked; the gate and review were not enforced for this turn.';
  assert.equal(at('Halfway there.\n\nStatus: WIP (tests next)'), wip);
  assert.equal(at('Need a decision.\n\n**Status: BLOCKED** which error type?'), blockedMsg);
  assert.equal(at('- status: wip'), wip);
  assert.equal(at('> Status: `BLOCKED`'), blockedMsg);
  // the last Status line wins
  assert.equal(at('Status: DONE\n\nOn reflection:\nStatus: WIP'), wip);
});

// ------------------------------------------- Stop: the gate and the review

test('Stop: a red gate blocks; a repeat with stop_hook_active and no progress is let through', { skip: noCargo }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.gate(false);
  const input = { cwd: r.dir, session_id: 's-red' };
  const first = blocked(stop(input));
  assert.ok(first, 'blocked');
  assert.match(first, new RegExp(`^temper gate: RED \\(fingerprint ${fp}, cached\\)\\nfmt ok \\| clippy FAILED\\n--- clippy ---\\nerror: seeded failure`));
  assert.match(first, /The work is not done while the gate is red\. Fix the failures \(never by weakening tests or lints, H-12\)\. If you cannot finish now, end your message with "Status: WIP"/);
  const stuck = stop({ ...input, stop_hook_active: true });
  assert.equal(message(stuck), 'temper: stopped with a RED gate (no progress after the previous block). clippy failing.');
  // without stop_hook_active it blocks again
  assert.ok(blocked(stop(input)));
  // a final message whose last Status line is DONE does not pause the gate
  assert.ok(blocked(stop({ ...input, last_assistant_message: 'Status: WIP\n\nDone after all.\nStatus: DONE' })));
  // a changed tree is progress: the repeat blocks again for the new fingerprint
  r.put(LIB, `${LIB_RS_2}\n/// Three.\n#[must_use]\npub fn three() -> u8 {\n    3\n}\n`);
  const next = r.gate(false);
  assert.notEqual(next, fp);
  assert.ok(blocked(stop({ ...input, stop_hook_active: true })));
  assert.match(message(stop({ ...input, stop_hook_active: true })), /stopped with a RED gate/);
});

test('Stop: a green gate without an approving review blocks for PROC-08; the stuck repeat is let through', { skip: noCargo }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.gate(true);
  const input = { cwd: r.dir, session_id: 's-review' };
  const res = blocked(stop(input));
  assert.ok(res, 'blocked');
  assert.match(res, new RegExp(`^The gate is green, but the current tree \\(fingerprint ${fp}\\) has no approving independent review \\(PROC-08\\)\\. Delegate to the temper:rust-reviewer subagent`));
  assert.ok(res.includes(`It reviews the diff since ${r.git('rev-parse', 'HEAD')} in a fresh context.`));
  assert.equal(message(stop({ ...input, stop_hook_active: true })), `temper: stopped without an approving review for fingerprint ${fp}.`);
});

test('Stop: a changes-requested verdict blocks with its findings', { skip: noCargo }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.gate(true);
  recordVerdict(r.proj(), verdict(fp, { verdict: 'changes-requested', findings: [{ rule: 'API-01', severity: 'major', file: LIB, line: 10, evidence: 'pub fn two', fix: 'name it for what it returns' }] }));
  const res = blocked(stop({ cwd: r.dir, session_id: 's-cr' }));
  assert.ok(res.startsWith(`The reviewer requested changes for this tree (fingerprint ${fp}):\n- [major] API-01 ${LIB}:10: name it for what it returns\nAddress each finding, or rebut it with evidence, then request a fresh review.`), res);
});

test('Stop: green with an approved verdict for the fingerprint ends the turn', { skip: noCargo }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.gate(true);
  recordVerdict(r.proj(), verdict(fp));
  assert.equal(message(stop({ cwd: r.dir, session_id: 's-ok' })), `temper: gate green and review approved (fingerprint ${fp}).`);
  // an approval for another fingerprint does not count
  r.put(LIB, `${LIB_RS_2}\n/// Three.\n#[must_use]\npub fn three() -> u8 {\n    3\n}\n`);
  r.gate(true);
  assert.ok(blocked(stop({ cwd: r.dir, session_id: 's-ok' })));
});

test('Stop: review "advisory" and "off" in .claude/temper.json', { skip: noCargo }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.gate(true);
  r.config({ review: 'advisory' });
  assert.equal(message(stop({ cwd: r.dir, session_id: 's-adv' })), `temper: gate green; no approving review for fingerprint ${fp} (review is advisory in this repo).`);
  r.config({ review: 'off' });
  assert.equal(message(stop({ cwd: r.dir, session_id: 's-off' })), `temper: gate green (fingerprint ${fp}).`);
  // a red gate still blocks when review is off
  r.gate(false);
  assert.ok(blocked(stop({ cwd: r.dir, session_id: 's-off' })));
});

// -------------------------------------------------------------- SubagentStart

test('SubagentStart: the reviewer, implementer and architect get the fingerprint and base; other agents get nothing', { skip: noGit }, (t) => {
  const r = repo(t);
  const head = r.git('rev-parse', 'HEAD');
  r.put(LIB, LIB_RS_2);
  r.put('.temper/plans/core.md', '# plan\n');
  const fp = r.gate(false);
  const rev = subagentStart({ cwd: r.dir, session_id: 's-sub', agent_type: 'temper:rust-reviewer', agent_id: 'rv1' });
  assert.equal(rev.json.hookSpecificOutput.hookEventName, 'SubagentStart');
  const text = context(rev);
  assert.match(text, /^temper context for temper:rust-reviewer:\n/);
  assert.ok(text.includes(`- Base revision: ${head}; review scope: \`git diff ${head}\` plus untracked files.`), text);
  assert.ok(text.includes(`- Current tree fingerprint: ${fp}`), text);
  assert.ok(text.includes(`- Changed Rust files (1): ${LIB}`), text);
  assert.ok(text.includes('- Gate for this fingerprint: red (clippy).'), text);
  assert.ok(text.includes('- Plans: .temper/plans/core.md'), text);
  assert.ok(text.includes(`- Your verdict's "fingerprint" field must be ${fp}.`), text);
  assert.match(text, /- Diff stat:\n.*crates\/core\/src\/lib\.rs/);

  const impl = context(subagentStart({ cwd: r.dir, session_id: 's-sub', agent_type: 'rust-implementer', agent_id: 'im1' }));
  assert.match(impl, /^temper context for rust-implementer:\n/);
  assert.ok(impl.includes(`- Current tree fingerprint: ${fp}`));
  assert.doesNotMatch(impl, /Your verdict's/);
  assert.match(context(subagentStart({ cwd: r.dir, session_id: 's-sub', agent_type: 'temper:rust-architect', agent_id: 'ar1' })), /^temper context for temper:rust-architect:\n/);

  assert.deepEqual(subagentStart({ cwd: r.dir, session_id: 's-sub', agent_type: 'general-purpose' }), {});
  assert.deepEqual(subagentStart({ cwd: r.dir, session_id: 's-sub' }), {});
});

// -------------------------------------------------------------- SubagentStop

test('SubagentStop (reviewer): an invalid verdict blocks, and is let through on the stop_hook_active repeat', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const input = { cwd: r.dir, session_id: 's-rv', agent_type: 'temper:rust-reviewer', agent_id: 'rv1' };
  const none = blocked(subagentStop({ ...input, last_assistant_message: 'Looks fine to me.' }));
  assert.match(none, /^temper: your review verdict is invalid:\n- no verdict block found/);
  assert.match(none, /End your final message with a corrected ```json verdict block\.$/);
  const fp = r.fp();
  const unread = blocked(subagentStop({ ...input, last_assistant_message: fenced(verdict(fp, { evidence: { files_read: ['crates/core/Cargo.toml'], commands_run: ['gate'] } })) }));
  assert.match(unread, /- changed files not reviewed: crates\/core\/src\/lib\.rs/);
  assert.deepEqual(subagentStop({ ...input, stop_hook_active: true, last_assistant_message: 'Looks fine to me.' }), {});
  assert.equal(verdictFor(r.proj(), fp), null, 'nothing recorded');
});

test('SubagentStop (reviewer): an approval is refused while the cached gate for its fingerprint is red', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.gate(false);
  const res = blocked(subagentStop({ cwd: r.dir, session_id: 's-rv-red', agent_type: 'temper:rust-reviewer', agent_id: 'rv1', last_assistant_message: fenced(verdict(fp)) }));
  assert.match(res, /- "approved" is impossible: the gate is red for this fingerprint/);
});

test('SubagentStop (reviewer): a valid verdict is recorded, flagged stale when the tree moved', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.fp();
  const input = { cwd: r.dir, session_id: 's-rv-ok', agent_type: 'temper:rust-reviewer', agent_id: 'rv1' };
  assert.deepEqual(subagentStop({ ...input, last_assistant_message: fenced(verdict(fp)) }), {});
  const proj = r.proj();
  const rec = verdictFor(proj, fp);
  assert.equal(rec.verdict, 'approved');
  assert.equal(rec.stale, false);
  assert.equal(rec.agent_id, 'rv1');
  assert.equal(fs.readFileSync(path.join(sessionDir(proj, 's-rv-ok'), 'verdict-agent-rv1'), 'utf8'), fp);
  // once delivered, a final message without a verdict passes
  assert.deepEqual(subagentStop({ ...input, last_assistant_message: 'Done.' }), {});
  // a verdict for a fingerprint that is no longer the tree's is recorded as stale
  const old = 'fedcba9876543210';
  assert.deepEqual(subagentStop({ ...input, agent_id: 'rv2', last_assistant_message: fenced(verdict(old, { verdict: 'changes-requested' })) }), {});
  assert.equal(verdictFor(proj, old).stale, true);
});

test('SubagentStop (implementer): the gate is enforced without requiring review', { skip: noCargo }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const input = { cwd: r.dir, session_id: 's-im', agent_type: 'temper:rust-implementer', agent_id: 'im1' };
  let fp = r.gate(false);
  assert.match(blocked(subagentStop(input)), /^temper gate: RED/);
  fp = r.gate(true);
  assert.equal(message(subagentStop(input)), `temper: gate green (fingerprint ${fp}).`);
  assert.equal(message(subagentStop({ ...input, last_assistant_message: 'Status: BLOCKED\nwhich type?' })), 'temper: Rust changes left blocked; the gate and review were not enforced for this turn.');
});

test('SubagentStop: other agent types are not checked', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  for (const agent_type of ['general-purpose', 'temper:rust-architect', undefined]) {
    assert.deepEqual(subagentStop({ cwd: r.dir, session_id: 's-other', agent_type, agent_id: 'x1' }), {}, String(agent_type));
  }
});

// ---------------------------------------------------- PreToolUse SubagentHandback

test('SubagentHandback: only the reviewer is checked; an invalid verdict is denied with the problem list', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const input = { cwd: r.dir, session_id: 's-hb', tool_name: 'SubagentHandback', agent_type: 'temper:rust-reviewer', agent_id: 'rv1' };
  assert.deepEqual(preHandback({ ...input, agent_type: 'temper:rust-implementer', tool_input: { message: 'done' } }), {});
  const res = preHandback({ ...input, tool_input: { message: fenced(verdict('nothex')) } });
  assert.equal(decision(res), 'deny');
  assert.match(reason(res), /^temper: the review verdict is invalid, so the report was not delivered:\n- "fingerprint" must be the 16-hex fingerprint/);
  assert.match(reason(res), /\nFix the verdict block and hand back again\.$/);
  const fp = r.fp();
  assert.deepEqual(preHandback({ ...input, tool_input: { message: fenced(verdict(fp)) } }), {});
  assert.equal(verdictFor(r.proj(), fp).stale, false);
  assert.equal(fs.readFileSync(path.join(sessionDir(r.proj(), 's-hb'), 'verdict-agent-rv1'), 'utf8'), fp);
});
