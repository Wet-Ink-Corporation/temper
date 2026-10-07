// scripts/lib/shell.mjs: the one shell reader behind the preBash write rules.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import os from 'node:os';
import path from 'node:path';
import { parseCommands, writeTargets, landing, patchCommand, patchFiles, codeWriteTargets, resolvePath, pathspecPattern } from '../scripts/lib/shell.mjs';

const root = path.resolve(os.tmpdir(), 'shell-test-root');
const at = (...p) => path.join(root, ...p);
const targets = (cmd, opts = {}) => writeTargets(cmd, { cwd: root, ...opts }).map((t) => [t.verb, t.path]);

test('shell: quotes, escapes, comments and heredoc bodies never make a write', () => {
  for (const cmd of [
    "echo 'x > clippy.toml'",
    'echo "x > clippy.toml"',
    'echo x \\> clippy.toml',
    'echo hi # > clippy.toml',
    'cat <<EOF > /dev/null\necho x > clippy.toml\nEOF',
    'echo $((1<<2))',
    'git commit -m "cp a b > c"',
  ]) {
    assert.deepEqual(targets(cmd).filter(([, p]) => p !== '/dev/null' && p !== path.resolve('/dev/null')), [], cmd);
  }
});

test('shell: redirects, write verbs and interpreter code are reported with absolute paths', () => {
  assert.deepEqual(targets('echo x>a 2>&1 >>b &>c'), [['redirect', at('a')], ['redirect', at('b')], ['redirect', at('c')]]);
  assert.deepEqual(targets('jq . x | tee -a t1 t2'), [['tee', at('t1')], ['tee', at('t2')]]);
  assert.deepEqual(targets("sed -i.bak 's/a/b/' s1"), [['sed -i', at('s1')]]);
  assert.deepEqual(targets('dd if=x of=d1 && curl -sSLo c1 u && wget -O w1 u'), [['dd', at('d1')], ['curl', at('c1')], ['wget', at('w1')]]);
  assert.deepEqual(targets("python3 -c \"p='py1'; open(p, 'w')\""), [['code', at('py1')]]);
  assert.deepEqual(targets("node -e \"require('fs').writeFileSync('n1', '')\""), [['code', at('n1')]]);
  assert.deepEqual(targets('Set-Content -Path ps1 -Value x'), [['set-content', at('ps1')]]);
  assert.deepEqual(targets('git checkout main -- g1').map((t) => t[0]), ['git checkout']);
  assert.equal(writeTargets('git restore --source=v1 g1', { cwd: root })[0].source, 'v1');
});

test('shell: compound keywords, wrappers, bash -c, eval, find -exec and $(...) all reach the command inside', () => {
  for (const cmd of [
    'if true; then cp x k1; fi', 'for f in a; do cp x k1; done', 'while :; do cp x k1; done', '{ cp x k1; }', '! cp x k1',
    'sudo -u root cp x k1', 'env A=1 cp x k1', 'timeout 5 cp x k1', 'nice -n 5 cp x k1', 'bash -c "cp x k1"', "eval 'cp x k1'",
    'find . -exec cp {} k1 \\;', 'echo "$(cp x k1)"', 'echo `cp x k1`',
  ]) {
    assert.deepEqual(targets(cmd).map((t) => t[1]), [at('k1')], cmd);
  }
  assert.deepEqual(targets('command -v cp'), []);
});

test('shell: cd, pushd/popd, subshells and pipelines set the directory each command runs in', () => {
  const dirOf = (cmd) => parseCommands(cmd, { cwd: root, home: at('home') }).filter((c) => c.name === 'touch').map((c) => c.cwd);
  assert.deepEqual(dirOf('cd a && touch x; cd .. && touch y'), [at('a'), root]);
  assert.deepEqual(dirOf('(cd a && touch x); touch y'), [at('a'), root]);
  assert.deepEqual(dirOf('pushd a; touch x; popd; touch y'), [at('a'), root]);
  assert.deepEqual(dirOf('cd a | cat; touch y'), [root]);
  assert.deepEqual(dirOf('cd ~ && touch x; cd - && touch y'), [at('home'), root]);
  assert.deepEqual(dirOf('if true; then cd a; fi; touch x'), [at('a')]);
  assert.deepEqual(dirOf('cd "$UNKNOWN" && touch x'), [null]);
  assert.deepEqual(dirOf('bash -c "cd a"; touch x'), [root]);
});

test('shell: variables, ~, $HOME and git rev-parse expand; unknown values stay null', () => {
  const t = (cmd) => writeTargets(cmd, { cwd: root, home: at('home'), gitDir: at('.git'), top: at('top') }).map((x) => x.path);
  assert.deepEqual(t('D=a/b; echo x > $D/c'), [at('a', 'b', 'c')]);
  assert.deepEqual(t('echo x > ~/h'), [at('home', 'h')]);
  assert.deepEqual(t('echo x > "$HOME/h"'), [at('home', 'h')]);
  assert.deepEqual(t('echo x > "$(git rev-parse --git-common-dir)/temper/v"'), [at('.git', 'temper', 'v')]);
  assert.deepEqual(t('cd "$(git rev-parse --show-toplevel)" && echo > t'), [at('top', 't')]);
  assert.deepEqual(t('X=a cmd; echo > $X'), [null]); // a prefix assignment only sets cmd's environment
  assert.deepEqual(t('echo > *.toml'), [null]);
});

test('shell: copies report sources, and landing places each one', () => {
  const [cp] = writeTargets('cp -r a/x b/y c/', { cwd: root });
  assert.equal(cp.intoDir, true);
  assert.equal(cp.recursive, true);
  assert.deepEqual(cp.sources.map((s) => landing(cp, s)), [at('c', 'x'), at('c', 'y')]);
  const [mv] = writeTargets('mv a b', { cwd: root });
  assert.equal(landing(mv, mv.sources[0], false), at('b'));
  assert.equal(landing(mv, mv.sources[0], true), at('b', 'a'));
  const [t] = writeTargets('cp -t d a', { cwd: root });
  assert.equal(landing(t, t.sources[0]), at('d', 'a'));
  const [r] = writeTargets('rsync -a src/ dst', { cwd: root });
  assert.equal(landing(r, r.sources[0], true), at('dst'));
  assert.deepEqual(writeTargets('rsync -a x host:/y', { cwd: root }), []);
  assert.deepEqual(writeTargets('install -d a b', { cwd: root }), []);
  assert.equal(writeTargets('Copy-Item -Path a -Destination b -Recurse', { cwd: root })[0].recursive, true);
});

test('shell: patchCommand reads git apply, git am and patch; inspecting forms apply nothing', () => {
  const p = (cmd) => patchCommand(parseCommands(cmd, { cwd: root })[0]);
  assert.equal(p('cargo test'), null);
  for (const cmd of ['git apply --check x', 'git apply --stat x', 'git apply -h', 'git apply --help', 'patch --dry-run -p1', 'patch --version', 'patch -v', 'git am --abort']) {
    assert.equal(p(cmd).apply, false, cmd);
  }
  assert.deepEqual(p('git apply -p1 a.diff').inputs.map((w) => w.value), ['a.diff']);
  assert.deepEqual(p('git -C sub am a.mbox').inputs.map((w) => w.value), ['a.mbox']);
  assert.equal(p('git -C sub am a.mbox').cwd, at('sub'));
  assert.equal(p('git am --continue').stored, true);
  const pt = p('patch -d sub -p1 -i x.diff');
  assert.equal(pt.cwd, at('sub'));
  assert.deepEqual(pt.inputs.map((w) => w.value), ['x.diff']);
  assert.deepEqual(patchFiles('--- a/x.rs\t2026-01-01 00:00:00\n+++ b/x.rs\n'), ['a/x.rs', 'b/x.rs']);
});

test('shell: codeWriteTargets finds literals in write positions only', () => {
  assert.deepEqual(codeWriteTargets("open('w.txt', 'w'); open('r.txt').read()"), ['w.txt']);
  assert.deepEqual(codeWriteTargets("from pathlib import Path\np = Path('p.txt')\np.write_text('x')"), ['p.txt']);
  assert.deepEqual(codeWriteTargets("shutil.copy('a', 'dst')"), ['dst']);
  assert.deepEqual(codeWriteTargets('open(my $fh, ">", "perl.txt")'), ['perl.txt']);
  assert.deepEqual(codeWriteTargets("fs.writeFileSync('/tmp/o', fs.readFileSync('in'))"), ['/tmp/o']);
});

test('shell: resolvePath reads git-bash paths on Windows only', () => {
  assert.equal(resolvePath('/c/x', root, 'linux'), path.resolve('/c/x'));
  assert.equal(resolvePath('rel', null), null);
  assert.equal(resolvePath(null, root), null);
});

test('shell: temp directories ($TMPDIR, $TEMP, $env:TEMP, $(mktemp)) resolve under the temp dir; a mktemp in the current directory does not', () => {
  const tmp = path.resolve(os.tmpdir(), 'shell-test-tmp');
  const t = (cmd) => writeTargets(cmd, { cwd: root, tmp }).map((x) => x.path);
  const under = (p) => p !== null && !path.relative(tmp, p).startsWith('..') && !path.isAbsolute(path.relative(tmp, p));
  for (const cmd of [
    'echo x > "$TMPDIR/u"', 'echo x > ${TMP}/u', 'echo x > $TEMP/u', 'echo x > $env:TEMP/u', 'echo x > $Env:TMP/u',
    'echo x > "$(mktemp)"', 'echo x > "$(mktemp -d)/u"', 'D=$(mktemp -d); echo x > $D/u', 'cd "$(mktemp -d)" && echo x > u',
    'echo x > "$(mktemp -t temper.XXXX)"', 'echo x > `mktemp -d`/u',
  ]) {
    const [p] = t(cmd);
    assert.ok(under(p), `${cmd}: ${p}`);
  }
  assert.deepEqual(t('TMPDIR=/elsewhere; echo x > $TMPDIR/u'), [path.resolve('/elsewhere/u')]);
  for (const cmd of ['echo x > "$(mktemp ./x.XXXX)"', 'echo x > "$(mktemp -p . x.XXXX)"', 'echo x > $env:FOO/u', 'echo x > $OUT_DIR/u']) {
    assert.deepEqual(t(cmd), [null], cmd);
  }
});

test('shell: git rev-parse --path-format names the git dir too', () => {
  const t = (cmd) => writeTargets(cmd, { cwd: root, gitDir: at('.git') }).map((x) => x.path);
  assert.deepEqual(t('echo x > "$(git rev-parse --path-format=absolute --git-dir)/temper/v"'), [at('.git', 'temper', 'v')]);
  assert.deepEqual(t('echo x > "$(git rev-parse --path-format=relative --git-common-dir)/temper/v"'), [at('.git', 'temper', 'v')]);
  assert.deepEqual(t('echo x > "$(git rev-parse --path-format=absolute --show-prefix)/v"'), [null]);
});

test('shell: deeply nested find -exec stops at the depth limit instead of overflowing the stack', () => {
  const cmd = `${'find . -exec '.repeat(5000)}cp x k1 \\;`;
  assert.doesNotThrow(() => parseCommands(cmd, { cwd: root }));
  assert.deepEqual(targets('find . -exec find . -exec cp x k1 \\; \\;').map((t) => t[1]), [at('k1')]);
});

test('shell: a test command ([ or [[) is not read as a PowerShell expression; heredoc bodies are not code of the line', () => {
  assert.deepEqual(targets("[ -d docs ] || mkdir docs; cat <<'EOF'\nPath(\".cargo/config.toml\").write_text(x)\nEOF"), []);
  assert.deepEqual(targets("[[ -n x ]] && echo \"open('w.txt', 'w')\""), []);
  assert.deepEqual(targets("$x = 1\ncat <<'EOF'\nopen('w.txt', 'w')\nEOF"), []);
  // PowerShell expressions still count, with a variable assigned earlier in the line.
  const once = (cmd) => [...new Set(targets(cmd).map((t) => t.join('|')))];
  assert.deepEqual(once("$p = 'ps.txt'; [IO.File]::WriteAllText($p, 'x')"), [`code|${at('ps.txt')}`]);
  assert.deepEqual(once("[System.IO.File]::WriteAllText('ps2.txt', 'x')"), [`code|${at('ps2.txt')}`]);
});

test('shell: code and word scans stay roughly linear on pathological input', () => {
  const time = (fn) => { const t0 = performance.now(); fn(); return performance.now() - t0; };
  for (const code of [
    `copy( ${'x '.repeat(20000)}`, `cp(${'a'.repeat(40000)}`, `copy(${' '.repeat(40000)}`, `"""${'\\"'.repeat(20000)}`,
    `'${'a'.repeat(40000)}`, `"""${'x'.repeat(40000)}`, `${'"'.repeat(20000)}x`,
  ]) {
    assert.ok(time(() => codeWriteTargets(code)) < 1000, code.slice(0, 20));
  }
  assert.ok(time(() => writeTargets(`echo {${','.repeat(40000)}`, { cwd: root })) < 1000, 'brace list');
  assert.ok(time(() => writeTargets(`$x copy( ${'x '.repeat(20000)}`, { cwd: root })) < 1000, 'PowerShell line');
  // The literal pass still reads escapes and both quote kinds.
  assert.deepEqual(codeWriteTargets("open(\"a\\\"b\", 'w'); open('c', \"w\")"), ['a\\"b', 'c']);
});

test('shell: pathspecPattern spots wildcards and magic in git checkout/restore paths', () => {
  for (const raw of ['clippy.toml', '.cargo/config.toml', 'docs']) assert.equal(pathspecPattern({ raw }), false, raw);
  for (const raw of ['*.toml', '.cargo/conf?g.toml', '[a-c]lippy.toml', ':(glob)**/clippy.toml', ':/clippy.toml', ':!docs', ':^docs']) {
    assert.equal(pathspecPattern({ raw }), true, raw);
  }
});
