// Cargo manifest checks shared by the pre-edit hook and the gate.
import path from 'node:path';
import { parseToml } from './toml.mjs';
import { readText, norm } from './project.mjs';

export const inheritsLints = (m) => m?.lints?.workspace === true;
export const definesWorkspaceLints = (m) => m?.workspace?.lints !== undefined;
export const hasProfile = (m) => m?.profile !== undefined;

/**
 * The workspace root that governs the package manifest at `manifestPath`,
 * following Cargo's rules: the manifest itself if it has a `[workspace]` table,
 * else `package.workspace`, else the nearest ancestor with a `[workspace]`
 * table that does not exclude it. Returns null for a standalone package.
 * @returns {{ path: string, manifest: object, self: boolean } | null}
 */
export function workspaceRootFor(manifestPath, manifest) {
  const abs = path.resolve(manifestPath);
  if (manifest?.workspace) return { path: abs, manifest, self: true };
  const crateDir = path.dirname(abs);
  const explicit = manifest?.package?.workspace;
  if (typeof explicit === 'string') {
    const p = path.resolve(crateDir, explicit, 'Cargo.toml');
    const m = parseToml(readText(p));
    return m?.workspace ? { path: p, manifest: m, self: false } : null;
  }
  let dir = path.dirname(crateDir);
  for (;;) {
    const p = path.join(dir, 'Cargo.toml');
    const text = readText(p);
    if (text !== null && /^\s*\[\s*workspace\s*[\].]/m.test(text)) {
      const m = parseToml(text);
      if (m?.workspace) {
        const rel = norm(path.relative(dir, crateDir));
        const excluded = (m.workspace.exclude ?? []).some((e) => {
          const x = norm(String(e)).replace(/\/+$/, '');
          return rel === x || rel.startsWith(`${x}/`);
        });
        return excluded ? null : { path: p, manifest: m, self: false };
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

/**
 * Structural problems with a package manifest inside a workspace:
 * missing lint inheritance (MOD-09/ARC-01) and member profiles (PERF-09).
 * @returns {{ id: string, msg: string, fix: string }[]}
 */
export function memberProblems(manifest, root) {
  const out = [];
  if (!manifest || !root) return out;
  if (manifest.package && definesWorkspaceLints(root.manifest) && !inheritsLints(manifest)) {
    out.push({
      id: 'MOD-09',
      msg: 'the package does not inherit the workspace lints, so clippy would accept unwrap, panic!, as casts and #[allow] in it',
      fix: 'add a `[lints]` table containing only `workspace = true` (MOD-09, ARC-01).',
    });
  }
  if (!root.self && hasProfile(manifest)) {
    out.push({
      id: 'PERF-09',
      msg: 'a `[profile]` table in a member manifest is ignored by Cargo',
      fix: 'move the profile setting to the workspace root manifest, as an H-16 decision recorded in the root AGENTS.md (PERF-09).',
    });
  }
  return out;
}

const TARGET_KEYS = ['test', 'doctest', 'harness', 'bench', 'required-features'];
const AUTO_KEYS = ['autotests', 'autobenches', 'autoexamples', 'autobins', 'autolib', 'build'];
const MSRV_KEYS = ['rust-version', 'edition', 'resolver'];

/**
 * Manifest switches whose change is a human decision, as label -> JSON value.
 * H-12: switches that can stop tests, doctests or targets from running.
 * H-16: dependency source overrides, MSRV/edition/resolver and default features.
 */
export function watchedKeys(m) {
  const out = new Map();
  const put = (id, label, v) => { if (v !== undefined) out.set(label, { id, value: JSON.stringify(v) }); };
  const pkg = m?.package ?? {};
  for (const k of AUTO_KEYS) put('H-12', `package.${k}`, pkg[k]);
  for (const k of MSRV_KEYS) {
    put('H-16', `package.${k}`, pkg[k]);
    put('H-16', `workspace.package.${k}`, m?.workspace?.package?.[k]);
  }
  put('H-16', 'workspace.resolver', m?.workspace?.resolver);
  put('H-16', 'features.default', m?.features?.default);
  put('H-16', 'patch', m?.patch);
  put('H-16', 'replace', m?.replace);
  const targets = [['lib', m?.lib ? [m.lib] : []]];
  for (const kind of ['bin', 'test', 'bench', 'example']) targets.push([kind, Array.isArray(m?.[kind]) ? m[kind] : []]);
  for (const [kind, list] of targets) {
    list.forEach((t, i) => {
      const head = kind === 'lib' ? '[lib]' : `[[${kind}]] ${typeof t?.name === 'string' ? t.name : `#${i + 1}`}`;
      for (const k of TARGET_KEYS) {
        if (kind === 'bench' && k === 'harness') continue; // criterion and divan need `harness = false`
        put('H-12', `${head} ${k}`, t?.[k]);
      }
    });
  }
  return out;
}

/** Is this watched value benign in a brand-new manifest (plain inheritance from the workspace)? */
export function benignWhenNew(label, entry) {
  if (/^package\.(rust-version|edition)$/.test(label)) return entry.value === '{"workspace":true}';
  return label === 'features.default';
}

/** Dependency version requirements, as "table:name" -> requirement string. */
export function depRequirements(m) {
  const out = new Map();
  const visit = (label, deps) => {
    if (!deps || typeof deps !== 'object') return;
    for (const [name, spec] of Object.entries(deps)) {
      const req = typeof spec === 'string' ? spec : (typeof spec?.version === 'string' ? spec.version : null);
      if (req !== null) out.set(`${label}:${name}`, req);
    }
  };
  for (const t of ['dependencies', 'dev-dependencies', 'build-dependencies']) visit(t, m?.[t]);
  visit('workspace.dependencies', m?.workspace?.dependencies);
  for (const [cfg, tt] of Object.entries(m?.target ?? {})) {
    for (const t of ['dependencies', 'dev-dependencies', 'build-dependencies']) visit(`target.${cfg}.${t}`, tt?.[t]);
  }
  return out;
}

/** Why a version requirement is not a caret floor (ARC-12), or null if it is one. */
export function nonCaretRequirement(req) {
  const parts = String(req).split(',').map((s) => s.trim()).filter(Boolean);
  if (!parts.length) return null;
  if (parts.some((p) => p.startsWith('='))) return 'an exact `=` pin';
  if (parts.some((p) => p.startsWith('~'))) return 'a tilde `~` requirement';
  if (parts.some((p) => p.includes('*'))) return 'a wildcard `*`';
  if (parts.some((p) => p.startsWith('<'))) return 'an upper bound `<`';
  if (parts.every((p) => p.startsWith('>'))) return 'an open-ended `>=`';
  return null;
}
