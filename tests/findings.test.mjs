// Findings collection: cargo JSON parsing on real captured output (clippy and
// rustdoc 1.98.1 with --cap-lints warn; see tests/fixtures/), crate attribution,
// the scanner source, and the generated rule map.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import {
  parseCargoMessages, dedupeFindings, mergeCollections, crateTable, crateFor, scanFindings, rulesFor, toolOf, ruleMap,
  isComplete, SOURCES, forbiddenLints, pruneFindingsCache, collectFindings,
} from '../scripts/lib/findings.mjs';
import { RULES } from '../scripts/lib/scan.mjs';
import { discover } from '../scripts/lib/project.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const fixture = (name) => fs.readFileSync(path.join(here, 'fixtures', name), 'utf8');
// The fixtures were captured in a workspace at /work (paths rewritten from the capture dir).
const WS = path.resolve('/work');
const parse = (name, tool, projectRoot = WS) => parseCargoMessages(fixture(name), { tool, workspaceRoot: WS, projectRoot });
const key = (f) => `${f.file}:${f.line} ${f.lint}`;

test('clippy JSON with --cap-lints warn: deny-level lints arrive as findings, lib and lib-test repeats collapse', () => {
  const { findings, errors } = parse('clippy-capped.jsonl', 'clippy');
  assert.deepEqual(errors, []);
  assert.equal(findings.length, 19, 'every compiler-message with a lint code (lib, lib test, integration test, dependent crate)');
  assert.ok(findings.every((f) => f.tool === 'clippy' && !f.file.startsWith('/') && !f.file.includes('\\')));
  const unique = dedupeFindings(findings);
  assert.deepEqual(unique.map(key).sort(), [
    'crates/alpha/src/lib.rs:10 clippy::indexing_slicing',
    'crates/alpha/src/lib.rs:16 clippy::as_conversions',
    'crates/alpha/src/lib.rs:16 clippy::cast_possible_truncation',
    'crates/alpha/src/lib.rs:5 clippy::missing_panics_doc',
    'crates/alpha/src/lib.rs:6 clippy::unwrap_used',
    'crates/alpha/src/lib.rs:9 clippy::must_use_candidate',
    'crates/alpha/src/lib.rs:9 missing_docs',
    'crates/alpha/tests/it.rs:4 clippy::unwrap_used',
    'crates/beta/src/lib.rs:9 clippy::must_use_candidate',
    'crates/beta/src/lib.rs:9 missing_docs',
  ]);
  const unwrap = unique.find((f) => f.lint === 'clippy::unwrap_used' && f.file.endsWith('lib.rs'));
  assert.deepEqual(
    { rules: unwrap.rules, severity: unwrap.severity, line: unwrap.line, end_line: unwrap.end_line, message: unwrap.message },
    { rules: ['H-02'], severity: 'must', line: 6, end_line: 6, message: 'used `unwrap()` on a `Result` value' },
  );
  assert.equal(unique.find((f) => f.lint === 'missing_docs').severity, 'must');
  assert.deepEqual(unique.find((f) => f.lint === 'missing_docs').rules, ['H-15', 'API-15']);
  assert.equal(unique.find((f) => f.lint === 'clippy::must_use_candidate').severity, 'should');
  // the dependent crate (beta) was checked although alpha has deny-level lints
  assert.ok(unique.some((f) => f.file.startsWith('crates/beta/')));
});

test('rustdoc JSON: rustdoc lints keep their prefix; a clippy-reported missing_docs is not counted twice', () => {
  const clippy = parse('clippy-capped.jsonl', 'clippy');
  const doc = parse('rustdoc-capped.jsonl', 'rustdoc');
  assert.deepEqual(doc.errors, []);
  const link = doc.findings.find((f) => f.lint === 'rustdoc::broken_intra_doc_links');
  assert.equal(key(link), 'crates/alpha/src/lib.rs:3 rustdoc::broken_intra_doc_links');
  assert.deepEqual(link.rules, ['API-15']);
  const base = { temper_findings: 1, sources: ['clippy', 'scan'], crates: [], findings: dedupeFindings(clippy.findings), errors: [] };
  const merged = mergeCollections(base, { ...base, sources: ['rustdoc'], findings: dedupeFindings(doc.findings) });
  assert.deepEqual(merged.sources, ['clippy', 'rustdoc', 'scan']);
  const docs = merged.findings.filter((f) => f.lint === 'missing_docs');
  assert.equal(docs.length, 2, 'one per crate');
  assert.ok(docs.every((f) => f.tool === 'clippy'), 'the first report (clippy) wins');
  assert.equal(merged.findings.length, 11);
});

test('a local macro_rules! expansion: each invocation is its own finding, at its call site', () => {
  // clippy-macro.jsonl: `getter!` (lines 3-11) has `v[$i] as u32` on line 8; it is invoked on lines 13 and 14
  const { findings } = parse('clippy-macro.jsonl', 'clippy');
  const unique = dedupeFindings(findings);
  assert.deepEqual(unique.map(key).sort(), [
    'crates/beta/src/lib.rs:13 clippy::as_conversions',
    'crates/beta/src/lib.rs:13 clippy::cast_possible_truncation',
    'crates/beta/src/lib.rs:13 clippy::indexing_slicing',
    'crates/beta/src/lib.rs:14 clippy::as_conversions',
    'crates/beta/src/lib.rs:14 clippy::cast_possible_truncation',
    'crates/beta/src/lib.rs:14 clippy::indexing_slicing',
  ]);
  const idx = unique.find((f) => f.lint === 'clippy::indexing_slicing' && f.line === 14);
  assert.deepEqual(idx.expansion, ['crates/beta/src/lib.rs:8:13', 'crates/beta/src/lib.rs:14:1']);
  // two violations in one macro body reach the same call site: the chain keeps them apart
  const twin = { ...idx, expansion: ['crates/beta/src/lib.rs:9:13', 'crates/beta/src/lib.rs:14:1'] };
  assert.equal(dedupeFindings([idx, twin]).length, 2);
});

test('E-codes and code-less errors are errors, not findings; notes are dropped', () => {
  const { findings, errors } = parse('clippy-errors.jsonl', 'clippy');
  assert.deepEqual(findings, []);
  assert.deepEqual(errors.map((e) => `${e.file}:${e.line} ${e.code} ${e.message}`), [
    'crates/beta/src/lib.rs:16 E0308 mismatched types',
    'crates/gamma/src/lib.rs:4 null this file contains an unclosed delimiter',
  ]);
});

test('file paths are relative to the project (git) root, with forward slashes, for CRLF and Windows-style input', () => {
  const nested = parseCargoMessages(fixture('clippy-capped.jsonl'), { tool: 'clippy', workspaceRoot: WS, projectRoot: path.dirname(WS) });
  assert.ok(nested.findings.every((f) => f.file.startsWith('work/crates/')), nested.findings[0].file);
  const line = fixture('clippy-capped.jsonl').split('\n')[0].replace('"file_name":"crates/alpha/src/lib.rs"', '"file_name":"crates\\\\alpha\\\\src\\\\lib.rs"');
  const crlf = `${line}\r\n${line}\r\n`;
  const win = parseCargoMessages(crlf, { tool: 'clippy', workspaceRoot: WS, projectRoot: WS });
  assert.equal(win.findings.length, 2);
  assert.equal(win.findings[0].file, 'crates/alpha/src/lib.rs');
});

test('crate attribution: the longest matching crate dir wins', () => {
  const crates = [{ name: 'root', dir: '.' }, { name: 'a', dir: 'crates/a' }, { name: 'ab', dir: 'crates/a/b' }, { name: 'ax', dir: 'crates/ax' }];
  assert.equal(crateFor('crates/a/b/src/lib.rs', crates).name, 'ab');
  assert.equal(crateFor('crates/a/src/lib.rs', crates).name, 'a');
  assert.equal(crateFor('crates/ax/src/lib.rs', crates).name, 'ax');
  assert.equal(crateFor('src/main.rs', crates).name, 'root');
  assert.equal(crateFor('src/main.rs', crates.slice(1)), null);
});

function tempWorkspace() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-findings-'));
  const put = (rel, text) => { const p = path.join(dir, ...rel.split('/')); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, text); };
  return { dir, put };
}

test('crateTable reads lint inheritance from the member manifest and the charter from AGENTS.md', () => {
  const { dir, put } = tempWorkspace();
  try {
    put('crates/a/Cargo.toml', '[package]\nname = "a"\n\n[lints]\nworkspace = true\n');
    put('crates/a/AGENTS.md', '# a\n');
    put('crates/b/Cargo.toml', '[package]\nname = "b"\n');
    const meta = {
      workspace_members: ['a-id', 'b-id'],
      packages: [
        { id: 'b-id', name: 'b', manifest_path: path.join(dir, 'crates', 'b', 'Cargo.toml') },
        { id: 'a-id', name: 'a', manifest_path: path.join(dir, 'crates', 'a', 'Cargo.toml') },
        { id: 'dep', name: 'dep', manifest_path: path.join(dir, 'vendor', 'dep', 'Cargo.toml') },
      ],
    };
    assert.deepEqual(crateTable(meta, dir), [
      { name: 'a', dir: 'crates/a', lints_inherited: true, charter: true },
      { name: 'b', dir: 'crates/b', lints_inherited: false, charter: false },
    ]);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('scan source: hard hits are findings, approved exceptions (ask) are flagged, soft hits and exempt paths are dropped', () => {
  const { dir, put } = tempWorkspace();
  try {
    put('Cargo.toml', '[workspace]\nmembers = ["crates/*"]\n');
    put('crates/a/Cargo.toml', '[package]\nname = "a"\n');
    put('crates/a/src/lib.rs', [
      '//! A.',
      'pub fn f(x: Option<u8>) -> u8 {',
      '    x.unwrap()',
      '}',
      'pub fn g(x: Option<u8>) -> u8 {',
      '    #[expect(clippy::unwrap_used, reason = "checked above")]',
      '    let v = x.unwrap();',
      '    let _ = v;',
      '    v',
      '}',
      '',
    ].join('\n'));
    put('crates/a/src/ffi.rs', 'pub fn h(x: u64) -> u32 { x as u32 }\n');
    put('stray/loose.rs', 'fn main() { None::<u8>.unwrap(); }\n');
    const proj = discover(dir);
    const crates = [{ name: 'a', dir: 'crates/a' }];
    const cfg = { ...proj.config, scanner: { exempt: [{ rule: 'H-11', paths: ['crates/a/src/ffi.rs'] }] } };
    const found = scanFindings(proj, crates, { cfg });
    assert.deepEqual(found.filter((f) => !f.exception).map(key), ['crates/a/src/lib.rs:3 scan:unwrap']);
    const unwrap = found.find((f) => !f.exception);
    assert.deepEqual({ tool: unwrap.tool, crate: unwrap.crate, rules: unwrap.rules, severity: unwrap.severity }, { tool: 'scan', crate: 'a', rules: ['H-02'], severity: 'must' });
    const exceptions = found.filter((f) => f.exception);
    assert.ok(exceptions.length >= 1, 'the #[expect]-covered unwrap is an exception request');
    assert.ok(exceptions.every((f) => f.file === 'crates/a/src/lib.rs'));
    assert.ok(!found.some((f) => f.lint === 'scan:let-underscore'), 'soft hits are advisory, not findings');
    assert.ok(!found.some((f) => f.file.startsWith('stray/')), 'files outside workspace crates are not compiled');
    const noExempt = scanFindings(proj, crates, { cfg: proj.config });
    assert.ok(noExempt.some((f) => f.lint === 'scan:as-cast' && f.file === 'crates/a/src/ffi.rs'));
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('forbidden lints come from [workspace.lints] for inheriting members and from a member\'s own [lints]', () => {
  const { dir, put } = tempWorkspace();
  try {
    put('Cargo.toml', '[workspace]\nmembers = ["crates/*"]\n\n[workspace.lints.rust]\nunsafe_code = "forbid"\nmissing_docs = "warn"\n\n[workspace.lints.clippy]\ndbg_macro = { level = "forbid", priority = 1 }\n');
    put('crates/a/Cargo.toml', '[package]\nname = "a"\n\n[lints]\nworkspace = true\n');
    put('crates/a/src/lib.rs', '//! A.\n#[expect(unsafe_code, reason = "ffi")]\npub fn y() {}\n#[allow(clippy::dbg_macro)]\nfn z() {}\n#[allow(missing_docs)]\npub fn w() {}\n');
    put('crates/own/Cargo.toml', '[package]\nname = "own"\n\n[lints.rust]\nunsafe_code = "deny"\n[lints.clippy]\nprint_stdout = "forbid"\n');
    put('crates/own/src/lib.rs', '//! Own.\n#[allow(unsafe_code)]\nfn y() {}\n');
    const proj = discover(dir);
    const crates = [{ name: 'a', dir: 'crates/a' }, { name: 'own', dir: 'crates/own' }];
    const forbids = forbiddenLints(proj, crates);
    assert.deepEqual([...forbids.get('a')].sort(), ['clippy::dbg_macro', 'unsafe_code']);
    assert.deepEqual([...forbids.get('own')], ['clippy::print_stdout']);
    const found = scanFindings(proj, crates).filter((f) => f.lint === 'scan:forbid-override');
    assert.deepEqual(found.map((f) => `${f.file}:${f.line}`), ['crates/a/src/lib.rs:2', 'crates/a/src/lib.rs:4']);
    assert.ok(found.every((f) => !f.exception && f.severity === 'must' && f.rules.includes('H-05')), 'always counted, even for #[expect]');
    assert.deepEqual(scanFindings(proj, crates, { only: ['forbid-override'] }).map((f) => f.lint), ['scan:forbid-override', 'scan:forbid-override']);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('crate-level #![forbid] reaches the crate root\'s module tree; a module\'s forbid only its own subtree', () => {
  const { dir, put } = tempWorkspace();
  try {
    put('Cargo.toml', '[workspace]\nmembers = ["crates/*"]\n');
    put('crates/a/Cargo.toml', '[package]\nname = "a"\n');
    put('crates/a/src/lib.rs', '//! A.\n#![forbid(clippy::indexing_slicing)]\npub mod util;\nmod ops;\n');
    put('crates/a/src/util.rs', '#[expect(clippy::indexing_slicing, reason = "checked")]\npub fn f(v: &[u8]) -> u8 { v[1] }\n');
    put('crates/a/src/ops/mod.rs', '#![forbid(unsafe_code)]\nmod inner;\n#[allow(clippy::indexing_slicing)]\nfn g() {}\n');
    put('crates/a/src/ops/inner.rs', '#[allow(unsafe_code)]\nfn h() {}\n');
    put('crates/a/src/loose.rs', '#[allow(unsafe_code)]\nfn not_in_the_tree() {}\n');
    put('crates/a/tests/it.rs', '#[allow(clippy::indexing_slicing)]\nfn separate_crate() {}\n');
    const proj = discover(dir);
    const found = scanFindings(proj, [{ name: 'a', dir: 'crates/a' }], { only: ['forbid-override'] });
    assert.deepEqual(found.map((f) => `${f.file}:${f.line}`), ['crates/a/src/ops/inner.rs:1', 'crates/a/src/ops/mod.rs:3', 'crates/a/src/util.rs:1']);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('scanner scope: member files only, not nested non-member packages or excluded paths', () => {
  const { dir, put } = tempWorkspace();
  try {
    put('Cargo.toml', '[package]\nname = "root"\n\n[workspace]\nmembers = [".", "crates/m"]\nexclude = ["tools/gen"]\n');
    put('src/lib.rs', 'pub fn f(x: Option<u8>) -> u8 { x.unwrap() }\n');
    put('vendor/depx/Cargo.toml', '[package]\nname = "depx"\n');
    put('vendor/depx/src/lib.rs', 'pub fn g(s: &str) -> u8 { s.parse().unwrap() }\n');
    put('fuzz/Cargo.toml', '[package]\nname = "fuzz"\n');
    put('fuzz/fuzz_targets/t.rs', 'fn main() { None::<u8>.unwrap(); }\n');
    put('tools/gen/main.rs', 'fn main() { None::<u8>.unwrap(); }\n');
    put('crates/m/Cargo.toml', '[package]\nname = "m"\n');
    put('crates/m/src/lib.rs', 'pub fn h(x: Option<u8>) -> u8 { x.unwrap() }\n');
    const proj = discover(dir);
    const crates = [{ name: 'root', dir: '.' }, { name: 'm', dir: 'crates/m' }];
    assert.deepEqual(scanFindings(proj, crates).map(key).sort(), ['crates/m/src/lib.rs:1 scan:unwrap', 'src/lib.rs:1 scan:unwrap']);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('scanFindings leaves out new-code-only rules: a baseline recorded before them holds no count (decisions.md 3.2)', () => {
  const { dir, put } = tempWorkspace();
  try {
    put('Cargo.toml', '[package]\nname = "root"\n\n[workspace]\n');
    put('src/lib.rs', 'pub fn f(x: Option<u8>) -> u8 {\n    if cfg!(test) { return 0; }\n    x.unwrap()\n}\n');
    const proj = discover(dir);
    assert.ok(RULES.find((r) => r.key === 'test-branch').newCodeOnly);
    assert.deepEqual(scanFindings(proj, [{ name: 'root', dir: '.' }]).map(key), ['src/lib.rs:3 scan:unwrap']);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('toolOf, severities and completeness', () => {
  assert.equal(toolOf('scan:unwrap'), 'scan');
  assert.equal(toolOf('rustdoc::broken_intra_doc_links'), 'rustdoc');
  assert.equal(toolOf('clippy::unwrap_used'), 'clippy');
  assert.equal(toolOf('missing_docs'), 'clippy');
  assert.deepEqual(rulesFor('clippy::no_such_lint'), { rules: [], severity: 'unmapped' });
  assert.deepEqual(rulesFor('scan:print'), { rules: ['H-06'], severity: 'must' });
  const cfg = { gate: { doc: true } };
  assert.equal(isComplete({ sources: ['clippy', 'scan'] }, cfg), false);
  assert.equal(isComplete({ sources: SOURCES }, cfg), true);
  assert.equal(isComplete({ sources: ['clippy', 'scan'] }, { gate: { doc: false } }), true);
  assert.equal(isComplete({ sources: ['clippy'] }, { gate: { doc: false, scan: false } }), true);
});

test('the findings cache keeps the newest 20 collections', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-cache-'));
  try {
    for (let i = 0; i < 25; i++) {
      const p = path.join(dir, `findings-${String(i).padStart(16, '0')}.json`);
      fs.writeFileSync(p, '{}');
      fs.utimesSync(p, new Date(1e12 + i * 1000), new Date(1e12 + i * 1000));
    }
    fs.writeFileSync(path.join(dir, 'telemetry.jsonl'), '');
    pruneFindingsCache({ stateDir: dir });
    const left = fs.readdirSync(dir).filter((f) => f.startsWith('findings-')).sort();
    assert.equal(left.length, 20);
    assert.equal(left[0], `findings-${'5'.padStart(16, '0')}.json`, 'the oldest go first');
    assert.ok(fs.existsSync(path.join(dir, 'telemetry.jsonl')));
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('a Cargo workspace below the git root is reported plainly', () => {
  const { dir, put } = tempWorkspace();
  try {
    put('rust/Cargo.toml', '[workspace]\nmembers = []\n');
    spawnSync('git', ['init', '-q'], { cwd: dir });
    const proj = discover(path.join(dir, 'rust'));
    if (!proj?.gitRoot) return; // git missing
    const c = collectFindings(proj, {});
    assert.equal(c.errors[0].code, 'workspace-below-git-root');
    assert.match(c.errors[0].message, /temper needs the Cargo workspace at the git root/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('rulemap.json: lint resolution as lint-claims resolves it, every scanner key, every cited rule defined', () => {
  const map = ruleMap();
  assert.equal(map.lints ? true : false, true);
  assert.deepEqual(map.lints['clippy::unwrap_used'], ['H-02']);
  assert.deepEqual(map.lints['missing_docs'], ['H-15', 'API-15']);
  assert.ok(map.lints['clippy::string_slice'].includes('H-04'), 'a bare token after a clippy:: token is a clippy lint');
  assert.ok(map.lints['clippy::cast_possible_truncation'].includes('H-11'), '`cast_*` expands to its members');
  assert.deepEqual(map.lints['rustdoc::broken_intra_doc_links'], ['API-15']);
  assert.ok(!('string_slice' in map.lints) && !('clippy::missing_docs' in map.lints));
  for (const r of RULES) assert.ok(map.scan[r.key]?.includes(r.id), `scan key ${r.key}`);
  const cited = new Set([...Object.values(map.lints), ...Object.values(map.scan)].flat());
  for (const id of cited) assert.ok(map.rules[id], `rule ${id} is defined`);
  for (const [id, r] of Object.entries(map.rules)) {
    assert.match(r.level, /^(MUST|SHOULD|CONSIDER)$/, id);
    assert.equal(r.family, id.split('-')[0]);
    assert.ok(r.title.length > 0 && r.title.length <= 140, `${id} title length`);
  }
  assert.match(map.rules['H-02'].title, /^No `unwrap`\/`expect`/);
});
