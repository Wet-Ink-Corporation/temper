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
// Message only: an old Node adds a warning and changes no hook decision.

const README = () => fs.readFileSync(path.join(here, '..', 'README.md'), 'utf8');
// Run the real CLI as if Node were `version`: the preload rewrites process.versions.node before
// temper.mjs loads. `noClone` also removes structuredClone, as on Node 16, where temper cannot run.
const cliAsNode = (version, cwd, args, input, { noClone = false, env: extra = {} } = {}) => {
  const env = { ...process.env, ...extra };
  for (const k of ['TEMPER_BASE', 'CLAUDECODE', 'TEMPER_DISABLE', 'CLAUDE_ENV_FILE', 'CLAUDE_PROJECT_DIR']) if (!(k in extra)) delete env[k];
  const fake = `data:text/javascript,Object.defineProperty(process.versions,"node",{value:${JSON.stringify(version)}});${noClone ? 'delete globalThis.structuredClone;' : ''}`;
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

test('runtimeWarnings: a line for a Node below the minimum; a current or unknown Node says nothing', async () => {
  const { runtimeWarnings } = await import('../scripts/lib/requirements.mjs');
  const [node] = runtimeWarnings({ node: '16.20.2' });
  assert.match(node, /^temper: Node 16\.20\.2 is older than the documented minimum, Node 18\.0\.0\b/);
  assert.match(node, /PATH that Claude Code sees/);
  assert.deepEqual(runtimeWarnings({ node: '18.0.0' }), []);
  assert.deepEqual(runtimeWarnings({ node: process.versions.node }), []);
  assert.deepEqual(runtimeWarnings({}), []);
});

test('earlyNodeCheck: SessionStart stops early only where temper cannot run, and only in a Rust workspace', async () => {
  const { earlyNodeCheck } = await import('../scripts/lib/requirements.mjs');
  const norust = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-early-'));
  try {
    const nested = path.join(ws, 'crates', 'core');
    // Node 16: no structuredClone, so the handler would throw. Report the old Node instead.
    for (const cwd of [ws, nested]) {
      const start = await earlyNodeCheck('16.20.2', ['hook', 'session-start'], {}, { canRun: false, cwd });
      assert.equal(start.exit, 0);
      const json = JSON.parse(start.stdout);
      assert.match(json.systemMessage, /^temper: Node 16\.20\.2 is older than the documented minimum/);
      assert.equal(json.hookSpecificOutput.hookEventName, 'SessionStart');
      assert.equal(json.hookSpecificOutput.additionalContext, json.systemMessage);
      assert.ok(!('decision' in json) && !('permissionDecision' in json.hookSpecificOutput));
    }
    // The hook input's cwd decides, as in SessionStart; the process directory is the fallback.
    let reads = 0;
    const stdinOf = (input) => () => { reads += 1; return Promise.resolve(JSON.stringify(input)); };
    const named = await earlyNodeCheck('16.20.2', ['hook', 'session-start'], {}, { canRun: false, cwd: norust, readStdin: stdinOf({ cwd: ws }) });
    assert.equal(named.exit, 0);
    const elsewhere = JSON.stringify({ cwd: norust, source: 'startup' });
    const away = await earlyNodeCheck('16.20.2', ['hook', 'session-start'], { CLAUDE_PROJECT_DIR: ws }, { canRun: false, cwd: ws, readStdin: () => Promise.resolve(elsewhere) });
    assert.deepEqual(away, { stdin: elsewhere }, 'carries on and hands the input it read to the hook');
    for (const raw of ['', 'not json', JSON.stringify({ cwd: path.join(norust, 'gone') })]) {
      assert.equal((await earlyNodeCheck('16.20.2', ['hook', 'session-start'], {}, { canRun: false, cwd: ws, readStdin: () => Promise.resolve(raw) })).exit, 0, raw);
    }
    // Near misses that carry on unchanged: outside a Rust workspace (SessionStart does nothing there),
    // a Node 17 where temper runs (the handler adds the warning itself), and every other hook.
    assert.deepEqual(await earlyNodeCheck('16.20.2', ['hook', 'session-start'], {}, { canRun: false, cwd: norust }), { stdin: '' });
    assert.equal(await earlyNodeCheck('17.9.1', ['hook', 'session-start'], {}, { canRun: true, cwd: ws, readStdin: stdinOf({}) }), null);
    for (const event of ['pre-bash', 'pre-edit', 'stop', 'user-prompt']) assert.equal(await earlyNodeCheck('16.20.2', ['hook', event], {}, { canRun: false, cwd: ws, readStdin: stdinOf({}) }), null, event);
    const gate = await earlyNodeCheck('17.9.1', ['gate'], {}, { canRun: true, cwd: ws });
    assert.match(gate.stderr, /^temper: Node 17\.9\.1 is older than the documented minimum.*\n$/);
    assert.ok(!('stdout' in gate) && !('exit' in gate), 'the command still runs, with its own exit code');
    for (const v of ['18.0.0', '18.20.4', '20.11.1', process.versions.node]) assert.equal(await earlyNodeCheck(v, ['hook', 'session-start'], {}, { canRun: false, cwd: ws, readStdin: stdinOf({}) }), null, v);
    assert.equal(await earlyNodeCheck('16.20.2', ['hook', 'session-start'], { TEMPER_DISABLE: '1' }, { canRun: false, cwd: ws, readStdin: stdinOf({}) }), null);
    assert.equal(reads, 1, 'stdin is read only on the path that needs the input');
  } finally { fs.rmSync(norust, { recursive: true, force: true }); }
});

test('the CLI on a simulated Node 17: SessionStart runs in full and adds the warning', () => {
  const envDir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-envfile-'));
  try {
    const envFile = path.join(envDir, 'env.sh');
    fs.writeFileSync(envFile, '');
    const input = JSON.stringify({ cwd: ws, session_id: 's-node17', source: 'startup', hook_event_name: 'SessionStart' });
    const res = cliAsNode('17.9.1', ws, ['hook', 'session-start'], input, { env: { CLAUDE_ENV_FILE: envFile } });
    assert.equal(res.code, 0, res.stderr);
    const json = JSON.parse(res.stdout);
    assert.match(json.systemMessage, /^temper: Node 17\.9\.1 is older than the documented minimum, Node 18\.0\.0/);
    const context = json.hookSpecificOutput.additionalContext;
    assert.match(context, /governed by the temper plugin/);
    assert.match(context, /- Gate command: /);
    assert.ok(context.endsWith(`\n- ${json.systemMessage}`), context);
    assert.match(fs.readFileSync(envFile, 'utf8'), /^export TEMPER_CLI="/);
  } finally { fs.rmSync(envDir, { recursive: true, force: true }); }
});

test('the CLI on a simulated Node 16: SessionStart warns in a Rust workspace; elsewhere, hooks and the gate are unchanged', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-oldnode-'));
  try {
    const rustInput = JSON.stringify({ cwd: ws, session_id: 's-old', source: 'startup', hook_event_name: 'SessionStart' });
    const start = cliAsNode('16.20.2', ws, ['hook', 'session-start'], rustInput, { noClone: true });
    assert.equal(start.code, 0, start.stderr);
    const json = JSON.parse(start.stdout);
    assert.match(json.systemMessage, /^temper: Node 16\.20\.2 is older than the documented minimum, Node 18\.0\.0/);
    assert.equal(json.hookSpecificOutput.additionalContext, json.systemMessage);
    // Outside a Rust workspace SessionStart prints nothing, as on a current Node.
    const input = JSON.stringify({ cwd: dir, session_id: 's-old', source: 'startup', hook_event_name: 'SessionStart' });
    const away = cliAsNode('16.20.2', dir, ['hook', 'session-start'], input, { noClone: true });
    assert.deepEqual({ code: away.code, stdout: away.stdout }, { code: 0, stdout: '' });
    // A PreToolUse hook outside a Rust project: same empty answer as on a current Node.
    const bash = JSON.stringify({ cwd: dir, session_id: 's-old', hook_event_name: 'PreToolUse', tool_name: 'Bash', tool_input: { command: 'cargo test' } });
    const old = cliAsNode('16.20.2', dir, ['hook', 'pre-bash'], bash, { noClone: true });
    const cur = cliAsNode(process.versions.node, dir, ['hook', 'pre-bash'], bash);
    assert.deepEqual({ code: old.code, stdout: old.stdout }, { code: cur.code, stdout: cur.stdout });
    // A command keeps its exit code and output; stderr gains the warning line first.
    const gate = cliAsNode('16.20.2', dir, ['gate'], '', { noClone: true });
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

test('the CLI on a simulated Node 16: SessionStart judges the workspace by the hook input cwd, as the handler does', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-oldnode-cwd-'));
  try {
    // The hook process runs outside the project, but the input names the Rust workspace: warn.
    const rustInput = JSON.stringify({ cwd: ws, session_id: 's-cwd', source: 'startup', hook_event_name: 'SessionStart' });
    const start = cliAsNode('16.20.2', dir, ['hook', 'session-start'], rustInput, { noClone: true });
    assert.equal(start.code, 0, start.stderr);
    assert.match(JSON.parse(start.stdout).systemMessage, /^temper: Node 16\.20\.2 is older than the documented minimum/);
    // The hook process runs in the Rust workspace, but the input names another directory: the
    // handler gets the same input (stdin is read once and passed on) and prints nothing.
    const input = JSON.stringify({ cwd: dir, session_id: 's-cwd', source: 'startup', hook_event_name: 'SessionStart' });
    const away = cliAsNode('16.20.2', ws, ['hook', 'session-start'], input, { noClone: true });
    assert.deepEqual({ code: away.code, stdout: away.stdout }, { code: 0, stdout: '' });
    // CLAUDE_PROJECT_DIR does not decide it either: the handler never reads it.
    const env = cliAsNode('16.20.2', dir, ['hook', 'session-start'], input, { noClone: true, env: { CLAUDE_PROJECT_DIR: ws } });
    assert.deepEqual({ code: env.code, stdout: env.stdout }, { code: 0, stdout: '' });
    // No cwd in the input: both fall back to the process directory.
    const bare = cliAsNode('16.20.2', ws, ['hook', 'session-start'], JSON.stringify({ session_id: 's-cwd', source: 'startup' }), { noClone: true });
    assert.equal(bare.code, 0, bare.stderr);
    assert.match(JSON.parse(bare.stdout).systemMessage, /^temper: Node 16\.20\.2 is older than the documented minimum/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
