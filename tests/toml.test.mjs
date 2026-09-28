import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseToml } from '../scripts/lib/toml.mjs';
import { workspaceRootFor, memberProblems, watchedKeys, nonCaretRequirement, depRequirements } from '../scripts/lib/manifest.mjs';

test('parses the Cargo manifest shapes temper reads', () => {
  const m = parseToml(`# comment
[package]
name = "ledger-cli" # trailing comment
edition.workspace = true
"quoted.key" = 'lit#eral'

[lints]
workspace = true

[[bin]]
name = "ledger"
path = "src/main.rs"

[[bin]]
name = "tool"
test = false

[target.'cfg(unix)'.dependencies]
nix = { version = "0.29", default-features = false, features = [
  "fs",   # files
  "user",
] }

[profile.release.package."*"]
opt-level = 3

[features]
default = []
desc = """
two \\
  lines"""
raw = '''a
b'''
n = 0x1F
f = 1_000.5
d = 1979-05-27 07:32:00Z
`);
  assert.equal(m.package.name, 'ledger-cli');
  assert.deepEqual(m.package.edition, { workspace: true });
  assert.equal(m.package['quoted.key'], 'lit#eral');
  assert.equal(m.lints.workspace, true);
  assert.deepEqual(m.bin.map((b) => b.name), ['ledger', 'tool']);
  assert.equal(m.bin[1].test, false);
  assert.deepEqual(m.target['cfg(unix)'].dependencies.nix.features, ['fs', 'user']);
  assert.equal(m.profile.release.package['*']['opt-level'], 3);
  assert.deepEqual(m.features.default, []);
  assert.equal(m.features.desc, 'two lines');
  assert.equal(m.features.raw, 'a\nb');
  assert.equal(m.features.n, 31);
  assert.equal(m.features.f, 1000.5);
  assert.equal(m.features.d, '1979-05-27 07:32:00Z');
});

test('CRLF, escapes and invalid input', () => {
  assert.deepEqual(parseToml('a = "x\\ty\\u00e9"\r\n[b]\r\nc = [1,\r\n 2]\r\n'), { a: 'x\tyé', b: { c: [1, 2] } });
  assert.equal(parseToml('a = '), null);
  assert.equal(parseToml('[x\nb = 1'), null);
  assert.equal(parseToml('a = "unterminated\n'), null);
  assert.equal(parseToml('a = 1 b = 2'), null);
});

test('the template and example manifests parse', () => {
  const here = path.dirname(fileURLToPath(import.meta.url));
  for (const rel of ['../templates/workspace/Cargo.toml', '../templates/example-workspace/Cargo.toml']) {
    const text = fs.readFileSync(path.join(here, rel), 'utf8');
    const m = parseToml(text);
    assert.ok(m?.workspace?.lints, rel);
  }
});

test('non-caret requirements (ARC-12)', () => {
  for (const ok of ['1', '1.4', '^1.4.2', '0.3', '1.4.2']) assert.equal(nonCaretRequirement(ok), null, ok);
  assert.match(nonCaretRequirement('=1.4.2'), /exact/);
  assert.match(nonCaretRequirement('~1.4'), /tilde/);
  assert.match(nonCaretRequirement('*'), /wildcard/);
  assert.match(nonCaretRequirement('1.*'), /wildcard/);
  assert.match(nonCaretRequirement('>=1.2, <1.5'), /upper bound/);
  assert.match(nonCaretRequirement('>=1.2'), /open-ended/);
  const reqs = depRequirements(parseToml('[workspace.dependencies]\na = "1"\nb = { version = "=2" }\nc = { path = "../c" }\n[target.x.dev-dependencies]\nd = "~3"\n'));
  assert.deepEqual([...reqs], [['workspace.dependencies:a', '1'], ['workspace.dependencies:b', '=2'], ['target.x.dev-dependencies:d', '~3']]);
});

test('workspace root discovery follows Cargo: self, package.workspace, nearest ancestor, exclude', () => {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-toml-'));
  try {
    fs.mkdirSync(path.join(ws, 'crates', 'a'), { recursive: true });
    fs.mkdirSync(path.join(ws, 'tools', 'x'), { recursive: true });
    fs.writeFileSync(path.join(ws, 'Cargo.toml'), '[workspace]\nmembers = ["crates/*"]\nexclude = ["tools/x/"]\n[workspace.lints.rust]\nunsafe_code = "forbid"\n');
    const a = parseToml('[package]\nname = "a"\n');
    const root = workspaceRootFor(path.join(ws, 'crates', 'a', 'Cargo.toml'), a);
    assert.equal(root.path, path.join(ws, 'Cargo.toml'));
    assert.equal(root.self, false);
    assert.deepEqual(memberProblems(a, root).map((p) => p.id), ['MOD-09']);
    assert.equal(workspaceRootFor(path.join(ws, 'tools', 'x', 'Cargo.toml'), a), null);
    const own = parseToml('[package]\nname = "r"\n[workspace]\n[profile.release]\nlto = true\n[lints]\nworkspace = true\n[workspace.lints.rust]\nunsafe_code = "forbid"\n');
    const self = workspaceRootFor(path.join(ws, 'tools', 'x', 'Cargo.toml'), own);
    assert.equal(self.self, true);
    assert.deepEqual(memberProblems(own, self), []);
    assert.equal(workspaceRootFor(path.join(os.tmpdir(), 'no-such-dir-temper', 'Cargo.toml'), a), null);
  } finally {
    fs.rmSync(ws, { recursive: true, force: true });
  }
});

test('watched manifest switches carry their rule id', () => {
  const w = watchedKeys(parseToml('[package]\nrust-version = "1.96"\nautobins = false\n[lib]\ndoctest = false\n[[example]]\nname = "demo"\nrequired-features = ["cli"]\n[patch.crates-io]\nfoo = { path = "../foo" }\n[features]\ndefault = ["std"]\n'));
  assert.deepEqual(Object.fromEntries([...w].map(([k, v]) => [k, v.id])), {
    'package.autobins': 'H-12',
    'package.rust-version': 'H-16',
    'features.default': 'H-16',
    patch: 'H-16',
    '[lib] doctest': 'H-12',
    '[[example]] demo required-features': 'H-12',
  });
});
