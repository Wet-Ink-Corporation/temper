// One parity-case run: an engine in its own fresh copy of a fixture (§2.2), with the tree's
// snapshot taken before and after, so a case also proves the engine wrote no file. A hook case
// also has stdin, an environment, a scratch temp dir and plugin data dir, and compares what it wrote.
import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { hermeticEnv, runEngine } from './engines.mjs';
import { HarnessError } from './error.mjs';
import { freshCopy, snapshotTree } from './fixtures.mjs';
import { maskRoots } from './normalise.mjs';

/** The relative paths added, removed or changed between two snapshots, sorted. */
export function changedFiles(before, after) {
  const a = new Map(before);
  const b = new Map(after);
  const keys = new Set([...a.keys(), ...b.keys()]);
  return [...keys].filter((k) => a.get(k) !== b.get(k)).sort();
}

function gitInit(dir) {
  const r = spawnSync('git', ['init', '-q'], { cwd: dir, env: hermeticEnv(), stdio: ['ignore', 'pipe', 'pipe'], timeout: 60_000 });
  if (r.error || r.status !== 0) throw new HarnessError(`git init in ${dir} failed: ${r.error?.message ?? r.stderr}`);
}

/**
 * Runs `engine args` from `cwd` (relative to the copy) in a fresh copy of `src`, optionally a git
 * repository, and removes the copy. Returns `{ exit, stdout, stderr, wrote }`: the streams decoded
 * as UTF-8 with the copy's root masked, and the files the run added, removed or changed.
 */
/**
 * Every file and directory under `root` with its content as UTF-8 text with `roots` masked, then
 * `maskFile(prefix + rel, text)`; a directory is `'dir'`, and a file over 4 KiB is its masked
 * text's SHA-256, so a diff stays short.
 */
export function treeContents(root, roots, { prefix = '', maskFile = (_rel, text) => text } = {}) {
  const out = {};
  const rec = (dir, rel) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const r = rel ? `${rel}/${e.name}` : e.name;
      const p = path.join(dir, e.name);
      if (e.isDirectory()) {
        out[`${prefix}${r}/`] = 'dir';
        rec(p, r);
      } else {
        const text = maskFile(`${prefix}${r}`, maskRoots(fs.readFileSync(p, 'utf8'), roots));
        out[`${prefix}${r}`] = text.length > 4096 ? `sha256:${crypto.createHash('sha256').update(text).digest('hex')}` : text;
      }
    }
  };
  rec(root, '');
  return out;
}

const jsonEscaped = (s) => JSON.stringify(s).slice(1, -1);

/** A pid no process has: the largest int32 but one, which `kill(pid, 0)` finds dead. */
export const DEAD_PID = 2147483646;

/**
 * The placeholders of a case (port-phase-3 "Parity harness"), expanded in `text`: `<ROOT>` (the
 * copy's path), `<PID>` (this harness's pid, alive through both runs), `<DEADPID>`, and
 * `<ISO:-n>` and `<MS:-n>` (the case's `now` minus `n` ms, as an ISO time or epoch ms). `now` is
 * taken once per case, so both engines see the same times.
 */
export function expandPlaceholders(text, { root, now }) {
  return text
    .replaceAll('<ROOT>', root)
    .replaceAll('<PID>', String(process.pid))
    .replaceAll('<DEADPID>', String(DEAD_PID))
    .replace(/<ISO:-(\d+)>/g, (_, n) => new Date(now - Number(n)).toISOString())
    .replace(/<MS:-(\d+)>/g, (_, n) => String(now - Number(n)));
}

/** The fixed author and committer time of a case's `n`th git call: one second apart. */
const gitDate = (n) => `${1767225600 + n} +0000`;

/**
 * Applies a case's setup actions to `copy`, the same way for both engines, before the snapshot:
 * `{ git: [args] }` (run with a fixed identity, no signing, `core.autocrlf` off and dates one
 * second apart per git call, so both copies get the same commit ids), `{ write: { rel: text |
 * { base64 } } }` (parents created), `{ remove: [rel] }` and `{ age: { rel: ms } }` (mtime and
 * atime set to `now - ms`). Strings and paths go through `expandPlaceholders`. A failing action is a
 * HarnessError.
 */
export function applySetup(copy, actions = [], { now, ceiling }) {
  const expand = (s) => expandPlaceholders(s, { root: copy, now });
  let calls = 0;
  for (const action of actions) {
    if (action.git) {
      calls += 1;
      const env = { ...hermeticEnv(), GIT_AUTHOR_DATE: gitDate(calls), GIT_COMMITTER_DATE: gitDate(calls), GIT_CEILING_DIRECTORIES: ceiling };
      const args = ['-c', 'user.name=t', '-c', 'user.email=t@example.com', '-c', 'commit.gpgsign=false', '-c', 'core.autocrlf=false', ...action.git.map(expand)];
      const r = spawnSync('git', args, { cwd: copy, env, stdio: ['ignore', 'pipe', 'pipe'], timeout: 60_000 });
      if (r.error || r.status !== 0) throw new HarnessError(`setup git ${args.join(' ')} failed: ${r.error?.message ?? r.stderr}`);
    } else if (action.write) {
      for (const [rel, value] of Object.entries(action.write)) {
        const file = path.join(copy, expand(rel));
        fs.mkdirSync(path.dirname(file), { recursive: true });
        fs.writeFileSync(file, typeof value === 'string' ? expand(value) : Buffer.from(value.base64, 'base64'));
      }
    } else if (action.remove) {
      for (const rel of action.remove) fs.rmSync(path.join(copy, expand(rel)), { recursive: true, force: true });
    } else if (action.age) {
      for (const [rel, ms] of Object.entries(action.age)) {
        const t = new Date(now - ms);
        fs.utimesSync(path.join(copy, expand(rel)), t, t);
      }
    } else {
      throw new HarnessError(`unknown setup action ${JSON.stringify(action)}`);
    }
  }
}

/**
 * One hook case (port-phase-2 "Parity harness"): `steps` (`{ event, stdin, env }`, run in order
 * in one fresh copy of `src`, so a marker one step writes is seen by the next; a null `event` runs
 * a bare `hook`) from `cwd`. Stdin is the null device when `stdin` is not a string. Beside the
 * copy is a scratch directory whose `tmp/` is TMPDIR, TMP and TEMP. `pluginData` is `true`
 * for a fresh CLAUDE_PLUGIN_DATA (`<scratch>/data`), a path relative to the copy, or absent for
 * none (the tmpdir fallback). `setup` actions are applied first (`applySetup`), and a step with
 * `args` runs `temper <args>` instead of a hook. In `stdin`, `env` and `args`, the placeholders
 * of `expandPlaceholders` are expanded (with `now`), `<ROOT>` JSON-escaped in `stdin`. Returns `{ steps: [{ exit, stdout, stderr, ms }], files }`: the streams decoded as
 * UTF-8 with the copy and scratch roots masked, and `files` the copy's changed files as
 * `tree/<rel>` and everything in the scratch directory as `scratch/<rel>`, with `maskFile` applied
 * as in `treeContents`.
 */
export function runHookCase(engine, src, { steps, cwd = '.', git = false, pluginData, env = {}, maskFile, setup = [], now = Date.now() }) {
  const copy = freshCopy(src, { skip: [] });
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-parity-scratch-'));
  try {
    if (git) gitInit(copy);
    applySetup(copy, setup, { now, ceiling: path.dirname(copy) });
    const tmp = path.join(scratch, 'tmp');
    fs.mkdirSync(tmp);
    const base = { TMPDIR: tmp, TMP: tmp, TEMP: tmp, ...env };
    if (pluginData === true) {
      base.CLAUDE_PLUGIN_DATA = path.join(scratch, 'data');
      fs.mkdirSync(base.CLAUDE_PLUGIN_DATA);
    } else if (typeof pluginData === 'string') base.CLAUDE_PLUGIN_DATA = path.join(copy, pluginData);
    const before = snapshotTree(copy);
    const roots = [copy, scratch];
    const mask = (buf) => maskRoots(buf.toString('utf8'), roots);
    const out = steps.map((step) => {
      const stepEnv = Object.fromEntries(Object.entries({ ...base, ...step.env }).map(([k, v]) => [k, expandPlaceholders(v, { root: copy, now })]));
      const input = typeof step.stdin === 'string' ? expandPlaceholders(step.stdin, { root: jsonEscaped(copy), now }) : undefined;
      // A CLI step (`args`) runs `temper <args>`; a hook step runs `temper hook <event>`.
      const args = step.args ? step.args.map((a) => expandPlaceholders(a, { root: copy, now }))
        : step.event == null ? ['hook'] : ['hook', step.event];
      const r = runEngine(engine, args, path.join(copy, cwd), { ceiling: path.dirname(copy), input, env: stepEnv, timeoutMs: 60_000 });
      return { exit: r.exit, stdout: mask(r.stdout), stderr: mask(r.stderr), ms: r.ms };
    });
    const tree = treeContents(copy, roots, { prefix: 'tree/', maskFile });
    const files = {};
    for (const rel of changedFiles(before, snapshotTree(copy))) files[`tree/${rel}`] = tree[`tree/${rel}`] ?? 'removed';
    Object.assign(files, treeContents(scratch, roots, { prefix: 'scratch/', maskFile }));
    return { steps: out, files };
  } finally {
    fs.rmSync(copy, { recursive: true, force: true });
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

export function runInFreshCopy(engine, src, { args, cwd = '.', git = false, skip = [] }) {
  const copy = freshCopy(src, { skip });
  try {
    if (git) gitInit(copy);
    const before = snapshotTree(copy);
    const r = runEngine(engine, args, path.join(copy, cwd), { ceiling: path.dirname(copy) });
    const mask = (buf) => maskRoots(buf.toString('utf8'), [copy]);
    return { exit: r.exit, stdout: mask(r.stdout), stderr: mask(r.stderr), wrote: changedFiles(before, snapshotTree(copy)) };
  } finally {
    fs.rmSync(copy, { recursive: true, force: true });
  }
}
