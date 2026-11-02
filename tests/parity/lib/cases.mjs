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

/**
 * One hook case (port-phase-2 "Parity harness"): `steps` (`{ event, stdin, env }`, run in order
 * in one fresh copy of `src`, so a marker one step writes is seen by the next; a null `event` runs
 * a bare `hook`) from `cwd`. Stdin is the null device when `stdin` is not a string. Beside the
 * copy is a scratch directory whose `tmp/` is TMPDIR, TMP and TEMP. `pluginData` is `true`
 * for a fresh CLAUDE_PLUGIN_DATA (`<scratch>/data`), a path relative to the copy, or absent for
 * none (the tmpdir fallback). In `stdin` and `env` values, `<ROOT>` is the copy's path, JSON-escaped
 * in `stdin`. Returns `{ steps: [{ exit, stdout, stderr, ms }], files }`: the streams decoded as
 * UTF-8 with the copy and scratch roots masked, and `files` the copy's changed files as
 * `tree/<rel>` and everything in the scratch directory as `scratch/<rel>`, with `maskFile` applied
 * as in `treeContents`.
 */
export function runHookCase(engine, src, { steps, cwd = '.', git = false, pluginData, env = {}, maskFile }) {
  const copy = freshCopy(src, { skip: [] });
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-parity-scratch-'));
  try {
    if (git) gitInit(copy);
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
      const stepEnv = Object.fromEntries(Object.entries({ ...base, ...step.env }).map(([k, v]) => [k, v.replaceAll('<ROOT>', copy)]));
      const input = typeof step.stdin === 'string' ? step.stdin.replaceAll('<ROOT>', jsonEscaped(copy)) : undefined;
      const r = runEngine(engine, step.event == null ? ['hook'] : ['hook', step.event], path.join(copy, cwd), { ceiling: path.dirname(copy), input, env: stepEnv, timeoutMs: 60_000 });
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
