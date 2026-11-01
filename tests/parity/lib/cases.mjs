// One parity-case run: an engine in its own fresh copy of a fixture (§2.2), with the tree's
// snapshot taken before and after, so a case also proves the engine wrote no file.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
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
