// Project discovery, state storage, configuration and tree fingerprinting.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export const norm = (p) => p.replace(/\\/g, '/');

export function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, {
    // stdin is /dev/null: cargo's rustc probes read stdin, and a hook's stdin is not theirs.
    cwd: opts.cwd, env: { ...process.env, ...(opts.env ?? {}) }, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    maxBuffer: 256 * 1024 * 1024, timeout: opts.timeoutMs ?? 0, windowsHide: true,
  });
  return { code: r.status ?? (r.error ? 127 : 1), stdout: r.stdout ?? '', stderr: r.stderr ?? '', error: r.error };
}

const git = (cwd, args, opts) => run('git', args, { cwd, ...opts });

export function readText(p) {
  try { return fs.readFileSync(p, 'utf8'); } catch { return null; }
}

export function writeAtomic(p, text) {
  fs.mkdirSync(path.dirname(p), { recursive: true });
  const tmp = `${p}.${process.pid}.${Date.now()}.tmp`;
  fs.writeFileSync(tmp, text);
  fs.renameSync(tmp, p);
}

/** Nearest ancestor (inclusive) containing `name`. */
export function findUp(start, predicate) {
  let dir = path.resolve(start);
  for (;;) {
    if (predicate(dir)) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

const hasFile = (name) => (dir) => fs.existsSync(path.join(dir, name));

/**
 * Locate the project: git top-level if inside a repo, otherwise the outermost
 * Cargo workspace root above `cwd`. Returns null when this isn't a Rust project.
 */
export function discover(cwd) {
  const start = cwd && fs.existsSync(cwd) ? cwd : process.cwd();
  const top = git(start, ['rev-parse', '--show-toplevel']);
  const gitRoot = top.code === 0 ? path.resolve(top.stdout.trim()) : null;
  let root = gitRoot;
  if (!root) {
    let dir = findUp(start, hasFile('Cargo.toml'));
    let outer = dir;
    while (dir) {
      outer = dir;
      const parent = path.dirname(dir);
      dir = parent === dir ? null : findUp(parent, hasFile('Cargo.toml'));
    }
    root = outer;
  }
  if (!root) return null;
  const isRust = fs.existsSync(path.join(root, 'Cargo.toml')) || fs.existsSync(path.join(start, 'Cargo.toml'))
    || findUp(start, hasFile('Cargo.toml')) !== null;
  if (!isRust) return null;
  let stateDir;
  if (gitRoot) {
    const common = git(gitRoot, ['rev-parse', '--git-common-dir']);
    const gd = common.code === 0 ? path.resolve(gitRoot, common.stdout.trim()) : path.join(gitRoot, '.git');
    stateDir = path.join(gd, 'temper');
  } else {
    stateDir = path.join(root, 'target', 'temper');
  }
  return { root, gitRoot, stateDir, config: loadConfig(root) };
}

export const DEFAULT_CONFIG = {
  skillGate: true,
  review: 'required',              // required | advisory | off
  askOnNewDependency: true,
  // Dependencies the standard itself relies on; adding them does not prompt (H-16 pre-approval).
  preapprovedDependencies: ['thiserror', 'anyhow', 'serde', 'serde_json', 'tracing', 'tracing-subscriber', 'proptest', 'insta', 'trybuild', 'divan', 'criterion', 'tempfile'],
  gate: {
    allFeatures: true,
    doc: true,
    scan: true,
    stepTimeoutSec: 900,
    steps: null,                   // override: [{ name, cmd: [..], env: {..} }]
  },
  protected: [
    '**/clippy.toml', '**/.clippy.toml', '**/rustfmt.toml', '**/.rustfmt.toml', '**/deny.toml',
    'rust-toolchain', 'rust-toolchain.toml', '.cargo/**', '.claude/**', '.github/**',
    '.githooks/**', 'CLAUDE.md', 'AGENTS.md',
  ],
  scanner: { exempt: [] },         // [{ rule: 'H-01', paths: ['crates/ffi-sys/**'] }]
  // Members (dirs relative to the project root, globs allowed) that a human approved to carry
  // their own full [lints] table instead of `[lints] workspace = true`: the H-01 FFI exception,
  // the ERR-09 arithmetic opt-in. Any change to their [lints] still asks (H-05).
  manifests: { ownLints: [] },
};

export function loadConfig(root) {
  const text = readText(path.join(root, '.claude', 'temper.json'));
  if (!text) return structuredClone(DEFAULT_CONFIG);
  try {
    const user = JSON.parse(text);
    return {
      ...DEFAULT_CONFIG, ...user,
      gate: { ...DEFAULT_CONFIG.gate, ...(user.gate ?? {}) },
      scanner: { ...DEFAULT_CONFIG.scanner, ...(user.scanner ?? {}) },
      manifests: { ...DEFAULT_CONFIG.manifests, ...(user.manifests ?? {}) },
      protected: user.protected ?? DEFAULT_CONFIG.protected,
    };
  } catch {
    return structuredClone(DEFAULT_CONFIG);
  }
}

// Glob match: "**/" = any directories, "**" = anything, "*" = within a segment, "?" = one char.
export function globMatch(pattern, rel) {
  let re = '';
  const p = norm(pattern);
  for (let i = 0; i < p.length; i++) {
    const c = p[i];
    if (c === '*' && p[i + 1] === '*') {
      if (p[i + 2] === '/') { re += '(?:.*/)?'; i += 2; } else { re += '.*'; i += 1; }
    } else if (c === '*') re += '[^/]*';
    else if (c === '?') re += '[^/]';
    else re += c.replace(/[.+^${}()|[\]\\]/g, '\\$&');
  }
  return new RegExp(`^${re}$`).test(norm(rel));
}

// cargo and rustfmt on Windows can report verbatim paths (\\?\C:\..., //?/C:/...), which
// path.relative treats as a different root from C:\...
const unverbatim = (p) => (process.platform === 'win32'
  ? p.replace(/^[\\/]{2}\?[\\/](UNC[\\/])?/i, (_m, unc) => (unc ? '\\\\' : ''))
  : p);

// The long, real form of a path that may not exist yet: its nearest existing ancestor
// through realpath, plus the rest.
function longPath(p) {
  try {
    return fs.realpathSync.native(p);
  } catch {
    const parent = path.dirname(p);
    return parent === p ? p : path.join(longPath(parent), path.basename(p));
  }
}

export function relTo(root, file) {
  const r = unverbatim(root);
  const f = path.resolve(unverbatim(file));
  const rel = path.relative(r, f);
  // One directory can be spelled two ways: through a symlinked ancestor (macOS TMPDIR under
  // /var, a link to the /private/var that cargo reports) or, on Windows, with 8.3 short names
  // (C:\Users\RUNNER~1) from TEMP and the cwd against long names from git. Compare the real,
  // long forms when the lexical ones put the file outside the root.
  if (rel.startsWith('..') || path.isAbsolute(rel)) {
    return norm(path.relative(longPath(r), longPath(f)));
  }
  return norm(rel);
}

/** Crate root (dir with a Cargo.toml containing [package]) for a file. */
export function crateOf(file, stopAt) {
  const dir = findUp(path.dirname(path.resolve(file)), (d) => {
    const t = readText(path.join(d, 'Cargo.toml'));
    return t !== null && /^\s*\[package\]/m.test(t);
  });
  if (!dir) return null;
  if (stopAt && !path.resolve(dir).startsWith(path.resolve(stopAt))) return null;
  return { dir, hasLib: fs.existsSync(path.join(dir, 'src', 'lib.rs')) };
}

// ---------- session state ----------

export function sessionDir(proj, sid) {
  return path.join(proj.stateDir, 'sessions', String(sid || 'unknown').replace(/[^\w.-]/g, '_'));
}

export function agentKey(input) {
  return input.agent_id ? `agent-${String(input.agent_id).replace(/[^\w.-]/g, '_')}` : 'main';
}

// Skill-load markers are per session and agent context, independent of which
// project the session happens to be in (the session may start outside the repo).
function markerDir(input) {
  const base = process.env.CLAUDE_PLUGIN_DATA || path.join(os.tmpdir(), 'temper-plugin-data');
  return path.join(base, 'sessions', String(input.session_id || 'unknown').replace(/[^\w.-]/g, '_'));
}

export function markSkillLoaded(_proj, input) {
  writeAtomic(path.join(markerDir(input), `skill-${agentKey(input)}`), new Date().toISOString());
}

export function skillLoaded(_proj, input) {
  return fs.existsSync(path.join(markerDir(input), `skill-${agentKey(input)}`));
}

export function pruneMarkers(maxAgeDays = 7) {
  const dir = path.join(process.env.CLAUDE_PLUGIN_DATA || path.join(os.tmpdir(), 'temper-plugin-data'), 'sessions');
  let entries = [];
  try { entries = fs.readdirSync(dir); } catch { return; }
  const cutoff = Date.now() - maxAgeDays * 86400000;
  for (const e of entries) {
    const p = path.join(dir, e);
    try { if (fs.statSync(p).mtimeMs < cutoff) fs.rmSync(p, { recursive: true, force: true }); } catch { /* ignore */ }
  }
}

/**
 * Where temper may write an output (`baseline --findings-out`, `report --out`):
 * under the project's target/ or .temper/ after every symlink is resolved, with
 * no `..`, and never onto a symlink. So an allow-listed command cannot be aimed
 * at sources or configuration, not even through a symlinked directory.
 * @param {string} root the project root
 * @param {string} out the path as given (relative to the cwd)
 * @param {{ dir?: boolean, files?: string[] }} opts `dir`: `out` is a directory
 *   to write into (it may be target/ or .temper/ itself); `files`: the names
 *   that will be written inside it
 * @returns {string|null} the absolute path, or null when refused
 */
export function safeOutputPath(root, out, { dir = false, files = [] } = {}) {
  if (!out || norm(String(out)).split('/').includes('..')) return null;
  const abs = path.resolve(out);
  const rel = relTo(root, abs);
  if (!(dir ? /^(?:target|\.temper)(?:\/.|$)/ : /^(?:target|\.temper)\/./).test(rel)) return null;
  const real = (p) => { try { return fs.realpathSync.native(p); } catch { return null; } };
  const realRoot = real(root) ?? path.resolve(root);
  const allowed = ['target', '.temper'].map((d) => {
    const p = path.join(root, d);
    try { fs.lstatSync(p); } catch { return path.join(realRoot, d); } // not there yet: it will be created under the root
    return real(p); // a dangling link resolves to null and allows nothing
  }).filter(Boolean);
  // The deepest existing ancestor of the directory that will hold the output, resolved.
  let existing = dir ? abs : path.dirname(abs);
  const rest = [];
  for (;;) {
    try { fs.lstatSync(existing); break; } catch { /* not there yet */ }
    const up = path.dirname(existing);
    if (up === existing) return null;
    rest.unshift(path.basename(existing));
    existing = up;
  }
  const resolvedBase = real(existing);
  if (!resolvedBase) return null; // a dangling symlink on the way
  const resolved = path.join(resolvedBase, ...rest);
  if (!allowed.some((a) => resolved === a || resolved.startsWith(`${a}${path.sep}`))) return null;
  const finals = dir ? files.map((f) => path.join(abs, f)) : [abs];
  for (const f of finals) { try { if (fs.lstatSync(f).isSymbolicLink()) return null; } catch { /* not there yet */ } }
  return abs;
}

/** A revision as a commit sha of this repository, or null when it does not resolve. */
export function resolveRev(proj, rev) {
  if (!proj?.gitRoot || !rev) return null;
  const r = git(proj.gitRoot, ['rev-parse', '--verify', '--quiet', `${rev}^{commit}`]);
  return r.code === 0 ? r.stdout.trim() || null : null;
}

export function headRev(proj) {
  if (!proj.gitRoot) return null;
  const r = git(proj.gitRoot, ['rev-parse', 'HEAD']);
  return r.code === 0 ? r.stdout.trim() : null;
}

export function baseRev(proj, sid) {
  const t = readText(path.join(sessionDir(proj, sid), 'base'));
  // A recorded base that no longer resolves (history rewritten, another repository): HEAD.
  return (t && t.trim() && resolveRev(proj, t.trim())) || headRev(proj);
}

export function setBase(proj, sid, force) {
  const p = path.join(sessionDir(proj, sid), 'base');
  if (!force && fs.existsSync(p)) return;
  const head = headRev(proj);
  writeAtomic(p, head ?? '');
  writeAtomic(path.join(sessionDir(proj, sid), 'started'), String(Date.now()));
  // Fingerprint of the tree as the session found it: uncommitted work that
  // predates the session (e.g. a diff the user asked us to review) is not ours.
  writeAtomic(path.join(sessionDir(proj, sid), 'start-fp'), fingerprint(proj, head));
}

export function startFingerprint(proj, sid) {
  const t = readText(path.join(sessionDir(proj, sid), 'start-fp'));
  return t ? t.trim() : null;
}

export function pruneSessions(proj, maxAgeDays = 14) {
  const dir = path.join(proj.stateDir, 'sessions');
  let entries = [];
  try { entries = fs.readdirSync(dir); } catch { return; }
  const cutoff = Date.now() - maxAgeDays * 86400000;
  for (const e of entries) {
    const p = path.join(dir, e);
    try { if (fs.statSync(p).mtimeMs < cutoff) fs.rmSync(p, { recursive: true, force: true }); } catch { /* ignore */ }
  }
}

export function telemetry(proj, input, event, detail = {}) {
  try {
    fs.mkdirSync(proj.stateDir, { recursive: true });
    fs.appendFileSync(path.join(proj.stateDir, 'telemetry.jsonl'), JSON.stringify({
      t: new Date().toISOString(), sid: input?.session_id, agent: input?.agent_type ?? 'main', event, ...detail,
    }) + '\n');
  } catch { /* best effort */ }
}

// ---------- change detection and fingerprint ----------

// What counts as "the Rust change": sources, manifests, toolchain and lint config,
// cargo config (rustflags), temper's own config (gate steps and features, scan,
// review mode), test expectations (snapshots, trybuild .stderr) and anything
// under a tests/ dir. Other files (notes, logs, transcripts) never invalidate a
// gate result or a review. Paths are relative to the git top-level, which is
// also the project root that temper reads .claude/temper.json from, so that
// file counts at the root only. Cargo reads .cargo/config(.toml) from its cwd
// and every parent, so one in any directory counts: the gate runs cargo at the
// root, but a workspace in a subdirectory, or cargo run in a member, reads a
// nested one too.
const TEMPER_CONFIG_FILE = '.claude/temper.json';
const RUST_RELEVANT = /(\.rs$)|(\.snap$)|(\.stderr$)|((^|\/)tests\/)|((^|\/)Cargo\.(toml|lock)$)|((^|\/)(rust-toolchain(\.toml)?|\.?clippy\.toml|\.?rustfmt\.toml|deny\.toml)$)|((^|\/)\.cargo\/config(\.toml)?$)|(^\.claude\/temper\.json$)/;
const RELEVANT_PATHSPECS = [
  ':(glob)**/*.rs', ':(glob)**/*.snap', ':(glob)**/*.stderr', ':(glob)**/tests/**',
  ':(glob)**/Cargo.toml', ':(glob)**/Cargo.lock', ':(glob)**/rust-toolchain', ':(glob)**/rust-toolchain.toml',
  ':(glob)**/clippy.toml', ':(glob)**/.clippy.toml', ':(glob)**/rustfmt.toml', ':(glob)**/.rustfmt.toml', ':(glob)**/deny.toml',
  ':(glob)**/.cargo/config', ':(glob)**/.cargo/config.toml', `:(literal)${TEMPER_CONFIG_FILE}`,
];
const EXCLUDE = ['.temper', 'target'];

/**
 * The committed lint baseline (temper baseline). It changes the gate's verdict,
 * so it is part of the fingerprint, but it is not a Rust change on its own.
 */
export const BASELINE_FILE = '.claude/temper-baseline.json';
const FINGERPRINT_PATHSPECS = [...RELEVANT_PATHSPECS, `:(literal)${BASELINE_FILE}`];
const inFingerprint = (rel) => RUST_RELEVANT.test(rel) || rel === BASELINE_FILE;

function untracked(proj) {
  const r = git(proj.gitRoot, ['ls-files', '--others', '--exclude-standard', '-z']);
  if (r.code !== 0) return [];
  return r.stdout.split('\0').filter(Boolean).filter((f) => !EXCLUDE.some((x) => f === x || f.startsWith(`${x}/`)));
}

/** Files changed since the session base (committed or not), plus untracked files. */
export function changedFiles(proj, base) {
  if (!proj.gitRoot) return walkRust(proj.root).map((f) => relTo(proj.root, f));
  const args = ['diff', '--name-only', '-z'];
  if (base) args.push(base);
  args.push('--', '.', ...EXCLUDE.map((x) => `:(exclude)${x}`));
  const r = git(proj.gitRoot, args);
  const tracked = r.code === 0 ? r.stdout.split('\0').filter(Boolean) : [];
  return [...new Set([...tracked, ...untracked(proj)])];
}

export function rustChanges(proj, base) {
  return changedFiles(proj, base).filter((f) => RUST_RELEVANT.test(norm(f)));
}

function walkRust(root) {
  const out = [];
  const walk = (d) => {
    let es = [];
    try { es = fs.readdirSync(d, { withFileTypes: true }); } catch { return; }
    for (const e of es) {
      if (e.name === 'target' || e.name === '.git' || e.name === 'node_modules' || e.name === '.temper') continue;
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (RUST_RELEVANT.test(norm(p))) out.push(p);
    }
  };
  walk(root);
  return out;
}

/** Stable fingerprint of the Rust-relevant working tree relative to base. */
export function fingerprint(proj, base) {
  const h = createHash('sha256');
  if (proj.gitRoot) {
    // The tree is the base commit plus the diff, so the commits are part of the key:
    // otherwise every clean tree hashes the same and a cached result from another
    // commit would be reused. HEAD always counts; a base that does not resolve here
    // (a TEMPER_BASE from another repository) is replaced by HEAD.
    const head = resolveRev(proj, 'HEAD');
    const rev = (base ? resolveRev(proj, base) : null) ?? head;
    h.update(`head:${head ?? ''}\nrev:${rev ?? ''}\n`);
    const args = ['diff', '--no-color', '--binary', '--no-ext-diff'];
    if (rev) args.push(rev);
    args.push('--', ...FINGERPRINT_PATHSPECS, ...EXCLUDE.map((x) => `:(exclude)${x}`));
    h.update(git(proj.gitRoot, args).stdout);
    for (const f of untracked(proj).filter((u) => inFingerprint(norm(u))).sort()) {
      h.update(`\n?${f}\n`);
      try { h.update(fs.readFileSync(path.join(proj.gitRoot, f))); } catch { /* vanished */ }
    }
  } else {
    const extra = [BASELINE_FILE, TEMPER_CONFIG_FILE].map((rel) => path.join(proj.root, ...rel.split('/')));
    const files = walkRust(proj.root).concat(extra.filter((f) => fs.existsSync(f)));
    for (const f of files.sort()) {
      h.update(`\n${relTo(proj.root, f)}\n`);
      try { h.update(fs.readFileSync(f)); } catch { /* vanished */ }
    }
  }
  return h.digest('hex').slice(0, 16);
}

/** File content at a revision (null if absent). */
export function showAt(proj, rev, rel) {
  if (!proj.gitRoot || !rev) return null;
  const r = git(proj.gitRoot, ['show', `${rev}:${norm(rel)}`]);
  return r.code === 0 ? r.stdout : null;
}

export function diffStat(proj, base) {
  if (!proj.gitRoot) return '';
  const args = ['diff', '--stat=120', '--no-color'];
  if (base) args.push(base);
  args.push('--', '.', ...EXCLUDE.map((x) => `:(exclude)${x}`));
  const r = git(proj.gitRoot, args);
  const u = untracked(proj).filter((f) => RUST_RELEVANT.test(norm(f)));
  return (r.stdout.trim() + (u.length ? `\nuntracked: ${u.join(', ')}` : '')).trim();
}
