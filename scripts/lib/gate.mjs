// The gate: fmt (auto-fix), the escape-hatch scan of changed files, the
// member-manifest check, clippy -D warnings, tests and rustdoc -D warnings.
// Results are cached per tree fingerprint so repeated Stop/SubagentStop checks
// cost nothing.
import fs from 'node:fs';
import path from 'node:path';
import {
  run, readText, writeAtomic, fingerprint, rustChanges, showAt, crateOf, relTo, globMatch,
} from './project.mjs';
import { scanSource, newHits, classifyPath, formatHit, isBuildScript } from './scan.mjs';
import { parseToml } from './toml.mjs';
import { memberProblems } from './manifest.mjs';

const has = (cmd, args) => run(cmd, args, { timeoutMs: 20000 }).code === 0;

export function defaultSteps(cfg) {
  const feat = cfg.gate.allFeatures ? ['--all-features'] : [];
  const steps = [
    { name: 'clippy', cmd: ['cargo', 'clippy', '--workspace', '--all-targets', ...feat, '--message-format=short', '--', '-D', 'warnings'] },
  ];
  if (has('cargo', ['nextest', '--version'])) {
    steps.push({ name: 'test', cmd: ['cargo', 'nextest', 'run', '--workspace', ...feat, '--no-fail-fast', '--no-tests=pass'] });
    steps.push({ name: 'doctest', cmd: ['cargo', 'test', '--doc', '--workspace', ...feat] });
  } else {
    steps.push({ name: 'test', cmd: ['cargo', 'test', '--workspace', ...feat, '--no-fail-fast'] });
  }
  if (cfg.gate.doc) {
    steps.push({ name: 'doc', cmd: ['cargo', 'doc', '--workspace', '--no-deps', ...feat], env: { RUSTDOCFLAGS: '-D warnings' } });
  }
  return steps;
}

function summarize(name, out) {
  const lines = out.split(/\r?\n/);
  let keep;
  if (name === 'clippy' || name === 'doc') {
    keep = lines.filter((l) => /(^|: )(error|warning)(\[|:)/.test(l) && !/could not compile|build failed|generated \d+ warning|aborting due to/.test(l));
  } else {
    keep = lines.filter((l) => /FAILED|panicked at|^---- .* ----$|^failures:|assertion|^\s+left:|^\s+right:|error\[E\d+\]|^error:|test result: FAILED|FAIL \[/.test(l));
  }
  if (!keep.length) keep = lines.filter((l) => l.trim()).slice(-15);
  const capped = keep.slice(0, 30);
  if (keep.length > capped.length) capped.push(`... ${keep.length - capped.length} more lines in the log`);
  return capped.join('\n');
}

/** Scan changed .rs files against their base-revision content: new hard hits fail the gate. */
export function scanChanged(proj, base) {
  const out = { hard: [], ask: [], soft: [] };
  for (const rel of rustChanges(proj, base).filter((f) => f.endsWith('.rs'))) {
    const abs = path.join(proj.gitRoot ?? proj.root, rel);
    const after = readText(abs);
    if (after === null) continue;
    const krate = crateOf(abs);
    const crateRel = krate ? relTo(krate.dir, abs) : rel;
    const kind = krate ? classifyPath(crateRel, krate.hasLib) : 'prod';
    const build = isBuildScript(crateRel);
    const before = showAt(proj, base, rel) ?? '';
    const exempt = (h) => (proj.config.scanner.exempt ?? []).some((e) => e.rule === h.id && (e.paths ?? []).some((p) => globMatch(p, rel)));
    const fresh = newHits(scanSource(before, { kind, build }).hits, scanSource(after, { kind, build }).hits).filter((h) => !exempt(h));
    for (const h of fresh) out[h.severity]?.push({ ...h, file: rel });
  }
  return out;
}

/**
 * Every workspace member inherits the workspace lints and declares no profile
 * (MOD-09, ARC-01, PERF-09). Stable Cargo warns about neither, so a new crate
 * without `[lints] workspace = true` would compile with every escape hatch allowed.
 * @returns {{ ok: boolean, summary: string }}
 */
export function checkManifests(proj) {
  const r = run('cargo', ['metadata', '--no-deps', '--format-version', '1'], { cwd: proj.root, env: { CARGO_TERM_COLOR: 'never' }, timeoutMs: 120000 });
  if (r.code !== 0) {
    const why = (r.error?.message ?? r.stderr).trim().split(/\r?\n/).filter(Boolean).slice(0, 6).join('\n');
    return { ok: false, summary: `cargo metadata failed:\n${why}` };
  }
  let meta;
  try { meta = JSON.parse(r.stdout); } catch { return { ok: false, summary: 'cargo metadata printed output that is not JSON' }; }
  const rootPath = path.join(meta.workspace_root, 'Cargo.toml');
  const rootManifest = parseToml(readText(rootPath));
  const members = new Set(meta.workspace_members ?? []);
  const problems = [];
  let count = 0;
  for (const pkg of meta.packages ?? []) {
    if (members.size && !members.has(pkg.id)) continue;
    count++;
    const rel = relTo(proj.root, pkg.manifest_path);
    const manifest = parseToml(readText(pkg.manifest_path));
    if (!manifest || !rootManifest) { problems.push(`${rel}: not readable as TOML by temper`); continue; }
    const self = path.resolve(pkg.manifest_path) === path.resolve(rootPath);
    for (const p of memberProblems(manifest, { path: rootPath, manifest: rootManifest, self })) {
      problems.push(`${p.id} ${rel}: ${p.msg}.\n    fix: ${p.fix}`);
    }
  }
  return {
    ok: problems.length === 0,
    summary: problems.length ? problems.join('\n') : `${count} member manifest(s) inherit the workspace lints; no member profiles`,
  };
}

/**
 * Run the gate. Returns { ok, fp, cached, steps: [{name, ok, ms, summary, log}] }.
 * @param {{ base?: string, noCache?: boolean, quiet?: boolean }} opts
 */
export function runGate(proj, opts = {}) {
  const cwd = proj.root;
  const cfg = proj.config;
  const steps = [];
  const logs = path.join(proj.stateDir, 'logs');
  fs.mkdirSync(logs, { recursive: true });
  const env = { CARGO_TERM_COLOR: 'never' };

  // 0. Is the toolchain usable here at all? An unusable toolchain is an
  // environment problem, not a red gate: report it instead of blocking.
  const probe = run('cargo', ['--version'], { cwd, env, timeoutMs: 60000 });
  if (probe.code !== 0) {
    const why = (probe.error?.message ?? `${probe.stderr}${probe.stdout}`).trim().split(/\r?\n/).slice(0, 3).join(' ');
    return { ok: null, unavailable: true, fp: null, at: new Date().toISOString(), cached: false, steps: [{ name: 'toolchain', ok: false, ms: 0, summary: `cargo is not usable in this environment: ${why}` }] };
  }

  // 1. fmt is mechanical: fix, don't argue. In check mode (reviewers) verify only.
  const t0 = Date.now();
  const fmtArgs = opts.check ? ['fmt', '--all', '--', '--check'] : ['fmt', '--all'];
  const fmt = run('cargo', fmtArgs, { cwd, env, timeoutMs: 120000 });
  const fmtSummary = opts.check ? 'needs formatting: the implementer runs `cargo fmt --all` (or the gate without --check)' : summarize('fmt', fmt.stderr + fmt.stdout);
  steps.push({ name: 'fmt', ok: fmt.code === 0, ms: Date.now() - t0, summary: fmt.code === 0 ? (opts.check ? 'clean' : 'applied') : fmtSummary });
  if (fmt.code !== 0) return finish(proj, null, steps, false);

  // 2. cache by fingerprint (after formatting)
  const fp = fingerprint(proj, opts.base);
  const cachePath = path.join(proj.stateDir, 'gate', `${fp}.json`);
  if (!opts.noCache) {
    const cached = readText(cachePath);
    if (cached) {
      try { return { ...JSON.parse(cached), cached: true }; } catch { /* rerun */ }
    }
  }

  // 3. escape-hatch scan of changed files (cheap; catches code written around the edit hooks)
  if (cfg.gate.scan) {
    const t = Date.now();
    const s = scanChanged(proj, opts.base);
    const ok = s.hard.length === 0;
    steps.push({
      name: 'scan', ok, ms: Date.now() - t,
      summary: ok ? `${s.soft.length} advisory, ${s.ask.length} exception request(s)` : s.hard.slice(0, 12).map((h) => formatHit(h, h.file)).join('\n'),
      exceptions: s.ask.map((h) => `${h.file}:${h.line} ${h.exception ?? h.id}`),
    });
    if (!ok) return finish(proj, fp, steps, true, cachePath);
  }

  // 4. member manifests (cargo metadata --no-deps: fast, no resolution), before clippy
  {
    const t = Date.now();
    const m = checkManifests(proj);
    steps.push({ name: 'manifests', ok: m.ok, ms: Date.now() - t, summary: m.summary });
    if (!m.ok) return finish(proj, fp, steps, true, cachePath);
  }

  // 5. cargo steps, fail fast
  for (const step of cfg.gate.steps ?? defaultSteps(cfg)) {
    const t = Date.now();
    const [cmd, ...args] = step.cmd;
    const r = run(cmd, args, { cwd, env: { ...env, ...(step.env ?? {}) }, timeoutMs: cfg.gate.stepTimeoutSec * 1000 });
    const out = `${r.stdout}\n${r.stderr}`;
    const log = path.join(logs, `${fp}-${step.name}.log`);
    try { fs.writeFileSync(log, `$ ${step.cmd.join(' ')}\n${out}`); } catch { /* ignore */ }
    const ok = r.code === 0;
    steps.push({ name: step.name, ok, ms: Date.now() - t, summary: ok ? tail(step.name, out) : (r.error ? `could not run: ${r.error.message}` : summarize(step.name, out)), log });
    if (!ok) break;
  }
  return finish(proj, fp, steps, true, cachePath);
}

function tail(name, out) {
  if (name === 'test' || name === 'doctest') {
    const res = out.split(/\r?\n/).filter((l) => /test result:|Summary \[|tests? run:/.test(l));
    return res.slice(-3).join(' | ') || 'passed';
  }
  return 'clean';
}

function finish(proj, fp, steps, cacheable, cachePath) {
  const ok = steps.every((s) => s.ok);
  const result = { ok, fp, at: new Date().toISOString(), steps };
  // Cache only if the tree did not change while we were running.
  if (cacheable && fp && cachePath) {
    try { writeAtomic(cachePath, JSON.stringify(result, null, 2)); } catch { /* ignore */ }
  }
  return { ...result, cached: false };
}

export function cachedGate(proj, fp) {
  const t = readText(path.join(proj.stateDir, 'gate', `${fp}.json`));
  if (!t) return null;
  try { return JSON.parse(t); } catch { return null; }
}

export function renderGate(res) {
  if (res.unavailable) return `temper gate: UNAVAILABLE (${res.steps[0].summary}). Nothing was checked; say so in "Not verified".`;
  const head = res.steps.map((s) => `${s.name} ${s.ok ? 'ok' : 'FAILED'}`).join(' | ');
  const fails = res.steps.filter((s) => !s.ok).map((s) => `--- ${s.name} ---\n${s.summary}${s.log ? `\n(full log: ${s.log})` : ''}`).join('\n');
  const passes = res.steps.filter((s) => s.ok && s.name.includes('test')).map((s) => `${s.name}: ${s.summary}`).join('\n');
  const exc = res.steps.flatMap((s) => s.exceptions ?? []);
  return [
    `temper gate: ${res.ok ? 'GREEN' : 'RED'} (fingerprint ${res.fp ?? 'n/a'}${res.cached ? ', cached' : ''})`,
    head, fails, passes,
    exc.length ? `lint exceptions in the diff (need human approval, H-05): ${exc.join('; ')}` : '',
  ].filter(Boolean).join('\n');
}
