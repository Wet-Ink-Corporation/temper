// Hook and CLI message text. Each test also pins the decision or exit code the message comes
// with, which these message changes leave as they were.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { preEdit, IRON_RULES } from '../scripts/lib/hooks.mjs';
import { summarizeFmt } from '../scripts/lib/gate.mjs';
import { vendorTool } from '../scripts/lib/vendor.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const CLI = path.join(here, '..', 'scripts', 'temper.mjs');

const cli = (cwd, args, { input, env: extra = {} } = {}) => {
  const env = { ...process.env, ...extra };
  delete env.TEMPER_BASE;
  delete env.CLAUDECODE;
  if (!('TEMPER_DISABLE' in extra)) delete env.TEMPER_DISABLE;
  const r = spawnSync(process.execPath, [CLI, ...args], { cwd, encoding: 'utf8', env, input });
  return { code: r.status, stdout: r.stdout, out: `${r.stdout}${r.stderr}` };
};

let ws;
let pluginData;
const savedPluginData = process.env.CLAUDE_PLUGIN_DATA;

before(() => {
  ws = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-messages-'));
  pluginData = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-messages-data-'));
  process.env.CLAUDE_PLUGIN_DATA = pluginData;
  fs.writeFileSync(path.join(ws, 'Cargo.toml'), '[workspace]\nmembers = ["crates/*"]\n');
});

after(() => {
  if (savedPluginData === undefined) delete process.env.CLAUDE_PLUGIN_DATA;
  else process.env.CLAUDE_PLUGIN_DATA = savedPluginData;
  fs.rmSync(ws, { recursive: true, force: true });
  fs.rmSync(pluginData, { recursive: true, force: true });
});

test('the skill-gate denial lists every Iron rule, H-01 to H-16', () => {
  // No agent_type and a session with no skill marker: the gate fires.
  const res = preEdit({
    tool_name: 'Write', cwd: ws, session_id: `messages-${process.pid}-${Date.now()}`,
    tool_input: { file_path: path.join(ws, 'crates', 'a', 'src', 'lib.rs'), content: '//! A.\n' },
  });
  const out = res.json?.hookSpecificOutput;
  assert.equal(out?.permissionDecision, 'deny');
  assert.match(out.permissionDecisionReason, /the rust-craft standard is not loaded/);
  assert.equal(IRON_RULES.length, 16);
  for (const rule of IRON_RULES) assert.ok(out.permissionDecisionReason.includes(rule), rule.slice(0, 5));
  assert.match(out.permissionDecisionReason, /H-13 .*H-16 /s);
});

test('a hook given malformed input says nothing was checked, and still exits 0 with no decision', () => {
  const r = cli(ws, ['hook', 'pre-edit'], { input: '{"tool_name": "Write",' });
  assert.equal(r.code, 0, r.out);
  const msg = JSON.parse(r.stdout);
  assert.deepEqual(Object.keys(msg), ['systemMessage']);
  assert.equal(msg.systemMessage, 'temper hook pre-edit: could not parse the hook input as JSON; nothing was checked');
  // Empty input is still read as {}, and TEMPER_DISABLE=1 stays silent.
  assert.doesNotMatch(cli(ws, ['hook', 'pre-edit'], { input: '' }).stdout, /could not parse/);
  const off = cli(ws, ['hook', 'pre-edit'], { input: 'not json', env: { TEMPER_DISABLE: '1' } });
  assert.equal(off.code, 0);
  assert.equal(off.stdout, '');
});

// Captured from `cargo fmt --all` (cargo 1.9x) in a workspace whose `crates/*` matches no crate.
const NO_MEMBER = [
  '`cargo metadata` exited with an error: error: failed to load manifest for workspace member `/tmp/ws/crates/*`',
  'referenced by workspace at `/tmp/ws/Cargo.toml`',
  '',
  'Caused by:',
  '  failed to read `/tmp/ws/crates/*/Cargo.toml`',
  '',
  'Caused by:',
  '  No such file or directory (os error 2)',
  '',
  'This utility formats all bin and lib files of the current crate using rustfmt.',
  '',
  'Usage: cargo fmt [OPTIONS] [-- <rustfmt_options>...]',
  '',
  'Options:',
  '  -h, --help',
  '          Print help',
].join('\n');

// The same, for a member manifest that does not parse.
const BAD_TOML = [
  '`cargo metadata` exited with an error: error: duplicate key',
  ' --> crates/a/Cargo.toml:3:1',
  '  |',
  '3 | name="b"',
  '  | ^^^^',
  'error: failed to load manifest for workspace member `/tmp/ws/crates/a`',
  'referenced via `crates/*` by workspace at `/tmp/ws/Cargo.toml`',
  '',
  'This utility formats all bin and lib files of the current crate using rustfmt.',
  '',
  'Usage: cargo fmt [OPTIONS] [-- <rustfmt_options>...]',
].join('\n');

test('fmt failing because the workspace does not load is summarised as that, not as rustfmt help', () => {
  for (const check of [false, true]) {
    const s = summarizeFmt(NO_MEMBER, { check });
    assert.equal(s, [
      'cargo cannot load the workspace, so nothing was formatted:',
      'error: failed to load manifest for workspace member `crates/*`',
      'no crate matches the workspace member `crates/*`: create one with `cargo new --lib crates/<name>`, or change `members` in Cargo.toml',
    ].join('\n'));
    assert.doesNotMatch(s, /Usage|Print help/);
  }
  // A wildcard in the middle of the pattern: the suggestion keeps the segments around it.
  const NESTED = NO_MEMBER.replaceAll('crates/*', 'crates/*/sub');
  assert.match(summarizeFmt(NESTED), /no crate matches the workspace member `crates\/\*\/sub`: create one with `cargo new --lib crates\/<name>\/sub`/);
  assert.equal(summarizeFmt(BAD_TOML), [
    'cargo cannot load the workspace, so nothing was formatted:',
    'error: duplicate key',
    '--> crates/a/Cargo.toml:3:1',
    'error: failed to load manifest for workspace member `crates/a`',
  ].join('\n'));
  // Windows paths under the workspace are shortened and get forward slashes; other backticked
  // tokens are left as cargo wrote them.
  const WIN = [
    '`cargo metadata` exited with an error: error: unexpected `a\\b` here',
    'error: failed to load manifest for workspace member `C:\\ws\\crates\\a`',
    'referenced via `crates/*` by workspace at `C:\\ws\\Cargo.toml`',
  ].join('\n');
  assert.equal(summarizeFmt(WIN), [
    'cargo cannot load the workspace, so nothing was formatted:',
    'error: unexpected `a\\b` here',
    'error: failed to load manifest for workspace member `crates/a`',
  ].join('\n'));
  // Other fmt failures keep their summaries.
  assert.equal(summarizeFmt('error: expected item, found `}`\n --> src/lib.rs:1:1\n'), 'error: expected item, found `}`');
  assert.equal(summarizeFmt('Diff in src/lib.rs', { check: true }), 'needs formatting: the implementer runs `cargo fmt --all` (or the gate without --check)');
});

test('vendor drift points to --vendor-only first, --force as the broader option', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-messages-vendor-'));
  try {
    vendorTool(dir, {});
    fs.writeFileSync(path.join(dir, '.claude', 'temper', 'VERSION'), '0.0.1\n');
    const report = vendorTool(dir, {});
    assert.equal(report.drift, true);
    assert.match(report.actions[0], /; rerun with --vendor-only to replace only the vendored CLI \(or --force to also overwrite the other template files; an existing Cargo\.toml is never overwritten\)$/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('setup appends /target/ to .gitignore once, unless a target line is there; never Cargo.lock', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-messages-setup-'));
  const gi = path.join(dir, '.gitignore');
  const read = () => fs.readFileSync(gi, 'utf8');
  try {
    fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[workspace]\nmembers = []\n');
    // A dry run reports both lines and writes nothing.
    const dry = cli(dir, ['setup', '--dry-run']);
    assert.equal(dry.code, 0, dry.out);
    assert.match(dry.out, /append \.gitignore: \.temper\/\n {2}append \.gitignore: \/target\//);
    assert.ok(!fs.existsSync(gi));
    // --vendor-only leaves .gitignore alone.
    const vo = cli(dir, ['setup', '--vendor-only']);
    assert.equal(vo.code, 0, vo.out);
    assert.ok(!fs.existsSync(gi));
    // A full setup adds both; a second run adds nothing.
    assert.equal(cli(dir, ['setup']).code, 0);
    assert.equal(read(), '.temper/\n/target/\n');
    const again = cli(dir, ['setup']);
    assert.equal(again.code, 0, again.out);
    assert.doesNotMatch(again.out, /append \.gitignore/);
    assert.equal(read(), '.temper/\n/target/\n');
    assert.doesNotMatch(read(), /Cargo\.lock/);
    // Every spelling of an existing target line counts; a file without a final newline gets one.
    for (const line of ['target', 'target/', '/target', '/target/']) {
      fs.writeFileSync(gi, `${line}\n.temper/`);
      const r = cli(dir, ['setup', '--dry-run']);
      assert.doesNotMatch(r.out, /append \.gitignore/, line);
    }
    fs.writeFileSync(gi, 'node_modules');
    cli(dir, ['setup']);
    assert.equal(read(), 'node_modules\n.temper/\n/target/\n');
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

// ------------------------------------------- documented minimum versions (README Install)
// Message only: an old Node or Claude Code adds a warning and changes no hook decision.

const README = () => fs.readFileSync(path.join(here, '..', 'README.md'), 'utf8');
// Run the real CLI as if Node were `version`: the preload rewrites process.versions.node before temper.mjs loads.
const cliAsNode = (version, cwd, args, input) => {
  const env = { ...process.env };
  for (const k of ['TEMPER_BASE', 'CLAUDECODE', 'TEMPER_DISABLE', 'CLAUDE_ENV_FILE']) delete env[k];
  const fake = `data:text/javascript,Object.defineProperty(process.versions,"node",{value:${JSON.stringify(version)}})`;
  const r = spawnSync(process.execPath, ['--import', fake, CLI, ...args], { cwd, encoding: 'utf8', env, input });
  return { code: r.status, stdout: r.stdout, stderr: r.stderr };
};

test('the minimum versions are the README Install minimums, in one constant', async () => {
  const { MINIMUM_VERSIONS } = await import('../scripts/lib/requirements.mjs');
  const readme = README();
  const [major] = MINIMUM_VERSIONS.node.split('.');
  assert.ok(readme.includes(`**Claude Code ${MINIMUM_VERSIONS.claudeCode} or later.**`), 'README Claude Code minimum');
  assert.ok(readme.includes(`**Node ${major} or later**`), 'README Node minimum');
  assert.ok(Object.isFrozen(MINIMUM_VERSIONS));
});

test('isOlder compares numerically and treats an unparsable version as not older', async () => {
  const { isOlder } = await import('../scripts/lib/requirements.mjs');
  for (const [v, min] of [['16.20.2', '18.0.0'], ['17.9.1', '18.0.0'], ['v14.21.3', '18.0.0'], ['2.1.285', '2.1.286'], ['2.0.999', '2.1.286'], ['1.9.0', '2.1.286']]) {
    assert.equal(isOlder(v, min), true, `${v} < ${min}`);
  }
  for (const [v, min] of [['18.0.0', '18.0.0'], ['18.20.4', '18.0.0'], ['22.22.0', '18.0.0'], ['2.1.286', '2.1.286'], ['2.1.292', '2.1.286'], ['2.2.0', '2.1.286'], ['10.0.0', '2.1.286'], ['', '2.1.286'], [null, '2.1.286'], ['unknown', '18.0.0']]) {
    assert.equal(isOlder(v, min), false, `${v} >= ${min} or unknown`);
  }
});

test('the Claude Code version comes from CLAUDE_CODE_EXECPATH --version, and unknown stays unknown', async () => {
  const { claudeCodeVersion, parseClaudeCodeVersion } = await import('../scripts/lib/requirements.mjs');
  assert.equal(parseClaudeCodeVersion('2.1.292 (Claude Code)\n'), '2.1.292');
  assert.equal(parseClaudeCodeVersion('v22.22.0\n'), null);
  assert.equal(parseClaudeCodeVersion(''), null);
  const exe = path.join(os.tmpdir(), 'no-such-dir-temper', process.platform === 'win32' ? 'claude.exe' : 'claude');
  const calls = [];
  const fake = (out) => (cmd, args, opts) => { calls.push({ cmd, args, opts }); return out; };
  assert.equal(claudeCodeVersion({ CLAUDE_CODE_EXECPATH: exe }, fake({ status: 0, stdout: '2.1.200 (Claude Code)\n' })), '2.1.200');
  assert.deepEqual(calls[0].args, ['--version']);
  assert.equal(calls[0].cmd, exe);
  assert.ok(calls[0].opts.timeout > 0 && calls[0].opts.timeout <= 3000, 'bounded spawn');
  assert.equal(claudeCodeVersion({ CLAUDE_CODE_EXECPATH: exe }, fake({ status: 1, stdout: '2.1.200 (Claude Code)\n' })), null);
  assert.equal(claudeCodeVersion({ CLAUDE_CODE_EXECPATH: exe }, fake({ status: null, stdout: null, error: new Error('ENOENT') })), null);
  assert.equal(claudeCodeVersion({ CLAUDE_CODE_EXECPATH: exe }, () => { throw new Error('boom'); }), null);
  // Never spawned: no path, or a path that is not a claude binary (an npm install runs under node).
  calls.length = 0;
  assert.equal(claudeCodeVersion({}, fake({ status: 0, stdout: '2.1.200 (Claude Code)\n' })), null);
  assert.equal(claudeCodeVersion({ CLAUDE_CODE_EXECPATH: process.execPath }, fake({ status: 0, stdout: '2.1.200 (Claude Code)\n' })), null);
  // CLAUDE_CODE_VERSION is not trusted: other tools set it to something else.
  assert.equal(claudeCodeVersion({ CLAUDE_CODE_VERSION: '2.1.42' }, fake({ status: 0, stdout: '2.1.200 (Claude Code)\n' })), null);
  assert.equal(calls.length, 0);
  // A real spawn of a missing binary is unknown, not an error.
  assert.equal(claudeCodeVersion({ CLAUDE_CODE_EXECPATH: exe }), null);
});

test('runtimeWarnings: one line per runtime below its minimum; current or unknown versions say nothing', async () => {
  const { runtimeWarnings } = await import('../scripts/lib/requirements.mjs');
  const [node] = runtimeWarnings({ node: '16.20.2', claudeCode: '2.1.292' });
  assert.match(node, /^temper: Node 16\.20\.2 is older than the documented minimum, Node 18\.0\.0\b/);
  assert.match(node, /PATH that Claude Code sees/);
  const [cc] = runtimeWarnings({ node: process.versions.node, claudeCode: '2.1.200' });
  assert.match(cc, /^temper: Claude Code 2\.1\.200 is older than the documented minimum, Claude Code 2\.1\.286\b/);
  assert.equal(runtimeWarnings({ node: '16.20.2', claudeCode: '2.1.200' }).length, 2);
  assert.deepEqual(runtimeWarnings({ node: '18.0.0', claudeCode: '2.1.286' }), []);
  assert.deepEqual(runtimeWarnings({ node: process.versions.node, claudeCode: null }), []);
  assert.deepEqual(runtimeWarnings({}), []);
});

test('earlyNodeCheck: on an old Node, SessionStart reports it; other hooks run as before; commands warn on stderr', async () => {
  const { earlyNodeCheck } = await import('../scripts/lib/requirements.mjs');
  const start = earlyNodeCheck('16.20.2', ['hook', 'session-start'], {});
  assert.equal(start.exit, 0);
  const json = JSON.parse(start.stdout);
  assert.match(json.systemMessage, /^temper: Node 16\.20\.2 is older than the documented minimum/);
  assert.equal(json.hookSpecificOutput.hookEventName, 'SessionStart');
  assert.equal(json.hookSpecificOutput.additionalContext, json.systemMessage);
  assert.ok(!('decision' in json) && !('permissionDecision' in json.hookSpecificOutput));
  for (const event of ['pre-bash', 'pre-edit', 'stop', 'user-prompt']) assert.equal(earlyNodeCheck('16.20.2', ['hook', event], {}), null, event);
  const gate = earlyNodeCheck('17.9.1', ['gate'], {});
  assert.match(gate.stderr, /^temper: Node 17\.9\.1 is older than the documented minimum.*\n$/);
  assert.ok(!('stdout' in gate) && !('exit' in gate), 'the command still runs, with its own exit code');
  // Near misses: a supported Node, or the human emergency switch.
  for (const v of ['18.0.0', '18.20.4', '20.11.1', process.versions.node]) assert.equal(earlyNodeCheck(v, ['hook', 'session-start'], {}), null, v);
  assert.equal(earlyNodeCheck('16.20.2', ['hook', 'session-start'], { TEMPER_DISABLE: '1' }), null);
});

test('the CLI on a simulated old Node: SessionStart warns, a PreToolUse hook and the gate are unchanged', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-oldnode-'));
  try {
    const input = JSON.stringify({ cwd: dir, session_id: 's-old', source: 'startup', hook_event_name: 'SessionStart' });
    const start = cliAsNode('16.20.2', dir, ['hook', 'session-start'], input);
    assert.equal(start.code, 0, start.stderr);
    const json = JSON.parse(start.stdout);
    assert.match(json.systemMessage, /^temper: Node 16\.20\.2 is older than the documented minimum, Node 18\.0\.0/);
    assert.equal(json.hookSpecificOutput.additionalContext, json.systemMessage);
    // A PreToolUse hook outside a Rust project: same empty answer as on a current Node.
    const bash = JSON.stringify({ cwd: dir, session_id: 's-old', hook_event_name: 'PreToolUse', tool_name: 'Bash', tool_input: { command: 'cargo test' } });
    const old = cliAsNode('16.20.2', dir, ['hook', 'pre-bash'], bash);
    const cur = cliAsNode(process.versions.node, dir, ['hook', 'pre-bash'], bash);
    assert.deepEqual({ code: old.code, stdout: old.stdout }, { code: cur.code, stdout: cur.stdout });
    // A command keeps its exit code and output; stderr gains the warning line first.
    const gate = cliAsNode('16.20.2', dir, ['gate'], '');
    const gateNow = cliAsNode(process.versions.node, dir, ['gate'], '');
    assert.equal(gate.code, gateNow.code);
    assert.equal(gate.stdout, gateNow.stdout);
    assert.ok(gate.stderr.startsWith('temper: Node 16.20.2 is older than the documented minimum'), gate.stderr);
    assert.ok(gate.stderr.endsWith(gateNow.stderr), gate.stderr);
    // Current Node: SessionStart outside a Rust project still prints nothing.
    const now = cliAsNode(process.versions.node, dir, ['hook', 'session-start'], input);
    assert.deepEqual({ code: now.code, stdout: now.stdout }, { code: 0, stdout: '' });
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
