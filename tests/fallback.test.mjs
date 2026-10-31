// scripts/fallback.mjs, the launcher's fallback build and the D6 behaviours without an engine
// (port plan §5.4, D6-C, Phase 0c). A fake `cargo` on PATH stands in for the real build.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { buildEngine, engineDir, withoutEngine, pruneEngines, sessionStartEngine, LOCK_STALE_MS, MSRV } from '../scripts/fallback.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const FALLBACK = path.join(here, '..', 'scripts', 'fallback.mjs');
const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'temper-fallback-'));

// The fake cargo is a shell script, and the launcher spawns without a shell, so the build tests run
// on POSIX. The D6 tests below need no cargo and run everywhere.
const POSIX_ONLY = { skip: process.platform === 'win32' ? 'the fake cargo is a shell script' : false };

/**
 * A directory holding a fake `cargo`. It appends its cwd and argv to `calls`, sleeps
 * FAKE_CARGO_SLEEP seconds, then fails (FAKE_CARGO_FAIL=1) or writes
 * <target-dir>/release/temper, a script that prints "temper fake".
 */
function fakeCargo() {
  const dir = tmp();
  const calls = path.join(dir, 'calls');
  fs.writeFileSync(path.join(dir, 'cargo'), `#!/bin/sh
printf '%s %s\\n' "$(pwd)" "$*" >> "${calls}"
[ -n "$FAKE_CARGO_SLEEP" ] && sleep "$FAKE_CARGO_SLEEP"
if [ "$FAKE_CARGO_FAIL" = 1 ]; then echo "error[E0000]: fake failure" >&2; exit 101; fi
target=""
while [ $# -gt 0 ]; do [ "$1" = "--target-dir" ] && target="$2"; shift; done
# Like cargo, a build target moves the output to <target-dir>/<triple>/release.
out="$target/release"; [ -n "$CARGO_BUILD_TARGET" ] && out="$target/$CARGO_BUILD_TARGET/release"
mkdir -p "$out"
printf '#!/bin/sh\\necho temper fake\\n' > "$out/temper"
chmod 755 "$out/temper"
echo "Finished release"
`, { mode: 0o755 });
  const count = () => (fs.existsSync(calls) ? fs.readFileSync(calls, 'utf8').trim().split('\n').length : 0);
  const lines = () => fs.readFileSync(calls, 'utf8').trim().split('\n');
  return { env: { ...process.env, PATH: `${dir}${path.delimiter}${process.env.PATH}` }, count, lines };
}

const opts = (env, data, extra = {}) => ({ env, pluginRoot: '/plugin', dataDir: data, key: '2.0.0', platform: process.platform, ...extra });

test('buildEngine: builds with cargo from the data directory and installs the engine', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const r = buildEngine(opts(cargo.env, data));
  const dir = engineDir(data, '2.0.0');
  assert.equal(r.status, 'built', JSON.stringify(r));
  assert.equal(r.bin, path.join(dir, 'temper'), 'installed path');
  assert.equal(fs.readFileSync(r.bin, 'utf8'), '#!/bin/sh\necho temper fake\n', 'the built binary');
  assert.equal(fs.statSync(r.bin).mode & 0o777, 0o755, 'installed executable');
  const [line] = cargo.lines();
  assert.equal(line.split(' ')[0], fs.realpathSync(data), 'cwd is the data directory, so the repo toolchain file is not used');
  assert.equal(line.slice(line.indexOf(' ') + 1), `build --release --locked -p temper-cli --manifest-path ${path.join('/plugin', 'Cargo.toml')} --target-dir ${path.join(dir, 'target')}`, 'cargo argv');
  assert.match(fs.readFileSync(path.join(dir, 'build.log'), 'utf8'), /Finished release/, 'build.log');
  assert.ok(!fs.existsSync(path.join(dir, '.lock')), 'lock released');
  assert.deepEqual(fs.readdirSync(dir).filter((f) => f.startsWith('temper.tmp')), [], 'no temp copy left');
});

test('buildEngine: a cached key with an installed engine does not build again; dev always asks cargo', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  buildEngine(opts(cargo.env, data));
  assert.equal(buildEngine(opts(cargo.env, data)).status, 'exists', 'immutable cached copy');
  assert.equal(cargo.count(), 1, 'one build');
  buildEngine(opts(cargo.env, data, { key: 'dev' }));
  assert.equal(buildEngine(opts(cargo.env, data, { key: 'dev' })).status, 'built', 'cargo decides freshness');
  assert.equal(cargo.count(), 3, 'dev builds each time');
});

test('buildEngine: a failed build writes build.failed with the log, and the next call retries', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const dir = engineDir(data, '2.0.0');
  const failed = buildEngine(opts({ ...cargo.env, FAKE_CARGO_FAIL: '1' }, data));
  assert.equal(failed.status, 'failed');
  assert.equal(failed.log, path.join(dir, 'build.log'));
  const marker = JSON.parse(fs.readFileSync(path.join(dir, 'build.failed'), 'utf8'));
  assert.equal(marker.log, failed.log, 'marker names the log');
  assert.equal(marker.code, 101, 'cargo exit code');
  assert.match(fs.readFileSync(failed.log, 'utf8'), /fake failure/, 'stderr in the log');
  assert.ok(!fs.existsSync(path.join(dir, '.lock')), 'lock released');
  const again = buildEngine(opts(cargo.env, data));
  assert.equal(again.status, 'built', 'the next SessionStart retries');
  assert.ok(!fs.existsSync(path.join(dir, 'build.failed')), 'marker cleared');
});

test('buildEngine: cargo missing is a failed build, not a crash', () => {
  const data = tmp();
  const r = buildEngine(opts({ ...process.env, PATH: tmp() }, data));
  assert.equal(r.status, 'failed');
  assert.match(r.reason, /cargo was not found/, r.reason);
  assert.ok(fs.existsSync(path.join(engineDir(data, '2.0.0'), 'build.failed')));
});

test('buildEngine: a second concurrent build waits on the lock and uses the first one\'s engine', POSIX_ONLY, async () => {
  const cargo = fakeCargo();
  const data = tmp();
  const lock = path.join(engineDir(data, '2.0.0'), '.lock');
  const run = () => new Promise((resolve) => {
    const script = `import { buildEngine } from ${JSON.stringify(pathToFileURL(FALLBACK).href)};
      process.stdout.write(JSON.stringify(buildEngine({ env: process.env, pluginRoot: '/plugin', dataDir: ${JSON.stringify(data)}, key: '2.0.0', platform: process.platform })));`;
    const child = spawn(process.execPath, ['--input-type=module', '-e', script], { env: { ...cargo.env, FAKE_CARGO_SLEEP: '2' } });
    let out = '';
    child.stdout.on('data', (c) => { out += c; });
    child.on('close', () => resolve(JSON.parse(out)));
  });
  const first = run();
  // Start the second only once the first holds the lock, so it must wait.
  while (!fs.existsSync(lock)) await new Promise((r) => setTimeout(r, 20));
  const results = await Promise.all([first, run()]);
  assert.deepEqual(results.map((r) => r.status), ['built', 'built'], JSON.stringify(results));
  assert.deepEqual(results.map((r) => Boolean(r.waited)), [false, true], 'the second waited');
  assert.equal(cargo.count(), 1, 'cargo ran once');
});

test('buildEngine: a lock whose owner process is gone is stale at once', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const lock = path.join(engineDir(data, '2.0.0'), '.lock');
  fs.mkdirSync(lock, { recursive: true });
  fs.writeFileSync(path.join(lock, 'owner.json'), JSON.stringify({ pid: 2147483640, host: os.hostname(), at: Date.now() }));
  assert.equal(buildEngine(opts(cargo.env, data, { waitMs: 300 })).status, 'built', 'taken over without waiting out the 15 minutes');
});

test('without an engine, a lock whose owner is gone is not reported as building', () => {
  const data = tmp();
  const lock = path.join(engineDir(data, '2.0.0'), '.lock');
  fs.mkdirSync(lock, { recursive: true });
  fs.writeFileSync(path.join(lock, 'owner.json'), JSON.stringify({ pid: 2147483640, host: os.hostname(), at: Date.now() }));
  const r = withoutEngine({ argv: ['hook', 'stop'], env: {}, stdin: '{}', pluginRoot: '/plugin', dataDir: data, key: '2.0.0', platform: process.platform, stopWaitMs: 5000 });
  assert.match(JSON.parse(r.stdout).systemMessage, /UNAVAILABLE \(the engine is not installed\)/, r.stdout);
});

test('buildEngine: a lock is released only by its owner', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  buildEngine(opts(cargo.env, data));
  assert.ok(!fs.existsSync(path.join(engineDir(data, '2.0.0'), '.lock')), 'own lock released');
});

test('buildEngine: a build over its budget fails with the reason', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const r = buildEngine(opts({ ...cargo.env, FAKE_CARGO_SLEEP: '3' }, data, { budgetMs: 300 }));
  assert.equal(r.status, 'failed');
  assert.match(r.reason, /^the build took over /, r.reason);
});

test('buildEngine: a successful build prunes old version keys', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const t = Date.now();
  for (const [key, ageMin] of [['1.0.0', 30], ['1.1.0', 20]]) {
    fs.mkdirSync(engineDir(data, key), { recursive: true });
    const at = new Date(t - ageMin * 60000);
    fs.utimesSync(engineDir(data, key), at, at);
  }
  buildEngine(opts(cargo.env, data));
  assert.deepEqual(fs.readdirSync(path.join(data, 'engine')).sort(), ['1.1.0', '2.0.0']);
});

test('buildEngine: CARGO_BUILD_TARGET is not passed on, so the engine lands in target/release', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const r = buildEngine(opts({ ...cargo.env, CARGO_BUILD_TARGET: 'x86_64-unknown-linux-musl' }, data));
  assert.equal(r.status, 'built', JSON.stringify(r));
});

test('sessionStartEngine: a dev engine is rebuilt at each session start; a cached or npm engine is run as is', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const dev = buildEngine(opts(cargo.env, data, { key: 'dev' }));
  assert.equal(cargo.count(), 1);
  const base = { argv: ['hook', 'session-start'], env: cargo.env, pluginRoot: '/plugin', dataDir: data, platform: process.platform };
  assert.equal(sessionStartEngine({ ...base, key: 'dev', bin: dev.bin }), dev.bin, 'same path');
  assert.equal(cargo.count(), 2, 'cargo asked again for dev');
  assert.equal(sessionStartEngine({ ...base, key: 'dev', bin: '/npm/temper' }), '/npm/temper', 'an npm engine');
  assert.equal(sessionStartEngine({ ...base, argv: ['hook', 'stop'], key: 'dev', bin: dev.bin }), dev.bin, 'only at session start');
  assert.equal(cargo.count(), 2, 'no build for those');
});

test('the MSRV in the fallback message matches the workspace rust-version', () => {
  const toml = fs.readFileSync(path.join(here, '..', 'Cargo.toml'), 'utf8');
  assert.equal(MSRV, /^rust-version\s*=\s*"([^"]+)"/m.exec(toml)[1]);
});

test('buildEngine: a lock older than 15 minutes is stale and taken over', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const lock = path.join(engineDir(data, '2.0.0'), '.lock');
  fs.mkdirSync(lock, { recursive: true });
  const old = new Date(Date.now() - LOCK_STALE_MS - 60000);
  fs.utimesSync(lock, old, old);
  assert.equal(buildEngine(opts(cargo.env, data, { waitMs: 300 })).status, 'built');
});

test('buildEngine: a fresh lock held past the wait gives busy', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  fs.mkdirSync(path.join(engineDir(data, '2.0.0'), '.lock'), { recursive: true });
  const r = buildEngine(opts(cargo.env, data, { waitMs: 300 }));
  assert.equal(r.status, 'busy');
  assert.equal(cargo.count(), 0, 'did not build under another session\'s lock');
});

test('pruneEngines: keeps the two newest version keys and dev', () => {
  const data = tmp();
  const t = Date.now();
  for (const [key, ageMin] of [['1.0.0', 30], ['1.1.0', 20], ['1.2.0', 10], ['2.0.0', 0], ['dev', 40]]) {
    const dir = engineDir(data, key);
    fs.mkdirSync(dir, { recursive: true });
    const at = new Date(t - ageMin * 60000);
    fs.utimesSync(dir, at, at);
  }
  pruneEngines(data);
  assert.deepEqual(fs.readdirSync(path.join(data, 'engine')).sort(), ['1.2.0', '2.0.0', 'dev']);
});

// ------------------------------------------------------------- D6: no engine

const ctx = (argv, input, data, extra = {}) => ({
  argv, env: {}, stdin: JSON.stringify(input), pluginRoot: '/plugin', dataDir: data, key: '2.0.0', platform: process.platform, ...extra,
});
const json = (r) => JSON.parse(r.stdout);

test('without an engine, pre-edit denies .rs and Cargo.toml edits with a retry message; other edits pass', () => {
  const data = tmp();
  for (const file of ['/w/src/lib.rs', '/w/Cargo.toml', 'C:\\w\\crates\\a\\Cargo.toml', 'C:\\w\\src\\main.rs', 'C:\\w\\src\\Lib.RS', '/w/cargo.toml']) {
    const r = withoutEngine(ctx(['hook', 'pre-edit'], { session_id: 's', tool_name: 'Edit', tool_input: { file_path: file } }, data));
    assert.equal(r.exit, 0, file);
    assert.deepEqual(json(r), { hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: 'deny',
      permissionDecisionReason: "temper: the engine is not installed, so Rust edits are paused until the next session start builds it. To work without temper, set TEMPER_DISABLE=1." } }, file);
  }
  for (const file of ['/w/README.md', '/w/Cargo.toml.bak', '/w/notCargo.toml']) {
    const r = withoutEngine(ctx(['hook', 'pre-edit'], { session_id: 's', tool_name: 'Write', tool_input: { file_path: file } }, data));
    assert.deepEqual(r, { stdout: '', exit: 0 }, file);
  }
});

test('without an engine, the reason says building or failed with the log', () => {
  const data = tmp();
  const dir = engineDir(data, '2.0.0');
  fs.mkdirSync(path.join(dir, '.lock'), { recursive: true });
  const input = { session_id: 's', tool_name: 'Edit', tool_input: { file_path: '/w/a.rs' } };
  assert.match(json(withoutEngine(ctx(['hook', 'pre-edit'], input, data))).hookSpecificOutput.permissionDecisionReason, /^temper: the engine is still building, so Rust edits are paused\. Wait a minute and retry\./);
  fs.rmdirSync(path.join(dir, '.lock'));
  fs.writeFileSync(path.join(dir, 'build.failed'), JSON.stringify({ log: '/d/build.log', code: 101 }));
  assert.match(json(withoutEngine(ctx(['hook', 'pre-edit'], input, data))).hookSpecificOutput.permissionDecisionReason, /^temper: the engine failed to build \(log: \/d\/build\.log\), so Rust edits are paused\. Install Rust 1\.96 or newer/);
});

test('without an engine, pre-bash and pre-handback pass with one notice per session', () => {
  const data = tmp();
  const notice = { systemMessage: 'temper is not enforcing: the engine is not installed. Rust edits are paused until it is built (the next session start builds it); shell commands and handbacks are not checked. To work without temper, set TEMPER_DISABLE=1.' };
  const first = withoutEngine(ctx(['hook', 'pre-bash'], { session_id: 's1', tool_input: { command: 'ls' } }, data));
  assert.deepEqual(json(first), notice);
  assert.equal(first.exit, 0);
  assert.deepEqual(withoutEngine(ctx(['hook', 'pre-handback'], { session_id: 's1' }, data)), { stdout: '', exit: 0 }, 'once per session');
  assert.deepEqual(json(withoutEngine(ctx(['hook', 'pre-handback'], { session_id: 's2' }, data))), notice, 'another session gets it');
  assert.deepEqual(json(withoutEngine(ctx(['hook', 'pre-bash'], { session_id: '../../x' }, data))), notice, 'a strange session id is made safe');
  assert.ok(fs.readdirSync(engineDir(data, '2.0.0')).every((f) => !f.includes('/') && !f.includes('..')));
});

test('without an engine, post-tool, user-prompt and other events pass silently', () => {
  const data = tmp();
  for (const event of ['post-tool', 'user-prompt', 'subagent-start', 'nosuch']) {
    assert.deepEqual(withoutEngine(ctx(['hook', event], { session_id: 's' }, data)), { stdout: '', exit: 0 }, event);
  }
});

test('without an engine, stop and subagent-stop report the gate UNAVAILABLE', () => {
  const data = tmp();
  for (const event of ['stop', 'subagent-stop']) {
    const r = withoutEngine(ctx(['hook', event], { session_id: 's' }, data));
    assert.deepEqual(json(r), { systemMessage: 'temper: temper gate: UNAVAILABLE (the engine is not installed). Nothing was checked, not even the member manifests; say so in "Not verified".' }, event);
    assert.equal(r.exit, 0);
  }
});

test('stop waits for an in-flight build, then runs the engine it built', () => {
  const data = tmp();
  const dir = engineDir(data, '2.0.0');
  const exe = process.platform === 'win32' ? 'temper.exe' : 'temper';
  fs.mkdirSync(path.join(dir, '.lock'), { recursive: true });
  // The other session's build finishes during the wait (onWait runs once per poll).
  const r = withoutEngine(ctx(['hook', 'stop'], { session_id: 's' }, data, {
    stopWaitMs: 2000,
    onWait: (n) => { if (n === 2) { fs.writeFileSync(path.join(dir, exe), ''); fs.rmdirSync(path.join(dir, '.lock')); } },
  }));
  assert.deepEqual(r, { bin: path.join(dir, exe) });
});

test('without an engine, session-start without a data directory explains and passes', () => {
  const r = withoutEngine(ctx(['hook', 'session-start'], { session_id: 's' }, undefined));
  const out = json(r);
  assert.equal(r.exit, 0);
  assert.equal(out.hookSpecificOutput.hookEventName, 'SessionStart');
  assert.equal(out.systemMessage, 'temper is not enforcing: the engine is not installed, and CLAUDE_PLUGIN_DATA is not set, so it cannot be built. Install the @wet-ink/temper npm package for this platform, set TEMPER_BIN, or set TEMPER_DISABLE=1.');
  assert.equal(out.hookSpecificOutput.additionalContext, out.systemMessage);
});

test('session-start builds the engine, then runs it', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const r = withoutEngine({ ...ctx(['hook', 'session-start'], { session_id: 's' }, data), env: cargo.env });
  assert.deepEqual(r, { bin: path.join(engineDir(data, '2.0.0'), 'temper') });
});

test('session-start with a failing build starts degraded and says how to fix it', POSIX_ONLY, () => {
  const cargo = fakeCargo();
  const data = tmp();
  const log = path.join(engineDir(data, '2.0.0'), 'build.log');
  const r = withoutEngine({ ...ctx(['hook', 'session-start'], { session_id: 's' }, data), env: { ...cargo.env, FAKE_CARGO_FAIL: '1' } });
  const out = json(r);
  assert.equal(out.systemMessage, `temper is not enforcing: the engine failed to build (log: ${log}). Install Rust 1.96 or newer (rustup update) and start a new session, or set TEMPER_DISABLE=1.`);
  assert.equal(out.hookSpecificOutput.additionalContext, out.systemMessage);
  // Later hooks report the failure without building again.
  const stop = withoutEngine({ ...ctx(['hook', 'stop'], { session_id: 's' }, data), env: cargo.env });
  assert.match(json(stop).systemMessage, /UNAVAILABLE \(the engine failed to build \(log: /);
  assert.equal(cargo.count(), 1, 'not retried before the next SessionStart');
});

test('malformed hook input is read as an empty object', () => {
  const r = withoutEngine({ ...ctx(['hook', 'pre-edit'], {}, tmp()), stdin: 'not json' });
  assert.deepEqual(r, { stdout: '', exit: 0 });
});

test('an unwritable data directory still gives hook JSON, never a crash', () => {
  const file = path.join(tmp(), 'not-a-dir');
  fs.writeFileSync(file, '');
  const start = withoutEngine(ctx(['hook', 'session-start'], { session_id: 's' }, file));
  assert.equal(start.exit, 0);
  assert.match(json(start).systemMessage, /^temper hook session-start failed: /, start.stdout);
  const bash = withoutEngine(ctx(['hook', 'pre-bash'], { session_id: 's' }, file));
  assert.match(json(bash).systemMessage, /^temper is not enforcing/, 'the notice is shown when its marker cannot be written');
});

test('a very long session id still gets its notice once', () => {
  const data = tmp();
  const sid = 'x'.repeat(5000);
  assert.ok(json(withoutEngine(ctx(['hook', 'pre-bash'], { session_id: sid }, data))).systemMessage);
  assert.deepEqual(withoutEngine(ctx(['hook', 'pre-bash'], { session_id: sid }, data)), { stdout: '', exit: 0 });
});
