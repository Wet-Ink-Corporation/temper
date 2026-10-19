// Review verdicts: parsing, validation and storage keyed by tree fingerprint.
import path from 'node:path';
import { readText, writeAtomic } from './project.mjs';

export const DIMENSIONS = [
  'correctness', 'escape_hatches', 'ownership', 'errors', 'api', 'architecture',
  'tests', 'performance', 'concurrency', 'observability', 'craft', 'docs',
];
const SEVERITIES = ['blocker', 'major', 'minor', 'nit'];

/**
 * The dimensions every verdict must rate 0-4 rather than "n/a": they apply to
 * any Rust change, however small.
 */
export const ALWAYS_SCORED = ['correctness', 'escape_hatches', 'craft'];

/** A path with "/" separators and no leading "./". */
const normPath = (f) => String(f).trim().replace(/\\/g, '/').replace(/^(?:\.\/)+/, '');

/** An absolute path once normalised: "/x", "//?/C:/x", "//server/x" or "C:/x". */
const isAbs = (f) => f.startsWith('/') || /^[A-Za-z]:\//.test(f);

/**
 * The changed files (normalised) that a files_read entry (normalised) covers: the
 * same repo-relative path, or for an absolute entry the longest changed path its
 * trailing components equal. A bare suffix such as "lib.rs" or "src/lib.rs" for
 * crates/core/src/lib.rs covers nothing, and the absolute path of
 * crates/core/src/lib.rs does not cover a root package's src/lib.rs as well.
 */
function coveredBy(read, changed) {
  if (changed.includes(read)) return [read];
  if (!isAbs(read)) return [];
  const tails = changed.filter((f) => f !== '' && read.endsWith(`/${f}`)).sort((a, b) => b.length - a.length);
  return tails.slice(0, 1);
}

/** Extract the last fenced json block containing "temper_verdict". */
export function extractVerdict(text) {
  if (!text) return null;
  const blocks = [...text.matchAll(/```(?:json)?\s*\n([\s\S]*?)```/g)].map((m) => m[1]).filter((b) => b.includes('temper_verdict'));
  const raw = blocks.at(-1);
  if (!raw) return null;
  try { return JSON.parse(raw); } catch (e) { return { __parseError: String(e.message) }; }
}

/**
 * Validate a verdict. Returns a list of problems (empty = valid).
 * @param {object} v
 * @param {{ changedRust?: string[], gate?: {ok:boolean}|null }} ctx
 */
export const validateVerdict = (v, ctx = {}) => verdictProblems(v, ctx).map((x) => x.message);

/**
 * The problems validateVerdict reports, each as { kind, message }: `kind` is a short, stable
 * name for telemetry (the message is prose the reviewer reads and may change).
 */
export function verdictProblems(v, ctx = {}) {
  const p = [];
  const add = (kind, message) => p.push({ kind, message });
  if (!v) return [{ kind: 'no-block', message: 'no verdict block found: end the final report with a ```json block containing "temper_verdict": 1' }];
  if (v.__parseError) return [{ kind: 'json-parse', message: `verdict JSON does not parse: ${v.__parseError}` }];
  if (v.temper_verdict !== 1) add('schema-version', '"temper_verdict" must be 1');
  if (!['approved', 'changes-requested'].includes(v.verdict)) add('verdict-value', '"verdict" must be "approved" or "changes-requested"');
  if (typeof v.fingerprint !== 'string' || !/^[0-9a-f]{16}$/.test(v.fingerprint)) add('fingerprint', '"fingerprint" must be the 16-hex fingerprint given in your review ticket');
  if (!Array.isArray(v.findings)) add('findings-array', '"findings" must be an array (empty if none)');
  const findings = Array.isArray(v.findings) ? v.findings : [];
  findings.forEach((f, i) => {
    if (!f || typeof f !== 'object') { add('finding-object', `findings[${i}] must be an object`); return; }
    const rule = String(f.rule ?? '');
    if (!/^[A-Z]{1,4}-\d{2}$/.test(rule) && !(rule === 'none' && f.severity === 'nit')) add('finding-rule', `findings[${i}].rule must be a rule ID like OWN-08 ("none" is allowed only for a nit no rule covers)`);
    if (!SEVERITIES.includes(f.severity)) add('finding-severity', `findings[${i}].severity must be one of ${SEVERITIES.join('/')}`);
    if (!f.file) add('finding-file', `findings[${i}].file is required`);
    if (!f.evidence) add('finding-evidence', `findings[${i}].evidence is required (quote the code)`);
    if (!f.fix) add('finding-fix', `findings[${i}].fix is required`);
  });
  const scores = v.scores && typeof v.scores === 'object' ? v.scores : null;
  if (!scores) add('scores-missing', '"scores" object is required');
  else {
    for (const d of DIMENSIONS) {
      const s = scores[d];
      if (s === undefined) add('score-missing', `scores.${d} is missing (0-4 or "n/a")`);
      else if (s !== 'n/a' && !(Number.isInteger(s) && s >= 0 && s <= 4)) add('score-range', `scores.${d} must be 0-4 or "n/a"`);
    }
    const na = ALWAYS_SCORED.filter((d) => scores[d] === 'n/a');
    if (na.length) add('score-na', `scores must rate ${ALWAYS_SCORED.slice(0, -1).join(', ')} and ${ALWAYS_SCORED.at(-1)} 0-4: they apply to any change ("n/a" only where another dimension does not apply); "n/a": ${na.join(', ')}`);
  }
  const ev = v.evidence ?? {};
  if (!Array.isArray(ev.files_read) || ev.files_read.length === 0) add('files-read', 'evidence.files_read must list the files you actually read');
  if (!Array.isArray(ev.commands_run) || ev.commands_run.length === 0) add('commands-run', 'evidence.commands_run must list the commands you ran (at least the gate)');
  if (ctx.changedRust?.length && Array.isArray(ev.files_read)) {
    const changed = ctx.changedRust.map(normPath);
    const seen = new Set(ev.files_read.flatMap((r) => coveredBy(normPath(r), changed)));
    const missed = ctx.changedRust.filter((f) => f.endsWith('.rs') && !seen.has(normPath(f)));
    if (missed.length) add('files-unreviewed', `changed files not reviewed: ${missed.slice(0, 10).join(', ')}`);
  }
  if (v.verdict === 'approved') {
    const bad = findings.filter((f) => f.severity === 'blocker' || f.severity === 'major');
    if (bad.length) add('approved-with-findings', `"approved" is inconsistent with ${bad.length} blocker/major finding(s): use "changes-requested"`);
    if (scores) {
      const low = DIMENSIONS.filter((d) => Number.isInteger(scores[d]) && scores[d] < 3);
      if (low.length) add('approved-low-score', `"approved" requires every applicable dimension >= 3; low: ${low.join(', ')}`);
    }
    if (ctx.gate && ctx.gate.ok === false) add('approved-red-gate', '"approved" is impossible: the gate is red for this fingerprint');
    const g = v.gate ?? {};
    if (Object.values(g).some((x) => x !== 'pass' && x !== 'n/a')) add('approved-gate-entry', '"approved" requires every gate entry to be "pass"');
  }
  return p;
}

/** The distinct problem kinds of a verdictProblems list, in order, for a telemetry row. */
export const problemKinds = (problems) => [...new Set(problems.map((x) => x.kind))];

export function recordVerdict(proj, v, meta = {}) {
  const rec = { ...v, recorded_at: new Date().toISOString(), ...meta };
  writeAtomic(path.join(proj.stateDir, 'verdicts', `${v.fingerprint}.json`), JSON.stringify(rec, null, 2));
  return rec;
}

export function verdictFor(proj, fp) {
  const t = readText(path.join(proj.stateDir, 'verdicts', `${fp}.json`));
  if (!t) return null;
  try { return JSON.parse(t); } catch { return null; }
}

export function summarizeFindings(v, max = 10) {
  const fs = (v.findings ?? []).filter((f) => f.severity === 'blocker' || f.severity === 'major');
  const rest = (v.findings ?? []).length - fs.length;
  return fs.slice(0, max).map((f) => `- [${f.severity}] ${f.rule} ${f.file}${f.line ? `:${f.line}` : ''}: ${f.fix}`).join('\n')
    + (rest > 0 ? `\n(${rest} minor/nit finding(s) in the full verdict)` : '');
}
