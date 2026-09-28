#!/usr/bin/env node
// temper: enforcement CLI for the rust-craft standard.
//
//   temper hook <event>        hook entry point (reads the hook JSON on stdin)
//   temper gate [--check] [--no-cache]   fmt (--check: verify only) + scan + manifests + clippy -D warnings + tests + rustdoc
//   temper scan [files..] [--all] [--json]
//   temper status              fingerprint, changed files, gate and review state
//   temper setup [--dry-run] [--force]   install repo templates (lints, rules, charters, CI)
//   temper stats               enforcement telemetry summary
//
// Zero dependencies; Node 18+. Works on Windows, macOS and Linux.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as H from './lib/hooks.mjs';
import { discover, baseRev, rustChanges, fingerprint, relTo, crateOf, readText, headRev } from './lib/project.mjs';
import { runGate, renderGate, cachedGate, scanChanged } from './lib/gate.mjs';
import { scanSource, classifyPath, formatHit, isBuildScript } from './lib/scan.mjs';
import { verdictFor } from './lib/verdict.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const pluginRoot = path.resolve(here, '..');

const HOOKS = {
  'session-start': H.sessionStart,
  'user-prompt': H.userPromptSubmit,
  'pre-edit': H.preEdit,
  'pre-bash': H.preBash,
  'pre-handback': H.preHandback,
  'post-tool': H.postTool,
  stop: H.stop,
  'subagent-stop': H.subagentStop,
  'subagent-start': H.subagentStart,
};

async function readStdin() {
  if (process.stdin.isTTY) return '';
  const chunks = [];
  for await (const c of process.stdin) chunks.push(c);
  return Buffer.concat(chunks).toString('utf8');
}

async function hook(event) {
  const handler = HOOKS[event];
  if (!handler) { process.stderr.write(`temper: unknown hook event ${event}\n`); return 0; }
  let input = {};
  try { input = JSON.parse((await readStdin()) || '{}'); } catch { return 0; }
  if (process.env.TEMPER_DISABLE === '1') return 0;
  let res;
  try {
    res = handler(input) ?? {};
  } catch (e) {
    // Never wedge the session on a hook bug: report and let the action proceed.
    process.stdout.write(JSON.stringify({ systemMessage: `temper hook ${event} failed: ${e?.message ?? e}` }));
    return 0;
  }
  if (res.json) process.stdout.write(JSON.stringify(res.json));
  else if (res.text) process.stdout.write(res.text);
  return res.exit ?? 0;
}

function project() {
  const proj = discover(process.cwd());
  if (!proj) { process.stderr.write('temper: not inside a Rust project (no Cargo.toml found)\n'); process.exit(2); }
  return proj;
}

function sessionBase(proj) {
  // CLI runs outside a hook: the base is the merge-base with the default branch if available, else HEAD.
  const env = process.env.TEMPER_BASE;
  if (env) return env;
  return headRev(proj);
}

function gate(args) {
  const proj = project();
  const res = runGate(proj, { base: sessionBase(proj), noCache: args.includes('--no-cache'), check: args.includes('--check') });
  if (args.includes('--json')) process.stdout.write(JSON.stringify(res, null, 2) + '\n');
  else process.stdout.write(renderGate(res) + '\n');
  return res.ok ? 0 : 1;
}

function scan(args) {
  const proj = project();
  const json = args.includes('--json');
  const files = args.filter((a) => !a.startsWith('--'));
  let results = [];
  if (args.includes('--all') || files.length) {
    const targets = files.length ? files.map((f) => path.resolve(f)) : walk(proj.root);
    for (const abs of targets) {
      const src = readText(abs);
      if (src === null) continue;
      const krate = crateOf(abs);
      const crateRel = krate ? relTo(krate.dir, abs) : relTo(proj.root, abs);
      const kind = krate ? classifyPath(crateRel, krate.hasLib) : 'prod';
      for (const h of scanSource(src, { kind, build: isBuildScript(crateRel) }).hits) results.push({ ...h, file: relTo(proj.root, abs) });
    }
  } else {
    const s = scanChanged(proj, sessionBase(proj));
    results = [...s.hard, ...s.ask, ...s.soft];
  }
  if (json) { process.stdout.write(JSON.stringify(results, null, 2) + '\n'); }
  else {
    const order = { hard: 0, ask: 1, soft: 2 };
    results.sort((a, b) => order[a.severity] - order[b.severity] || a.file.localeCompare(b.file) || a.line - b.line);
    for (const h of results) process.stdout.write(`[${h.severity}] ${formatHit(h, h.file)}\n`);
    const by = (s) => results.filter((h) => h.severity === s).length;
    process.stdout.write(`\n${by('hard')} blocking, ${by('ask')} exception request(s), ${by('soft')} advisory\n`);
  }
  return results.some((h) => h.severity === 'hard') ? 1 : 0;
}

function walk(root) {
  const out = [];
  const rec = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      if (['target', '.git', 'node_modules', '.temper'].includes(e.name)) continue;
      const p = path.join(d, e.name);
      if (e.isDirectory()) rec(p); else if (e.name.endsWith('.rs')) out.push(p);
    }
  };
  rec(root);
  return out;
}

function status() {
  const proj = project();
  const base = sessionBase(proj);
  const changes = rustChanges(proj, base);
  const fp = fingerprint(proj, base);
  const g = cachedGate(proj, fp);
  const v = verdictFor(proj, fp);
  process.stdout.write([
    `root:        ${proj.root}`,
    `state:       ${proj.stateDir}`,
    `base:        ${base ?? 'none'}`,
    `fingerprint: ${fp}`,
    `changed:     ${changes.length ? changes.join(', ') : 'none'}`,
    `gate:        ${g ? (g.ok ? 'green' : 'red') : 'not run for this fingerprint'}`,
    `review:      ${v ? `${v.verdict}${v.stale ? ' (stale)' : ''}` : 'none for this fingerprint'}`,
    `config:      review=${proj.config.review} skillGate=${proj.config.skillGate}`,
  ].join('\n') + '\n');
  return 0;
}

function setup(args) {
  const dry = args.includes('--dry-run');
  const force = args.includes('--force');
  const root = discover(process.cwd())?.root ?? process.cwd();
  const tpl = path.join(pluginRoot, 'templates', 'workspace');
  const actions = [];
  const copy = (srcRel, dstRel = srcRel) => {
    const src = path.join(tpl, srcRel);
    const dst = path.join(root, dstRel);
    const exists = fs.existsSync(dst);
    if (exists && !force) { actions.push(`skip   ${dstRel} (exists; compare with ${path.relative(root, src)})`); return; }
    actions.push(`${exists ? 'update' : 'create'} ${dstRel}`);
    if (!dry) { fs.mkdirSync(path.dirname(dst), { recursive: true }); fs.copyFileSync(src, dst); }
  };
  const list = (d, base = '') => fs.readdirSync(path.join(d, base), { withFileTypes: true }).flatMap((e) => {
    const r = path.join(base, e.name);
    return e.isDirectory() ? list(d, r) : [r];
  });
  for (const f of list(tpl)) {
    if (f === 'Cargo.toml') continue; // merged by hand: never overwrite a manifest
    copy(f);
  }
  const manifest = path.join(root, 'Cargo.toml');
  if (fs.existsSync(manifest)) {
    const t = readText(manifest) ?? '';
    if (!/\[workspace\.lints/.test(t)) actions.push(`manual Cargo.toml: merge [workspace.lints.*] and [workspace.package] from ${path.join(tpl, 'Cargo.toml')}, then add \`[lints] workspace = true\` to each member`);
  } else {
    copy('Cargo.toml');
  }
  const gi = path.join(root, '.gitignore');
  const giText = readText(gi) ?? '';
  if (!/^\/?\.temper\/?$/m.test(giText)) {
    actions.push('append .gitignore: .temper/');
    if (!dry) fs.appendFileSync(gi, `${giText.endsWith('\n') || giText === '' ? '' : '\n'}.temper/\n`);
  }
  process.stdout.write(`${dry ? '[dry run] ' : ''}temper setup in ${root}\n${actions.map((a) => `  ${a}`).join('\n')}\n`);
  process.stdout.write('\nNext: give every crate a charter (copy templates/crate/AGENTS.md and CLAUDE.md into each crate root and fill it in).\n');
  return 0;
}

function stats() {
  const proj = project();
  const t = readText(path.join(proj.stateDir, 'telemetry.jsonl')) ?? '';
  const rows = t.split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
  const count = {};
  for (const r of rows) count[r.event] = (count[r.event] ?? 0) + 1;
  const rules = {};
  for (const r of rows.filter((x) => x.event === 'deny-scan')) for (const id of r.rules ?? []) rules[id] = (rules[id] ?? 0) + 1;
  const sessions = new Set(rows.map((r) => r.sid)).size;
  process.stdout.write(`sessions: ${sessions}\nevents:\n${Object.entries(count).sort((a, b) => b[1] - a[1]).map(([k, v]) => `  ${k.padEnd(26)} ${v}`).join('\n')}\n`);
  if (Object.keys(rules).length) process.stdout.write(`escape hatches blocked by rule:\n${Object.entries(rules).sort((a, b) => b[1] - a[1]).map(([k, v]) => `  ${k.padEnd(8)} ${v}`).join('\n')}\n`);
  return 0;
}

// A closed pipe (`temper scan | head`) ends output quietly instead of crashing.
process.stdout.on('error', (e) => { if (e.code === 'EPIPE') process.exit(process.exitCode ?? 0); else throw e; });

const [cmd, ...rest] = process.argv.slice(2);
const code = await (async () => {
  switch (cmd) {
    case 'hook': return hook(rest[0]);
    case 'gate': return gate(rest);
    case 'scan': return scan(rest);
    case 'status': return status();
    case 'setup': return setup(rest);
    case 'stats': return stats();
    default:
      process.stdout.write('usage: temper <hook EVENT | gate [--check] [--no-cache] [--json] | scan [FILES..] [--all] [--json] | status | setup [--dry-run] [--force] | stats>\n');
      return cmd ? 2 : 0;
  }
})();
process.exitCode = code;
