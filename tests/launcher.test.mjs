// scripts/launch.mjs, the 2.0 launcher (port plan §5.1, Phase 0b): routing, binary resolution,
// and passing argv, stdin, the environment and the exit code through to the engine.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { route, resolveBinary, engineKey, runBinary, runEngine, RUST_READY } from '../scripts/launch.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const LAUNCH = path.join(here, '..', 'scripts', 'launch.mjs');

const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'temper-launch-'));

// A stand-in engine: node running this script. It echoes argv, stdin and two env vars as JSON,
// then exits with STUB_EXIT, or kills itself with STUB_SIGNAL.
function stubEngine(dir) {
  const file = path.join(dir, 'stub.mjs');
  fs.writeFileSync(file, `
    const chunks = [];
    for await (const c of process.stdin) chunks.push(c);
    process.stdout.write(JSON.stringify({
      argv: process.argv.slice(2),
      stdin: Buffer.concat(chunks).toString('utf8'),
      root: process.env.TEMPER_PLUGIN_ROOT,
      version: process.env.TEMPER_PLUGIN_VERSION,
    }));
    if (process.env.STUB_SIGNAL) process.kill(process.pid, process.env.STUB_SIGNAL);
    process.exitCode = Number(process.env.STUB_EXIT || 0);
  `);
  return file;
}

test('route: 1.x runs everything until a subcommand graduates; --version goes to the engine', () => {
  assert.equal(route(['hook', 'pre-edit'], {}), 'node', 'not opted in');
  assert.equal(route(['hook', 'pre-edit'], { TEMPER_ENGINE: 'rust' }), 'node', 'not graduated yet');
  assert.equal(route(['gate'], { TEMPER_ENGINE: 'rust' }), 'node', 'gate not graduated');
  assert.equal(route(['--version'], {}), 'engine', '--version is new in 2.0 (D16(g))');
  assert.equal(route([], {}), 'node', 'usage text from 1.x');
  assert.deepEqual([...RUST_READY], ['--version'], 'nothing has graduated in Phase 0b');
});

test('route: TEMPER_DISABLE=1 on a hook exits without an engine', () => {
  assert.equal(route(['hook', 'stop'], { TEMPER_DISABLE: '1' }), 'disabled', 'hook');
  assert.equal(route(['gate'], { TEMPER_DISABLE: '1' }), 'node', 'a command still runs');
});

test('engineKey: the plugin version for a cached copy, dev for a git checkout', () => {
  const dir = tmp();
  assert.equal(engineKey(dir, '2.0.0'), '2.0.0', 'no .git');
  fs.mkdirSync(path.join(dir, '.git'));
  assert.equal(engineKey(dir, '2.0.0'), 'dev', 'source checkout');
});

test('resolveBinary: TEMPER_BIN wins, then the npm package, then the built engine', () => {
  const root = tmp();
  const data = tmp();
  const exe = process.platform === 'win32' ? 'temper.exe' : 'temper';
  const pkg = path.join(root, 'node_modules', '@wet-ink', `temper-${process.platform}-${process.arch}`);
  fs.mkdirSync(path.join(pkg, 'bin'), { recursive: true });
  fs.writeFileSync(path.join(pkg, 'package.json'), '{"name":"x","version":"2.0.0"}');
  const built = path.join(data, 'engine', '2.0.0');
  fs.mkdirSync(built, { recursive: true });
  fs.writeFileSync(path.join(built, exe), '');
  const opts = { pluginRoot: root, version: '2.0.0', platform: process.platform, arch: process.arch };

  assert.equal(resolveBinary({ ...opts, env: { TEMPER_BIN: '/x/temper', CLAUDE_PLUGIN_DATA: data } }), '/x/temper', 'TEMPER_BIN');
  assert.equal(resolveBinary({ ...opts, env: { CLAUDE_PLUGIN_DATA: data } }), path.join(built, exe), 'package dir has no binary yet');
  fs.writeFileSync(path.join(pkg, 'bin', exe), '');
  assert.equal(resolveBinary({ ...opts, env: { CLAUDE_PLUGIN_DATA: data } }), path.join(pkg, 'bin', exe), 'npm package');
  // Off Windows the engine has no .exe, whatever platform runs this test.
  fs.writeFileSync(path.join(built, 'temper'), '');
  assert.equal(resolveBinary({ ...opts, platform: 'freebsd', env: { CLAUDE_PLUGIN_DATA: data } }), path.join(built, 'temper'), 'unsupported platform: the built engine');
  assert.equal(resolveBinary({ ...opts, platform: 'freebsd', env: {} }), null, 'nothing');
});

test("resolveBinary: only the plugin directory's own node_modules counts, not an ancestor's", () => {
  const outer = tmp();
  const root = path.join(outer, 'plugin');
  fs.mkdirSync(root);
  const exe = process.platform === 'win32' ? 'temper.exe' : 'temper';
  const stray = path.join(outer, 'node_modules', '@wet-ink', `temper-${process.platform}-${process.arch}`);
  fs.mkdirSync(path.join(stray, 'bin'), { recursive: true });
  fs.writeFileSync(path.join(stray, 'package.json'), '{"name":"x","version":"9.9.9"}');
  fs.writeFileSync(path.join(stray, 'bin', exe), '');
  assert.equal(resolveBinary({ env: {}, pluginRoot: root, version: '2.0.0', platform: process.platform, arch: process.arch }), null, 'a stray engine above the plugin is not run');
});

test('runBinary: argv, stdin, the plugin env and the exit code pass through', async () => {
  const dir = tmp();
  const stub = stubEngine(dir);
  const out = [];
  const code = await runBinary(process.execPath, [stub, 'hook', 'pre-bash', 'a b'], {
    env: { ...process.env, STUB_EXIT: '3' },
    pluginRoot: '/plugin',
    version: '2.0.0-alpha.1',
    stdin: '{"tool":"Bash"}',
    onStdout: (chunk) => out.push(chunk),
  });
  assert.equal(code, 3, 'exit code');
  assert.deepEqual(JSON.parse(Buffer.concat(out).toString('utf8')), {
    argv: ['hook', 'pre-bash', 'a b'],
    stdin: '{"tool":"Bash"}',
    root: '/plugin',
    version: '2.0.0-alpha.1',
  });
});

test('runBinary: a signal-killed engine gives exit 1', async () => {
  const dir = tmp();
  const stub = stubEngine(dir);
  const code = await runBinary(process.execPath, [stub], {
    env: { ...process.env, STUB_SIGNAL: 'SIGKILL' },
    pluginRoot: '/plugin',
    version: '2.0.0',
    stdin: '',
    onStdout: () => {},
  });
  assert.equal(code, 1, 'fails open as a non-blocking hook error');
});

test('runBinary: a binary that lost its exec bit is made executable once and run', { skip: process.platform === 'win32' ? 'no exec bit on Windows' : false }, async () => {
  const dir = tmp();
  const bin = path.join(dir, 'temper');
  fs.writeFileSync(bin, '#!/bin/sh\nexit 7\n', { mode: 0o644 });
  const code = await runBinary(bin, [], { env: process.env, pluginRoot: dir, version: '2.0.0', stdin: '', onStdout: () => {} });
  assert.equal(code, 7, 'ran after chmod');
  assert.equal(fs.statSync(bin).mode & 0o777, 0o755, 'mode');
});

test('runBinary: a directory is never chmodded', { skip: process.platform === 'win32' ? 'no exec bit on Windows' : false }, async () => {
  const dir = tmp();
  const target = path.join(dir, 'not-a-binary');
  fs.mkdirSync(target, { mode: 0o700 });
  await assert.rejects(runBinary(target, [], { env: process.env, pluginRoot: dir, version: '2.0.0', stdin: '', onStdout: () => {} }), /EACCES/);
  assert.equal(fs.statSync(target).mode & 0o777, 0o700, 'mode unchanged');
});

test("runEngine: below the Node minimum, a command gets 1.x's warning and still runs; TEMPER_DISABLE silences it", async () => {
  const dir = tmp();
  const stub = stubEngine(dir);
  const run = async (env) => {
    const err = [];
    const out = [];
    const code = await runEngine(process.execPath, [stub, '--version'], {
      env: { ...process.env, ...env, STUB_EXIT: '4' }, pluginRoot: '/plugin', version: '2.0.0', nodeVersion: '16.20.2',
      stdin: '', onStdout: (c) => out.push(c), writeStderr: (t) => err.push(t),
    });
    return { code, err: err.join(''), ran: out.length > 0 };
  };
  const warned = await run({});
  assert.match(warned.err, /^temper: Node 16\.20\.2 is older than the documented minimum/, 'the #89 warning');
  assert.equal(warned.code, 4, 'the engine still ran, with its own exit code');
  assert.ok(warned.ran, 'engine ran');
  const quiet = await run({ TEMPER_DISABLE: '1' });
  assert.equal(quiet.err, '', 'no warning under TEMPER_DISABLE');
  assert.equal(quiet.code, 4, 'engine exit code');
});

// The launcher passes only its own argv to the engine, so these process tests point TEMPER_BIN at a
// shell wrapper that runs the stub. A .cmd wrapper would need a shell on Windows, and the launcher
// never uses one, so they run on POSIX; the runBinary tests above cover Windows.
const POSIX_ONLY = { skip: process.platform === 'win32' ? 'the stub engine is a shell wrapper' : false };

test('launch.mjs: --version through TEMPER_BIN, as a process', POSIX_ONLY, () => {
  const dir = tmp();
  const stub = stubEngine(dir);
  const wrapper = path.join(dir, 'engine');
  fs.writeFileSync(wrapper, `#!/bin/sh\nexec "${process.execPath}" "${stub}" "$@"\n`, { mode: 0o755 });
  const r = spawnSync(process.execPath, [LAUNCH, '--version'], { encoding: 'utf8', env: { ...process.env, TEMPER_BIN: wrapper, STUB_EXIT: '3' }, input: '{"tool":"Bash"}' });
  assert.equal(r.status, 3, `the engine's exit code: ${r.stderr}`);
  const seen = JSON.parse(r.stdout);
  assert.deepEqual(seen.argv, ['--version'], 'argv');
  assert.equal(seen.stdin, '{"tool":"Bash"}', 'stdin');
  assert.equal(seen.root, path.resolve(here, '..'), 'TEMPER_PLUGIN_ROOT');
  assert.equal(seen.version, JSON.parse(fs.readFileSync(path.join(here, '..', '.claude-plugin', 'plugin.json'), 'utf8')).version, 'TEMPER_PLUGIN_VERSION');
});

test('launch.mjs: a signal-killed engine gives exit 1, as a process', POSIX_ONLY, () => {
  const dir = tmp();
  const stub = stubEngine(dir);
  const wrapper = path.join(dir, 'engine');
  fs.writeFileSync(wrapper, `#!/bin/sh\nexec "${process.execPath}" "${stub}" "$@"\n`, { mode: 0o755 });
  const r = spawnSync(process.execPath, [LAUNCH, '--version'], { encoding: 'utf8', env: { ...process.env, TEMPER_BIN: wrapper, STUB_SIGNAL: 'SIGKILL' }, input: '' });
  assert.equal(r.status, 1, r.stderr);
});

test('launch.mjs: a TEMPER_BIN that cannot be run is a one-line error, not a stack trace', () => {
  const r = spawnSync(process.execPath, [LAUNCH, '--version'], { encoding: 'utf8', env: { ...process.env, TEMPER_BIN: path.join(tmp(), 'missing') } });
  assert.equal(r.status, 1, 'fails open as a non-blocking error');
  assert.match(r.stderr, /^temper: cannot run the engine at .*missing \(ENOENT\)\n$/, r.stderr);
});

test('launch.mjs: started through a symlinked path, it still runs', POSIX_ONLY, () => {
  const link = path.join(tmp(), 'link');
  fs.symlinkSync(path.join(here, '..'), link);
  const r = spawnSync(process.execPath, [path.join(link, 'scripts', 'launch.mjs'), 'bogus'], { encoding: 'utf8' });
  assert.equal(r.status, 2, 'usage exit, not a silent 0');
  assert.match(r.stdout, /^usage: temper/, 'usage text');
});

test('launch.mjs: a 1.x subcommand runs the Node engine in-process, unchanged', () => {
  const r = spawnSync(process.execPath, [LAUNCH, 'bogus'], { encoding: 'utf8', env: { ...process.env, TEMPER_BIN: '/nonexistent/temper' } });
  const direct = spawnSync(process.execPath, [path.join(here, '..', 'scripts', 'temper.mjs'), 'bogus'], { encoding: 'utf8' });
  assert.equal(r.status, 2, 'usage exit');
  assert.equal(r.stdout, direct.stdout, 'same usage text as temper.mjs');
  assert.equal(r.stderr, direct.stderr, 'same stderr');
});

test('launch.mjs: TEMPER_DISABLE=1 hook exits 0 without resolving a binary', () => {
  // An unknown event: 1.x would print "unknown hook event" even under TEMPER_DISABLE, so silence
  // here shows the launcher stopped before running anything.
  const r = spawnSync(process.execPath, [LAUNCH, 'hook', 'nosuch'], { encoding: 'utf8', env: { ...process.env, TEMPER_DISABLE: '1', TEMPER_ENGINE: 'rust', TEMPER_BIN: '/nonexistent/temper' }, input: '{"x":1}' });
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout, '', 'no output');
  assert.equal(r.stderr, '', 'nothing on stderr');
});

test('launch.mjs: --version with no engine falls back to the Node engine while the port is under way', () => {
  const r = spawnSync(process.execPath, [LAUNCH, '--version'], { encoding: 'utf8', env: { ...process.env, TEMPER_BIN: '', CLAUDE_PLUGIN_DATA: tmp() } });
  assert.equal(r.status, 2, '1.x prints its usage for --version');
  assert.match(r.stdout, /^usage: temper/, 'usage text');
});

test('launch.mjs: the engine gets SIGTERM when the launcher does', POSIX_ONLY, async () => {
  const dir = tmp();
  const marker = path.join(dir, 'got-term');
  const engine = path.join(dir, 'engine');
  fs.writeFileSync(path.join(dir, 'wait.mjs'), `
    import fs from 'node:fs';
    process.on('SIGTERM', () => { fs.writeFileSync(${JSON.stringify(marker)}, 'x'); process.exit(0); });
    process.stdout.write('ready\\n');
    setInterval(() => {}, 1000);
  `);
  fs.writeFileSync(engine, `#!/bin/sh\nexec "${process.execPath}" "${path.join(dir, 'wait.mjs')}" "$@"\n`, { mode: 0o755 });
  const child = spawn(process.execPath, [LAUNCH, '--version'], { env: { ...process.env, TEMPER_BIN: engine }, stdio: ['pipe', 'pipe', 'inherit'] });
  await new Promise((resolve) => child.stdout.once('data', resolve));
  child.kill('SIGTERM');
  const code = await new Promise((resolve) => child.once('exit', (c) => resolve(c)));
  assert.ok(fs.existsSync(marker), 'the engine saw SIGTERM');
  assert.equal(code, 0, 'the engine exit code');
});
