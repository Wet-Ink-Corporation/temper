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
  assert.equal(summarizeFmt(BAD_TOML), [
    'cargo cannot load the workspace, so nothing was formatted:',
    'error: duplicate key',
    '--> crates/a/Cargo.toml:3:1',
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
    assert.match(report.actions[0], /; rerun with --vendor-only to replace only the vendored CLI \(or --force to also overwrite every template file\)$/);
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
