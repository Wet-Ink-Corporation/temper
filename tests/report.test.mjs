// temper report: the model's numbers, the renderers (escaping, the Markdown
// line budget, a self-contained HTML page) and the CLI on fixture collections.
// No cargo: every input is a JSON fixture, and new/legacy/fixed come from the
// real ratchet() over a changed-lines map (no git).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { buildReport, renderMarkdown, renderHtml, reportOutDir, MD_MAX_LINES } from '../scripts/lib/report.mjs';
import { ratchet } from '../scripts/lib/baseline.mjs';
import { sample, sampleRaised, RULEMAP } from './fixtures/report-sample.mjs';
import { canSymlink } from './fixtures/can-symlink.mjs';

const REPORT_MJS = fileURLToPath(new URL('../scripts/lib/report.mjs', import.meta.url));
const META = { generated: '2026-09-28T12:00:00.000Z', version: '9.9.9', base: 'origin/main', title: 'ws', rulemap: RULEMAP };
const MB = 'abcdef0123456789abcdef0123456789abcdef01';

const f = (crate, lint, file, line, rules, severity, extra = {}) => ({
  tool: lint.startsWith('scan:') ? 'scan' : lint.startsWith('rustdoc::') ? 'rustdoc' : 'clippy',
  lint, crate, file, line, end_line: line, message: `${lint} here`, rules, severity, ...extra,
});

/** A changed-lines map as changedLines() returns it: file -> Set of lines, `.base` the merge base. */
function changedAt(...locs) {
  const m = new Map();
  for (const [file, line] of locs) {
    if (!m.has(file)) m.set(file, new Set());
    m.get(file).add(line);
  }
  m.base = MB;
  return m;
}

/** Three crates: `a` carries debt with one fix, `b` regressed (a new missing_docs), `c` is clean. */
function small() {
  const findings = [
    f('a', 'clippy::unwrap_used', 'crates/a/src/lib.rs', 10, ['H-02'], 'must'),
    f('a', 'clippy::unwrap_used', 'crates/a/src/lib.rs', 20, ['H-02'], 'must'),
    f('a', 'clippy::indexing_slicing', 'crates/a/src/io.rs', 5, ['H-04'], 'must'),
    f('a', 'clippy::must_use_candidate', 'crates/a/src/io.rs', 7, ['API-11'], 'should'),
    f('b', 'missing_docs', 'crates/b/src/lib.rs', 3, ['H-15', 'API-15'], 'must'),
    f('b', 'clippy::semicolon_if_nothing_returned', 'crates/b/src/lib.rs', 9, [], 'unmapped'),
    f('b', 'clippy::semicolon_if_nothing_returned', 'crates/b/src/lib.rs', 11, [], 'unmapped'),
    f('b', 'clippy::rc_buffer', 'crates/b/src/lib.rs', 12, ['OWN-15'], 'consider'),
  ];
  const collection = {
    temper_findings: 1, at: '2026-09-28T11:59:00.000Z', commit: 'abcdef0123456789', toolchain: '1.98.1',
    sources: ['clippy', 'rustdoc', 'scan'],
    crates: [
      { name: 'a', dir: 'crates/a', lints_inherited: true, charter: true },
      { name: 'b', dir: 'crates/b', lints_inherited: false, charter: false },
      { name: 'c', dir: 'crates/c', lints_inherited: true, charter: true },
    ],
    findings, errors: [],
  };
  const baseline = {
    temper_baseline: 1, created: '2026-09-01T00:00:00Z', updated: '2026-09-02T00:00:00Z', commit: '0123abcd', toolchain: '1.98.1',
    crates: { a: 'crates/a', b: 'crates/b', c: 'crates/c' },
    totals: {
      a: { 'clippy::indexing_slicing': 2, 'clippy::must_use_candidate': 1, 'clippy::unwrap_used': 2 },
      b: { 'clippy::rc_buffer': 1, 'clippy::semicolon_if_nothing_returned': 2 },
    },
    files: {},
  };
  const res = ratchet(collection, baseline, { changed: changedAt(['crates/b/src/lib.rs', 3]) });
  return { collection, baseline, res };
}

test('report: the numbers come from the ratchet and match the input', () => {
  const { collection, baseline, res } = small();
  const r = buildReport(collection, baseline, res, META);
  assert.equal(r.status, 'regressed');
  assert.deepEqual(
    { ...r.totals },
    { findings: 8, must: 4, should: 1, consider: 1, unmapped: 2, h: 4, errors: 0, exceptions: 0, new: 1, fixed: 1, legacy: 7, baseline: 8, new_code: 1, raised: 0 },
  );
  assert.equal(r.totals.legacy, res.legacy_total, 'one definition of legacy: the ratchet');
  assert.equal(r.totals.fixed, res.fixed_count);
  assert.deepEqual(r.crates_summary, { total: 3, clean: 1, debt: 1, regressed: 1 });
  const by = Object.fromEntries(r.crates.map((c) => [c.name, c]));
  assert.deepEqual(r.crates.map((c) => c.name), ['b', 'a', 'c'], 'worst first');
  assert.equal(by.a.status, 'debt');
  assert.deepEqual([by.a.must, by.a.should, by.a.other, by.a.h, by.a.new, by.a.fixed, by.a.baseline_total], [3, 1, 0, 3, 0, 1, 5]);
  assert.deepEqual(by.a.top_rules, [{ id: 'H-02', count: 2 }, { id: 'API-11', count: 1 }, { id: 'H-04', count: 1 }]);
  assert.equal(by.b.status, 'regressed');
  assert.deepEqual([by.b.must, by.b.should, by.b.other, by.b.h, by.b.new, by.b.fixed], [1, 0, 3, 1, 1, 0]);
  assert.equal(by.b.lints_inherited, false);
  assert.equal(by.b.charter, false);
  assert.equal(by.c.status, 'clean');
  assert.equal(by.c.total, 0);
  // A finding that breaks two rules counts for both.
  assert.deepEqual(r.top_rules.slice(0, 1), [{ id: 'H-02', count: 2, level: 'MUST', family: 'H', title: RULEMAP.rules['H-02'].title }]);
  assert.equal(r.top_rules.find((x) => x.id === 'H-15').count, 1);
  assert.equal(r.top_rules.find((x) => x.id === 'API-15').count, 1);
  assert.deepEqual(r.top_files.map((x) => [x.file, x.must, x.total]), [
    ['crates/a/src/lib.rs', 2, 2], ['crates/b/src/lib.rs', 1, 4], ['crates/a/src/io.rs', 1, 2],
  ]);
  assert.deepEqual(r.increases, [{ crate: 'b', lint: 'missing_docs', baseline: 0, current: 1 }]);
  assert.deepEqual(r.untouched_increases, [], 'the rise is explained by the new-code finding');
  assert.deepEqual(r.fixed, [{ crate: 'a', lint: 'clippy::indexing_slicing', baseline: 2, current: 1 }]);
  assert.deepEqual(r.unmapped_lints, [{ lint: 'clippy::semicolon_if_nothing_returned', count: 2 }]);
  assert.deepEqual(r.new_code.map((x) => `${x.file}:${x.line}`), ['crates/b/src/lib.rs:3']);
  assert.deepEqual(r.ratchet, { mode: 'diff+totals', ok: false, base: 'origin/main', merge_base: MB, notes: [], computed_by: 'caller' });
  assert.deepEqual(r.provenance, {
    generated: META.generated, collected: collection.at, commit: collection.commit, toolchain: '1.98.1', temper: '9.9.9', base: 'origin/main', source: null,
  });
  assert.equal(r.crates.reduce((s, c) => s + c.total, 0), r.totals.findings);
  assert.equal(r.crates.reduce((s, c) => s + c.fixed, 0), r.totals.fixed);
  assert.doesNotMatch(r.status_text, /--tighten/, 'no tighten advice on a failing ratchet');
});

test('report: new = max(findings on changed lines, count rise) per crate and lint', () => {
  const { collection, baseline } = small();
  const b = structuredClone(baseline);
  b.totals.b.missing_docs = 1;
  // A swap: one unwrap fixed and one added on a changed line; the count is unchanged.
  const res = ratchet(collection, b, { changed: changedAt(['crates/a/src/lib.rs', 10]) });
  const r = buildReport(collection, b, res, META);
  assert.equal(r.totals.new, 1);
  assert.equal(r.crates.find((c) => c.name === 'a').status, 'regressed');
  assert.equal(r.crates.find((c) => c.name === 'b').status, 'debt');
  assert.deepEqual(r.increases, []);
  // An increase on an untouched line is new too, and listed as such.
  const b2 = structuredClone(baseline);
  b2.totals.b.missing_docs = 1;
  b2.totals.a['clippy::unwrap_used'] = 1;
  const r2 = buildReport(collection, b2, ratchet(collection, b2, { changed: changedAt() }), META);
  assert.equal(r2.totals.new, 1);
  assert.deepEqual(r2.untouched_increases, [{ crate: 'a', lint: 'clippy::unwrap_used', baseline: 1, current: 2 }]);
  assert.match(r2.status_text, /1 crate\/lint count above the baseline on untouched lines/);
});

test('report: counts like the ratchet (approved exceptions, sources not run, the (none) crate)', () => {
  const { collection, baseline } = small();
  collection.findings.push(f('a', 'scan:allow', 'crates/a/src/lib.rs', 40, ['H-05'], 'must', { exception: 'H-05' }));
  collection.findings.push(f(null, 'clippy::unwrap_used', 'build/gen.rs', 2, ['H-02'], 'must'));
  collection.sources = ['clippy', 'scan'];
  const b = structuredClone(baseline);
  b.totals.b['rustdoc::broken_intra_doc_links'] = 4; // rustdoc did not run: not "fixed"
  b.totals.b.missing_docs = 1;
  b.totals['(none)'] = { 'clippy::unwrap_used': 1 };
  const r = buildReport(collection, b, null, META);
  assert.equal(r.ratchet.computed_by, 'buildReport (totals only)');
  assert.equal(r.totals.findings, 9, 'the approved exception is not debt');
  assert.equal(r.totals.exceptions, 1);
  assert.deepEqual(r.exceptions, [{ crate: 'a', file: 'crates/a/src/lib.rs', line: 40, lint: 'scan:allow', exception: 'H-05' }]);
  assert.deepEqual([r.totals.new, r.totals.fixed, r.totals.baseline], [0, 1, 10]);
  assert.equal(r.status, 'debt');
  assert.equal(r.crates.find((c) => c.name === '(none)').total, 1);
  assert.match(renderHtml(r), /1 approved lint exception \(<code>#\[expect\]<\/code> with a reason\) not counted/);
});

test('report: a crate renamed in place keeps its baseline under its new name', () => {
  const { collection, baseline } = small();
  const b = structuredClone(baseline);
  b.crates = { 'a-old': 'crates/a', b: 'crates/b', c: 'crates/c' };
  b.totals['a-old'] = b.totals.a;
  delete b.totals.a;
  b.totals.b.missing_docs = 1;
  const r = buildReport(collection, b, ratchet(collection, b, { changed: changedAt() }), META);
  assert.equal(r.status, 'debt');
  const a = r.crates.find((c) => c.name === 'a');
  assert.deepEqual([a.new, a.fixed, a.baseline_total], [0, 1, 5]);
  assert.deepEqual(r.fixed, [{ crate: 'a', lint: 'clippy::indexing_slicing', baseline: 2, current: 1 }]);
  assert.equal(r.crates.some((c) => c.name === 'a-old'), false);
  assert.ok(r.ratchet.notes.some((n) => /a-old/.test(n)));
});

test('report: ratchet notes and a totals-only ratchet are shown as such', () => {
  const { collection, baseline } = small();
  const res = ratchet(collection, baseline, { changed: null });
  res.notes.push('git diff failed; only the totals were compared');
  const r = buildReport(collection, baseline, res, META);
  assert.equal(r.ratchet.mode, 'totals');
  assert.equal(r.provenance.base, null);
  assert.doesNotMatch(r.status_text, /since `origin\/main`/);
  assert.match(renderMarkdown(r), /\n\n_Note: git diff failed; only the totals were compared_\n/);
  assert.match(renderHtml(r), /Only the totals were compared \(see the notes above\)/);
});

test('report: without a ratchet result the totals are compared', () => {
  const { collection, baseline } = small();
  const r = buildReport(collection, baseline, null, META);
  assert.equal(r.ratchet.mode, 'totals');
  assert.equal(r.status, 'regressed', 'b has a missing_docs the baseline does not allow');
  assert.equal(r.totals.new, 1);
  assert.equal(r.totals.new_code, 0);
  assert.deepEqual(r.untouched_increases, [{ crate: 'b', lint: 'missing_docs', baseline: 0, current: 1 }]);
  assert.match(renderHtml(r), /Only the totals were compared\. Pass the findings file/);
  const b = structuredClone(baseline);
  b.totals.b.missing_docs = 1;
  const within = buildReport(collection, b, null, META);
  assert.equal(within.status, 'debt');
  assert.match(within.status_text, /Nothing new against the baseline totals/);
  assert.match(within.status_text, /temper baseline --tighten/, 'a passing ratchet with a fix suggests tightening');
});

test('report: without a baseline the ratchet is strict and every finding fails', () => {
  const { collection } = small();
  const r = buildReport(collection, null, ratchet(collection, null, {}), META);
  assert.equal(r.status, 'failing');
  assert.equal(r.ratchet.mode, 'strict');
  assert.deepEqual([r.totals.new, r.totals.fixed, r.totals.legacy, r.totals.baseline], [8, null, null, null]);
  assert.equal(r.baseline.present, false);
  assert.deepEqual(r.crates.map((c) => [c.name, c.status]), [['a', 'regressed'], ['b', 'regressed'], ['c', 'clean']]);
  assert.match(r.status_text, /zero findings are allowed, and 8 findings fail the gate.*temper baseline --init/);
  const md = renderMarkdown(r);
  assert.match(md, /\| 8 \(no baseline\) \| – \|/);
  assert.match(md, /\*\*Findings\*\* \(8, no baseline, so each one fails the gate\)/);
  assert.match(renderHtml(r), /<h2 id="new-code">Findings that fail the gate<\/h2>/);
  const scanOnly = { ...collection, findings: [f('a', 'scan:print', 'crates/a/build.rs', 3, ['H-06'], 'must')] };
  assert.equal(buildReport(scanOnly, null, null, META).status, 'failing', 'strict counts scanner findings too, like --check');
  const clean = buildReport({ ...collection, findings: [] }, null, null, META);
  assert.equal(clean.status, 'clean');
  assert.deepEqual(clean.crates_summary, { total: 3, clean: 3, debt: 0, regressed: 0 });
});

test('report: compile errors make the status "build errors"', () => {
  const { collection, baseline } = small();
  const broken = { ...collection, errors: [{ code: 'E0425', file: 'crates/a/src/lib.rs', line: 4, message: 'cannot find value `x` in this scope' }, 'linking failed'] };
  const r = buildReport(broken, baseline, null, META);
  assert.equal(r.status, 'error');
  assert.equal(r.totals.errors, 2);
  assert.match(renderMarkdown(r), /\*\*Compile errors\*\* \(2\)\n\n- crates\/a\/src\/lib\.rs:4 E0425 cannot find value `x` in this scope\n- linking failed/);
  assert.match(renderHtml(r), /<h2 id="errors">Compile errors<\/h2>/);
});

test('report: the realistic sample adds up', () => {
  const { collection, baseline, ratchet: res } = sample();
  const r = buildReport(collection, baseline, res, META);
  assert.equal(r.totals.findings, 198);
  assert.equal(r.totals.findings, r.totals.must + r.totals.should + r.totals.consider + r.totals.unmapped);
  assert.deepEqual([r.totals.new, r.totals.fixed, r.totals.new_code, r.totals.legacy, r.totals.baseline], [4, 5, 3, 195, 199]);
  assert.deepEqual(r.untouched_increases.map((x) => `${x.crate} ${x.lint}`), ['ledger-app clippy::indexing_slicing']);
  assert.deepEqual(r.crates_summary, { total: 6, clean: 2, debt: 2, regressed: 2 });
  assert.equal(r.top_rules.length, 15);
  assert.equal(r.top_files.length, 15);
  assert.ok(r.top_files.every((x, i, xs) => i === 0 || xs[i - 1].must >= x.must), 'files sorted by MUST count');
});

test('report: a raised baseline is called out in the Markdown and the HTML', () => {
  const { collection, baseline, ratchet: res } = sampleRaised();
  const r = buildReport(collection, baseline, res, META);
  assert.equal(r.totals.raised, 2);
  const md = renderMarkdown(r);
  assert.match(md, /^<!-- temper-report -->\n### temper compliance: regressed · baseline raised\n/);
  assert.match(md, /\n> \*\*BASELINE RAISED\.\*\* 2 counts higher in `\.claude\/temper-baseline\.json` since `5e1f00d2c4b3`: the accepted debt grew\./);
  assert.match(md, /\n> - `ledger-sync` `clippy::unwrap_used`: 9 → 12\n> - `ledger-sync` `missing_docs`: 6 → 10\n/);
  const html = renderHtml(r);
  assert.match(html, /<span id="raised" class="pill pill-raised">[^]*?Baseline raised<\/span>/);
  assert.ok(html.indexOf('class="raised"') < html.indexOf('class="kpis"'), 'right under the status, above the numbers');
  assert.match(html, /<td class="num">9<\/td><td class="num">12<\/td><td class="num">\+3<\/td>/);
  // Many raised counts still fit the comment budget, with the first ones shown.
  res.raised = Array.from({ length: 40 }, (_, i) => ({ crate: `c${i}`, lint: 'clippy::unwrap_used', from: 0, to: 1 }));
  const md2 = renderMarkdown(buildReport(collection, baseline, res, META));
  assert.ok(md2.split('\n').length - 1 <= MD_MAX_LINES);
  assert.match(md2, /> - … 35 more in report\.html/);
});

test('report: HTML escapes every diagnostic string and makes no requests', () => {
  const { collection, baseline } = small();
  const evil = '<script>alert(1)</script>';
  collection.crates.push({ name: '<b>x</b>', dir: 'x', lints_inherited: true, charter: true });
  const bad = f('<b>x</b>', 'clippy::<i>lint</i>', 'crates/x/"><img src=x onerror=alert(1)>.rs', 1, ['H-02'], 'must', { message: `used ${evil} and \`<T as Trait>\`` });
  collection.findings.push(bad);
  const res = ratchet(collection, baseline, { changed: changedAt(['crates/b/src/lib.rs', 3], [bad.file, 1]) });
  res.raised = [{ crate: '<u>r</u>', lint: 'clippy::<s>x</s>', from: 1, to: 2 }];
  const r = buildReport(collection, baseline, res, { ...META, title: '<svg onload=alert(1)>', base: '"><script>x</script>' });
  const html = renderHtml(r);
  for (const needle of ['<script', '<img', '<svg', '<b>x', '<i>lint', '<u>r', '<s>x', 'onerror=alert(1)>']) assert.ok(!html.includes(needle), `unescaped ${needle}`);
  assert.ok(html.includes('&lt;script&gt;alert(1)&lt;/script&gt;'));
  assert.ok(html.includes('<code>&lt;T as Trait&gt;</code>'), 'code spans in messages render as <code>, escaped');
  assert.ok(html.includes('&lt;svg onload=alert(1)&gt;'));
  // Self-contained: no scripts, stylesheets, images, fonts or remote URLs.
  assert.doesNotMatch(html, /<(?:script|link|img|iframe|object|embed|svg)\b|@import|url\(|https?:\/\//i);
  assert.doesNotMatch(html, /<[a-z][^>]*\s(?:src|href|on[a-z]+)=(?!["']?#)/i, 'only in-page links, no handlers');
  assert.match(html, /^<!doctype html>/);
  assert.match(html, /:root \{[^}]*--page:/);
  assert.match(html, /@media \(prefers-color-scheme: dark\) \{\s*:root \{/);
  assert.match(html, /font-variant-numeric: tabular-nums/);
  assert.match(html, /class="table-wrap"/);
  // Status is carried by text, not colour alone.
  for (const word of ['regressed', 'debt', 'clean']) assert.match(html, new RegExp(`class="pill pill-${word}">[^<]*<span[^>]*>[^<]*</span>${word}</span>`));
});

test('report: HTML shows the headline numbers and sections in order', () => {
  const { collection, baseline, ratchet: res } = sample();
  const html = renderHtml(buildReport(collection, baseline, res, META));
  const kpis = [...html.matchAll(/<p class="kpi-label">([^<]+)<\/p><p class="kpi-value">([^<]+)/g)].map((m) => [m[1], m[2]]);
  assert.deepEqual(kpis, [['MUST findings', '144'], ['Iron-rule (H) findings', '144'], ['New findings', '4'], ['Fixed since baseline', '5'], ['Crates clean', '2']]);
  const order = ['class="status', 'class="kpis"', 'id="crates"', 'id="rules"', 'id="files"', 'id="new-code"', 'Provenance'].map((s) => html.indexOf(s));
  assert.ok(order.every((x, i) => x > 0 && (i === 0 || x > order[i - 1])), `section order ${order}`);
  assert.equal((html.match(/<tr><td class="nowrap"><code>ledger-/g) ?? []).length, 6 + 1 + 2, 'scorecard, untouched increases and fixed rows');
});

test('report: Markdown stays within the line budget and escapes diagnostics', () => {
  const { collection, baseline, ratchet: res } = sample();
  const md = renderMarkdown(buildReport(collection, baseline, res, META));
  assert.ok(md.split('\n').length - 1 <= MD_MAX_LINES);
  assert.match(md, /^<!-- temper-report -->\n### temper compliance: regressed\n/);
  assert.match(md, /\| 144 of 198 \| 144 \| 4 \| 5 \| 2 of 6 \(2 debt, 2 regressed\) \|/);
  assert.match(md, /- `crates\/ledger-cli\/src\/[a-z]+\.rs:\d+` `clippy::unwrap_used` \(H-02\): used `unwrap\(\)` on a `Result` value/);
  assert.match(md, /\*\*Counts above the baseline on untouched lines\*\* \(1;/);

  // A stress case: many crates, rules, new findings, raised counts, notes and errors still fit.
  const big = sample();
  for (let i = 0; i < 40; i++) {
    big.collection.crates.push({ name: `extra-${i}`, dir: `crates/extra-${i}`, lints_inherited: true, charter: true });
    for (let j = 0; j < 12; j++) big.collection.findings.push(f(`extra-${i}`, 'clippy::unwrap_used', `crates/extra-${i}/src/lib.rs`, j + 1, ['H-02'], 'must'));
  }
  big.collection.errors = Array.from({ length: 20 }, (_, i) => `error ${i}`);
  const res2 = ratchet(big.collection, big.baseline, { changed: changedAt(...big.collection.findings.slice(-300).map((x) => [x.file, x.line])) });
  res2.raised = Array.from({ length: 12 }, (_, i) => ({ crate: `extra-${i}`, lint: 'clippy::unwrap_used', from: 0, to: 12 }));
  res2.notes.push('first note', 'second note');
  const lines = renderMarkdown(buildReport(big.collection, big.baseline, res2, META)).split('\n');
  assert.ok(lines.length - 1 <= MD_MAX_LINES, `${lines.length - 1} lines`);
  assert.ok(lines.some((l) => /^\*\*Crates\*\* \(\d+ of 46;/.test(l)), 'says how many crates it shows');

  // Injection: HTML, table pipes, links and @-mentions from a diagnostic stay inert.
  const { collection: c2, baseline: b2 } = small();
  const nasty = f('b', 'missing_docs', 'crates/b/src/lib.rs', 30, ['H-15'], 'must', { message: 'a <script>x</script> | [click](https://evil.example) @octocat `code | span`' });
  c2.findings.push(nasty);
  const md2 = renderMarkdown(buildReport(c2, b2, ratchet(c2, b2, { changed: changedAt([nasty.file, 30]) }), META));
  const line = md2.split('\n').find((l) => l.includes('crates/b/src/lib.rs:30'));
  assert.ok(line.includes('a &lt;script&gt;x&lt;/script&gt; | \\[click\\](https://evil.example) @​octocat `code | span`'), line);
  assert.ok(!md2.includes('<script'));
});

test('report: Markdown table cells escape pipes', () => {
  const { collection, baseline, res } = small();
  const r = buildReport(collection, baseline, res, { ...META, rulemap: { rules: { 'H-02': { level: 'MUST', title: 'a | b <c>' } } } });
  const row = renderMarkdown(r).split('\n').find((l) => l.startsWith('| H-02 |'));
  assert.equal(row, '| H-02 | MUST | 2 | a \\| b &lt;c&gt; |');
});

test('report: Markdown table cells keep backslashes before pipes literal', () => {
  const { collection, baseline, res } = small();
  // Outside a code span the backslash is escaped; inside one GFM keeps it literal. Either way
  // the pipe gets its own `\|`, so `\\|` reads as a backslash and a pipe, never a cell break.
  const r = buildReport(collection, baseline, res, { ...META, rulemap: { rules: { 'H-02': { level: 'MUST', title: 'x\\|y `p\\|q`' } } } });
  const row = renderMarkdown(r).split('\n').find((l) => l.startsWith('| H-02 |'));
  assert.equal(row, '| H-02 | MUST | 2 | x\\\\\\|y `p\\\\|q` |');
});

test('report: --out stays under target/ or .temper/', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-out-'));
  try {
    assert.equal(reportOutDir(root, path.join(root, 'target', 'temper-report')), path.join(root, 'target', 'temper-report'));
    assert.equal(reportOutDir(root, path.join(root, '.temper', 'r')), path.join(root, '.temper', 'r'));
    for (const bad of ['src', '.github/workflows', '.claude', 'target/../src', 'targets/x', path.join(os.tmpdir(), 'elsewhere')]) {
      assert.equal(reportOutDir(root, path.isAbsolute(bad) ? bad : path.join(root, bad)), null, bad);
    }
    if (canSymlink) {
      fs.mkdirSync(path.join(root, 'target'));
      fs.symlinkSync(path.join(root, 'src'), path.join(root, 'target', 'link'));
      assert.equal(reportOutDir(root, path.join(root, 'target', 'link')), null, 'a symlink');
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

// ---------- the CLI, in a child process (runReport reads the cwd) ----------

// A fixed script: the module URL and the arguments travel in argv, never spliced into the code.
const CLI_CODE = 'const [url, ...args] = process.argv.slice(1); import(url).then((m) => m.runReport(args)).then((c) => { process.exitCode = c; });';

function runCli(cwd, args) {
  return spawnSync(process.execPath, ['--input-type=module', '-e', CLI_CODE, pathToFileURL(REPORT_MJS).href, ...args], { cwd, encoding: 'utf8', env: { ...process.env, GIT_CEILING_DIRECTORIES: path.dirname(cwd), TEMPER_BASE: '' } });
}

function scratch(withRatchet) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-report-'));
  const { collection, baseline, res } = small();
  fs.mkdirSync(path.join(dir, 'target'));
  fs.writeFileSync(path.join(dir, 'target', 'findings.json'), JSON.stringify(withRatchet ? { ...collection, ratchet: res } : collection));
  return { dir, collection, baseline };
}
const readOut = (dir, name) => fs.readFileSync(path.join(dir, 'target', 'out', name), 'utf8');

test('report CLI: without a baseline it is strict; json, md and html agree', () => {
  const { dir } = scratch(false);
  try {
    const r = runCli(dir, ['--findings', 'target/findings.json', '--out', 'target/out']);
    assert.equal(r.status, 0, r.stderr);
    assert.match(r.stdout, /temper report: failing: 8 findings \(4 MUST, 4 iron-rule\); no baseline/);
    const json = JSON.parse(readOut(dir, 'report.json'));
    assert.equal(json.totals.findings, 8);
    assert.equal(json.ratchet.mode, 'strict');
    assert.equal(json.provenance.source, 'file target/findings.json');
    assert.match(json.provenance.temper, /^\d+\.\d+\.\d+/, 'the version comes from plugin.json');
    assert.match(readOut(dir, 'report.md'), new RegExp(`\\| ${json.totals.must} of ${json.totals.findings} \\| ${json.totals.h} \\| 8 \\(no baseline\\) \\|`));
    assert.match(readOut(dir, 'report.html'), new RegExp(`<p class="kpi-value">${json.totals.must}</p>`));
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('report CLI: uses the ratchet result the findings file carries, as-is', () => {
  const { dir } = scratch(true);
  try {
    // No baseline file here and no --base: the numbers still come from the lint step's ratchet.
    const r = runCli(dir, ['--findings', 'target/findings.json', '--out', 'target/out', '--base', 'HEAD']);
    assert.equal(r.status, 2, 'an explicit --base still has to resolve');
    const ok = runCli(dir, ['--findings', 'target/findings.json', '--out', 'target/out']);
    assert.equal(ok.status, 0, ok.stderr);
    const json = JSON.parse(readOut(dir, 'report.json'));
    assert.deepEqual([json.status, json.totals.new, json.totals.fixed, json.totals.legacy, json.ratchet.mode, json.ratchet.computed_by],
      ['regressed', 1, 1, 7, 'diff+totals', 'baseline --check']);
    assert.match(readOut(dir, 'report.md'), /\*\*New-code findings\*\* \(1, on lines changed since `abcdef012345`\)/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('report CLI: without an embedded result it runs the ratchet against the committed baseline', () => {
  const { dir, baseline } = scratch(false);
  try {
    fs.mkdirSync(path.join(dir, '.claude'));
    fs.writeFileSync(path.join(dir, '.claude', 'temper-baseline.json'), `${JSON.stringify(baseline, null, 2)}\n`);
    const r = runCli(dir, ['--findings=target/findings.json', '--out', 'target/out', '--format', 'json,md']);
    assert.equal(r.status, 0, r.stderr);
    assert.deepEqual(fs.readdirSync(path.join(dir, 'target', 'out')).sort(), ['report.json', 'report.md']);
    const json = JSON.parse(readOut(dir, 'report.json'));
    assert.equal(json.baseline.present, true);
    assert.deepEqual([json.totals.new, json.totals.fixed, json.ratchet.mode, json.ratchet.computed_by], [1, 1, 'totals', 'temper report']);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('report CLI: usage errors, refused paths and unusable input exit 2', () => {
  const { dir } = scratch(false);
  try {
    assert.equal(runCli(dir, ['--bogus']).status, 2);
    assert.equal(runCli(dir, ['--format', 'pdf']).status, 2);
    assert.equal(runCli(dir, ['--out']).status, 2);
    for (const out of ['out', 'src', 'target/../src', '.github/workflows']) {
      const r = runCli(dir, ['--findings', 'target/findings.json', '--out', out]);
      assert.equal(r.status, 2, out);
      assert.match(r.stderr, /refused: write the report under target\/ or \.temper\//);
    }
    assert.equal(fs.existsSync(path.join(dir, 'src')), false);
    if (canSymlink) {
      fs.mkdirSync(path.join(dir, 'target', 'out'));
      fs.symlinkSync(path.join(dir, 'victim.rs'), path.join(dir, 'target', 'out', 'report.md'));
      assert.equal(runCli(dir, ['--findings', 'target/findings.json', '--out', 'target/out']).status, 2, 'a report file that is a symlink');
      assert.equal(fs.existsSync(path.join(dir, 'victim.rs')), false);
      fs.rmSync(path.join(dir, 'target', 'out'), { recursive: true });
    }
    const noGit = runCli(dir, ['--findings', 'target/findings.json', '--out', 'target/out', '--base', 'origin/main']);
    assert.equal(noGit.status, 2);
    assert.match(noGit.stderr, /--base needs a git repository/);
    fs.writeFileSync(path.join(dir, 'bad.json'), '{"not":"a collection"}');
    assert.equal(runCli(dir, ['--findings', 'bad.json']).status, 2);
    const missing = runCli(dir, ['--findings', 'nope.json']);
    assert.equal(missing.status, 2, 'no findings file and not a Rust project: nothing to collect');
    assert.match(missing.stderr, /nope\.json not found; collecting instead/);
    assert.equal(runCli(dir, ['--help']).status, 0);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
