#!/usr/bin/env node
// The scanner corpus driver (port Phase 1e; .github/workflows/corpus.yml runs it). A script, not a
// node:test file, so `node --test tests/*.test.mjs` never picks it up.
//
//   node tests/parity/corpus.mjs --bin PATH [--node-cli PATH] [--lock MANIFEST]... [--root DIR]...
//                                [--budget-ms 10000] [--report DIR]
//
// --lock M   cargo fetch --locked, then cargo metadata, for manifest M: every registry package of
//            its lockfile is a root, scanned in place (1e-2, wi-37a444).
// --root D   a fresh copy of D is a root (§2.2), outside any git checkout.
// The generated large file is always a root (invariant 3, D16(k)).
//
// Exit 0: every root matches within the bound. 1: a mismatch, a file at or over the bound, or a
// large-file failure. 2: a usage or harness error, with `corpus: <message>` on stderr.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { listRsFiles, registryDirs, renderSummary, runCorpus } from './lib/corpus.mjs';
import { nodeEngine, runEngine, rustEngine } from './lib/engines.mjs';
import { HarnessError } from './lib/error.mjs';
import { freshCopy, largeCrate, largeFile, writeTree } from './lib/fixtures.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '..', '..');
const USAGE = 'usage: node tests/parity/corpus.mjs --bin PATH [--node-cli PATH] [--lock MANIFEST]... [--root DIR]... '
  + '[--budget-ms N] [--report DIR]';

/** A command-line mistake: exit 2 with the usage text. */
export class UsageError extends Error {
  constructor(message) {
    super(message);
    this.name = 'UsageError';
    this.exitCode = 2;
  }
}

/** The driver's options from `argv` (without node and the script). */
export function parseArgs(argv) {
  const o = { bin: null, nodeCli: path.join(REPO, 'scripts', 'temper.mjs'), locks: [], roots: [], budgetMs: 10_000, report: null };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    const value = () => {
      i += 1;
      if (i >= argv.length) throw new UsageError(`${flag} needs a value`);
      return argv[i];
    };
    switch (flag) {
      case '--bin': o.bin = value(); break;
      case '--node-cli': o.nodeCli = value(); break;
      case '--lock': o.locks.push(value()); break;
      case '--root': o.roots.push(value()); break;
      case '--report': o.report = value(); break;
      case '--budget-ms': {
        const v = value();
        if (!/^[1-9][0-9]*$/.test(v)) throw new UsageError(`--budget-ms must be a positive integer, not '${v}'`);
        o.budgetMs = Number(v);
        break;
      }
      default: throw new UsageError(`unknown argument '${flag}'`);
    }
  }
  if (!o.bin) throw new UsageError('--bin is required');
  return o;
}

function cargo(args, { maxBuffer = 1 << 20 } = {}) {
  const r = spawnSync('cargo', args, { cwd: REPO, stdio: ['ignore', 'pipe', 'pipe'], timeout: 900_000, maxBuffer, windowsHide: true });
  if (r.error) throw new HarnessError(`cargo ${args.join(' ')}: ${r.error.message}`, { cause: r.error });
  if (r.status !== 0) throw new HarnessError(`cargo ${args.join(' ')} exited ${r.status ?? r.signal}: ${r.stderr.toString().trim()}`);
  return r.stdout;
}

function lockRoots(manifest) {
  cargo(['fetch', '--locked', '--manifest-path', manifest]);
  const out = cargo(['metadata', '--format-version', '1', '--locked', '--offline', '--manifest-path', manifest], { maxBuffer: 256 << 20 });
  let metadata;
  try {
    metadata = JSON.parse(out.toString('utf8'));
  } catch (e) {
    throw new HarnessError(`cargo metadata for ${manifest} printed no JSON: ${e.message}`, { cause: e });
  }
  return registryDirs(metadata);
}

function report(dir, result, summary) {
  if (dir) {
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'report.json'), JSON.stringify(result, null, 2) + '\n');
    fs.writeFileSync(path.join(dir, 'summary.md'), summary);
  }
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
  const lines = [];
  for (const e of result.roots.filter((x) => !x.ok)) lines.push(`corpus: mismatch: ${e.label} (exit 1.x ${e.exit.node}, Rust ${e.exit.rust})`);
  for (const t of result.overBudget) lines.push(`corpus: at or over ${result.budgetMs} ms: ${t.label} ${t.file} (${Math.round(t.ms)} ms)`);
  if (result.large && !result.large.ok) lines.push(`corpus: ${result.large.message}`);
  const slow = result.roots.reduce((a, b) => (b.rustMs > a.rustMs ? b : a), result.roots[0]);
  lines.push(`corpus: ${result.roots.length} roots, ${result.files} files, ${result.hits} hits; `
    + `1.x ${(result.nodeMs / 1000).toFixed(1)} s, Rust ${(result.rustMs / 1000).toFixed(1)} s; `
    + `slowest Rust root ${slow.label} (${Math.round(slow.rustMs)} ms); D16(e) normalised: ${result.normalised}; `
    + `${result.roots.filter((x) => !x.ok).length} mismatches; ${result.ok ? 'ok' : 'FAILED'}`);
  process.stdout.write(lines.join('\n') + '\n');
}

function main(argv) {
  const o = parseArgs(argv);
  const engines = { '1.x': { ...nodeEngine(REPO), pre: [path.resolve(o.nodeCli)] }, Rust: rustEngine(o.bin) };
  const temps = [];
  try {
    const roots = [];
    const seen = new Set();
    for (const m of o.locks) {
      for (const dir of lockRoots(m)) {
        if (!seen.has(dir)) roots.push({ dir, label: path.basename(dir) });
        seen.add(dir);
      }
    }
    for (const r of o.roots) {
      const copy = freshCopy(path.resolve(r));
      temps.push(copy);
      roots.push({ dir: copy, label: r });
    }
    const largeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-parity-large-'));
    temps.push(largeDir);
    writeTree(largeDir, largeCrate());
    const large = { dir: largeDir, label: 'large file', seedLine: largeFile().seedLine };
    const run = (name, args, cwd) => runEngine(engines[name], args, cwd, { ceiling: path.dirname(cwd) });
    const result = runCorpus({ roots, large, run, listFiles: listRsFiles, budgetMs: o.budgetMs });
    report(o.report && path.resolve(o.report), result, renderSummary(result));
    return result.ok ? 0 : 1;
  } finally {
    for (const t of temps) fs.rmSync(t, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (e) {
    const known = e instanceof UsageError || e instanceof HarnessError;
    process.stderr.write(`corpus: ${known ? e.message : e.stack}\n${e instanceof UsageError ? `${USAGE}\n` : ''}`);
    process.exitCode = 2;
  }
}
