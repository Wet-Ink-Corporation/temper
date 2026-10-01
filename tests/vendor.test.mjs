// Vendoring the CLI into <repo>/.claude/temper/, and the example workspace's
// vendored copy staying identical to scripts/.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { vendorTool, temperVersion, VENDOR_DIR } from '../scripts/lib/vendor.mjs';

const PLUGIN = fileURLToPath(new URL('..', import.meta.url));
const SCRIPTS = path.join(PLUGIN, 'scripts');
const EXAMPLE = path.join(PLUGIN, 'templates', 'example-workspace');
const VERSION = JSON.parse(fs.readFileSync(path.join(PLUGIN, '.claude-plugin', 'plugin.json'), 'utf8')).version;

const libFiles = () => fs.readdirSync(path.join(SCRIPTS, 'lib')).filter((n) => !n.startsWith('.')).sort();

function scratch() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'temper-vendor-'));
}

test('vendor: creates .claude/temper with the CLI, every lib file and VERSION', () => {
  const root = scratch();
  try {
    const res = vendorTool(root);
    assert.equal(res.drift, false);
    assert.equal(res.actions.length, 1);
    assert.match(res.actions[0], new RegExp(`^create ${VENDOR_DIR}/ \\(temper ${VERSION.replace(/\./g, '\\.')}: `));
    const dest = path.join(root, '.claude', 'temper');
    assert.deepEqual(fs.readdirSync(path.join(dest, 'lib')).sort(), libFiles());
    assert.ok(libFiles().includes('report.mjs') && libFiles().includes('vendor.mjs'));
    assert.equal(fs.readFileSync(path.join(dest, 'temper.mjs'), 'utf8'), fs.readFileSync(path.join(SCRIPTS, 'temper.mjs'), 'utf8'));
    assert.equal(fs.readFileSync(path.join(dest, 'VERSION'), 'utf8'), `${VERSION}\n`);
    assert.equal(temperVersion(), VERSION);
    assert.deepEqual(vendorTool(root), { actions: [`skip   ${VENDOR_DIR}/ (up to date with temper ${VERSION})`], drift: false });
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('vendor: dry run writes nothing', () => {
  const root = scratch();
  try {
    const res = vendorTool(root, { dry: true });
    assert.match(res.actions[0], /^create /);
    assert.equal(fs.existsSync(path.join(root, '.claude')), false);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('vendor: drift is reported, and replaced only with --force', () => {
  const root = scratch();
  try {
    vendorTool(root);
    const dest = path.join(root, '.claude', 'temper');
    const gate = path.join(dest, 'lib', 'gate.mjs');
    fs.writeFileSync(gate, '// edited by hand\n');
    fs.writeFileSync(path.join(dest, 'lib', 'stale.mjs'), 'export {};\n');
    fs.rmSync(path.join(dest, 'VERSION'));

    const report = vendorTool(root);
    assert.equal(report.drift, true);
    assert.match(report.actions[0], /^drift  \.claude\/temper\/ differs from temper [^:]+: lib\/gate\.mjs \(changed\), VERSION \(missing\), lib\/stale\.mjs \(not in temper [^)]+\); rerun with --force to replace it$/);
    assert.equal(fs.readFileSync(gate, 'utf8'), '// edited by hand\n', 'no --force: untouched');

    const dry = vendorTool(root, { force: true, dry: true });
    assert.match(dry.actions[0], /^update /);
    assert.equal(fs.readFileSync(gate, 'utf8'), '// edited by hand\n', 'dry run: untouched');

    const forced = vendorTool(root, { force: true });
    assert.equal(forced.drift, true);
    assert.match(forced.actions[0], /^update \.claude\/temper\/ to temper /);
    assert.equal(fs.readFileSync(gate, 'utf8'), fs.readFileSync(path.join(SCRIPTS, 'lib', 'gate.mjs'), 'utf8'));
    assert.equal(fs.existsSync(path.join(dest, 'lib', 'stale.mjs')), false, 'files temper no longer ships are removed');
    assert.equal(vendorTool(root).drift, false);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('vendor: CRLF line endings from a Windows checkout are not drift', () => {
  const root = scratch();
  try {
    vendorTool(root);
    const p = path.join(root, '.claude', 'temper', 'lib', 'report.mjs');
    fs.writeFileSync(p, fs.readFileSync(p, 'utf8').replace(/\n/g, '\r\n'));
    assert.equal(vendorTool(root).drift, false);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('vendor: the copy runs standalone and reports its own VERSION', () => {
  const root = scratch();
  try {
    vendorTool(root);
    fs.writeFileSync(path.join(root, 'findings.json'), JSON.stringify({
      temper_findings: 1, at: '2026-09-28T00:00:00Z', commit: 'abc', toolchain: '1.98.1',
      crates: [{ name: 'a', dir: 'crates/a', lints_inherited: true, charter: true }],
      findings: [{ tool: 'clippy', lint: 'clippy::unwrap_used', crate: 'a', file: 'crates/a/src/lib.rs', line: 1, end_line: 1, message: 'used `unwrap()`', rules: ['H-02'], severity: 'must' }],
      errors: [],
    }));
    const r = spawnSync(process.execPath, [path.join(root, '.claude', 'temper', 'temper.mjs'), 'report', '--findings', 'findings.json', '--out', 'target/out'], {
      cwd: root, encoding: 'utf8', env: { ...process.env, GIT_CEILING_DIRECTORIES: path.dirname(root) },
    });
    assert.equal(r.status, 0, `${r.stdout}\n${r.stderr}`);
    const json = JSON.parse(fs.readFileSync(path.join(root, 'target', 'out', 'report.json'), 'utf8'));
    assert.equal(json.provenance.temper, VERSION);
    assert.equal(json.totals.must, 1);
    assert.ok(json.top_rules[0].title, 'rulemap.json travels with the copy');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('vendor: the example workspace carries an up-to-date copy of scripts/', () => {
  const res = vendorTool(EXAMPLE, { dry: true });
  assert.equal(res.drift, false,
    `${res.actions.join('\n')}\nRefresh it: node -e "import('./scripts/lib/vendor.mjs').then((m) => console.log(m.vendorTool('templates/example-workspace', { force: true }).actions.join('\\n')))"`);
  assert.match(res.actions[0], /^skip /, 'the example must have a vendored copy');
});
