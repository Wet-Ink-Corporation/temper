// The two engines and how one is run: the same arguments, cwd and hermetic environment for both
// (§2.2, invariant 1 of .temper/plans/port-phase-1e.md).
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

import { HarnessError } from './error.mjs';

const KEEP = ['PATH', 'HOME', 'USERPROFILE', 'TMPDIR', 'TMP', 'TEMP', 'LANG'];
const KEEP_WIN32 = ['SYSTEMROOT', 'PATHEXT', 'COMSPEC'];

/**
 * A new environment with only what both engines need to find git and a temp dir. TEMPER_*,
 * CLAUDE*, GIT_DIR, GIT_WORK_TREE and the rest are dropped; the system git config is ignored.
 * Windows names are matched case-insensitively, as Windows does.
 */
export function hermeticEnv(env = process.env, platform = process.platform) {
  const win = platform === 'win32';
  const keep = new Set(win ? [...KEEP, ...KEEP_WIN32] : KEEP);
  const out = {};
  for (const [k, v] of Object.entries(env)) if (keep.has(win ? k.toUpperCase() : k)) out[k] = v;
  out.GIT_CONFIG_NOSYSTEM = '1';
  return out;
}

/**
 * Runs `engine` with `args` in `cwd` and returns `{ exit, stdout, stderr, ms }`, the streams as
 * Buffers and `ms` the wall time of the spawn. Every exit code is data. A spawn failure, a timeout,
 * a stream past `maxBuffer` or a death by signal is a HarnessError. `ceiling` sets
 * GIT_CEILING_DIRECTORIES, so git in a temp root never finds a repository above it.
 */
export function runEngine(engine, args, cwd, { timeoutMs = 600_000, maxBuffer = 512 << 20, ceiling = null } = {}) {
  const env = hermeticEnv();
  if (ceiling) env.GIT_CEILING_DIRECTORIES = ceiling;
  const t0 = process.hrtime.bigint();
  const r = spawnSync(engine.cmd, [...engine.pre, ...args], {
    cwd, env, stdio: ['ignore', 'pipe', 'pipe'], timeout: timeoutMs, maxBuffer, windowsHide: true,
  });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  const what = `${engine.name} ${args.join(' ')} (in ${cwd})`;
  if (r.error) throw new HarnessError(`${what}: ${r.error.message}`, { cause: r.error });
  if (r.signal) throw new HarnessError(`${what}: killed by signal ${r.signal}`);
  return { exit: r.status, stdout: r.stdout, stderr: r.stderr, ms };
}

/** 1.x: `node scripts/temper.mjs` of the checkout at `repo`. */
export function nodeEngine(repo) {
  return { name: '1.x', cmd: process.execPath, pre: [path.join(repo, 'scripts', 'temper.mjs')] };
}

/** The Rust binary at `bin` (TEMPER_BIN), made absolute, since each run has its own cwd. */
export function rustEngine(bin) {
  const abs = path.resolve(bin);
  let file = false;
  try {
    file = fs.statSync(abs).isFile();
  } catch (e) {
    if (e.code !== 'ENOENT') throw new HarnessError(`TEMPER_BIN ${abs}: ${e.message}`, { cause: e });
  }
  if (!file) {
    throw new HarnessError(`TEMPER_BIN ${abs} is not a file: build temper-cli (cargo build -p temper-cli) and point TEMPER_BIN at it`);
  }
  return { name: 'Rust', cmd: abs, pre: [] };
}
