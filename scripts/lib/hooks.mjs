// Hook handlers. Each returns { json?, text?, exit? }.
// Context strings are phrased as facts about the project, not as out-of-band
// commands, so Claude treats them as project information.
import fs from 'node:fs';
import path from 'node:path';
import {
  discover, relTo, norm, readText, writeAtomic, globMatch, crateOf, sessionDir, agentKey,
  markSkillLoaded, skillLoaded, setBase, baseRev, pruneSessions, pruneMarkers, telemetry, rustChanges,
  fingerprint, diffStat, run, startFingerprint, pruneFingerprintState,
} from './project.mjs';
import { scanSource, newHits, classifyPath, formatHit, isBuildScript } from './scan.mjs';
import { parseToml } from './toml.mjs';
import {
  workspaceRootFor, memberProblems, watchedKeys, benignWhenNew, depRequirements, nonCaretRequirement, inheritsLints,
  ownLintsMember, splitProblems, dependencyNames, majorOf,
} from './manifest.mjs';
import { runGate, renderGate, cachedGate } from './gate.mjs';
import { extractVerdict, validateVerdict, recordVerdict, verdictFor, summarizeFindings } from './verdict.mjs';

const REVIEWER = /(^|:)rust-reviewer$/;
const IMPLEMENTER = /(^|:)rust-implementer$/;
const PRELOADED = /(^|:)rust-(implementer|reviewer|architect)$/;
const SKILL = /(^|:)rust-craft$/;

const pluginRoot = () => process.env.CLAUDE_PLUGIN_ROOT ?? path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')), '..', '..');
const cli = () => path.join(pluginRoot(), 'scripts', 'temper.mjs');
const gateCmd = () => `node "${cli()}" gate`;

const ctx = (event, text) => ({ json: { hookSpecificOutput: { hookEventName: event, additionalContext: text } } });
const deny = (reason, extraContext) => ({ json: { hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: 'deny', permissionDecisionReason: reason, ...(extraContext ? { additionalContext: extraContext } : {}) } } });
const ask = (reason, extraContext) => ({ json: { hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: 'ask', permissionDecisionReason: reason, ...(extraContext ? { additionalContext: extraContext } : {}) } } });
const block = (reason) => ({ json: { decision: 'block', reason } });
const notice = (msg) => ({ json: { systemMessage: msg } });
const none = () => ({});

// Compact form of the Iron rules table in skills/rust-craft/SKILL.md; keep the two in step.
export const IRON_RULES = [
  'H-01 no unsafe',
  'H-02 no unwrap/expect in production code or doctests',
  'H-03 no panic!/todo!/unimplemented!/unreachable!, and no std call that panics on input you do not control (integer / and %, thread::spawn, env::args, Duration::from_secs_f64, ...)',
  'H-04 no panicking index/slice on unproven bounds, including map[&k] and deque[i]',
  'H-05 no #[allow]; #[expect(lint, reason)] needs human approval; never weaken lint/CI/hook config',
  'H-06 no dbg!, no print macros in production code (binaries: writeln! on a locked handle), no commented-out code, no untracked TODOs',
  'H-07 no Box::leak/mem::forget/transmute/static mut/\'static-to-silence-lifetimes; shared mutable state is justified',
  'H-08 no clones to appease the borrow checker',
  'H-09 no String/Box<dyn Error>/anyhow errors in public library signatures',
  'H-10 no silently discarded failures (let _ =, .ok();, empty Err arms, masking defaults)',
  'H-11 no lossy `as` numeric casts',
  'H-12 never weaken tests or checks to get green: no deleted or #[ignore]d tests, --no-verify, --ignore-rust-version or manifest switches that stop tests (test, doctest, harness, autotests)',
  'H-13 bounded channels/buffers, timeouts, owned tasks and threads',
  'H-14 no blocking in async; no lock guard, RefCell borrow, entered span or watch::Ref across .await; Drop never blocks',
  'H-15 zero warnings; public items documented',
  'H-16 new deps, major bumps, [patch], rust-version raises, API breaks, exceptions, crate boundaries, migrations, test rewrites are human decisions',
];

function discoverFor(input, file) {
  if (file) {
    let dir = path.dirname(path.resolve(file));
    while (!fs.existsSync(dir)) { const p = path.dirname(dir); if (p === dir) break; dir = p; }
    const proj = discover(dir);
    if (proj) return proj;
  }
  return discover(input.cwd);
}

// ---------------------------------------------------------------- SessionStart

export function sessionStart(input) {
  const proj = discover(input.cwd);
  if (!proj) return none();
  const sid = input.session_id;
  const fresh = ['startup', 'clear', 'fork'].includes(input.source);
  setBase(proj, sid, fresh);
  if (input.source === 'startup') { pruneSessions(proj); pruneMarkers(); pruneFingerprintState(proj, startFingerprint(proj, sid)); }
  if (process.env.CLAUDE_ENV_FILE) {
    const baseLine = baseRev(proj, sid) ? `export TEMPER_BASE="${baseRev(proj, sid)}"\n` : '';
    try { fs.appendFileSync(process.env.CLAUDE_ENV_FILE, `export TEMPER_CLI="${norm(cli())}"\n${baseLine}`); } catch { /* ignore */ }
  }
  const base = baseRev(proj, sid);
  const changes = rustChanges(proj, base);
  let state = 'no Rust changes since the session base';
  if (changes.length) {
    const fp = fingerprint(proj, base);
    const g = cachedGate(proj, fp);
    const v = verdictFor(proj, fp);
    state = `${changes.length} Rust file(s) changed; gate ${g ? (g.ok ? 'green' : 'red') : 'not run'}; review ${v ? v.verdict : 'none'} (fingerprint ${fp})`;
  }
  telemetry(proj, input, 'session-start', { source: input.source });
  const loaded = skillLoaded(proj, input);
  const text = [
    'This repository is a Rust workspace governed by the temper plugin and the rust-craft engineering standard.',
    `- The standard is the skill \`temper:rust-craft\`. It ${loaded ? 'is loaded in this session' : 'has not been loaded in this session yet'}. Edits to .rs and Cargo.toml files are refused until it has been loaded in the current agent context.`,
    `- Iron rules (production code): ${IRON_RULES.join('; ')}.`,
    '- Workflow: orient, frame, plan (.temper/plans/), red, green, refactor, self-review, gate, independent review by the temper:rust-reviewer subagent, then report with the Status block.',
    `- Gate command: ${gateCmd()} (also available as: node "$TEMPER_CLI" gate). It runs cargo fmt, the escape-hatch scan, the member-manifest check, clippy -D warnings, tests and rustdoc -D warnings, cached per tree fingerprint. When .claude/temper-baseline.json exists, clippy and rustdoc findings ratchet against it: legacy debt passes; findings on changed lines and counts above the baseline fail.`,
    '- A turn that changed Rust ends when the gate is green and temper:rust-reviewer has approved the current tree. To pause instead, the final message carries a line "Status: WIP" or "Status: BLOCKED" with the reason or question.',
    '- Edits to lint, format, toolchain, CI, hook and agent configuration, new dependencies, lint exceptions and removed tests are routed to the human for approval.',
    `- Session base revision: ${base ?? 'none (no git)'}. Current state: ${state}.`,
  ].join('\n');
  return ctx('SessionStart', text);
}

// ------------------------------------------------------------ UserPromptSubmit

export function userPromptSubmit(input) {
  if (skillLoaded(null, input)) return none();
  const proj = discover(input.cwd);
  if (!proj || !proj.config.skillGate || skillLoaded(proj, input)) return none();
  return ctx('UserPromptSubmit', 'temper: this is a Rust workspace and the rust-craft standard (skill `temper:rust-craft`) is not loaded in this session yet; Rust edits are refused until it is. Planning Rust work benefits from loading it first.');
}

// ------------------------------------------------------------------ PreToolUse

export function applyEdit(before, ti, tool) {
  if (tool === 'Write') return ti.content ?? '';
  const edits = tool === 'MultiEdit' ? (ti.edits ?? []) : [ti];
  let text = before;
  for (const e of edits) {
    if (typeof e.old_string !== 'string' || !text.includes(e.old_string)) return null;
    text = e.replace_all ? text.split(e.old_string).join(e.new_string ?? '') : text.replace(e.old_string, () => e.new_string ?? '');
  }
  return text;
}

/** Text of Cargo.toml sections that control lints and profiles. */
export function lintSections(toml) {
  const out = [];
  let keep = false;
  for (const line of toml.split(/\r?\n/)) {
    const h = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (h) keep = /(^|\.)lints(\.|$)|^profile(\.|$)/.test(h[1].trim());
    if (keep) out.push(line.trim());
  }
  return out.filter(Boolean).join('\n');
}

/**
 * Do the lint or profile settings differ? Adding or keeping a bare
 * `[lints] workspace = true` (plain inheritance) is not a change of settings.
 */
export function lintConfigChanged(before, after) {
  const pb = before === '' ? {} : parseToml(before);
  const pa = parseToml(after);
  if (!pa || !pb) return lintSections(before) !== lintSections(after);
  const pick = (m) => JSON.stringify({
    lints: inheritsLints(m) && Object.keys(m.lints).length === 1 ? undefined : m.lints,
    workspaceLints: m.workspace?.lints,
    profile: m.profile,
  });
  return pick(pb) !== pick(pa);
}

const short = (v) => (v === undefined ? 'unset' : v.length > 80 ? `${v.slice(0, 77)}...` : v);

/**
 * Cargo.toml rules beyond lints and new dependencies. Denies a member that
 * newly lacks lint inheritance or newly has a profile (a ratchet: problems the
 * manifest already had only advise); asks on test and toolchain switches and
 * major-version bumps; advises on non-caret requirements.
 */
export function manifestChecks(file, rel, before, after, proj) {
  const out = { denies: [], asks: [], advice: [] };
  const a = parseToml(after);
  if (!a) return out; // invalid TOML: cargo rejects it on its own
  const b = before === '' ? {} : parseToml(before);
  const root = a.package ? workspaceRootFor(file, a) : null;
  const opts = { ownLints: proj ? ownLintsMember(proj.config, proj.root, file) : false };
  const { fresh, legacy } = splitProblems(b ? memberProblems(b, root, opts) : [], memberProblems(a, root, opts));
  for (const p of fresh) out.denies.push(`${p.id} ${rel}: ${p.msg}.\n    fix: ${p.fix}`);
  for (const p of legacy) out.advice.push(`${p.id} ${rel} (already the case before this edit): ${p.msg}.\n    fix: ${p.fix}`);
  if (before === '' && a.package && root && !root.self) {
    out.asks.push(`${rel}: new crate \`${a.package.name ?? rel}\` in the workspace. Crate boundaries are a human decision (H-16, ARC-07).`);
  }
  if (b) {
    const wa = watchedKeys(a);
    const wb = watchedKeys(b);
    for (const label of new Set([...wb.keys(), ...wa.keys()])) {
      const x = wa.get(label);
      const y = wb.get(label);
      if (x?.value === y?.value) continue;
      if (before === '' && x && benignWhenNew(label, x)) continue;
      out.asks.push(`${rel}: \`${label}\` changes (${short(y?.value)} -> ${short(x?.value)}); ${(x ?? y).why}.`);
    }
  }
  const ra = depRequirements(a);
  const rb = b ? depRequirements(b) : new Map();
  for (const [k, req] of ra) {
    const old = rb.get(k);
    if (old === req) continue;
    const name = k.slice(k.lastIndexOf(':') + 1);
    if (old !== undefined && majorOf(old) !== null && majorOf(req) !== null && majorOf(old) !== majorOf(req)) {
      out.asks.push(`${rel}: \`${name}\` moves to a new major version ("${old}" -> "${req}"). Major-version bumps are a human decision (H-16, ARC-12).`);
    }
    const why = nonCaretRequirement(req);
    if (why) out.advice.push(`ARC-12 ${rel}: \`${name} = "${req}"\` is ${why}.\n    fix: declare a caret floor at the oldest version whose API you use (\`"1.4"\`), never \`=\`, \`~\`, \`*\` or an upper bound.`);
  }
  return out;
}

/** Dependency names declared in Cargo.toml (all dependency tables), as "kind:name". */
export function depNames(toml) {
  const m = parseToml(toml);
  return m ? dependencyNames(m) : depNamesByLine(toml);
}

/** Line-based fallback for manifests that are not valid TOML. */
function depNamesByLine(toml) {
  const names = new Set();
  let table = null;
  for (const line of toml.split(/\r?\n/)) {
    const h = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (h) {
      const t = h[1].trim();
      const inline = t.match(/(?:^|\.)((?:dev-|build-)?dependencies)\.([\w-]+)$/);
      if (inline) { names.add(`${inline[1]}:${inline[2]}`); table = null; }
      else table = /(?:^|\.)((?:dev-|build-)?dependencies)$/.test(t) ? t.match(/((?:dev-|build-)?dependencies)$/)[1] : null;
      continue;
    }
    if (table) {
      const k = line.match(/^\s*([A-Za-z0-9_-]+)\s*=/);
      if (k) names.add(`${table}:${k[1]}`);
    }
  }
  return names;
}

export function preEdit(input) {
  const tool = input.tool_name;
  const ti = input.tool_input ?? {};
  const file = ti.file_path;
  if (!file) return none();
  const proj = discoverFor(input, file);
  if (!proj) return none();
  const rel = relTo(proj.root, file);
  if (rel.startsWith('..')) return none();
  const absState = path.resolve(proj.stateDir);
  if (path.resolve(file).startsWith(absState) || /(^|\/)\.git\//.test(rel)) {
    return deny('temper: files under .git (including temper gate state and review verdicts) are not edited directly. Verdicts come from the temper:rust-reviewer subagent; gate results come from the gate command.');
  }
  if (/(^|\/)Cargo\.lock$/.test(rel)) {
    telemetry(proj, input, 'deny-lockfile', { file: rel });
    return deny(LOCKFILE_DENY(rel));
  }
  const before = readText(file) ?? '';
  const after = applyEdit(before, ti, tool);
  if (after === null) return none(); // the edit will fail on its own
  const isRust = rel.endsWith('.rs');
  const isManifest = /(^|\/)Cargo\.toml$/.test(rel);

  const denies = [];
  const asks = [];
  const advice = [];

  // Skill gate
  if ((isRust || isManifest) && proj.config.skillGate && !PRELOADED.test(input.agent_type ?? '') && !skillLoaded(proj, input)) {
    telemetry(proj, input, 'deny-skill-gate', { file: rel });
    return deny([
      'temper: the rust-craft standard is not loaded in this agent context, so this Rust edit was not applied.',
      'Invoke the Skill tool with skill "temper:rust-craft" (or read the rust-craft SKILL.md), then retry the same edit.',
      `Non-negotiables this edit will be checked against: ${IRON_RULES.join('; ')}.`,
    ].join('\n'));
  }

  // Protected configuration
  if (proj.config.protected.some((p) => globMatch(p, rel))) {
    asks.push(`${rel} is verification/agent configuration. Changing it can weaken the checks (H-05, H-12). Approve only if you asked for this change.`);
  }
  if (isManifest) {
    const mc = manifestChecks(file, rel, before, after, proj);
    denies.push(...mc.denies);
    asks.push(...mc.asks);
    advice.push(...mc.advice);
    if (lintConfigChanged(before, after)) asks.push(`${rel}: the [lints]/[profile] sections change (H-05). Approve only if intended.`);
    if (proj.config.askOnNewDependency) {
      const was = depNames(before);
      const approved = new Set(proj.config.preapprovedDependencies ?? []);
      const added = [...depNames(after)].filter((d) => !was.has(d) && !approved.has(d.split(':')[1]));
      if (added.length && before !== '') asks.push(`${rel}: new dependenc${added.length > 1 ? 'ies' : 'y'} ${added.map((d) => d.split(':')[1] + (d.startsWith('dev-') ? ' (dev)' : '')).join(', ')} (H-16, ARC-12). Approve if the addition is justified.`);
    }
  }

  if (isRust) {
    const krate = crateOf(file);
    const crateRel = krate ? relTo(krate.dir, file) : rel;
    const kind = krate ? classifyPath(crateRel, krate.hasLib) : 'prod';
    const build = isBuildScript(crateRel);
    if (tool === 'Write' && before === '' && /(^|\/)mod\.rs$/.test(rel) && kind !== 'test') {
      denies.push(`ARC-06 ${rel}: new mod.rs file.\n    fix: use \`foo.rs\` plus \`foo/bar.rs\`.`);
    }
    const b = scanSource(before, { kind, build });
    const a = scanSource(after, { kind, build });
    const exempt = (h) => (proj.config.scanner.exempt ?? []).some((e) => e.rule === h.id && (e.paths ?? []).some((p) => globMatch(p, rel)));
    const fresh = newHits(b.hits, a.hits).filter((h) => !exempt(h));
    // One approval line per exception: the #[expect] attribute carries the reason.
    const attrLints = new Set(fresh.filter((h) => h.key === 'expect-attr' && h.severity === 'ask').map((h) => h.exception));
    for (const h of fresh) {
      if (h.severity === 'ask' && h.key !== 'expect-attr' && [...attrLints].some((l) => (h.exception ?? '').length && String(l).includes(h.exception))) continue;
      if (h.severity === 'hard') denies.push(formatHit(h, rel));
      else if (h.severity === 'ask') asks.push(`Lint exception requested at ${rel}:${h.line} (${h.exception ?? h.id}): \`${h.text}\`. Approve only if the stated reason holds (H-05).`);
      else advice.push(formatHit(h, rel));
    }
    if (a.tests < b.tests) asks.push(`${rel}: this edit removes ${b.tests - a.tests} test(s) (H-12). Approve only if the test is being moved or intentionally retired.`);
    const ignored = a.ignores - b.ignores - Math.max(0, a.tests - b.tests);
    if (ignored > 0) asks.push(`${rel}: this edit adds \`#[ignore]\` to ${ignored} existing test(s) (H-12). Approve only if you asked for the test to stop running.`);
    if (a.clones > b.clones) advice.push(`H-08 ${rel}: ${a.clones - b.clones} new .clone() call(s). Each needs an ownership reason (callee stores it, cheap handle, or genuine duplication).`);
  }

  if (denies.length) {
    telemetry(proj, input, isManifest ? 'deny-manifest' : 'deny-scan', { file: rel, rules: denies.map((d) => d.split(' ')[0]) });
    return deny([
      isManifest
        ? `temper: this manifest edit breaks ${denies.length} workspace rule(s), so it was not applied. Fix the manifest text:`
        : `temper: this edit introduces ${denies.length} escape hatch(es), so it was not applied. Rewrite it without them:`,
      ...denies.slice(0, 10),
      denies.length > 10 ? `... and ${denies.length - 10} more.` : '',
      isManifest ? '' : 'If one of these is a genuine exception, put `#[expect(<lint>, reason = "...")]` on the item; the human will be asked to approve it. See rust-craft references/escape-hatches.md.',
    ].filter(Boolean).join('\n'));
  }
  if (asks.length) {
    telemetry(proj, input, 'ask', { file: rel, count: asks.length });
    return ask(`temper needs your approval:\n- ${asks.join('\n- ')}`, advice.length ? `temper advisories for this edit:\n${advice.join('\n')}` : undefined);
  }
  if (advice.length) return ctx('PreToolUse', `temper advisories for this edit (not blocking, but a reviewer will ask):\n${advice.slice(0, 8).join('\n')}`);
  return none();
}

const LOCKFILE_DENY = (rel) => `temper: ${rel} is maintained by Cargo and is never edited by hand (PROC-13). Change it with cargo: \`cargo update -p <crate> --precise <version>\` to pin or roll back one dependency, \`cargo update --workspace\` after a manifest edit, \`cargo update --dry-run\` to preview. For a merge conflict, check out one side's Cargo.lock and re-run a cargo command.`;

const MANIFEST_SHELL_DENY = 'temper: edit Cargo.toml with the Edit or Write tools, not the shell, so the manifest checks run on it (MOD-09, PERF-09, H-12, H-16). Use the Edit tool for this change.';

/**
 * Shell commands that write a file whose name ends with `target` (a regex source).
 * `copies` also treats the file as the destination of cp/mv/install.
 */
const shellWrites = (target, { copies = false } = {}) => {
  const t = `${target}(?![\\w.])`;
  const res = [
    new RegExp(`(?:&>>?|(?:^|[^>&0-9])>>?\\|?)\\s*["']?[^\\s"'|;&<>]*${t}`),
    new RegExp(`\\btee\\s+(?:-a\\s+)?["']?[^\\s"'|;&]*${t}`),
    new RegExp(`\\bsed\\b[^|;&]*\\s-[a-zA-Z]*i[^|;&]*${t}`),
    new RegExp(`\\bsed\\b[^|;&]*--in-place[^|;&]*${t}`),
    new RegExp(`\\bperl\\b[^|;&]*\\s-[a-zA-Z]*i[^|;&]*${t}`),
    new RegExp(`\\bopen\\s*\\([^)]*${target}["']\\s*,\\s*["'][wa+]`),
    new RegExp(`\\b(?:write_text|writeFileSync|Set-Content|Out-File|Add-Content)\\b[^|;&]*${t}`),
  ];
  if (copies) res.push(new RegExp(`\\b(?:cp|mv|install|Copy-Item|Move-Item)\\b[^|;&]*\\s["']?[^\\s"'|;&<>]*${t}["']?\\s*(?:$|[|;&)])`, 'm'));
  return res;
};
const SHELL_WRITE_RS = shellWrites('\\.rs');
const SHELL_WRITE_LOCK = shellWrites('Cargo\\.lock', { copies: true });
const SHELL_WRITE_TOML = shellWrites('Cargo\\.toml', { copies: true });
const SHELL_WRITE_BASELINE = shellWrites('temper-baseline\\.json', { copies: true });
const SHELL_WRITE_VENDORED = shellWrites('\\.claude/temper/[\\w./-]*', { copies: true });
const VENDORED_SHELL_DENY = 'temper: .claude/temper/ is the vendored temper CLI that CI and the pre-commit hook run (the lint ratchet and the baseline-raise check live in it). It is written only by `temper setup`, never by hand (H-12). Leave it unchanged, or ask the human to re-vendor it.';

const BASELINE_SHELL_DENY = 'temper: .claude/temper-baseline.json records the accepted lint debt and is written only by `temper baseline` (H-12). After fixing findings, lower it with `node "$TEMPER_CLI" baseline --tighten`. Raising it (`baseline --update`) is a human decision: ask the human.';

/** Shell segments of a command with quotes removed (`"--update"` and `'x'` read as `--update` and `x`). */
const unquotedSegments = (cmd) => String(cmd).split(/&&|\|\||[;|\n]/)
  .map((seg) => seg.replace(/"((?:[^"\\]|\\.)*)"|'([^']*)'/g, (_m, a, b) => a ?? b ?? ''));

/**
 * `temper baseline --init` / `--update` through the plugin CLI, the vendored copy
 * (`node .claude/temper/temper.mjs`), `$TEMPER_CLI`, or any variable holding the
 * CLI path (`node $T baseline --update`). They record the current findings as
 * accepted debt, so they weaken verification (H-12). Quotes do not hide a flag.
 */
export function baselineRaise(cmd) {
  for (const seg of unquotedSegments(cmd)) {
    const call = /(?:^|[\s=])(?:\S*temper\.mjs|\$\{?\w+\}?|temper)\s+(?:--?\S+\s+)*baseline\b(.*)$/.exec(seg);
    if (!call) continue;
    const flag = /(?:^|\s)--?(init|update)(?![\w-])/.exec(call[1]);
    if (flag) return `--${flag[1]}`;
  }
  return null;
}

/**
 * git commands that put an older `.claude/` (the baseline, settings, the vendored
 * CLI) back into the working tree: checkout/restore/stash naming a `.claude`
 * path, and `git show ... >` redirected into `.claude/`. Restoring from HEAD or
 * the index (`git checkout -- f`, `git restore f`) undoes one's own edit and passes.
 */
export function claudeRestore(cmd) {
  for (const seg of unquotedSegments(cmd)) {
    const m = /\bgit\s+(?:(?:-C|-c)\s+\S+\s+)*(checkout|restore|stash)\b(.*)$/.exec(seg);
    if (m && /(?:^|[\s=:/])\.claude(?:\/|\s|$)|temper-baseline\.json/.test(m[2])) {
      const args = m[2].trim().split(/\s+/).filter(Boolean);
      let source = null;
      if (m[1] === 'checkout') {
        const dd = args.indexOf('--');
        const before = (dd === -1 ? args.slice(0, -1) : args.slice(0, dd)).filter((a) => !a.startsWith('-'));
        source = before[0] ?? null; // no tree-ish: the index
      } else if (m[1] === 'restore') {
        const s = args.findIndex((a) => a === '-s' || a === '--source' || a.startsWith('--source=') || /^-s./.test(a));
        if (s !== -1) source = args[s].includes('=') ? args[s].slice(args[s].indexOf('=') + 1) : /^-s./.test(args[s]) ? args[s].slice(2) : args[s + 1];
      } else source = 'stash';
      if (source !== null && source !== 'HEAD' && source !== '@') return seg.trim();
    }
    if (/\bgit\s+(?:-C\s+\S+\s+)*show\b[^>]*>\s*\S*(?:^|\/|\s)?\.claude\//.test(seg)) return seg.trim();
  }
  return null;
}

/** Command segments with quoted strings blanked, so flags inside messages do not count. */
const segments = (cmd) => cmd.replace(/'[^']*'|"(?:[^"\\]|\\.)*"/g, "''").split(/&&|\|\||[;|\n]/);
const isCargo = (seg) => /(?:^|\s)cargo(?:\s|$)/.test(seg);

// cargo add/remove options that take a separate value.
const CARGO_VALUE_FLAGS = new Set(['-p', '--package', '-F', '--features', '--rename', '--registry', '--path', '--git', '--branch', '--tag', '--rev', '--manifest-path', '--target', '--lockfile-path', '--config', '-Z', '--color']);

/** `cargo add`/`cargo remove` invocations and the crate names they touch. */
export function cargoDependencyCommands(cmd) {
  const out = [];
  for (const seg of segments(cmd)) {
    const m = /(?:^|\s)cargo\s+(?:\+\S+\s+)?(add|remove|rm)(?:\s+(.*))?$/.exec(seg.trim());
    if (!m) continue;
    const toks = (m[2] ?? '').trim().split(/\s+/).filter(Boolean);
    const crates = [];
    for (let i = 0; i < toks.length; i++) {
      if (toks[i].startsWith('-')) { if (CARGO_VALUE_FLAGS.has(toks[i])) i++; continue; }
      crates.push(toks[i].split('@')[0]);
    }
    out.push({ op: m[1] === 'rm' ? 'remove' : m[1], crates });
  }
  return out;
}

export function preBash(input) {
  const cmd = String(input.tool_input?.command ?? '');
  const proj = discover(input.cwd);
  if (!proj) return none();
  if (SHELL_WRITE_RS.some((re) => re.test(cmd))) {
    telemetry(proj, input, 'deny-shell-write', {});
    return deny('temper: write Rust sources with the Edit or Write tools, not the shell, so the standard\'s checks run on them (H-12). Use the Edit tool for this change.');
  }
  if (SHELL_WRITE_LOCK.some((re) => re.test(cmd))) {
    telemetry(proj, input, 'deny-lockfile', { via: 'shell' });
    return deny(LOCKFILE_DENY('Cargo.lock'));
  }
  if (SHELL_WRITE_TOML.some((re) => re.test(cmd))) {
    telemetry(proj, input, 'deny-shell-write', { file: 'Cargo.toml' });
    return deny(MANIFEST_SHELL_DENY);
  }
  if (SHELL_WRITE_BASELINE.some((re) => re.test(cmd))) {
    telemetry(proj, input, 'deny-shell-write', { file: 'temper-baseline.json' });
    return deny(BASELINE_SHELL_DENY);
  }
  if (/TEMPER_ALLOW_RAISE/.test(cmd)) {
    telemetry(proj, input, 'ask', { via: 'TEMPER_ALLOW_RAISE' });
    return ask(`temper: this command sets TEMPER_ALLOW_RAISE, which lets \`temper baseline --init\`/\`--update\` record every current finding as accepted legacy debt, so the gate and CI stop failing on it. Raising the baseline weakens verification (H-12): approve only if you asked for it.`);
  }
  if (SHELL_WRITE_VENDORED.some((re) => re.test(cmd))) {
    telemetry(proj, input, 'deny-shell-write', { file: '.claude/temper/' });
    return deny(VENDORED_SHELL_DENY);
  }
  const raise = baselineRaise(cmd);
  if (raise) {
    telemetry(proj, input, 'deny-baseline-raise', { via: `baseline ${raise}` });
    return deny(`temper: \`temper baseline ${raise}\` records every current finding as accepted legacy debt (H-12), so it is the human's decision, and the CLI refuses it inside Claude Code. To request it, run the same command prefixed with \`TEMPER_ALLOW_RAISE=1\`: the human will be asked to approve it. (\`baseline --tighten\` and \`--check\` never raise it and run freely.)`);
  }
  const restore = claudeRestore(cmd);
  if (restore) {
    telemetry(proj, input, 'ask', { via: 'git-restore-claude' });
    return ask(`temper: \`${restore.slice(0, 160)}\` puts older verification configuration back into .claude/ (the lint baseline, settings or the vendored CLI). An older baseline can accept more debt (H-12): approve only if you asked for it.`);
  }
  if (segments(cmd).some((seg) => isCargo(seg) && /(?:^|\s)--ignore-rust-version(?![\w-])/.test(seg))) {
    telemetry(proj, input, 'deny-ignore-rust-version', {});
    return deny('temper: --ignore-rust-version bypasses the MSRV check, so green would not mean the declared rust-version builds (MOD-01, H-12). If a dependency needs a newer Rust, raising rust-version is a human decision (H-16): say so and ask.');
  }
  if (/--no-verify\b|\bgit\s+commit\b[^|;&]*\s-n\b/.test(cmd)) {
    return deny('temper: skipping git hooks (--no-verify) is not allowed (H-12). Fix what the hook reports.');
  }
  if (/\bcargo\s+(?:\+\S+\s+)?clippy\b[^|;&]*(\s-A\s|\s--allow\b|--cap-lints)/.test(cmd) || /RUSTFLAGS=[^\s]*(-A|--cap-lints)/.test(cmd)) {
    return deny('temper: lowering lint levels on the command line is not allowed (H-05). Fix the code the lint points at.');
  }
  if (proj.config.askOnNewDependency) {
    const approved = new Set(proj.config.preapprovedDependencies ?? []);
    const changes = cargoDependencyCommands(cmd).filter((c) => !c.crates.length || c.crates.some((n) => !approved.has(n)));
    if (changes.length) {
      telemetry(proj, input, 'ask', { via: 'cargo-deps' });
      const what = changes.map((c) => `cargo ${c.op} ${c.crates.join(' ') || '(no crate named)'}`).join('; ');
      return ask(`temper: \`${what}\` changes the dependency graph without the manifest checks. Adding or removing a dependency is a human decision (H-16, ARC-12). Approve only if you asked for it.`);
    }
  }
  if (/\bcargo\s+insta\s+(?:accept|test\b[^|;&]*--accept)/.test(cmd)) {
    return ask('temper: accepting all snapshot changes at once can hide regressions (H-12). Approve only after reviewing the snapshot diffs (cargo insta review).');
  }
  return none();
}

export function preHandback(input) {
  if (!REVIEWER.test(input.agent_type ?? '')) return none();
  const proj = discover(input.cwd);
  if (!proj) return none();
  const v = extractVerdict(String(input.tool_input?.message ?? ''));
  const problems = checkVerdict(proj, input, v);
  if (problems.length) return deny(`temper: the review verdict is invalid, so the report was not delivered:\n- ${problems.join('\n- ')}\nFix the verdict block and hand back again.`);
  storeVerdict(proj, input, v);
  return none();
}

function checkVerdict(proj, input, v) {
  const base = baseRev(proj, input.session_id);
  const gate = v?.fingerprint ? cachedGate(proj, v.fingerprint) : null;
  return validateVerdict(v, { changedRust: rustChanges(proj, base).filter((f) => f.endsWith('.rs')), gate });
}

function storeVerdict(proj, input, v) {
  const base = baseRev(proj, input.session_id);
  const current = fingerprint(proj, base);
  recordVerdict(proj, v, { agent_id: input.agent_id, stale: v.fingerprint !== current });
  writeAtomic(path.join(sessionDir(proj, input.session_id), `verdict-${agentKey(input)}`), v.fingerprint);
  telemetry(proj, input, 'verdict', { verdict: v.verdict, fp: v.fingerprint, stale: v.fingerprint !== current, findings: (v.findings ?? []).length });
}

// ----------------------------------------------------------------- PostToolUse

export function postTool(input) {
  const ti = input.tool_input ?? {};
  let via = null;
  if (input.tool_name === 'Skill' && SKILL.test(String(ti.skill ?? ti.name ?? ''))) via = 'Skill';
  else if (input.tool_name === 'Read' && /\/rust-craft\/SKILL\.md$/.test(norm(String(ti.file_path ?? '')))) via = 'Read';
  if (via) {
    markSkillLoaded(null, input);
    const proj = discover(input.cwd);
    if (proj) telemetry(proj, input, 'skill-loaded', { via });
  }
  return none();
}

// ------------------------------------------------------------------ Stop gates

function parseStatus(msg) {
  const all = [...String(msg ?? '').matchAll(/^\s*[*_`>-]*\s*Status:\s*[*_`]*\s*(DONE|WIP|BLOCKED)\b/gim)];
  return all.length ? all.at(-1)[1].toUpperCase() : null;
}

function lastBlock(proj, input) {
  const p = path.join(sessionDir(proj, input.session_id), `stop-${agentKey(input)}.json`);
  const t = readText(p);
  let prev = null;
  try { prev = t ? JSON.parse(t) : null; } catch { /* ignore */ }
  return { prev, save: (rec) => writeAtomic(p, JSON.stringify(rec)) };
}

function completionGate(input, { requireReview }) {
  const proj = discover(input.cwd);
  if (!proj || input.permission_mode === 'plan') return none();
  const base = baseRev(proj, input.session_id);
  const changes = rustChanges(proj, base);
  if (!changes.length) return none();
  const startFp = startFingerprint(proj, input.session_id);
  if (startFp && startFp === fingerprint(proj, base)) return none(); // nothing changed by this session
  const status = parseStatus(input.last_assistant_message);
  if (status === 'WIP' || status === 'BLOCKED') {
    telemetry(proj, input, 'stop-declared', { status });
    return notice(`temper: Rust changes left ${status === 'WIP' ? 'in progress' : 'blocked'}; the gate and review were not enforced for this turn.`);
  }
  const res = runGate(proj, { base });
  if (res.unavailable) {
    telemetry(proj, input, 'gate-unavailable', {});
    return notice(`temper: ${renderGate(res)}`);
  }
  const fp = res.fp;
  const { prev, save } = lastBlock(proj, input);
  const stuck = (kind) => input.stop_hook_active && prev && prev.fp === fp && prev.kind === kind;
  const escape = 'If you cannot finish now, end your message with "Status: WIP" (pausing) or "Status: BLOCKED" and the question for the human.';

  if (!res.ok) {
    if (stuck('gate')) {
      telemetry(proj, input, 'stop-allowed-red', { fp });
      return notice(`temper: stopped with a RED gate (no progress after the previous block). ${res.steps.filter((s) => !s.ok).map((s) => s.name).join(', ')} failing.`);
    }
    save({ fp, kind: 'gate', at: Date.now() });
    telemetry(proj, input, 'stop-blocked', { reason: 'gate', fp });
    return block(`${renderGate(res)}\n\nThe work is not done while the gate is red. Fix the failures (never by weakening tests or lints, H-12). ${escape}`);
  }
  if (requireReview && proj.config.review !== 'off') {
    const v = verdictFor(proj, fp);
    if (!v || v.verdict !== 'approved') {
      if (proj.config.review === 'advisory') return notice(`temper: gate green; no approving review for fingerprint ${fp} (review is advisory in this repo).`);
      if (stuck('review')) {
        telemetry(proj, input, 'stop-allowed-unreviewed', { fp });
        return notice(`temper: stopped without an approving review for fingerprint ${fp}.`);
      }
      save({ fp, kind: 'review', at: Date.now() });
      telemetry(proj, input, 'stop-blocked', { reason: v ? 'changes-requested' : 'no-review', fp });
      const why = v
        ? `The reviewer requested changes for this tree (fingerprint ${fp}):\n${summarizeFindings(v)}\nAddress each finding, or rebut it with evidence, then request a fresh review.`
        : `The gate is green, but the current tree (fingerprint ${fp}) has no approving independent review (PROC-08). Delegate to the temper:rust-reviewer subagent with the goal of the change and the plan path. It reviews the diff since ${base ?? 'the session start'} in a fresh context. Address its findings until it approves.`;
      return block(`${why}\n${escape}`);
    }
  }
  telemetry(proj, input, 'stop-green', { fp });
  return notice(`temper: gate green${requireReview && proj.config.review === 'required' ? ' and review approved' : ''} (fingerprint ${fp}).`);
}

export function stop(input) {
  return completionGate(input, { requireReview: true });
}

export function subagentStop(input) {
  const type = input.agent_type ?? '';
  if (REVIEWER.test(type)) {
    const proj = discover(input.cwd);
    if (!proj) return none();
    const already = readText(path.join(sessionDir(proj, input.session_id), `verdict-${agentKey(input)}`));
    const v = extractVerdict(input.last_assistant_message);
    if (!v && already) return none(); // delivered through SubagentHandback
    const problems = checkVerdict(proj, input, v);
    if (problems.length) {
      if (input.stop_hook_active) { telemetry(proj, input, 'verdict-invalid-final', { problems: problems.length }); return none(); }
      return block(`temper: your review verdict is invalid:\n- ${problems.join('\n- ')}\nEnd your final message with a corrected \`\`\`json verdict block.`);
    }
    storeVerdict(proj, input, v);
    return none();
  }
  if (IMPLEMENTER.test(type)) return completionGate(input, { requireReview: false });
  return none();
}

// -------------------------------------------------------------- SubagentStart

export function subagentStart(input) {
  const type = input.agent_type ?? '';
  if (!PRELOADED.test(type)) return none();
  const proj = discover(input.cwd);
  if (!proj) return none();
  const base = baseRev(proj, input.session_id);
  if (REVIEWER.test(type)) run('cargo', ['fmt', '--all'], { cwd: proj.root, timeoutMs: 120000 });
  const changes = rustChanges(proj, base);
  const fp = fingerprint(proj, base);
  const g = cachedGate(proj, fp);
  const plans = listPlans(proj);
  const lines = [
    `temper context for ${type}:`,
    `- Project root: ${proj.root}`,
    `- Base revision: ${base ?? 'none (no git)'}; review scope: \`git diff ${base ?? ''}\` plus untracked files.`,
    `- Current tree fingerprint: ${fp}`,
    `- Changed Rust files (${changes.length}): ${changes.slice(0, 40).join(', ') || 'none'}`,
    `- Gate for this fingerprint: ${g ? (g.ok ? 'green' : `red (${g.steps.filter((s) => !s.ok).map((s) => s.name).join(', ')})`) : 'not run yet'}. Gate command: ${gateCmd()}`,
    plans.length ? `- Plans: ${plans.join(', ')}` : '- Plans: none under .temper/plans/',
  ];
  if (REVIEWER.test(type)) {
    lines.push(`- Your verdict's "fingerprint" field must be ${fp}. If the tree changes during your review the verdict is recorded as stale and a new review will be needed.`);
    const stat = diffStat(proj, base);
    if (stat) lines.push(`- Diff stat:\n${stat.split('\n').slice(-25).join('\n')}`);
  }
  return ctx('SubagentStart', lines.join('\n'));
}

function listPlans(proj) {
  const dir = path.join(proj.root, '.temper', 'plans');
  try {
    return fs.readdirSync(dir).filter((f) => f.endsWith('.md'))
      .map((f) => ({ f, t: fs.statSync(path.join(dir, f)).mtimeMs }))
      .sort((a, b) => b.t - a.t).slice(0, 5).map((x) => `.temper/plans/${x.f}`);
  } catch { return []; }
}
