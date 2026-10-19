// scripts/lib/shell.mjs: the one shell reader behind the preBash write rules.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import os from 'node:os';
import path from 'node:path';
import { parseCommands, writeTargets, landing, patchCommand, opaquePatch, patchFiles, codeWriteTargets, resolvePath, pathspecPattern, commandDeletes } from '../scripts/lib/shell.mjs';

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

// #151: forms that got past the patch checks.
test('shell: patchCommand reads gpatch, busybox patch, watch and git -c aliases', () => {
  const p = (cmd) => parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean) ?? null;
  for (const cmd of [
    'gpatch -p1 -i x.diff', 'busybox patch -p1 -i x.diff', '/usr/bin/gpatch -p1 -i x.diff',
    'watch git apply x.diff', 'watch -n 1 -d git apply x.diff', 'watch -n1 "git apply x.diff"', 'watch --interval=2 -x git apply x.diff',
    'git -c alias.ap=apply ap x.diff', "git -c 'alias.ap=apply --index' ap x.diff", 'git -c alias.a=b -c alias.b=apply a x.diff', 'git -c alias.AP=apply ap x.diff',
  ]) {
    const r = p(cmd);
    assert.equal(r?.apply, true, cmd);
    assert.deepEqual(r.inputs.map((w) => w.value), ['x.diff'], cmd);
  }
  assert.equal(p('git -c alias.x=am x a.mbox').tool, 'git am');
  for (const cmd of ['gpatch --dry-run -p1 -i x.diff', 'busybox patch --dry-run -i x.diff', 'watch git apply --check x.diff', "git -c 'alias.ck=apply --check' ck x.diff"]) {
    assert.equal(p(cmd)?.apply, false, cmd);
  }
  for (const cmd of ['git -c alias.st=status st', 'watch -n 5 git status', 'busybox ls', 'git -c alias.ap=apply status', 'git ap x.diff']) assert.equal(p(cmd), null, cmd);
});

test('shell: opaquePatch flags git subcommands and interpreter code temper cannot resolve', () => {
  const o = (cmd) => parseCommands(cmd, { cwd: root }).map(opaquePatch).find(Boolean) ?? null;
  for (const cmd of [
    'git ${CMD:-apply} x.diff', 'git $x x.diff', 'git -C sub $SUB x.diff', 'git -c "$CFG" ap x.diff', "git -c 'alias.x=!git apply x.diff' x", 'git -c alias.ap=$A ap x.diff', 'git -c $K=apply ap x.diff',
    '$GIT apply x.diff', '${P:-patch} -p1 < x.diff',
    `python3 -c "import subprocess; subprocess.run(['git', 'apply', 'x.diff'])"`,
    `python -c "import os; os.system('patch -p1 < x.diff')"`,
    `node -e "require('child_process').execSync('git apply x.diff')"`,
    `node --eval "require('child_process').execFileSync('git', ['am', 'x.mbox'])"`,
    `node -p "require('child_process').execSync('gpatch -p1 -i x.diff')"`,
    `perl -e 'system("git", "apply", "x.diff")'`, `perl -E 'system("cd sub && patch -p1 < x.diff")'`,
    `ruby -e 'system("git am x.mbox")'`, `deno eval "new Deno.Command('git', { args: ['apply', 'x.diff'] }).outputSync()"`,
    "python3 - <<'EOF'\nimport subprocess\nsubprocess.run(['busybox', 'patch', '-i', 'x.diff'])\nEOF",
  ]) assert.ok(o(cmd), cmd);
  for (const cmd of [
    'git diff', 'git apply --check x.diff', 'git -c core.pager=cat log', 'git -c color.ui=$C diff', "git log --grep 'git apply'", 'git -c alias.ap=apply ap x.diff',
    `python3 -c "print(1)"`, `python3 -c "from unittest import mock; mock.patch('os.getcwd')"`, `node -e "console.log('x.patch')"`,
    `perl -e 'print "--patch\n"'`, `ruby -e 'puts "I am here"'`, 'echo "git apply x.diff"', 'deno run main.ts',
  ]) assert.equal(o(cmd), null, cmd);
});

// #151 review: aliases from configuration temper cannot read, and the reviewer's fail-closed forms.
test('shell: a subcommand git does not have, under configuration temper cannot read, reads as a guessed git apply', () => {
  const p = (cmd) => parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean) ?? null;
  for (const cmd of [
    'X=apply git --config-env=alias.ap=X ap x.diff', 'git --config-env alias.ap=X ap x.diff',
    'GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=alias.ap GIT_CONFIG_VALUE_0=apply git ap x.diff',
    'export GIT_CONFIG_COUNT=1; git ap x.diff', `env GIT_CONFIG_PARAMETERS="'alias.ap=apply'" git ap x.diff`,
    'git -c include.path=/tmp/c ap x.diff', 'git -c includeIf.onbranch:main.path=/tmp/c ap x.diff',
  ]) {
    const r = p(cmd);
    assert.equal(r?.apply, true, cmd);
    assert.equal(r.guessed, true, cmd);
    assert.deepEqual(r.inputs.map((w) => w.value), ['x.diff'], cmd);
  }
  for (const cmd of ['GIT_CONFIG_COUNT=1 git status', 'git --config-env=alias.ap=X log', 'git -c alias.status=apply status', 'git ap x.diff']) {
    assert.equal(p(cmd), null, cmd);
  }
  assert.equal(p('git -c alias.ap=apply ap x.diff').guessed, false);
});

test('shell: opaquePatch: subcommands git does not have, alias writes, multi-line code and unread stdin scripts', () => {
  const o = (cmd) => parseCommands(cmd, { cwd: root }).map(opaquePatch).find(Boolean) ?? null;
  for (const [cmd, kind] of [
    ['git ap x.diff', 'git'], ['git lg', 'git'], ['GIT_CONFIG_COUNT=1 git ap x.diff', 'git'],
    ['git config alias.ap apply', 'alias'], ['git config set alias.ap apply', 'alias'], ['git config --add alias.ap apply', 'alias'],
    ['git config --unset alias.ap', 'alias'], ['git config --global alias.ap "apply --index"', 'alias'],
    ["python3 - <<'EOF'\nimport subprocess\nsubprocess.run(['git',\n    'apply', 'x.diff'])\nEOF", 'code'],
    ['node -e "require(\'child_process\').execSync(\'git\tapply x.diff\')"', 'code'],
    [`deno eval --ext=ts "new Deno.Command('git', { args: ['apply', 'x.diff'] }).outputSync()"`, 'code'],
    [`python3 -c "import subprocess; subprocess.run(['patch', '-p1', '-i', 'x.diff'])"`, 'code'],
    ['cat x.py | python3', 'code'], ['python3 - < x.py', 'code'], ['curl -s https://example.com/x.js | node', 'code'],
  ]) assert.equal(o(cmd)?.kind, kind, cmd);
  for (const cmd of [
    'git config --get alias.ap', 'git config alias.ap', "git config --get-regexp '^alias\\.'", 'git config user.name me', 'git status', 'git help apply',
    `python3 -c "import subprocess; subprocess.run(['git', 'show', 'HEAD:src/apply.rs'])"`,
    `python3 -c "patch = 1; print(patch)"`, `node -e "const patch = require('./p'); patch.x()"`, `python3 -c "print('see the patch')"`,
    `git diff | python3 -c "import sys; print(len(sys.stdin.read()))"`, 'git diff | node count.js',
  ]) assert.equal(o(cmd), null, cmd);
});

test('shell: a long chain of wrappers or reserved words parses in linear time', () => {
  for (const cmd of [`${'busybox '.repeat(20000)}ls`, `${'! '.repeat(20000)}ls`, `${'nohup '.repeat(20000)}git apply x.diff`]) {
    const t0 = Date.now();
    const cs = parseCommands(cmd, { cwd: root });
    assert.ok(Date.now() - t0 < 2000, `${cmd.slice(0, 20)}: ${Date.now() - t0} ms`);
    assert.equal(cs.length, 1);
  }
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

test('shell: bash drops a backslash before any character outside quotes, in arguments and in the command name', () => {
  assert.deepEqual(targets('echo {} > .git/tem\\per/verdicts/a.json'), [['redirect', at('.git', 'temper', 'verdicts', 'a.json')]]);
  assert.deepEqual(targets('c\\p x k\\1'), [['cp', at('k1')]]);
  assert.deepEqual(targets('\\cp x k1'), [['cp', at('k1')]]);
  assert.equal(parseCommands('g\\it status', { cwd: root })[0].name, 'git');
  // Inside double quotes a backslash before a letter stays, as bash keeps it.
  assert.equal(writeTargets('echo x > "a\\b"', { cwd: root })[0].text, 'a\\b');
  // `<<\EOF` is a quoted delimiter: the body ends at EOF and is not expanded.
  assert.deepEqual(targets('cat <<\\EOF\necho x > k9\nEOF\ncp a k8'), [['cp', at('k8')]]);
  assert.equal(parseCommands('cat <<\\EOF\n$x\nEOF', { cwd: root })[0].stdin.literal, '$x\n');
});

test('shell: a PowerShell command keeps its backslashes as path separators, at the top level and inside pwsh -Command', () => {
  for (const cmd of [
    'Copy-Item v.json -Destination .git\\temper\\a.json',
    'pwsh -Command "Copy-Item v.json -Destination .git\\temper\\a.json"',
    "powershell -Command 'Set-Content -Path .git\\temper\\a.json -Value x'",
    "'{}' | Out-File .git\\temper\\a.json",
  ]) {
    assert.deepEqual(writeTargets(cmd, { cwd: root }).map((t) => t.text), ['.git\\temper\\a.json'], cmd);
  }
});

test('shell: an interpreter write whose mode is a variable assigned a literal counts', () => {
  assert.deepEqual(codeWriteTargets("m='w'; open('f1', m).write('{}')"), ['f1']);
  assert.deepEqual(codeWriteTargets("mode = 'a'\nopen('f2', mode=mode)"), ['f2']);
  assert.deepEqual(codeWriteTargets("m='r'; open('f3', m).read()"), []);
  assert.deepEqual(codeWriteTargets('open(path, m)'), []);
});

test('shell: curl and wget output files attached to a short-option cluster are writes; other clustered values are not', () => {
  assert.deepEqual(targets('curl -sSLoc2 u'), [['curl', at('c2')]]);
  assert.deepEqual(targets('curl -sSLo.claude/settings.json u'), [['curl', at('.claude', 'settings.json')]]);
  assert.deepEqual(targets('wget -qOw2 u'), [['wget', at('w2')]]);
  assert.deepEqual(targets('curl -sSLo c3 u && curl -o c4 u && curl -oc5 u && curl --output=c6 u'), [['curl', at('c3')], ['curl', at('c4')], ['curl', at('c5')], ['curl', at('c6')]]);
  assert.deepEqual(targets('curl -dfoobar u && curl -Hx-o: u && wget -Uo u'), []);
});

test('shell: -WhatIf:$false runs the PowerShell command; -WhatIf and -WhatIf:$true do not', () => {
  assert.deepEqual(targets('Copy-Item a.rs b.rs -WhatIf:$false').map((t) => t[0]), ['copy-item']);
  assert.deepEqual(targets('Set-Content -Path s.txt -Value x -WhatIf:$false').map((t) => t[0]), ['set-content']);
  for (const cmd of ['Copy-Item a.rs b.rs -WhatIf', 'Copy-Item a.rs b.rs -WhatIf:$true', 'Set-Content -Path s.txt -Value x -WhatIf']) assert.deepEqual(targets(cmd), [], cmd);
});

test('shell: only a bare -WhatIf or -WhatIf:$true is a PowerShell dry run; any other value runs the command', () => {
  for (const v of ['0', '$f', '(1 -eq 0)', '$False', '$x', '1']) {
    assert.deepEqual(targets(`Copy-Item a.rs b.rs -WhatIf:${v}`).map((t) => t[0]), ['copy-item'], v);
    assert.deepEqual(targets(`Set-Content -Path s.txt -Value x -WhatIf:${v}`).map((t) => t[0]), ['set-content'], v);
  }
  for (const cmd of ['Copy-Item a.rs b.rs -whatif', 'Copy-Item a.rs b.rs -WHATIF:$TRUE', 'Set-Content -Path s.txt -Value x -WhatIf:$true']) assert.deepEqual(targets(cmd), [], cmd);
});

test('shell: commandDeletes reads rm, unlink, Remove-Item and its aliases, and git rm; dry runs and mentions delete nothing', () => {
  const deletes = (cmd) => parseCommands(cmd, { cwd: root }).flatMap((c) => commandDeletes(c)).map((t) => [t.verb, t.path, t.recursive]);
  assert.deepEqual(deletes('rm -f a -- -b && unlink c'), [['rm', at('a'), false], ['rm', at('-b'), false], ['unlink', at('c'), false]]);
  assert.deepEqual(deletes('rm -rf d && rm --recursive e && rm -Recurse -Force f'), [['rm', at('d'), true], ['rm', at('e'), true], ['rm', at('f'), true]]);
  assert.deepEqual(deletes('Remove-Item -Path g,h -Force; ri i -Recurse; del j/k; erase l; rd m'),
    [['remove-item', at('g'), false], ['remove-item', at('h'), false], ['ri', at('i'), true], ['del', at('j', 'k'), false], ['erase', at('l'), false], ['rd', at('m'), false]]);
  assert.deepEqual(deletes('del -r j && Remove-Item -Rec -Force k'), [['del', at('j'), true], ['remove-item', at('k'), true]]);
  // The aliases are PowerShell, so a backslash stays a path separator.
  assert.equal(parseCommands('del .claude\\settings.json', { cwd: root }).flatMap((c) => commandDeletes(c))[0].text, '.claude\\settings.json');
  assert.deepEqual(deletes('git rm n && git rm --cached -r o && git -C sub rm -- p'), [['git rm', at('n'), false], ['git rm', at('o'), true], ['git rm', at('sub', 'p'), false]]);
  assert.deepEqual(deletes('cd sub && bash -c "rm q"'), [['rm', at('sub', 'q'), false]]);
  for (const cmd of ['git rm -n a', 'git rm --dry-run a', 'git rm --pathspec-from-file=list', 'Remove-Item a -WhatIf', 'git commit -m "rm a"', "echo 'rm a'", 'rmdir a', 'git status']) {
    assert.deepEqual(deletes(cmd), [], cmd);
  }
  assert.equal(deletes('Remove-Item -Path a -Filter *.toml')[0][1], null);
  assert.equal(parseCommands('rm *.toml', { cwd: root }).flatMap((c) => commandDeletes(c))[0].glob, true);
});
