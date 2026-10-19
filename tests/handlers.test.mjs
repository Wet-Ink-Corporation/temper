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
import { sessionStart, userPromptSubmit, postTool, preEdit, stop, subagentStop, subagentStart, preHandback, reportFromTranscript } from '../scripts/lib/hooks.mjs';
import { discover, fingerprint, loadConfig, readTelemetry, rustChanges, sessionDir, startFingerprint, writeAtomic } from '../scripts/lib/project.mjs';
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

test('SessionStart: an old runtime adds the warning for the human and for the model, and nothing else', async () => {
  const { withRuntimeWarnings } = await import('../scripts/lib/hooks.mjs');
  const out = { json: { hookSpecificOutput: { hookEventName: 'SessionStart', additionalContext: 'line one' } } };
  const warned = withRuntimeWarnings(out, ['temper: Node 17.9.1 is older than the documented minimum, Node 18.0.0.']);
  assert.deepEqual(warned, { json: {
    systemMessage: 'temper: Node 17.9.1 is older than the documented minimum, Node 18.0.0.',
    hookSpecificOutput: { hookEventName: 'SessionStart', additionalContext: 'line one\n- temper: Node 17.9.1 is older than the documented minimum, Node 18.0.0.' },
  } });
  assert.equal(withRuntimeWarnings(out, []), out);
});

test('SessionStart on a current Node adds no warning', { skip: noGit }, (t) => {
  const r = repo(t);
  const res = sessionStart({ cwd: r.dir, session_id: 's-ver', source: 'startup' });
  assert.equal(message(res), null);
  assert.doesNotMatch(context(res), /older than the documented minimum/);
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

/** The "Changed Rust files" line SubagentStart gives an implementer: the session's Rust changes. */
const changedFor = (r, sid) => {
  const line = context(subagentStart({ cwd: r.dir, session_id: sid, agent_type: 'rust-implementer', agent_id: 'im-cfg' }))
    .split('\n').find((l) => l.startsWith('- Changed Rust files'));
  return line.replace(/^- Changed Rust files \(\d+\): /, '');
};

test('change detection: .claude/temper.json and .cargo/config(.toml) count as Rust changes and change the fingerprint', { skip: noGit }, (t) => {
  // tracked files edited after the base commit (the git diff pathspecs)
  const r = repo(t);
  r.config({ review: 'required' });
  r.put('.cargo/config.toml', '[build]\nrustflags = []\n');
  r.git('add', '-A');
  r.git('commit', '-qm', 'config');
  const clean = r.fp();
  assert.equal(changedFor(r, 's-cfg-clean'), 'none');
  r.config({ review: 'required', gate: { allFeatures: false } });
  assert.equal(changedFor(r, 's-cfg-json'), '.claude/temper.json');
  const afterJson = r.fp();
  assert.notEqual(afterJson, clean, 'an edited .claude/temper.json changes the fingerprint');
  r.put('.cargo/config.toml', '[build]\nrustflags = ["-Dwarnings"]\n');
  assert.deepEqual(changedFor(r, 's-cfg-cargo').split(', ').sort(), ['.cargo/config.toml', '.claude/temper.json']);
  assert.notEqual(r.fp(), afterJson, 'an edited .cargo/config.toml changes the fingerprint');
  // untracked files: the legacy .cargo/config and a nested one, which cargo reads when run in that directory
  const u = repo(t);
  let fp = u.fp();
  for (const rel of ['.claude/temper.json', '.cargo/config', 'crates/core/.cargo/config.toml']) {
    u.put(rel, rel.endsWith('.json') ? '{"gate":{"doc":false}}' : '[build]\nrustflags = []\n');
    assert.ok(changedFor(u, 's-cfg-untracked').split(', ').includes(rel), `${rel} is a Rust change`);
    const next = u.fp();
    assert.notEqual(next, fp, `untracked ${rel} changes the fingerprint`);
    fp = next;
  }
});

test('change detection: other files under .claude/ and .cargo/ are not Rust changes and leave the fingerprint alone', { skip: noGit }, (t) => {
  const r = repo(t);
  const fp = r.fp();
  const nearMisses = [
    '.claude/settings.json', '.claude/settings.local.json', '.claude/temper.json.bak', '.claude/temper-notes.json',
    'docs/.claude/temper.json', '.cargo/credentials.toml', '.cargo/config.toml.orig', 'notes/cargo-config.toml', 'NOTES.md',
  ];
  for (const rel of nearMisses) r.put(rel, '{}\n');
  assert.equal(changedFor(r, 's-near'), 'none');
  assert.equal(r.fp(), fp);
  assert.deepEqual(stop({ cwd: r.dir, session_id: 's-near' }), {});
});

test('change detection: .claude/temper.json counts only when a key the gate reads (gate, scanner, manifests) changes', { skip: noGit }, (t) => {
  const r = repo(t);
  r.config({ review: 'required', gate: { allFeatures: true } });
  r.git('add', '-A');
  r.git('commit', '-qm', 'config');
  const fp = r.fp();
  // keys that steer hooks only, a reformat, another key order, and a default spelled out: no Rust change
  const hookOnly = [
    { review: 'off', gate: { allFeatures: true } },
    { review: 'advisory', skillGate: false, askOnNewDependency: false, preapprovedDependencies: ['rand'], protected: [], gate: { allFeatures: true } },
    { gate: { doc: true, allFeatures: true }, review: 'required', scanner: { exempt: [] } },
  ];
  for (const cfg of hookOnly) {
    r.put('.claude/temper.json', `${JSON.stringify(cfg, null, 2)}\n`);
    assert.equal(changedFor(r, 's-hook-only'), 'none', JSON.stringify(cfg));
    assert.equal(r.fp(), fp, JSON.stringify(cfg));
    assert.deepEqual(stop({ cwd: r.dir, session_id: 's-hook-only' }), {});
  }
  // each key the gate reads is a Rust change
  for (const cfg of [{ gate: { allFeatures: false } }, { scanner: { exempt: [{ rule: 'H-01', paths: ['**'] }] } }, { manifests: { ownLints: ['crates/core'] } }]) {
    r.config({ review: 'required', gate: { allFeatures: true }, ...cfg });
    assert.equal(changedFor(r, 's-gate-key'), '.claude/temper.json', JSON.stringify(cfg));
    assert.notEqual(r.fp(), fp, JSON.stringify(cfg));
  }
  // a deleted file falls back to the defaults, which here equal the committed gate config
  fs.rmSync(r.file('.claude/temper.json'));
  assert.equal(changedFor(r, 's-deleted'), 'none');
  assert.equal(r.fp(), fp);
});

test('change detection: a property the gate does not read under gate, scanner or manifests is no Rust change', { skip: noGit }, (t) => {
  const r = repo(t);
  r.config({ gate: { allFeatures: true } });
  r.git('add', '-A');
  r.git('commit', '-qm', 'config');
  const fp = r.fp();
  for (const cfg of [
    { gate: { allFeatures: true, colour: 'blue' } },
    { gate: { allFeatures: true }, scanner: { exempt: [], note: 'see ADR-7' } },
    { gate: { allFeatures: true }, manifests: { ownLints: [], owner: 'ffi team' } },
  ]) {
    r.config(cfg);
    assert.equal(changedFor(r, 's-unused'), 'none', JSON.stringify(cfg));
    assert.equal(r.fp(), fp, JSON.stringify(cfg));
  }
});

test('change detection outside git: config files count, and the fingerprint is stable and follows only the gate settings', (t) => {
  // a Cargo workspace with no repository: changedFiles walks the tree, fingerprint hashes the files
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-nogit-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const put = (rel, text) => { const f = path.join(dir, ...rel.split('/')); fs.mkdirSync(path.dirname(f), { recursive: true }); fs.writeFileSync(f, text); };
  put('Cargo.toml', ROOT);
  put('crates/core/Cargo.toml', MEMBER);
  put(LIB, LIB_RS);
  const proj = () => ({ root: dir, gitRoot: null, stateDir: path.join(dir, 'target', 'temper'), config: loadConfig(dir) });
  const changes = () => rustChanges(proj()).map((f) => f.split(path.sep).join('/')).sort();
  const fp = () => fingerprint(proj());
  const start = fp();
  assert.equal(fp(), start, 'the same tree hashes the same across calls');
  assert.deepEqual(changes(), ['Cargo.toml', 'crates/core/Cargo.toml', LIB]);
  // keys that steer hooks only, a default spelled out, and an unused property: the defaults' gate settings
  for (const cfg of [{ review: 'off', skillGate: false }, { gate: { doc: true }, scanner: { exempt: [] } }, { gate: { colour: 'blue' }, manifests: { owner: 'x' } }]) {
    put('.claude/temper.json', JSON.stringify(cfg));
    assert.equal(fp(), start, JSON.stringify(cfg));
    assert.ok(!changes().includes('.claude/temper.json'), JSON.stringify(cfg));
  }
  put('.claude/temper.json', JSON.stringify({ gate: { allFeatures: false } }));
  const afterJson = fp();
  assert.notEqual(afterJson, start, 'a gate setting changes the fingerprint');
  assert.equal(fp(), afterJson, 'and stays stable');
  assert.ok(changes().includes('.claude/temper.json'));
  put('.cargo/config.toml', '[build]\nrustflags = []\n');
  const afterCargo = fp();
  assert.notEqual(afterCargo, afterJson, 'a .cargo/config.toml changes the fingerprint');
  assert.ok(changes().includes('.cargo/config.toml'));
  put('.cargo/config.toml', '[build]\nrustflags = ["-Dwarnings"]\n');
  assert.notEqual(fp(), afterCargo, 'so does an edit to it');
  // other files under .claude/ and .cargo/ leave it alone
  const settled = fp();
  put('.claude/settings.json', '{}\n');
  put('.cargo/credentials.toml', '{}\n');
  assert.equal(fp(), settled);
  assert.ok(!changes().some((f) => f.endsWith('settings.json') || f.endsWith('credentials.toml')));
});

test('change detection: a git-ignored .claude/temper.json still counts, as temper reads it', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put('.gitignore', '.claude/\n');
  r.git('add', '-A');
  r.git('commit', '-qm', 'ignore');
  const fp = r.fp();
  r.config({ review: 'off' });
  assert.equal(changedFor(r, 's-ign'), 'none');
  assert.equal(r.fp(), fp);
  r.config({ gate: { allFeatures: false } });
  assert.equal(changedFor(r, 's-ign'), '.claude/temper.json');
  assert.notEqual(r.fp(), fp);
});

test('Stop: a session that edits only .claude/temper.json runs the gate and needs a review for the new tree', { skip: noCargo }, (t) => {
  const r = repo(t);
  const before = r.gate(true);
  recordVerdict(r.proj(), verdict(before));
  r.config({ gate: { allFeatures: false } });
  const fp = r.gate(true);
  assert.notEqual(fp, before, 'the cached gate and verdict for the old tree do not apply');
  const res = blocked(stop({ cwd: r.dir, session_id: 's-cfg-stop' }));
  assert.ok(res, 'blocked');
  assert.match(res, new RegExp(`^The gate is green, but the current tree \\(fingerprint ${fp}\\) has no approving independent review \\(PROC-08\\)`));
});

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

// ------------------------------- Stop: Rust changed in another worktree (#135)

/** A worktree of `r` under .claude/worktrees/<name>, with helpers like repo()'s. */
function worktree(r, name) {
  const dir = r.file(`.claude/worktrees/${name}`);
  r.git('worktree', 'add', '-q', '--detach', dir);
  const w = {
    dir,
    file: (rel) => path.join(dir, ...rel.split('/')),
    put: (rel, text) => fs.writeFileSync(w.file(rel), text),
    proj: () => discover(dir),
    /** Fingerprint of the worktree against the session base, as Stop computes it. */
    fp: (sid) => fingerprint(w.proj(), fs.readFileSync(path.join(sessionDir(r.proj(), sid), 'base'), 'utf8').trim()),
    gate: (sid, ok) => {
      const fp = w.fp(sid);
      const steps = [{ name: 'fmt', ok: true, ms: 0, summary: 'applied' }, { name: 'clippy', ok, ms: 0, summary: ok ? 'clean' : 'error: seeded failure' }];
      writeAtomic(path.join(r.proj().stateDir, 'gate', `${fp}.json`), JSON.stringify({ ok, fp, at: new Date().toISOString(), steps }));
      return fp;
    },
  };
  return w;
}

/** A subagent's Edit of `rel` in the worktree, as PreToolUse sees it (the skill gate denies it; the tree is noted first), then the write itself. */
function subagentEdits(w, sid, agent_id, rel, text) {
  preEdit({ cwd: w.dir, session_id: sid, agent_id, agent_type: 'general-purpose', tool_name: 'Write', tool_input: { file_path: w.file(rel), content: text } });
  w.put(rel, text);
}

test('Stop: Rust a subagent changed in its own worktree is gated and needs a review of that worktree', { skip: noCargo }, (t) => {
  const r = repo(t);
  const sid = 's-wt';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  const w = worktree(r, 'agent-a1');
  subagentEdits(w, sid, 'a1', LIB, LIB_RS_2);
  // the main checkout has no Rust change, yet the turn is not done
  let fp = w.gate(sid, false);
  const red = blocked(stop({ cwd: r.dir, session_id: sid }));
  assert.ok(red, 'blocked on the worktree gate');
  assert.match(red, new RegExp(`^In the worktree \\.claude/worktrees/agent-a1: temper gate: RED \\(fingerprint ${fp}, cached\\)`));
  fp = w.gate(sid, true);
  const unreviewed = blocked(stop({ cwd: r.dir, session_id: sid }));
  assert.match(unreviewed, new RegExp(`^In the worktree \\.claude/worktrees/agent-a1: The gate is green, but the current tree \\(fingerprint ${fp}\\) has no approving independent review`));
  // the path as git reports it (on Windows, the long form of a short 8.3 temp path)
  assert.ok(unreviewed.includes(`git -C "${w.proj().root.split(path.sep).join('/')}" diff`), unreviewed);
  // the reviewer, started in the main checkout, is pointed at the worktree, the only tree with changes
  const rv = { cwd: r.dir, session_id: sid, agent_type: 'temper:rust-reviewer', agent_id: 'rv1' };
  const said = context(subagentStart(rv));
  assert.match(said, /^- Target: the worktree \.claude\/worktrees\/agent-a1\. /m);
  assert.match(said, new RegExp(`^- Changed Rust files \\(1\\): ${LIB}$`, 'm'));
  const told = said.match(/^- Your verdict's "fingerprint" field must be ([0-9a-f]{16})\./m)?.[1];
  assert.equal(told, fp, 'the fingerprint the reviewer is told to use is the worktree\'s');
  // a verdict missing the worktree's changed file is refused; one built from the context is current
  assert.match(blocked(subagentStop({ ...rv, last_assistant_message: fenced(verdict(told, { evidence: { files_read: ['crates/core/Cargo.toml'], commands_run: ['gate'] } })) })), /changed files not reviewed: crates\/core\/src\/lib\.rs/);
  assert.deepEqual(subagentStop({ ...rv, last_assistant_message: fenced(verdict(told)) }), {});
  assert.equal(verdictFor(r.proj(), fp).stale, false);
  assert.equal(message(stop({ cwd: r.dir, session_id: sid })), `temper: In the worktree .claude/worktrees/agent-a1: gate green and review approved (fingerprint ${fp}).`);
  const rows = readTelemetry(r.proj()).filter((x) => x.event === 'stop-green');
  assert.equal(rows.at(-1).tree, '.claude/worktrees/agent-a1');
});

test('Stop: a worktree the session never changed, one merged into the main checkout, and a removed one are not gated', { skip: noCargo }, (t) => {
  const r = repo(t);
  const sid = 's-wt-skip';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  // another session's worktree with Rust changes: not this session's
  const other = worktree(r, 'agent-other');
  other.put(LIB, LIB_RS_2);
  assert.deepEqual(stop({ cwd: r.dir, session_id: sid }), {});
  // a subagent started in a worktree and changed nothing there
  const idle = worktree(r, 'agent-idle');
  subagentStart({ cwd: idle.dir, session_id: sid, agent_type: 'general-purpose', agent_id: 'i1' });
  assert.deepEqual(stop({ cwd: r.dir, session_id: sid }), {});
  // committed in the worktree and merged into the main checkout: the main checkout is gated, once
  const w = worktree(r, 'agent-m1');
  subagentEdits(w, sid, 'm1', LIB, LIB_RS_2);
  execFileSync('git', ['-c', 'user.email=t@example.com', '-c', 'user.name=t', '-c', 'commit.gpgsign=false', 'commit', '-qam', 'two'], { cwd: w.dir });
  r.git('merge', '-q', '--ff-only', execFileSync('git', ['rev-parse', 'HEAD'], { cwd: w.dir, encoding: 'utf8' }).trim());
  const fp = r.gate(true);
  recordVerdict(r.proj(), verdict(fp));
  // r.gate keys on HEAD; the session base is the commit before the merge, so seed that fingerprint too
  const base = fs.readFileSync(path.join(sessionDir(r.proj(), sid), 'base'), 'utf8').trim();
  const mainFp = fingerprint(r.proj(), base);
  writeAtomic(path.join(r.proj().stateDir, 'gate', `${mainFp}.json`), JSON.stringify({ ok: true, fp: mainFp, at: new Date().toISOString(), steps: [{ name: 'fmt', ok: true, ms: 0, summary: 'applied' }] }));
  recordVerdict(r.proj(), verdict(mainFp));
  assert.equal(message(stop({ cwd: r.dir, session_id: sid })), `temper: gate green and review approved (fingerprint ${mainFp}).`);
  // a removed worktree is skipped
  const gone = worktree(r, 'agent-gone');
  subagentEdits(gone, sid, 'g1', LIB, `${LIB_RS_2}\n/// Three.\n#[must_use]\npub fn three() -> u8 {\n    3\n}\n`);
  r.git('worktree', 'remove', '--force', gone.dir);
  assert.equal(message(stop({ cwd: r.dir, session_id: sid })), `temper: gate green and review approved (fingerprint ${mainFp}).`);
});

test('SubagentStart (reviewer): with changes in the main checkout and a worktree, the main tree is the target and the worktree is listed', { skip: noGit }, (t) => {
  const r = repo(t);
  const sid = 's-wt-both';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  r.put(LIB, LIB_RS_2);
  const w = worktree(r, 'agent-c1');
  subagentEdits(w, sid, 'c1', LIB, `${LIB_RS_2}\n/// Three.\n#[must_use]\npub fn three() -> u8 {\n    3\n}\n`);
  const said = context(subagentStart({ cwd: r.dir, session_id: sid, agent_type: 'temper:rust-reviewer', agent_id: 'rv1' }));
  assert.doesNotMatch(said, /^- Target:/m);
  assert.match(said, new RegExp(`Also changed in this session: the worktree \\.claude/worktrees/agent-c1 \\(fingerprint ${w.fp(sid)}, 1 Rust file\\(s\\)\\)`));
  assert.match(said, new RegExp(`must be ${fingerprint(r.proj(), r.git('rev-parse', 'HEAD'))}, or, if you were asked to review one of the worktrees listed above, that worktree's fingerprint\\.`));
});

test('SubagentStart: parallel agents in their own worktrees each keep their own tree as the target', { skip: noGit }, (t) => {
  const r = repo(t);
  const sid = 's-wt-par';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  const w1 = worktree(r, 'agent-p1');
  const w2 = worktree(r, 'agent-p2');
  subagentStart({ cwd: w1.dir, session_id: sid, agent_type: 'temper:rust-implementer', agent_id: 'p1' });
  subagentEdits(w1, sid, 'p1', LIB, LIB_RS_2);
  // the second implementer starts in its own, clean worktree: it is not sent to the first one's
  const second = context(subagentStart({ cwd: w2.dir, session_id: sid, agent_type: 'temper:rust-implementer', agent_id: 'p2' }));
  assert.doesNotMatch(second, /^- (Target|Also changed)/m);
  assert.ok(second.includes(`- Project root: ${w2.proj().root}`), second);
  assert.match(second, /^- Changed Rust files \(0\): none$/m);
  // a reviewer started inside the first worktree reviews that worktree, read-only
  const rv = context(subagentStart({ cwd: w1.dir, session_id: sid, agent_type: 'temper:rust-reviewer', agent_id: 'rv1' }));
  assert.doesNotMatch(rv, /^- (Target|Also changed)/m);
  assert.ok(rv.includes(`- Project root: ${w1.proj().root}`), rv);
  assert.match(rv, new RegExp(`^- Your verdict's "fingerprint" field must be ${w1.fp(sid)}\\.`, 'm'));
  assert.match(rv, /Gate command: TEMPER_BASE=[0-9a-f]{40} node "[^"]+" gate --check$/m);
});

test('Stop: a session started in a linked worktree keeps its own base when a subagent starts there later', { skip: noCargo }, (t) => {
  const r = repo(t);
  const w = worktree(r, 'session-wt');
  const sid = 's-wt-own';
  sessionStart({ cwd: w.dir, session_id: sid, source: 'startup' });
  // Rust arrives without an Edit (a cherry-pick, a merge) and is committed
  w.put(LIB, LIB_RS_2);
  execFileSync('git', ['-c', 'user.email=t@example.com', '-c', 'user.name=t', '-c', 'commit.gpgsign=false', 'commit', '-qam', 'two'], { cwd: w.dir });
  const fp = w.gate(sid, true);
  assert.ok(blocked(stop({ cwd: w.dir, session_id: sid })), 'unreviewed');
  // a subagent starting in that worktree records its HEAD, which must not become the session's base
  subagentStart({ cwd: w.dir, session_id: sid, agent_type: 'Explore', agent_id: 'x1' });
  const res = blocked(stop({ cwd: w.dir, session_id: sid }));
  assert.ok(res, 'still unreviewed after the subagent started');
  assert.ok(res.includes(`(fingerprint ${fp})`), res);
});

test('SessionStart: a compact in a worktree the session entered later keeps that worktree\'s base', { skip: noCargo }, (t) => {
  const r = repo(t);
  const sid = 's-wt-compact';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  // the main checkout commits, then the session moves into a worktree made from the new HEAD
  r.put('README.md', 'later\n');
  r.git('add', '-A');
  r.git('commit', '-qm', 'later');
  const w = worktree(r, 'entered');
  preEdit({ cwd: w.dir, session_id: sid, tool_name: 'Write', tool_input: { file_path: w.file(LIB), content: LIB_RS_2 } });
  w.put(LIB, LIB_RS_2);
  const head = r.git('rev-parse', 'HEAD');
  const fp = fingerprint(w.proj(), head);
  writeAtomic(path.join(r.proj().stateDir, 'gate', `${fp}.json`), JSON.stringify({ ok: true, fp, at: new Date().toISOString(), steps: [{ name: 'fmt', ok: true, ms: 0, summary: 'applied' }] }));
  recordVerdict(r.proj(), verdict(fp));
  assert.equal(message(stop({ cwd: w.dir, session_id: sid })), `temper: gate green and review approved (fingerprint ${fp}).`);
  sessionStart({ cwd: w.dir, session_id: sid, source: 'compact' });
  assert.equal(message(stop({ cwd: w.dir, session_id: sid })), `temper: gate green and review approved (fingerprint ${fp}).`);
});

test('SubagentStart (reviewer): a worktree on a later commit than the session base names its own base to the gate', { skip: noGit }, (t) => {
  const r = repo(t);
  const sid = 's-wt-ahead';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  // the main session commits, then a worktree starts from the new HEAD
  r.put('README.md', 'later\n');
  r.git('add', '-A');
  r.git('-c', 'user.email=t@example.com', '-c', 'user.name=t', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'later');
  const head = r.git('rev-parse', 'HEAD');
  const w = worktree(r, 'agent-e1');
  subagentEdits(w, sid, 'e1', LIB, LIB_RS_2);
  const said = context(subagentStart({ cwd: r.dir, session_id: sid, agent_type: 'temper:rust-reviewer', agent_id: 'rv1' }));
  assert.match(said, /^- Target: the worktree \.claude\/worktrees\/agent-e1\. /m);
  assert.ok(said.includes(`&& TEMPER_BASE=${head} node "`), said);
  assert.match(said, new RegExp(`^- Base revision: ${head}; `, 'm'));
  assert.match(said, /^- Changed Rust files \(1\): /m);
});

test('Stop: a removed worktree whose branch holds unmerged commits is reported, not gated', { skip: noGit }, (t) => {
  const r = repo(t);
  const sid = 's-wt-gone';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  const dir = r.file('.claude/worktrees/agent-d1');
  r.git('worktree', 'add', '-q', '-b', 'agent-d1', dir);
  const w = { dir, file: (rel) => path.join(dir, ...rel.split('/')) };
  preEdit({ cwd: dir, session_id: sid, agent_id: 'd1', agent_type: 'general-purpose', tool_name: 'Write', tool_input: { file_path: w.file(LIB), content: LIB_RS_2 } });
  fs.writeFileSync(w.file(LIB), LIB_RS_2);
  execFileSync('git', ['-c', 'user.email=t@example.com', '-c', 'user.name=t', '-c', 'commit.gpgsign=false', 'commit', '-qam', 'two'], { cwd: dir });
  r.git('worktree', 'remove', dir);
  assert.equal(message(stop({ cwd: r.dir, session_id: sid })), 'temper: the worktree .claude/worktrees/agent-d1 was removed, and its branch agent-d1 holds commits the main checkout does not have. They were not gated or reviewed here; CI checks them on their pull request.');
  // once merged into the main checkout, there is nothing to report (and the main checkout is gated instead)
  r.git('merge', '-q', '--ff-only', 'agent-d1');
  assert.equal(message(stop({ cwd: r.dir, session_id: sid, last_assistant_message: 'Status: WIP' })), 'temper: Rust changes left in progress; the gate and review were not enforced for this turn.');
});

test('Stop: a loop-breaker record from before #135 (one fp and kind) is still read', { skip: noCargo }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.gate(false);
  writeAtomic(path.join(sessionDir(r.proj(), 's-old'), 'stop-main.json'), JSON.stringify({ fp, kind: 'gate', at: Date.now() }));
  assert.match(message(stop({ cwd: r.dir, session_id: 's-old', stop_hook_active: true })), /^temper: stopped with a RED gate/);
});

test('Stop: with two red trees each is let through once stuck, without alternating blocks', { skip: noCargo }, (t) => {
  const r = repo(t);
  const sid = 's-wt-two';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  r.put(LIB, LIB_RS_2);
  r.gate(false);
  const w = worktree(r, 'agent-b1');
  subagentEdits(w, sid, 'b1', LIB, `${LIB_RS_2}\n/// Three.\n#[must_use]\npub fn three() -> u8 {\n    3\n}\n`);
  w.gate(sid, false);
  const input = { cwd: r.dir, session_id: sid };
  assert.match(blocked(stop(input)), /^temper gate: RED/);
  assert.match(blocked(stop({ ...input, stop_hook_active: true })), /^In the worktree \.claude\/worktrees\/agent-b1: temper gate: RED/);
  const through = message(stop({ ...input, stop_hook_active: true }));
  assert.equal(through, 'temper: stopped with a RED gate (no progress after the previous block). clippy failing.\ntemper: In the worktree .claude/worktrees/agent-b1: stopped with a RED gate (no progress after the previous block). clippy failing.');
});

test('SubagentStop (implementer): only the worktrees that implementer edited are gated', { skip: noCargo }, (t) => {
  const r = repo(t);
  const sid = 's-wt-im';
  sessionStart({ cwd: r.dir, session_id: sid, source: 'startup' });
  const mine = worktree(r, 'agent-im1');
  subagentEdits(mine, sid, 'im1', LIB, LIB_RS_2);
  const theirs = worktree(r, 'agent-x1');
  subagentEdits(theirs, sid, 'x1', LIB, `${LIB_RS_2}\n/// Three.\n#[must_use]\npub fn three() -> u8 {\n    3\n}\n`);
  theirs.gate(sid, false);
  const fp = mine.gate(sid, true);
  const im = { cwd: r.dir, session_id: sid, agent_type: 'temper:rust-implementer', agent_id: 'im1' };
  assert.equal(message(subagentStop(im)), `temper: In the worktree .claude/worktrees/agent-im1: gate green (fingerprint ${fp}).`);
  // the main session answers for both
  assert.match(blocked(stop({ cwd: r.dir, session_id: sid })), /^In the worktree \.claude\/worktrees\/agent-im1: The gate is green|^In the worktree \.claude\/worktrees\/agent-x1: temper gate: RED/);
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
  assert.match(none, /End your final report with a corrected ```json verdict block\.$/);
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
  // a hand-back report (no last_assistant_message) carries the Status line too
  r.gate(false);
  assert.equal(message(subagentStop({ ...input, agent_transcript_path: transcript(r, 'Status: WIP\nhalfway') })), 'temper: Rust changes left in progress; the gate and review were not enforced for this turn.');
});

test('SubagentStop: other agent types are not checked', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  for (const agent_type of ['general-purpose', 'temper:rust-architect', undefined]) {
    assert.deepEqual(subagentStop({ cwd: r.dir, session_id: 's-other', agent_type, agent_id: 'x1' }), {}, String(agent_type));
  }
});

test('SubagentStart and SubagentStop: every agent type is recorded in telemetry before filtering', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  assert.deepEqual(subagentStart({ cwd: r.dir, session_id: 's-tel', agent_type: 'general-purpose', agent_id: 'g1' }), {});
  assert.deepEqual(subagentStop({ cwd: r.dir, session_id: 's-tel', agent_type: 'general-purpose', agent_id: 'g1', agent_transcript_path: '/nowhere.jsonl' }), {});
  const rows = readTelemetry(r.proj()).filter((x) => x.sid === 's-tel');
  assert.deepEqual(rows.map((x) => [x.event, x.type]), [['subagent-start', 'general-purpose'], ['subagent-stop', 'general-purpose']]);
  assert.equal(rows[1].msg, false);
  assert.equal(rows[1].transcript, true);
});

// ------------------------------------------- hand-back delivery (cloud sessions, #97)

/** A subagent transcript that ends with a SubagentHandback call carrying `message`. */
function transcript(r, message, { text = 'Working on it.' } = {}) {
  const file = path.join(r.dir, '.temper', 'agent.jsonl');
  const rows = [
    { type: 'user', message: { role: 'user', content: 'review the change' } },
    { type: 'assistant', message: { role: 'assistant', content: [{ type: 'text', text }] } },
    { type: 'assistant', message: { role: 'assistant', content: [{ type: 'tool_use', id: 't1', name: 'SubagentHandback', input: { message } }] } },
    { type: 'user', message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: 't1', content: 'Report delivered to your caller.' }] } },
  ];
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, rows.map((x) => JSON.stringify(x)).join('\n') + '\n');
  return file;
}

test('reportFromTranscript: the last hand-back message, else the last assistant text', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-tx-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const r = { dir };
  assert.equal(reportFromTranscript(transcript(r, 'the report')), 'the report');
  const plain = path.join(dir, 'plain.jsonl');
  fs.writeFileSync(plain, [
    JSON.stringify({ message: { role: 'assistant', content: [{ type: 'text', text: 'final words' }] } }),
    'not json',
    JSON.stringify({ message: { role: 'user', content: 'ignored' } }),
  ].join('\n'));
  assert.equal(reportFromTranscript(plain), 'final words');
  assert.equal(reportFromTranscript(path.join(dir, 'missing.jsonl')), null);
  assert.equal(reportFromTranscript(undefined), null);
});

test('SubagentStop (reviewer): without last_assistant_message the verdict is read from the hand-back in the transcript', { skip: noGit }, (t) => {
  const r = repo(t);
  r.put(LIB, LIB_RS_2);
  const fp = r.fp();
  const input = { cwd: r.dir, session_id: 's-rv-tx', agent_type: 'temper:rust-reviewer', agent_id: 'rv1' };
  assert.match(blocked(subagentStop({ ...input, agent_transcript_path: transcript(r, 'Looks fine to me.') })), /- no verdict block found/);
  assert.deepEqual(subagentStop({ ...input, agent_transcript_path: transcript(r, fenced(verdict(fp))) }), {});
  assert.equal(verdictFor(r.proj(), fp).verdict, 'approved');
  const row = readTelemetry(r.proj()).find((x) => x.sid === 's-rv-tx' && x.event === 'verdict');
  assert.equal(row.via, 'subagent-stop');
});

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
  // SubagentStop after the hand-back: no last_assistant_message, and the verdict is already recorded
  assert.deepEqual(subagentStop({ ...input, tool_name: undefined }), {});
  const rows = readTelemetry(r.proj()).filter((x) => x.sid === 's-hb');
  assert.deepEqual(rows.filter((x) => x.event === 'handback').map((x) => x.reviewer), [false, true, true]);
  assert.equal(rows.find((x) => x.event === 'verdict').via, 'handback');
  assert.equal(rows.filter((x) => x.event === 'verdict-invalid').length, 1);
});
