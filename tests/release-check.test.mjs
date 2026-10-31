// tests/release-check.sh: the release checklist's version checks, run against
// throwaway git repositories built from the CHANGELOGs in
// tests/fixtures/release-check/.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

/** Escape every regular-expression metacharacter in `s`. */
const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const PLUGIN = fileURLToPath(new URL('..', import.meta.url));
const SCRIPT = path.join(PLUGIN, 'tests', 'release-check.sh');
const FIXTURES = path.join(PLUGIN, 'tests', 'fixtures', 'release-check');

// The script is bash and runs on Linux in CI and the release workflow.
const skip = process.platform === 'win32' ? 'bash script; runs on Linux in CI'
  : spawnSync('git', ['--version']).status !== 0 ? 'needs git' : false;

const git = (cwd, ...args) => {
  const res = spawnSync('git', args, { cwd, encoding: 'utf8' });
  assert.equal(res.status, 0, `git ${args.join(' ')}: ${res.stderr}`);
  return res.stdout.trim();
};

// Writes the release files: plugin.json, the temper-lens plugin.json, the vendored VERSION and
// CHANGELOG.md.
function release(root, { version, vendored = version, lens = version, changelog }) {
  fs.mkdirSync(path.join(root, '.claude-plugin'), { recursive: true });
  fs.writeFileSync(path.join(root, '.claude-plugin', 'plugin.json'), `${JSON.stringify({ name: 'temper', version }, null, 2)}\n`);
  const l = path.join(root, 'plugins', 'temper-lens', '.claude-plugin');
  fs.mkdirSync(l, { recursive: true });
  fs.writeFileSync(path.join(l, 'plugin.json'), `${JSON.stringify({ name: 'temper-lens', version: lens }, null, 2)}\n`);
  const v = path.join(root, 'templates', 'example-workspace', '.claude', 'temper');
  fs.mkdirSync(v, { recursive: true });
  fs.writeFileSync(path.join(v, 'VERSION'), `${vendored}\n`);
  fs.copyFileSync(path.join(FIXTURES, changelog), path.join(root, 'CHANGELOG.md'));
}

function write(root, rel, text) {
  fs.mkdirSync(path.dirname(path.join(root, rel)), { recursive: true });
  fs.writeFileSync(path.join(root, rel), text);
}

const commit = (root, msg) => { git(root, 'add', '-A'); git(root, 'commit', '-q', '-m', msg); };

// A repository with v1.0.0 tagged, then a change to `touch` and the 1.1.0
// release files committed on top.
function repo({ touch = 'scripts/temper.mjs', changelog = 'CHANGELOG-1.1.0-note.md', version = '1.1.0', vendored, lens, tag100 = true } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-release-'));
  git(root, 'init', '-q');
  git(root, 'config', 'user.email', 'test@example.com');
  git(root, 'config', 'user.name', 'test');
  git(root, 'config', 'commit.gpgsign', 'false');
  write(root, 'scripts/temper.mjs', '// 1.0.0\n');
  write(root, 'README.md', 'temper\n');
  release(root, { version: '1.0.0', changelog: 'CHANGELOG-1.0.0.md' });
  commit(root, 'temper 1.0.0');
  if (tag100) git(root, 'tag', 'v1.0.0');
  write(root, touch, `// changed after 1.0.0\n`);
  release(root, { version, vendored, lens, changelog });
  commit(root, 'temper 1.1.0');
  return root;
}

function check(root, ...args) {
  const res = spawnSync('bash', [SCRIPT, '--root', root, ...args], { encoding: 'utf8' });
  return { status: res.status, out: `${res.stdout}${res.stderr}` };
}

function withRepo(opts, fn) {
  const root = repo(opts);
  try { fn(root); } finally { fs.rmSync(root, { recursive: true, force: true }); }
}

test('release-check: agreeing versions, a matching tag and an Upgrading note pass', { skip }, () => {
  withRepo({}, (root) => {
    for (const args of [[], ['--tag', 'v1.1.0']]) {
      const r = check(root, ...args);
      assert.equal(r.status, 0, r.out);
      assert.match(r.out, /the 1\.1\.0 section ends with an Upgrading note/);
      assert.match(r.out, /versions agree: 1\.1\.0/);
    }
  });
});

test('release-check: plugin.json disagreeing with the CHANGELOG heading fails', { skip }, () => {
  withRepo({ version: '1.1.1', vendored: '1.1.1' }, (root) => {
    const r = check(root);
    assert.equal(r.status, 1);
    assert.match(r.out, /plugin\.json is 1\.1\.1, the top CHANGELOG heading is 1\.1\.0/);
  });
});

test('release-check: a stale vendored VERSION fails', { skip }, () => {
  withRepo({ vendored: '1.0.0' }, (root) => {
    const r = check(root);
    assert.equal(r.status, 1);
    assert.match(r.out, /plugin\.json is 1\.1\.0, templates\/example-workspace\/\.claude\/temper\/VERSION is 1\.0\.0/);
  });
});

test('release-check: a temper-lens plugin.json left behind fails', { skip }, () => {
  withRepo({ lens: '1.0.0' }, (root) => {
    const r = check(root);
    assert.equal(r.status, 1);
    assert.match(r.out, /plugin\.json is 1\.1\.0, plugins\/temper-lens\/\.claude-plugin\/plugin\.json is 1\.0\.0/);
  });
});

test('release-check: a tag that names another version fails', { skip }, () => {
  withRepo({}, (root) => {
    const r = check(root, '--tag', 'v1.2.0');
    assert.equal(r.status, 1);
    assert.match(r.out, /the tag is v1\.2\.0, plugin\.json is 1\.1\.0 \(want v1\.1\.0\)/);
  });
});

test('release-check: an existing tag that is not HEAD fails', { skip }, () => {
  withRepo({}, (root) => {
    git(root, 'tag', 'v1.1.0', 'HEAD~1');
    const r = check(root, '--tag', 'v1.1.0');
    assert.equal(r.status, 1);
    assert.match(r.out, /tag v1\.1\.0 exists and does not point at HEAD/);
  });
});

test('release-check: a release that changes scripts/ or templates/workspace/ needs a closing Upgrading note', { skip }, () => {
  for (const touch of ['scripts/temper.mjs', 'templates/workspace/AGENTS.md']) {
    for (const changelog of ['CHANGELOG-1.1.0-no-note.md', 'CHANGELOG-1.1.0-note-not-last.md']) {
      withRepo({ touch, changelog }, (root) => {
        const r = check(root);
        assert.equal(r.status, 1, `${touch} ${changelog}: ${r.out}`);
        assert.match(r.out, /since v1\.0\.0, but its CHANGELOG section does not end with an 'Upgrading from X\.Y' note/);
        assert.match(r.out, new RegExp(`^  ${escapeRegExp(touch)}$`, 'm'));
      });
    }
  }
});

test('release-check: a release that changes neither needs no Upgrading note', { skip }, () => {
  withRepo({ touch: 'README.md', changelog: 'CHANGELOG-1.1.0-no-note.md' }, (root) => {
    const r = check(root, '--tag', 'v1.1.0');
    assert.equal(r.status, 0, r.out);
    assert.match(r.out, /changes nothing in scripts\/ or templates\/workspace\/ since v1\.0\.0/);
  });
});

test('release-check: --base overrides the previous tag', { skip }, () => {
  withRepo({ changelog: 'CHANGELOG-1.1.0-no-note.md' }, (root) => {
    assert.equal(check(root, '--base', 'HEAD').status, 0);
    assert.equal(check(root, '--base', 'HEAD~1').status, 1);
  });
});

test('release-check: an Unreleased section above the release fails only with --tag', { skip }, () => {
  withRepo({ changelog: 'CHANGELOG-unreleased.md' }, (root) => {
    assert.equal(check(root).status, 0);
    const r = check(root, '--tag', 'v1.1.0');
    assert.equal(r.status, 1);
    assert.match(r.out, /an '## Unreleased' section above 1\.1\.0/);
  });
});

test('release-check: without --tag, an already tagged release is not re-checked', { skip }, () => {
  withRepo({ changelog: 'CHANGELOG-1.1.0-no-note.md' }, (root) => {
    git(root, 'tag', 'v1.1.0');
    const r = check(root);
    assert.equal(r.status, 0, r.out);
    assert.match(r.out, /v1\.1\.0 is already tagged/);
  });
});

test('release-check: without the previous tag the note check is skipped, except with --tag', { skip }, () => {
  withRepo({ tag100: false, changelog: 'CHANGELOG-1.1.0-no-note.md' }, (root) => {
    const r = check(root);
    assert.equal(r.status, 0, r.out);
    assert.match(r.out, /cannot resolve v1\.0\.0 \(no tags fetched\?\); Upgrading note not checked/);
    const t = check(root, '--tag', 'v1.1.0');
    assert.equal(t.status, 1);
    assert.match(t.out, /cannot resolve v1\.0\.0 .*fetch the tags/);
  });
});

test('release-check: --notes writes the release section without its heading, and only when the checks pass', { skip }, () => {
  withRepo({}, (root) => {
    const notes = path.join(root, 'notes.md');
    assert.equal(check(root, '--tag', 'v1.1.0', '--notes', notes).status, 0);
    assert.equal(fs.readFileSync(notes, 'utf8'), [
      'Changes to the CLI.',
      '- `temper gate` is faster.',
      '',
      'Upgrading from 1.0.x: `scripts/temper.mjs` changed, so refresh the vendored copy with `temper setup --force`.',
      '',
    ].join('\n'));
    fs.rmSync(notes);
    assert.equal(check(root, '--tag', 'v1.2.0', '--notes', notes).status, 1);
    assert.equal(fs.existsSync(notes), false);
  });
});

test('release-check: this repository passes', { skip }, () => {
  const r = spawnSync('bash', [SCRIPT], { encoding: 'utf8' });
  assert.equal(r.status, 0, `${r.stdout}${r.stderr}`);
});
