// The scanner corpus (port Phase 1e): both engines over each root, compared byte for byte, and the
// D16(k) per-file scan-time bound. Pure apart from the injected `run` and `listFiles`, so every
// behaviour is testable with fakes (harness.test.mjs), without sleeps or real engines.
import fs from 'node:fs';
import path from 'node:path';

import { canonical, firstDifference, hitDiff } from './diff.mjs';
import { HarnessError } from './error.mjs';
import { dropLoneSurrogateTail } from './normalise.mjs';

const SKIP = ['target', '.git', 'node_modules', '.temper']; // 1.x's walk (scripts/temper.mjs)
const ALL = ['scan', '--all', '--json'];
const SLOWEST = 10;
const SUMMARY_HITS = 20;

/**
 * The source directories of the registry packages in `cargo metadata --format-version 1` output
 * (1e-2, wi-37a444): `registry+` and `sparse+` sources only, deduplicated, sorted by code unit.
 */
export function registryDirs(metadata) {
  const dirs = new Set();
  for (const p of metadata.packages ?? []) {
    if (typeof p.source === 'string' && (p.source.startsWith('registry+') || p.source.startsWith('sparse+'))) {
      dirs.add(path.dirname(p.manifest_path));
    }
  }
  return [...dirs].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

/** The timings at or over the bound: D16(k) fails at `ms >= budgetMs`, where 1.x passed `ms < 10_000`. */
export function overBudget(timings, budgetMs) {
  return timings.filter((t) => t.ms >= budgetMs);
}

/** The `.rs` files under `root` by 1.x's walk rules. */
export function listRsFiles(root) {
  const out = [];
  const rec = (dir) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      if (SKIP.includes(e.name)) continue;
      const p = path.join(dir, e.name);
      if (e.isDirectory()) rec(p);
      else if (e.name.endsWith('.rs')) out.push(p);
    }
  };
  rec(root);
  return out;
}

function hitCount(stdout, label) {
  let hits;
  try {
    hits = JSON.parse(stdout.toString('utf8'));
  } catch (e) {
    throw new HarnessError(`${label}: 1.x printed no JSON (${e.message})`, { cause: e });
  }
  if (!Array.isArray(hits)) throw new HarnessError(`${label}: 1.x printed JSON that is not an array of hits`);
  return hits.length;
}

function compareRoot(root, run, listFiles, budgetMs) {
  const files = listFiles(root.dir);
  if (files.length === 0) throw new HarnessError(`${root.label} (${root.dir}): no .rs file to scan`);
  const node = run('1.x', ALL, root.dir);
  const rust = run('Rust', ALL, root.dir);
  const tail = dropLoneSurrogateTail(node.stdout.toString('utf8'));
  const nodeOut = tail.count > 0 ? Buffer.from(tail.text, 'utf8') : node.stdout;
  const stdout = firstDifference(nodeOut, rust.stdout);
  const stderr = firstDifference(node.stderr, rust.stderr);
  const ok = node.exit === rust.exit && stdout === null && stderr === null;
  const entry = {
    label: root.label, dir: root.dir, files: files.length, hits: hitCount(nodeOut, root.label),
    exit: { node: node.exit, rust: rust.exit }, nodeMs: node.ms, rustMs: rust.ms,
    ok, normalised: tail.count, stdout, stderr, hitDiff: stdout ? hitDiff(nodeOut, rust.stdout) : null,
    drilled: rust.ms >= budgetMs,
  };
  const timings = [];
  if (entry.drilled) {
    for (const abs of files) {
      const file = path.relative(root.dir, abs).split(path.sep).join('/');
      timings.push({ label: root.label, file, ms: run('Rust', ['scan', file, '--json'], root.dir).ms });
    }
  }
  return { entry, rust, over: overBudget(timings, budgetMs) };
}

function checkLarge(large, rust) {
  const expected = `unwrap:H-02:hard@${large.seedLine}`;
  let got;
  try {
    got = JSON.parse(rust.stdout.toString('utf8')).map((h) => `${h.key}:${h.id}:${h.severity}@${h.line}`);
  } catch (e) {
    return { ok: false, message: `large file: expected ${expected}, got output that is not a JSON array (${e.message})` };
  }
  if (got.length === 1 && got[0] === expected) return { ok: true, message: `large file: ${expected}` };
  return { ok: false, message: `large file: expected ${expected}, got ${got.join(', ') || 'no hit'}` };
}

/**
 * Compares the engines on every root, then on `large` (`{ dir, label, seedLine }`), which must
 * also give exactly its seeded hit (invariant 3). `run(engine, args, cwd)` returns
 * `{ exit, stdout, stderr, ms }` for engine `1.x` or `Rust`. A root whose Rust run takes
 * `budgetMs` or more is timed file by file (1e-5, wi-a24829): a root under the bound bounds every
 * file in it.
 */
export function runCorpus({ roots, large = null, run, listFiles, budgetMs = 10_000 }) {
  const entries = [];
  const over = [];
  let largeResult = null;
  for (const root of large ? [...roots, large] : roots) {
    const r = compareRoot(root, run, listFiles, budgetMs);
    entries.push(r.entry);
    over.push(...r.over);
    if (root === large) largeResult = checkLarge(large, r.rust);
  }
  const sum = (k) => entries.reduce((n, e) => n + e[k], 0);
  return {
    roots: entries, overBudget: over, budgetMs, normalised: sum('normalised'), large: largeResult,
    files: sum('files'), hits: sum('hits'), nodeMs: sum('nodeMs'), rustMs: sum('rustMs'),
    ok: entries.every((e) => e.ok) && over.length === 0 && (largeResult?.ok ?? true),
  };
}

const plural = (n, word) => `${n} ${word}${n === 1 ? '' : word.endsWith('h') ? 'es' : 's'}`;
const code = (s) => `\`${JSON.stringify(s).replaceAll('`', '\\u0060')}\``;
const seconds = (ms) => (ms / 1000).toFixed(1);

function renderMismatch(e) {
  const out = [`#### ${e.label}`, '', `- exit: 1.x ${e.exit.node}, Rust ${e.exit.rust}`];
  for (const [name, d] of [['stdout', e.stdout], ['stderr', e.stderr]]) {
    if (!d) continue;
    out.push(`- ${name}: first difference at byte ${d.offset}, line ${d.line}`, `  - 1.x: ${code(d.a)}`, `  - Rust: ${code(d.b)}`);
  }
  const h = e.hitDiff;
  if (h && h.orderOnly) out.push('- hits: the same multiset in another order, or the same hits spelled differently');
  else if (h) {
    out.push(`- hits only in 1.x: ${h.onlyNodeTotal}, only in Rust: ${h.onlyRustTotal}`);
    for (const [name, list] of [['1.x', h.onlyNode], ['Rust', h.onlyRust]]) {
      for (const x of list.slice(0, SUMMARY_HITS)) out.push(`  - ${name}: \`${JSON.stringify(canonical(x)).replaceAll('`', '\\u0060')}\``);
    }
  }
  out.push('');
  return out;
}

/** The run as Markdown: totals, the slowest Rust roots, each mismatch, the files over the bound. */
export function renderSummary(result) {
  const bad = result.roots.filter((e) => !e.ok);
  const problems = [];
  if (bad.length) problems.push(plural(bad.length, 'mismatch'));
  if (result.overBudget.length) problems.push(`${plural(result.overBudget.length, 'file')} at or over the ${result.budgetMs} ms bound`);
  if (result.large && !result.large.ok) problems.push(result.large.message);
  const out = [
    '## Scanner corpus', '',
    `${plural(result.roots.length, 'root')}, ${plural(result.files, 'file')}, ${plural(result.hits, 'hit')}. `
      + `1.x ${seconds(result.nodeMs)} s, Rust ${seconds(result.rustMs)} s. D16(e) normalised: ${result.normalised}.`, '',
    problems.length ? `**Result: ${problems.join(', ')}.**` : '**Result: every root matches within the bound.**', '',
  ];
  if (result.large?.ok) out.push(`${result.large.message[0].toUpperCase()}${result.large.message.slice(1)}.`, '');
  out.push('### Slowest Rust roots', '', '| Root | Files | Rust ms | 1.x ms |', '|---|---:|---:|---:|');
  const slow = [...result.roots].sort((a, b) => b.rustMs - a.rustMs).slice(0, SLOWEST);
  for (const e of slow) out.push(`| ${e.label} | ${e.files} | ${Math.round(e.rustMs)} | ${Math.round(e.nodeMs)} |`);
  out.push('');
  if (bad.length) out.push('### Mismatches', '', ...bad.flatMap(renderMismatch));
  if (result.overBudget.length) {
    out.push('### Files at or over the bound', '', '| Root | File | Rust ms |', '|---|---|---:|');
    for (const t of result.overBudget) out.push(`| ${t.label} | ${t.file} | ${Math.round(t.ms)} |`);
    out.push('');
  }
  return out.join('\n');
}
