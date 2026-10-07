// Hook handlers. Each returns { json?, text?, exit? }.
// Context strings are phrased as facts about the project, not as out-of-band
// commands, so Claude treats them as project information.
import fs from 'node:fs';
import path from 'node:path';
import {
  discover, relTo, norm, readText, writeAtomic, globMatch, crateOf, sessionDir, agentKey,
  markSkillLoaded, skillLoaded, setBase, baseRev, pruneSessions, pruneMarkers, telemetry, rustChanges,
  fingerprint, diffStat, run, startFingerprint,
} from './project.mjs';
import { scanSource, newHits, classifyPath, formatHit, isBuildScript } from './scan.mjs';
import os from 'node:os';
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
  if (input.source === 'startup') { pruneSessions(proj); pruneMarkers(); }
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

// ---- cp/mv/install/git mv/Copy-Item/Move-Item onto .rs, and git apply/patch (audit engine-1-8)

/**
 * The simple commands of a shell line, read the way the shell reads quotes: each is
 * `{ words, stdin, piped, writes }`. A word is `{ t, dyn }`, where `dyn` marks a value
 * temper cannot know (a variable, command or process substitution, a glob, `~`). `stdin`
 * is the word after `<` (`dyn` for a here-string; for a heredoc, `{ heredoc: true, text }`
 * where `text` is the body when it is literal: a quoted delimiter, or no `$` or backquote).
 * `piped` is true after `|`; `writes` are the `>` targets. A subshell's `(` and `)` appear
 * as `{ open: true }` and `{ close: true }` entries, so a `cd` inside one can be undone.
 */
function shellCommands(cmd) {
  const s = String(cmd);
  const out = [];
  let cur = { words: [], stdin: null, piped: false, writes: [] };
  let w = null;
  let redirect = null;
  let heredocs = [];
  const word = () => (w ??= { t: '', dyn: false });
  const endWord = () => {
    if (!w) return;
    if (redirect === '<') cur.stdin = w;
    else if (redirect === '<<' || redirect === '<<-') {
      cur.stdin = { t: '', dyn: true, heredoc: true, text: null };
      heredocs.push({ d: w.t, quoted: Boolean(w.q), tabs: redirect === '<<-', into: cur.stdin });
    } else if (redirect === '<<<') cur.stdin = { t: '', dyn: true };
    else if (redirect === '>') cur.writes.push(w);
    else cur.words.push(w);
    redirect = null;
    w = null;
  };
  const endCmd = (piped) => {
    endWord();
    if (cur.words.length) out.push(cur);
    cur = { words: [], stdin: null, piped, writes: [] };
  };
  // Index of the closing `close` that balances an `open` at i (or the end).
  const balanced = (i, open, close) => {
    let depth = 0;
    for (let j = i; j < s.length; j++) {
      if (s[j] === open) depth++;
      else if (s[j] === close && --depth === 0) return j;
    }
    return s.length - 1;
  };
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "'") {
      const j = s.indexOf("'", i + 1);
      const end = j === -1 ? s.length : j;
      word().t += s.slice(i + 1, end);
      w.q = true;
      i = end;
    } else if (c === '"') {
      const ww = word();
      ww.q = true;
      let j = i + 1;
      for (; j < s.length && s[j] !== '"'; j++) {
        if (s[j] === '\\' && '"\\$`\n'.includes(s[j + 1] ?? '')) { j++; if (s[j] !== '\n') ww.t += s[j]; continue; }
        if (s[j] === '$' || s[j] === '`') ww.dyn = true;
        ww.t += s[j];
      }
      i = j;
    } else if (c === '\\') {
      if (s[i + 1] !== '\n') { word().t += s[i + 1] ?? ''; w.q = true; }
      i++;
    } else if (c === '#' && !w) {
      const j = s.indexOf('\n', i);
      i = (j === -1 ? s.length : j) - 1;
    } else if (c === '\n') {
      endCmd(false);
      // Skip heredoc bodies up to each delimiter line, keeping a literal body as the command's stdin.
      for (const h of heredocs) {
        let pos = i + 1;
        const body = [];
        let closed = false;
        for (;;) {
          const nl = s.indexOf('\n', pos);
          const line = s.slice(pos, nl === -1 ? s.length : nl);
          pos = nl === -1 ? s.length : nl + 1;
          if (line.replace(/^\t+/, '').replace(/\r$/, '') === h.d) { closed = true; break; }
          if (nl === -1) break;
          body.push(h.tabs ? line.replace(/^\t+/, '') : line);
        }
        const text = body.length ? `${body.join('\n')}\n` : '';
        if (closed && (h.quoted || !/[$`]|\\[\\$`\n]/.test(text))) h.into.text = text;
        i = pos - 1;
      }
      heredocs = [];
    } else if (c === ';') {
      endCmd(false);
    } else if (c === '(' || c === ')') {
      endCmd(false);
      out.push(c === '(' ? { open: true } : { close: true });
    } else if (c === '&') {
      if (s[i + 1] === '>') { endWord(); redirect = '>'; i++; if (s[i + 1] === '>') i++; } else { if (s[i + 1] === '&') i++; endCmd(false); }
    } else if (c === '|') {
      if (s[i + 1] === '|') { i++; endCmd(false); } else { if (s[i + 1] === '&') i++; endCmd(true); }
    } else if ((c === '<' || c === '>') && s[i + 1] === '(') {
      const j = balanced(i + 1, '(', ')');
      const ww = word();
      ww.t += s.slice(i, j + 1);
      ww.dyn = true;
      i = j;
    } else if (c === '<' || c === '>') {
      if (w && !w.dyn && /^\d+$/.test(w.t)) w = null; else endWord();
      if (c === '>') {
        redirect = '>';
        while ('>|&'.includes(s[i + 1] ?? 'x')) i++;
      } else if (s.startsWith('<<<', i)) { redirect = '<<<'; i += 2; }
      else if (s.startsWith('<<', i)) { redirect = '<<'; i++; if (s[i + 1] === '-') { i++; redirect = '<<-'; } }
      else { redirect = '<'; if (s[i + 1] === '&') i++; }
    } else if (/\s/.test(c)) {
      endWord();
    } else if (c === '$' && (s[i + 1] === '(' || s[i + 1] === '{')) {
      const j = balanced(i + 1, s[i + 1], s[i + 1] === '(' ? ')' : '}');
      const ww = word();
      ww.t += s.slice(i, j + 1);
      ww.dyn = true;
      i = j;
    } else if (c === '`') {
      const j = s.indexOf('`', i + 1);
      const end = j === -1 ? s.length : j;
      const ww = word();
      ww.t += s.slice(i, end + 1);
      ww.dyn = true;
      i = end;
    } else {
      const ww = word();
      if ('$*?[{'.includes(c) || (c === '~' && ww.t === '')) ww.dyn = true;
      ww.t += c;
    }
  }
  endCmd(false);
  return out;
}

// Wrappers that run the command after them, with the options that take a value.
const COMMAND_WRAPPERS = new Map([
  ['sudo', ['-u', '-g', '-C', '-D', '-h', '-p', '-r', '-t', '-U']], ['doas', ['-u', '-C']], ['command', []], ['builtin', []],
  ['exec', ['-a']], ['nohup', []], ['time', []], ['env', ['-u', '-C', '-S']], ['nice', ['-n']], ['ionice', ['-c', '-n', '-p']],
  ['stdbuf', ['-i', '-o', '-e']], ['timeout', ['-s', '-k']], ['xargs', ['-a', '-d', '-E', '-I', '-L', '-n', '-P', '-s']],
]);
// Reserved words that a command can follow (`if cp ...`, `then cp ...`, `{ cp ...; }`, `! cp ...`).
const SHELL_KEYWORDS = new Set(['if', 'then', 'else', 'elif', 'do', 'while', 'until', '!', '{']);

/**
 * The words of a simple command without leading `VAR=value` assignments, reserved words and
 * wrappers (`sudo`, `env`, `timeout 5`, ...). After `xargs` without `-I`, the input xargs adds
 * is appended as an unknown word.
 */
function commandWords(words) {
  let i = 0;
  let xargs = false;
  while (i < words.length) {
    const t = words[i].t;
    if (SHELL_KEYWORDS.has(t)) { i++; continue; }
    if (!words[i].dyn && /^[A-Za-z_]\w*=/.test(t)) { i++; continue; }
    const valued = COMMAND_WRAPPERS.get(t);
    if (!valued || words[i].dyn) break;
    i++;
    if (t === 'xargs') xargs = true;
    for (; i < words.length && words[i].t.startsWith('-') && words[i].t !== '-'; i++) {
      if (t === 'xargs' && words[i].t.startsWith('-I')) xargs = false;
      if (valued.includes(words[i].t)) i++;
    }
    if (t === 'timeout' && i < words.length) i++; // the duration
  }
  const rest = words.slice(i);
  return xargs && rest.length ? [...rest, { t: '(xargs input)', dyn: true }] : rest;
}

/** A git-bash path (`/c/Users/...`, `/tmp/...`) as the Windows path it names; other paths unchanged. */
export function msysPath(t, platform = process.platform) {
  if (platform !== 'win32') return t;
  const drive = /^\/([A-Za-z])(\/.*)?$/.exec(t);
  if (drive) return `${drive[1].toUpperCase()}:${drive[2] ?? '/'}`;
  const tmp = /^\/tmp(\/.*)?$/.exec(t);
  if (tmp) return `${os.tmpdir().replace(/\\/g, '/')}${tmp[1] ?? ''}`;
  return t;
}

/** The absolute path a word names, or null when it is not a literal path or the directory is unknown. */
const shellPath = (cwd, w) => {
  if (!w || w.dyn || w.t === '') return null;
  const t = msysPath(w.t);
  if (path.isAbsolute(t)) return path.resolve(t);
  return cwd ? path.resolve(cwd, t) : null;
};
const statOf = (p) => { try { return fs.statSync(p); } catch { return null; } };
const isDirPath = (p) => statOf(p)?.isDirectory() ?? false;
const SPECIAL_FILE = /^\/(?:dev|proc)\//;
const MAX_SOURCE_BYTES = 4 * 1024 * 1024;

/** cp, mv and install arguments: sources, destination, and whether the destination is a directory. */
function posixCopyArgs(name, args) {
  const valueLetters = name === 'install' ? 'tSmog' : 'tS';
  let target = null;
  let noTarget = false;
  let recursive = name !== 'cp';
  const pos = [];
  let opts = true;
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (opts && !a.dyn && a.t === '--') { opts = false; continue; }
    if (opts && !a.dyn && a.t.startsWith('--')) {
      const eq = a.t.indexOf('=');
      const k = eq === -1 ? a.t : a.t.slice(0, eq);
      const v = eq === -1 ? undefined : { t: a.t.slice(eq + 1), dyn: false };
      if (k === '--target-directory') { target = v ?? args[++i] ?? null; if (!target) return null; }
      else if (k === '--no-target-directory') noTarget = true;
      else if (k === '--recursive' || k === '--archive') recursive = true;
      else if (k === '--directory' || k === '--dry-run') return null; // install -d makes directories
      else if (v === undefined && ['--suffix', '--mode', '--owner', '--group', '--strip-program'].includes(k)) i++;
      continue;
    }
    if (opts && !a.dyn && /^-[^-]/.test(a.t)) {
      const cluster = a.t.slice(1);
      for (let j = 0; j < cluster.length; j++) {
        const ch = cluster[j];
        if (valueLetters.includes(ch)) {
          const v = cluster.slice(j + 1) ? { t: cluster.slice(j + 1), dyn: false } : args[++i];
          if (ch === 't') { target = v ?? null; if (!target) return null; }
          break;
        }
        if (ch === 'T') noTarget = true;
        else if ('rRa'.includes(ch)) recursive = true;
        else if (ch === 'd' && name === 'install') return null;
        else if (ch === 'n' && name === 'git') return null; // git mv -n is a dry run
      }
      continue;
    }
    pos.push(a);
  }
  if (target) return { sources: pos, dest: target, destDir: true, recursive };
  if (pos.length < 2) return null;
  return { sources: pos.slice(0, -1), dest: pos.at(-1), destDir: noTarget ? false : pos.length > 2 ? true : undefined, recursive };
}

/** Copy-Item / Move-Item arguments (named or positional -Path and -Destination). */
function psCopyArgs(move, args) {
  const paths = [];
  const pos = [];
  let dest = null;
  let recursive = move;
  let filtered = false;
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (!a.dyn && /^-[A-Za-z]/.test(a.t)) {
      const colon = a.t.indexOf(':');
      const k = (colon === -1 ? a.t : a.t.slice(0, colon)).toLowerCase();
      const val = () => (colon === -1 ? args[++i] : { t: a.t.slice(colon + 1), dyn: false });
      if (['-path', '-literalpath', '-lp', '-pspath'].includes(k)) { const v = val(); if (v) paths.push(v); }
      else if (k.length > 2 && '-destination'.startsWith(k)) dest = val() ?? null;
      else if (['-filter', '-include', '-exclude'].includes(k)) { val(); filtered = true; }
      else if (['-credential', '-tosession', '-fromsession'].includes(k)) val();
      else if (k === '-recurse') recursive = true;
      else if (k === '-whatif') return null;
      continue;
    }
    pos.push(a);
  }
  if (!paths.length && pos.length) paths.push(pos.shift());
  if (!dest && pos.length) dest = pos.shift();
  if (!paths.length || !dest) return null;
  const sources = paths.flatMap((p) => (p.dyn ? [p] : p.t.split(',').map((t) => ({ t, dyn: filtered }))));
  return { sources, dest, destDir: sources.length > 1 ? true : undefined, recursive };
}

/** `git [-C dir] [-c k=v] <sub> args`: the subcommand, its arguments and the directory it runs in. */
function gitSubcommand(args, cwd) {
  let dir = cwd;
  for (let i = 0; i < args.length; i++) {
    const t = args[i].t;
    if (t === '-C') { dir = shellPath(dir, args[++i]); continue; }
    if (t === '-c') { i++; continue; }
    if (t.startsWith('-')) continue;
    return { sub: args[i].dyn ? '' : t, args: args.slice(i + 1), cwd: dir };
  }
  return { sub: '', args: [], cwd: dir };
}

/**
 * Files a unified, context or git diff names, with prefixes left on. A `---`/`+++`/`***`
 * header may end in a tab or space and a timestamp (`diff -u`), and lines may end in CR.
 */
const patchFiles = (text) => [...String(text).matchAll(/^(?:(?:\+\+\+|---|\*\*\*)[ \t]+("[^"]*"|[^\t\r\n]*?)(?:\t[^\r\n]*)?|diff --git ("[^"]*"|\S+) ("[^"]*"|\S+)|(?:rename|copy) (?:from|to) (.+?)|Index: (.+?))\r?$/gm)]
  .flatMap((m) => m.slice(1).filter(Boolean))
  .map((f) => f.replace(/^"|"$/g, '').replace(/\s+\d{4}-\d\d-\d\d[ \t].*$/, '').trim());

/**
 * git apply / patch: { read: false } when the patch cannot be read (`later` when the patch
 * file is written earlier in the same command), else the .rs files it changes.
 */
function patchCommand(name, c, args, cwd, written = new Set()) {
  let readOnly = false;
  let reportOnly = false;
  let applyToo = false;
  let base = cwd;
  const inputs = [];
  const touched = [];
  const pos = [];
  const PATCH_VALUE = new Set(['-p', '-i', '-o', '-d', '-D', '-F', '-B', '-Y', '-z', '-V', '-r', '-g', '--strip', '--input', '--output', '--directory', '--ifdef', '--fuzz', '--prefix', '--basename-prefix', '--suffix', '--version-control', '--reject-file', '--get', '--quoting-style']);
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a.dyn || !a.t.startsWith('-') || a.t === '-') { pos.push(a); continue; }
    const eq = a.t.indexOf('=');
    const k = eq === -1 ? a.t : a.t.slice(0, eq);
    if (name === 'git') {
      // --check never applies; --stat/--numstat/--summary only report unless --apply is given too.
      if (k === '--check') readOnly = true;
      else if (['--stat', '--numstat', '--summary'].includes(k)) reportOnly = true;
      else if (k === '--apply') applyToo = true;
      continue;
    }
    if (k === '--dry-run') { readOnly = true; continue; }
    let v = eq === -1 ? undefined : { t: a.t.slice(eq + 1), dyn: false };
    const short = /^-[pioDdFBYzVrg]/.test(k) && !k.startsWith('--');
    if (short && k.length > 2) v = { t: k.slice(2), dyn: false };
    const key = short ? k.slice(0, 2) : k;
    if (!PATCH_VALUE.has(key)) continue;
    if (v === undefined) v = args[++i];
    if (key === '-i' || key === '--input') inputs.push(v);
    else if (key === '-o' || key === '--output') { if (v && /\.rs$/.test(v.t)) touched.push(v.t); }
    else if (key === '-d' || key === '--directory') base = shellPath(cwd, v);
  }
  if ((readOnly || reportOnly) && !applyToo) return null;
  if (name === 'git') inputs.push(...pos.filter((p) => p.t !== '-'));
  else {
    if (pos[0] && /\.rs$/.test(pos[0].t)) touched.push(pos[0].t);
    if (pos[1]) inputs.push(pos[1]);
  }
  if (!inputs.length) {
    if (c.stdin?.heredoc && c.stdin.text !== null && !c.piped) inputs.push({ body: c.stdin.text });
    else if (c.stdin && !c.stdin.dyn && !c.piped) inputs.push({ ...c.stdin, fromCwd: true });
    else return { read: false };
  }
  for (const p of inputs) {
    const abs = p.body === undefined ? shellPath(p.fromCwd ? cwd : base, p) : null;
    if (abs && written.has(abs)) return { read: false, later: true };
    const st = abs && !SPECIAL_FILE.test(norm(abs)) ? statOf(abs) : null;
    const text = p.body ?? (st?.isFile() && st.size <= MAX_SOURCE_BYTES * 8 ? readText(abs) : null);
    if (text === null) return { read: false };
    touched.push(...patchFiles(text).filter((f) => /\.rs$/.test(f)));
  }
  return { read: true, touched: [...new Set(touched.map((f) => f.replace(/^[ab]\//, '')))] };
}

/** Every .rs file under dir, or null past `limit` entries. */
function rustFilesUnder(dir, limit = 10000) {
  const out = [];
  const stack = [dir];
  let seen = 0;
  while (stack.length) {
    const d = stack.pop();
    let entries;
    try { entries = fs.readdirSync(d, { withFileTypes: true }); } catch { continue; }
    for (const e of entries) {
      if (++seen > limit) return null;
      const p = path.join(d, e.name);
      if (e.isDirectory()) { if (e.name !== '.git') stack.push(p); } else if (e.isFile() && e.name.endsWith('.rs')) out.push(p);
    }
  }
  return out;
}

const RS_COPY_SCANNED = Symbol('temper-rs-copies-scanned');

/**
 * cp/mv/install/git mv/Copy-Item/Move-Item onto .rs files, and git apply/patch of Rust
 * sources (audit engine-1-8). A copy whose source temper can read is scanned the way
 * preEdit scans a Write of that content to the destination; a move of an in-repository
 * .rs file compares against the source's own content, as the gate does for a rename, so
 * moving legacy debt does not count as adding it. A copy duplicates the content, so it
 * is compared with the destination only, as a Write and the gate's view of a new file
 * are. A write temper cannot read (variable, glob, stdin, missing source) is refused,
 * and so is a patch that touches .rs files: the Edit tool applies it with the checks.
 * Returns a hook result, or null when the command writes no Rust source.
 */
function shellRustWrites(cmd, proj, input) {
  const denies = [];
  const asks = [];
  const advice = [];
  const unknown = []; // { t, why: 'dyn' | 'cwd' | 'patch' | 'later' }
  const patched = [];
  const inProject = (abs) => {
    const r = relTo(proj.root, abs);
    return !(r.startsWith('..') || path.isAbsolute(r) || /^[A-Za-z]:/.test(r) || /(^|\/)\.git(\/|$)/.test(r));
  };
  // The gate's scratch directories (project.mjs EXCLUDE): nothing there is gated or built.
  const scratch = (abs) => /^(?:\.temper|target)(?:\/|$)/.test(relTo(proj.root, abs));
  const exempt = (h, rel) => (proj.config.scanner.exempt ?? []).some((e) => e.rule === h.id && (e.paths ?? []).some((p) => globMatch(p, rel)));
  const kindOf = (abs) => {
    const krate = crateOf(abs);
    const crateRel = krate ? relTo(krate.dir, abs) : relTo(proj.root, abs);
    return { kind: krate ? classifyPath(crateRel, krate.hasLib) : 'prod', build: isBuildScript(crateRel) };
  };
  // A directory a copy would put Rust sources in: one that holds .rs files, or a crate's src/ (or under it), tests/, benches/ or examples/.
  const rustDir = (abs) => {
    if (!inProject(abs) || scratch(abs)) return false;
    try { if (fs.readdirSync(abs).some((n) => n.endsWith('.rs'))) return true; } catch { /* a new directory */ }
    const krate = crateOf(path.join(abs, 'x.rs'));
    return Boolean(krate) && /^(?:src(?:\/.*)?|tests|benches|examples)$/.test(relTo(krate.dir, abs));
  };

  const checkFile = (target, from, move) => {
    if (path.resolve(target) === path.resolve(from)) return;
    if (scratch(target)) return;
    const st = SPECIAL_FILE.test(norm(from)) ? null : statOf(from);
    const after = st?.isFile() && st.size <= MAX_SOURCE_BYTES ? readText(from) : null;
    if (after === null) { unknown.push({ t: norm(from), why: 'dyn' }); return; }
    const rel = relTo(proj.root, target);
    const exists = statOf(target)?.isFile() ?? false;
    const before = exists ? readText(target) ?? '' : '';
    const { kind, build } = kindOf(target);
    // A moved repository source brings its own findings along: count them as already there,
    // unless the move changes how the scanner reads it (test or example code into src/).
    const fromKind = move && from.endsWith('.rs') && inProject(from) ? kindOf(from) : null;
    const lenient = Boolean(fromKind) && fromKind.kind === kind && fromKind.build === build;
    if (proj.config.protected.some((p) => globMatch(p, rel))) asks.push(`${rel} is verification/agent configuration. Changing it can weaken the checks (H-05, H-12). Approve only if you asked for this change.`);
    if (!exists && /(^|\/)mod\.rs$/.test(rel) && kind !== 'test' && !(lenient && path.basename(from) === 'mod.rs')) {
      denies.push(`ARC-06 ${rel}: new mod.rs file.\n    fix: use \`foo.rs\` plus \`foo/bar.rs\`.`);
    }
    if (lenient && !exists) return; // nothing new can appear
    const b = scanSource(before, { kind, build });
    const a = scanSource(after, { kind, build });
    const was = lenient
      ? { hits: [...b.hits, ...a.hits], tests: b.tests + a.tests, ignores: b.ignores + a.ignores, clones: b.clones + a.clones }
      : b;
    const fresh = newHits(was.hits, a.hits).filter((h) => !exempt(h, rel));
    const attrLints = new Set(fresh.filter((h) => h.key === 'expect-attr' && h.severity === 'ask').map((h) => h.exception));
    for (const h of fresh) {
      if (h.severity === 'ask' && h.key !== 'expect-attr' && [...attrLints].some((l) => (h.exception ?? '').length && String(l).includes(h.exception))) continue;
      if (h.severity === 'hard') denies.push(formatHit(h, rel));
      else if (h.severity === 'ask') asks.push(`Lint exception requested at ${rel}:${h.line} (${h.exception ?? h.id}): \`${h.text}\`. Approve only if the stated reason holds (H-05).`);
      else advice.push(formatHit(h, rel));
    }
    if (a.tests < was.tests) asks.push(`${rel}: this command removes ${was.tests - a.tests} test(s) (H-12). Approve only if the test is being moved or intentionally retired.`);
    const ignored = a.ignores - was.ignores - Math.max(0, a.tests - was.tests);
    if (ignored > 0) asks.push(`${rel}: this command adds \`#[ignore]\` to ${ignored} existing test(s) (H-12). Approve only if you asked for the test to stop running.`);
    if (a.clones > was.clones) advice.push(`H-08 ${rel}: ${a.clones - was.clones} new .clone() call(s). Each needs an ownership reason (callee stores it, cheap handle, or genuine duplication).`);
  };

  // Paths an earlier cp/mv in this command creates: { real, copy }, where `real` is what they
  // hold and `copy` says the content was duplicated (so moving it on is still a copy).
  const earlierCopies = new Map();
  const earlier = (abs) => {
    for (let p = abs, rest = ''; ; rest = path.join(path.basename(p), rest), p = path.dirname(p)) {
      const hit = earlierCopies.get(p);
      if (hit) return { real: rest ? path.join(hit.real, rest) : hit.real, copy: hit.copy };
      if (path.dirname(p) === p) return null;
    }
  };
  const copy = (op, here, move) => {
    const destAbs = shellPath(here, op.dest);
    for (const s of op.sources) {
      const srcAbs = shellPath(here, s);
      if (srcAbs === null || destAbs === null) {
        const why = here === null && !s.dyn && !op.dest.dyn ? 'cwd' : 'dyn';
        const named = /\.rs$/.test(s.t) || /\.rs$/.test(op.dest.t);
        // A source temper cannot name, copied into a Rust source directory, may be Rust (`cp $DIR/* src/`).
        const intoRust = destAbs !== null && srcAbs === null && !/\.(?!rs$)\w+$/.test(s.t)
          && (op.destDir || isDirPath(destAbs) || /[\\/]$/.test(op.dest.t) || (op.recursive && !/\.\w+$/.test(path.basename(destAbs))))
          && rustDir(destAbs);
        if ((named && (destAbs === null || (inProject(destAbs) && !scratch(destAbs)))) || intoRust) unknown.push({ t: s.dyn || srcAbs === null ? s.t : op.dest.t, why });
        continue;
      }
      // A path an earlier command in this line creates reads as what it was made from.
      const prior = statOf(srcAbs) ? null : earlier(srcAbs);
      const real = prior?.real ?? srcAbs;
      const copied = !move || Boolean(prior?.copy);
      const st = SPECIAL_FILE.test(norm(real)) ? null : statOf(real);
      if (st?.isDirectory()) {
        if (!op.recursive) continue;
        const root = isDirPath(destAbs) ? path.join(destAbs, path.basename(srcAbs)) : destAbs;
        earlierCopies.set(root, { real, copy: copied });
        if (!inProject(root) || scratch(root)) continue;
        const files = rustFilesUnder(real);
        if (files === null) { unknown.push({ t: s.t, why: 'dyn' }); continue; }
        for (const f of files) {
          const target = path.join(root, path.relative(real, f));
          if (inProject(target)) checkFile(target, f, !copied);
        }
        continue;
      }
      const intoDir = op.destDir ?? (isDirPath(destAbs) || /[\\/]$/.test(op.dest.t));
      const target = intoDir ? path.join(destAbs, path.basename(srcAbs)) : destAbs;
      if (st?.isFile()) earlierCopies.set(target, { real, copy: copied });
      if (!target.endsWith('.rs') || !inProject(target) || scratch(target)) continue;
      if (!st?.isFile()) { unknown.push({ t: s.t, why: 'dyn' }); continue; }
      checkFile(target, real, !copied);
    }
  };

  // The directory each command runs in. null when temper cannot follow a `cd`; a subshell's
  // `(`...`)`, `bash -c` and pushd/popd restore it the way the shell does.
  let cwd = path.resolve(input.cwd ?? proj.root);
  let prev = null;
  const subshells = [];
  const dirStack = [];
  const written = new Set();
  const top = proj.gitRoot ?? proj.root;
  const cdTarget = (args) => {
    const a = args.filter((x) => x.dyn || !/^(?:--|-[LPe@]+)$/.test(x.t));
    if (!a.length) return os.homedir();
    if (a.length > 1) return null;
    const t = a[0].t;
    if (t === '-') return prev;
    if (t === '~' || t.startsWith('~/')) return path.join(os.homedir(), t.slice(2));
    if (/^(?:\$\(|`)git rev-parse --show-toplevel(?:\)|`)$/.test(t)) return top;
    return shellPath(cwd, a[0]);
  };
  const visit = (commands) => {
    for (const c of commands) {
      if (c.open) { subshells.push({ cwd, prev, dirs: dirStack.length }); continue; }
      if (c.close) { const o = subshells.pop(); if (o) { ({ cwd, prev } = o); dirStack.length = o.dirs; } continue; }
      for (const t of c.writes) { const abs = shellPath(cwd, t); if (abs) written.add(abs); }
      const words = commandWords(c.words);
      if (!words.length || words[0].dyn) continue;
      const name = path.basename(words[0].t.replace(/\\/g, '/')).toLowerCase();
      const args = words.slice(1);
      if (name === 'cd' || name === 'pushd' || name === 'popd') {
        const before = cwd;
        if (name === 'popd') cwd = dirStack.length ? dirStack.pop() : null;
        else {
          if (name === 'pushd') dirStack.push(cwd);
          cwd = cdTarget(args);
        }
        prev = before;
        continue;
      }
      if (['bash', 'sh', 'zsh', 'dash', 'ksh'].includes(name)) {
        const flag = args.findIndex((a) => !a.dyn && /^-[A-Za-z]*c[A-Za-z]*$/.test(a.t));
        if (flag !== -1 && args[flag + 1]) {
          const depth = subshells.push({ cwd, prev, dirs: dirStack.length });
          visit(shellCommands(args[flag + 1].t));
          const o = subshells[depth - 1];
          subshells.length = depth - 1;
          ({ cwd, prev } = o);
          dirStack.length = Math.min(dirStack.length, o.dirs);
        }
        continue;
      }
      if (name === 'eval') { visit(shellCommands(args.map((a) => a.t).join(' '))); continue; }
      if (name === 'find') {
        // find ... -exec cmd {} \; runs cmd once per match; `{}` reads as an unknown word.
        for (let i = 0; i < args.length; i++) {
          if (!['-exec', '-execdir', '-ok', '-okdir'].includes(args[i].t)) continue;
          const j = args.findIndex((a, k) => k > i && (a.t === ';' || a.t === '+'));
          const sub = args.slice(i + 1, j === -1 ? args.length : j);
          if (sub.length) visit([{ words: sub, stdin: null, piped: false, writes: [] }]);
          i = j === -1 ? args.length : j;
        }
        continue;
      }
      let op = null;
      let here = cwd;
      let move = false;
      let p = null;
      if (name === 'cp' || name === 'install' || name === 'mv') { op = posixCopyArgs(name, args); move = name === 'mv'; }
      else if (name === 'copy-item' || name === 'move-item') { move = name === 'move-item'; op = psCopyArgs(move, args); }
      else if (name === 'git') {
        const g = gitSubcommand(args, cwd);
        here = g.cwd;
        if (g.sub === 'mv') { op = posixCopyArgs('git', g.args); move = true; }
        else if (g.sub === 'apply') p = patchCommand('git', c, g.args, here, written);
      } else if (name === 'patch') p = patchCommand('patch', c, args, here, written);
      if (p && !p.read) unknown.push({ t: words.map((x) => x.t).join(' '), why: p.later ? 'later' : 'patch' });
      else if (p?.touched.length) patched.push(...p.touched);
      if (op) copy(op, here, move);
    }
  };
  visit(shellCommands(cmd));

  const unreadPatch = unknown.find((u) => u.why === 'patch' || u.why === 'later');
  if (unreadPatch) {
    telemetry(proj, input, 'deny-shell-write', { via: 'patch-unread' });
    const why = unreadPatch.why === 'later'
      ? 'its patch file does not exist yet, or is rewritten, earlier in this command'
      : 'stdin, a variable or a missing file';
    return deny(`temper: temper cannot read the patch that \`${unreadPatch.t.slice(0, 120)}\` applies (${why}), so it cannot tell whether it changes Rust sources, and the command was not run (H-12). Save the patch to a file in one command and check it with \`git apply --stat <file>\`; make changes to .rs files with the Edit tool so the standard's checks run on them.`);
  }
  if (unknown.length) {
    telemetry(proj, input, 'deny-shell-write', { via: 'copy-unread' });
    const u = unknown[0];
    const why = u.why === 'cwd'
      ? `temper cannot tell which directory \`${u.t.slice(0, 120)}\` is relative to (an earlier \`cd\` it cannot follow)`
      : `temper cannot read what this command would write to a .rs file (\`${u.t.slice(0, 120)}\` is a variable, glob, stdin or a missing path)`;
    return deny(`temper: write Rust sources with the Edit or Write tools, not the shell, so the standard's checks run on them (H-12). ${why}, so it was not run. Copy from a literal path that exists, or use the Edit tool.`);
  }
  if (patched.length) {
    telemetry(proj, input, 'deny-shell-write', { via: 'patch', files: patched.length });
    const list = patched.slice(0, 5).join(', ') + (patched.length > 5 ? `, and ${patched.length - 5} more` : '');
    return deny(`temper: this patch changes Rust sources (${list}), and applying it from the shell skips the standard's checks (H-12), so it was not run. Make the change with the Edit tool, so the scanner checks each hunk. \`git apply --check\`, \`git apply --stat\` and \`patch --dry-run\` still run for inspecting it.`);
  }
  if (denies.length) {
    telemetry(proj, input, 'deny-scan', { via: 'shell-copy', rules: denies.map((d) => d.split(' ')[0]) });
    return deny([
      `temper: this command writes ${denies.length} escape hatch(es) into Rust sources, so it was not run. Rewrite the source without them, or make the change with the Edit or Write tools:`,
      ...denies.slice(0, 10),
      denies.length > 10 ? `... and ${denies.length - 10} more.` : '',
      'If one of these is a genuine exception, put `#[expect(<lint>, reason = "...")]` on the item; the human will be asked to approve it. See rust-craft references/escape-hatches.md.',
    ].filter(Boolean).join('\n'));
  }
  if (asks.length) {
    telemetry(proj, input, 'ask', { via: 'shell-copy', count: asks.length });
    return ask(`temper needs your approval:\n- ${asks.join('\n- ')}`, advice.length ? `temper advisories for this command:\n${advice.join('\n')}` : undefined);
  }
  if (advice.length) return ctx('PreToolUse', `temper advisories for this command (not blocking, but a reviewer will ask):\n${advice.slice(0, 8).join('\n')}`);
  return null;
}
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
  // cp/mv/install onto .rs and git apply/patch: scanned like an Edit/Write (engine-1-8).
  // Only a deny returns at once; an ask or advisory waits for the checks below, and a deny from them wins.
  const rsWrite = input[RS_COPY_SCANNED] ? null : shellRustWrites(cmd, proj, input);
  if (rsWrite) {
    const own = rsWrite.json.hookSpecificOutput.permissionDecision;
    if (own === 'deny') return rsWrite;
    const rest = preBash({ ...input, [RS_COPY_SCANNED]: true });
    const other = rest.json?.hookSpecificOutput?.permissionDecision;
    if (other === 'deny' || (other === 'ask' && own !== 'ask')) return rest;
    if (other === 'ask') return ask(`${rsWrite.json.hookSpecificOutput.permissionDecisionReason}\n${rest.json.hookSpecificOutput.permissionDecisionReason}`, rsWrite.json.hookSpecificOutput.additionalContext);
    return rsWrite;
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
