// The launcher's fallback build, and what each hook does while no engine is available (port plan
// §5.4 and D6-C, Phase 0c). It is used only when no prebuilt engine resolves: an unsupported
// platform, a failed or skipped npm install, or a `--plugin-dir` checkout.
//
// The engine is built into ${CLAUDE_PLUGIN_DATA}/engine/<key>/, under a `.lock` directory that
// only one session can create, with an owner record so a lock left by a killed session is stale at
// once. A failed build leaves `build.failed`, which is retried at the next SessionStart, never on
// every hook. Without an engine, hooks degrade as D6-C says: Rust edits are denied with a retry
// message, Stop reports the gate UNAVAILABLE, and everything else passes, so a broken toolchain
// never wedges the session. TEMPER_DISABLE=1 short-circuits before any of this, in the launcher.
//
// This file is not vendored into user repositories (scripts/lib/ is).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

/** A `.lock` older than this belongs to a build that died: take it over. */
export const LOCK_STALE_MS = 15 * 60 * 1000;
/** The workspace's rust-version (D4); a test keeps it equal to the workspace manifest's. */
export const MSRV = '1.96';
/** SessionStart's build budget: under the 900 s SessionStart timeout the cutover sets (§5.2). */
const BUILD_BUDGET_MS = 840 * 1000;
/** How long a SessionStart waits for another session's build before starting degraded. */
const SESSION_WAIT_MS = 840 * 1000;
/** How long Stop and SubagentStop wait for an in-flight build (D6-C). */
const STOP_WAIT_MS = 600 * 1000;
const POLL_MS = 250;
/** The version keys kept besides `dev` (§5.4 pruning). */
const KEEP_VERSIONS = 2;

const exeName = (platform) => (platform === 'win32' ? 'temper.exe' : 'temper');

/** The directory one engine build lives in. */
export function engineDir(dataDir, key) {
  return path.join(dataDir, 'engine', key);
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function mtimeMs(file) {
  try {
    return fs.statSync(file).mtimeMs;
  } catch {
    return null;
  }
}

function pidAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === 'EPERM';
  }
}

function readOwner(lock) {
  try {
    return JSON.parse(fs.readFileSync(path.join(lock, 'owner.json'), 'utf8'));
  } catch {
    return null;
  }
}

/**
 * A lock is stale when its owner, on this host, has exited (a SessionStart killed mid-build), or
 * when it is older than LOCK_STALE_MS. A lock with no owner record yet is judged by age alone.
 */
function lockStale(lock, now) {
  const at = mtimeMs(lock);
  if (at === null) return false;
  if (now - at > LOCK_STALE_MS) return true;
  const owner = readOwner(lock);
  return Boolean(owner && owner.host === os.hostname() && Number.isInteger(owner.pid) && !pidAlive(owner.pid));
}

/** Create the lock directory and its owner record. True when this process holds it. */
function tryLock(lock, now) {
  try {
    fs.mkdirSync(lock);
  } catch (error) {
    if (error.code !== 'EEXIST' || !lockStale(lock, now)) return false;
    // Rename the stale lock away before removing it, so two sessions that both find it stale do
    // not remove each other's new lock: only one rename succeeds.
    const aside = `${lock}.stale-${process.pid}-${now}`;
    try {
      fs.renameSync(lock, aside);
    } catch {
      return false;
    }
    fs.rmSync(aside, { recursive: true, force: true });
    try {
      fs.mkdirSync(lock);
    } catch {
      return false;
    }
  }
  fs.writeFileSync(path.join(lock, 'owner.json'), JSON.stringify({ pid: process.pid, host: os.hostname(), at: now }));
  return true;
}

/** Remove the lock only if this process owns it. */
function unlock(lock) {
  if (readOwner(lock)?.pid === process.pid) fs.rmSync(lock, { recursive: true, force: true });
}

/** Wait until `lock` is gone or stale, or `waitMs` passes. True when it is gone or stale. */
function waitForLock(lock, waitMs, onWait) {
  const deadline = Date.now() + waitMs;
  for (let n = 1; fs.existsSync(lock); n += 1) {
    if (lockStale(lock, Date.now())) return true;
    if (Date.now() >= deadline) return false;
    if (onWait) onWait(n);
    sleep(POLL_MS);
  }
  return true;
}

function readFailed(dir) {
  try {
    return JSON.parse(fs.readFileSync(path.join(dir, 'build.failed'), 'utf8'));
  } catch {
    return null;
  }
}

/**
 * Build the engine for `key` with the user's cargo and install it. Returns `{ status, bin?, log?,
 * reason?, waited? }`: `exists` (an installed cached engine; nothing built), `built` (`waited`
 * when another session built it), `failed` (with the log; `build.failed` records it) or `busy`
 * (another session's build held the lock past `waitMs`).
 */
export function buildEngine({ env, pluginRoot, dataDir, key, platform, waitMs = SESSION_WAIT_MS, budgetMs = BUILD_BUDGET_MS }) {
  const dir = engineDir(dataDir, key);
  const bin = path.join(dir, exeName(platform));
  // A cached plugin directory is immutable per version, so its engine is too. A `dev` checkout's
  // sources change, so cargo's own freshness check decides.
  if (key !== 'dev' && fs.existsSync(bin)) return { status: 'exists', bin };
  fs.mkdirSync(dir, { recursive: true });
  const lock = path.join(dir, '.lock');
  const build = { env, pluginRoot, dataDir, platform, dir, bin, budgetMs, lock };
  if (tryLock(lock, Date.now())) return buildAndUnlock(build);
  waitForLock(lock, waitMs);
  if (!fs.existsSync(lock) && fs.existsSync(bin)) return { status: 'built', bin, waited: true };
  // The other build died (its lock is stale): build here.
  if (tryLock(lock, Date.now())) return buildAndUnlock(build);
  const failed = readFailed(dir);
  if (failed) return { status: 'failed', log: failed.log, reason: failed.reason };
  return { status: 'busy' };
}

function buildAndUnlock({ lock, ...build }) {
  try {
    return buildLocked(build);
  } finally {
    unlock(lock);
  }
}

function buildLocked({ env, pluginRoot, dataDir, platform, dir, bin, budgetMs }) {
  const log = path.join(dir, 'build.log');
  const failedMarker = path.join(dir, 'build.failed');
  fs.rmSync(failedMarker, { force: true });
  const target = path.join(dir, 'target');
  // cwd is the data directory, so the repository's rust-toolchain.toml does not make rustup
  // download its pinned toolchain: the user's default toolchain builds it (it must meet the MSRV).
  // The repository's .cargo/config.toml is not read either; nothing in it matters to this build.
  // CARGO_BUILD_TARGET would move the binary out of target/release, so it is not passed on.
  const childEnv = { ...env };
  delete childEnv.CARGO_BUILD_TARGET;
  const fd = fs.openSync(log, 'w');
  let r;
  try {
    r = spawnSync('cargo', ['build', '--release', '--locked', '-p', 'temper-cli',
      '--manifest-path', path.join(pluginRoot, 'Cargo.toml'), '--target-dir', target], {
      cwd: dataDir, env: childEnv, stdio: ['ignore', fd, fd], windowsHide: true, timeout: budgetMs, killSignal: 'SIGKILL',
    });
  } finally {
    fs.closeSync(fd);
  }
  const built = path.join(target, 'release', exeName(platform));
  if (r.error || r.status !== 0 || !fs.existsSync(built)) {
    let reason = `cargo exited with ${r.status ?? r.signal}`;
    if (r.error && r.error.code === 'ENOENT') reason = 'cargo was not found on PATH';
    else if (r.error && r.error.code === 'ETIMEDOUT') reason = `the build took over ${Math.round(budgetMs / 1000)} s`;
    else if (r.status === 0) reason = `cargo built no ${exeName(platform)}`;
    fs.writeFileSync(failedMarker, `${JSON.stringify({ log, code: r.status, reason, at: new Date().toISOString() })}\n`);
    return { status: 'failed', log, reason };
  }
  // Copy, then rename into place, so no session ever runs a half-written engine. A running .exe
  // cannot be replaced on Windows, so an existing engine stays when the rename is refused.
  const tmpBin = path.join(dir, `temper.tmp-${process.pid}`);
  fs.copyFileSync(built, tmpBin);
  if (platform !== 'win32') fs.chmodSync(tmpBin, 0o755);
  try {
    fs.renameSync(tmpBin, bin);
  } catch (error) {
    fs.rmSync(tmpBin, { force: true });
    if (!fs.existsSync(bin)) throw error;
  }
  pruneEngines(dataDir);
  return { status: 'built', bin, log };
}

/** Keep the newest version keys (by directory mtime) and `dev`; remove the rest, best effort. */
export function pruneEngines(dataDir) {
  const root = path.join(dataDir, 'engine');
  let keys;
  try {
    keys = fs.readdirSync(root).filter((k) => k !== 'dev');
  } catch {
    return;
  }
  const byAge = keys.map((k) => [k, mtimeMs(path.join(root, k)) ?? 0]).sort((a, b) => b[1] - a[1]);
  for (const [k] of byAge.slice(KEEP_VERSIONS)) {
    try {
      fs.rmSync(path.join(root, k), { recursive: true, force: true });
    } catch {
      // A running old engine cannot be removed on Windows; the next prune tries again.
    }
  }
}

/** Why no engine is available, as a phrase: "is not installed", "is still building", ... */
function why(dataDir, key) {
  if (!dataDir) return 'is not installed, and CLAUDE_PLUGIN_DATA is not set, so it cannot be built';
  const dir = engineDir(dataDir, key);
  const lock = path.join(dir, '.lock');
  if (fs.existsSync(lock) && !lockStale(lock, Date.now())) return 'is still building';
  const failed = readFailed(dir);
  if (failed) return `failed to build (log: ${failed.log})`;
  return 'is not installed';
}

/** What the agent can do about `reason`, as a sentence. */
function nextStep(reason) {
  if (/building/.test(reason)) return 'Wait a minute and retry.';
  if (/failed to build/.test(reason)) return `Install Rust ${MSRV} or newer (rustup update) and start a new session.`;
  if (/CLAUDE_PLUGIN_DATA/.test(reason)) return 'Install the @wet-ink/temper npm package for this platform, or set TEMPER_BIN.';
  return null;
}

function parseInput(stdin) {
  try {
    const value = JSON.parse(stdin);
    return value && typeof value === 'object' ? value : {};
  } catch {
    return {};
  }
}

const out = (value) => ({ stdout: JSON.stringify(value), exit: 0 });
const silent = { stdout: '', exit: 0 };
// Case-insensitive, for case-insensitive file systems.
const isRustSource = (file) => /\.rs$/i.test(file) || /(^|[\\/])cargo\.toml$/i.test(file);

/** True the first time a session asks; a marker file in the engine directory remembers it. */
function firstTimeFor(dataDir, key, sessionId) {
  if (!dataDir) return true;
  const dir = engineDir(dataDir, key);
  const safe = String(sessionId ?? 'none').replace(/[^A-Za-z0-9_-]/g, '_').slice(0, 100);
  try {
    fs.mkdirSync(dir, { recursive: true });
    fs.closeSync(fs.openSync(path.join(dir, `notified-${safe}`), 'wx'));
    return true;
  } catch (error) {
    // Only an existing marker means "already told"; any other failure shows the notice again.
    return error.code !== 'EEXIST';
  }
}

function startDegraded(reason) {
  const fix = /CLAUDE_PLUGIN_DATA/.test(reason)
    ? 'Install the @wet-ink/temper npm package for this platform, set TEMPER_BIN'
    : (nextStep(reason) ?? 'Start a new session to build it.').replace(/\.$/, '');
  const msg = `temper is not enforcing: the engine ${reason}. ${fix}, or set TEMPER_DISABLE=1.`;
  return out({ systemMessage: msg, hookSpecificOutput: { hookEventName: 'SessionStart', additionalContext: msg } });
}

/**
 * What a hook does when no engine resolved (D6-C). Returns `{ bin }` when an engine became
 * available (SessionStart built it, or Stop waited for another session's build): the launcher
 * runs it with the same input. Otherwise `{ stdout, exit }`, the hook's whole answer. It never
 * throws: a handler error is a notice with exit 0 (invariant 4).
 */
export function withoutEngine(args) {
  try {
    return noEngine(args);
  } catch (error) {
    return out({ systemMessage: `temper hook ${args.argv[1]} failed: ${error.message}` });
  }
}

/**
 * The engine to run at session start: the resolved `bin`, after rebuilding it when it is the
 * `dev` engine in the data directory (§5.3: a source checkout builds at every SessionStart, and
 * cargo makes that a no-op when nothing changed). A failed rebuild keeps the engine already built.
 */
export function sessionStartEngine({ argv, env, pluginRoot, dataDir, key, platform, bin }) {
  if (argv[0] !== 'hook' || argv[1] !== 'session-start' || key !== 'dev' || !dataDir) return bin;
  if (bin !== path.join(engineDir(dataDir, key), exeName(platform))) return bin;
  try {
    buildEngine({ env, pluginRoot, dataDir, key, platform });
  } catch {
    // The engine already built still runs.
  }
  return bin;
}

function noEngine({ argv, env, stdin, pluginRoot, dataDir, key, platform, stopWaitMs = STOP_WAIT_MS, onWait }) {
  const event = argv[1];
  const input = parseInput(stdin);
  const bin = dataDir ? path.join(engineDir(dataDir, key), exeName(platform)) : null;
  switch (event) {
    case 'session-start': {
      if (!dataDir) return startDegraded(why(dataDir, key));
      const r = buildEngine({ env, pluginRoot, dataDir, key, platform });
      if (r.bin) return { bin: r.bin };
      return startDegraded(why(dataDir, key));
    }
    case 'pre-edit': {
      const file = String(input.tool_input?.file_path ?? '');
      if (!isRustSource(file)) return silent;
      const reason = why(dataDir, key);
      const next = nextStep(reason);
      const paused = next ? `so Rust edits are paused. ${next}` : 'so Rust edits are paused until the next session start builds it.';
      return out({ hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: 'deny',
        permissionDecisionReason: `temper: the engine ${reason}, ${paused} To work without temper, set TEMPER_DISABLE=1.` } });
    }
    case 'pre-bash':
    case 'pre-handback':
      if (!firstTimeFor(dataDir, key, input.session_id)) return silent;
      return out({ systemMessage: `temper is not enforcing: the engine ${why(dataDir, key)}. Rust edits are paused until it is built (the next session start builds it); shell commands and handbacks are not checked. To work without temper, set TEMPER_DISABLE=1.` });
    case 'stop':
    case 'subagent-stop': {
      if (dataDir) {
        waitForLock(path.join(engineDir(dataDir, key), '.lock'), stopWaitMs, onWait);
        if (fs.existsSync(bin)) return { bin };
      }
      return out({ systemMessage: `temper: temper gate: UNAVAILABLE (the engine ${why(dataDir, key)}). Nothing was checked, not even the member manifests; say so in "Not verified".` });
    }
    default:
      // post-tool, user-prompt and the rest pass silently. The skill-load marker is not written,
      // so after the build the skill gate asks for the skill again, which heals itself.
      return silent;
  }
}
