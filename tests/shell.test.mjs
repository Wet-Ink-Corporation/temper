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
    // The shell passes `\t` and `\n` through as escapes the interpreter reads as whitespace.
    ['node -e "require(\'child_process\').execSync(\'git\\tapply x.diff\')"', 'code'],
    [`python3 -c "import os; os.system('git\\napply x.diff')"`, 'code'],
    [`python3 -c "import os; os.system('/usr/bin/patch -p1 < x.diff')"`, 'code'],
    [`python3 -c "import subprocess; subprocess.run(['patch', 'src/lib.rs', 'fix.patch'])"`, 'code'],
    [`python3 -c "import subprocess; subprocess.run(['git', '-C', 'sub', 'am', 'x.mbox'])"`, 'code'],
    ['git-diff HEAD', 'git'], ['/usr/lib/git-core/git-log -1', 'git'],
    [`git ${Array.from({ length: 10 }, (_, k) => `-c alias.a${k}=a${k + 1}`).join(' ')} -c alias.a10=apply a0 x.diff`, 'git'],
    [`deno eval --ext=ts "new Deno.Command('git', { args: ['apply', 'x.diff'] }).outputSync()"`, 'code'],
    [`python3 -c "import subprocess; subprocess.run(['patch', '-p1', '-i', 'x.diff'])"`, 'code'],
    ['cat x.py | python3', 'code'], ['python3 - < x.py', 'code'], ['curl -s https://example.com/x.js | node', 'code'],
  ]) assert.equal(o(cmd)?.kind, kind, cmd);
  for (const cmd of [
    'git config --get alias.ap', 'git config alias.ap', "git config --get-regexp '^alias\\.'", 'git config user.name me', 'git status', 'git help apply',
    `python3 -c "import subprocess; subprocess.run(['git', 'show', 'HEAD:src/apply.rs'])"`,
    `python3 -c "patch = 1; print(patch)"`, `node -e "const patch = require('./p'); patch.x()"`, `python3 -c "print('see the patch')"`,
    `git diff | python3 -c "import sys; print(len(sys.stdin.read()))"`, 'git diff | node count.js',
    `python3 -c "print(['patch', 'minor', 'major'])"`, `python3 -c "import subprocess; subprocess.run(['git', 'log', '--grep', 'apply'])"`,
  ]) assert.equal(o(cmd), null, cmd);
});

test('shell: the dashed forms git-apply and git-am, at any path, read as git apply and git am', () => {
  const p = (cmd) => parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean) ?? null;
  for (const cmd of ['git-apply x.diff', '/usr/lib/git-core/git-apply x.diff', '"$(git --exec-path)/git-apply" x.diff', 'sudo git-apply -p1 x.diff']) {
    const r = p(cmd);
    assert.equal(r?.tool, 'git apply', cmd);
    assert.equal(r.apply, true, cmd);
    assert.deepEqual(r.inputs.map((w) => w.value), ['x.diff'], cmd);
  }
  assert.equal(p('git-am x.mbox').tool, 'git am');
  assert.equal(p('/usr/lib/git-core/git-apply --check x.diff').apply, false);
  assert.equal(p('git-diff HEAD'), null);
});

// #151 review 4 (wi-0e226b): echo or printf output is not emulated. Text piped into a shell is
// a script temper cannot read exactly, which opaquePatch flags for the reviewer.
test('shell: echo or printf piped into a shell is not read as the script, and opaquePatch flags it', () => {
  const p = (cmd) => parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean) ?? null;
  const o = (cmd) => parseCommands(cmd, { cwd: root }).map(opaquePatch).find(Boolean) ?? null;
  for (const cmd of [
    "echo 'git apply x.diff' | bash", "printf 'git apply x.diff\\n' | sh", 'echo git apply x.diff | bash -s', "echo 'git apply x.diff' | bash -s -- a b",
    "echo -e 'true\\ngit apply x.diff' | sh", "printf '%s\\n' 'git apply x.diff' | bash", "echo 'git apply x.diff' |& sh",
    "X=x.diff; echo \"git apply $X\" | bash", "echo 'git apply x.diff' > out.txt | bash",
  ]) {
    assert.equal(p(cmd), null, cmd);
    assert.equal(o(cmd)?.kind, 'script', cmd);
  }
  assert.equal(p("echo 'git apply x.diff' | grep apply"), null);
  assert.equal(o("echo 'git apply x.diff' | grep apply"), null);
  // Heredoc and here-string bodies are read exactly, into a shell or through cat.
  for (const cmd of ["bash <<'EOF'\ngit apply x.diff\nEOF", "bash <<< 'git apply x.diff'", "cat <<'EOF' | bash\ngit apply x.diff\nEOF"]) {
    assert.deepEqual(p(cmd)?.inputs.map((w) => w.value), ['x.diff'], cmd);
  }
  for (const cmd of ["bash <<'EOF'\nls\nEOF", "sh <<< 'cargo test'", "cat <<'EOF' | bash\nls\nEOF", "cat - <<< ls | sh"]) assert.equal(o(cmd), null, cmd);
  // A body another command transforms is not the script the shell runs.
  assert.equal(o("tr a-z A-Z <<'EOF' | bash\nls\nEOF")?.kind, 'script');
});

// #151 review 4 (wi-0e226b): a shell or source whose script temper cannot read exactly.
test('shell: opaquePatch flags a shell or source reading /dev/stdin, a process substitution, xargs input or a subshell pipe', () => {
  const o = (cmd) => parseCommands(cmd, { cwd: root }).map(opaquePatch).find(Boolean) ?? null;
  for (const cmd of [
    'cat cmd.sh | bash /dev/stdin', "echo ls | sh /proc/self/fd/0", 'cat cmd.sh | bash /dev/fd/0', 'cat cmd.sh | bash /dev/./stdin',
    "bash <(echo 'git apply x.diff')", 'sh <(cat cmd.sh)', "source <(echo 'git apply x.diff')", ". <(echo ls)",
    'cat cmd.sh | source /dev/stdin', 'cat cmd.sh | . /dev/stdin', 'source', '.',
    "echo ls | tee >(bash)", "echo ls | tee >(bash) >/dev/null", "echo ls | tee >(sh -s)",
    "echo ls | xargs -I{} bash -c {}", 'echo ls | xargs -0 bash -c', 'echo cmd.sh | xargs bash', 'echo cmd.sh | xargs -I{} sh {}',
    "(echo 'git apply x.diff'; echo ls) | bash", "echo ls | ( bash )", "echo ls | { true; bash; }", 'bash',
    "for i in 1; do echo ls; done | bash", "echo ls | cat | bash", "echo ls | env bash", "echo ls | timeout 5 bash", "echo ls | watch bash",
  ]) assert.equal(o(cmd)?.kind, 'script', cmd);
  for (const cmd of [
    'bash script.sh', 'source ./env.sh', '. ./env.sh', "bash -c 'ls'", 'echo foo | sh -c cat', 'git diff | cat', "echo ls | xargs bash -c 'echo $0'",
    'echo a | xargs bash script.sh', 'bash --version', 'sh --help', "bash -c 'cat' < in.txt",
  ]) assert.equal(o(cmd), null, cmd);
});

// #151 review 5 (wi-0e226b): a -c script, a here-string, a heredoc, an eval argument or a command
// name built from an expansion is text temper cannot read exactly.
test('shell: opaquePatch flags a -c script, here-string, heredoc, eval argument or command name that is not literal', () => {
  const o = (cmd) => parseCommands(cmd, { cwd: root }).map(opaquePatch).find(Boolean) ?? null;
  for (const cmd of [
    'bash -c "$(cat cmd.sh)"', 'sh -c "`cat x`"', 'bash -c "$CMD"', 'X=ls; bash -c "$X"', 'bash -c', 'bash -c "ls; $(cat cmd.sh)"',
    'bash <<< "$(cat cmd.sh)"', 'bash <<< "$X"', 'bash <<< `cat x`', 'X=ls; bash <<< "$X"', 'bash <<EOF\n$(cat cmd.sh)\nEOF',
    'bash <<EOF\ngit $X\nEOF', 'cat <<EOF | bash\n$X\nEOF', 'cat <<< "$X" | bash',
    'eval `cat cmd.txt`', 'eval "$(cat cmd.txt)"', 'X=ls; eval "$X"', 'xargs -0 -n1 eval < cmd.sh', 'while read -r l; do eval "$l"; done < cmd.sh',
  ]) assert.equal(o(cmd)?.kind, 'script', cmd);
  // A command name temper cannot read is its own kind (#151 review 6).
  for (const cmd of ['X=$(cat cmd.txt); $X', '$X x.diff', '"$CMD" x.diff', '${X:-ls}', '$(cat cmd.txt)', 'X=ls; X+=";git apply x.diff"; $X']) {
    assert.equal(o(cmd)?.kind, 'name', cmd);
  }
  // An interpreter's stdin is read the same way.
  for (const cmd of ['python3 <<< "$X"', 'python3 - <<EOF\nprint("$(cat x.py)")\nEOF']) assert.equal(o(cmd)?.kind, 'code', cmd);
  for (const cmd of [
    "bash -c 'ls'", "bash -lc 'cargo test 2>&1 | tail -n 20'", "bash -euo pipefail -c 'cargo test'", "bash -c -- 'cargo test'",
    "bash <<< 'cargo test'", 'bash <<< ls', "bash <<'EOF'\ncargo test\nEOF", 'bash <<EOF\ncargo test\nEOF', "cat <<'EOF' | bash\nls\nEOF",
    "eval 'ls'", 'eval ls -la', "python3 - <<'EOF'\nprint(1)\nEOF", 'echo "$HOME"', '~/.cargo/bin/cargo test', 'cargo test -- "$X"',
    "find crates -name '*.rs' | head", 'X=1; echo $X',
  ]) assert.equal(o(cmd), null, cmd);
});

// #151 review 6 (wi-0e226b): a brace list or a glob is an expansion too; a command name is read
// when temper knows its value; interpreter code built from an expansion is code temper cannot read.
test('shell: opaquePatch: brace and glob words, command names temper reads, and interpreter code from an expansion', () => {
  const o = (cmd, opts = {}) => parseCommands(cmd, { cwd: root, ...opts }).map(opaquePatch).find(Boolean) ?? null;
  for (const [cmd, kind] of [
    ['{git,apply,doc.diff}', 'name'], ['eval {git,apply,doc.diff}', 'script'], ["bash -c {'git apply doc.diff',x}", 'script'],
    ["{bash,-c,'git apply doc.diff'}", 'name'], ["{eval,'git apply doc.diff'}", 'name'], ['{patch,-p1,-i,doc.diff}', 'name'],
    ['eval git*', 'script'], ["bash -c gi?' apply doc.diff'", 'script'], ["eval {'git apply',x}", 'script'], ["eval {'git apply'\\,x}", 'script'],
    ["sh -c {x,'git apply doc.diff'}", 'script'], ['git* apply doc.diff', 'name'], ['gi? apply doc.diff', 'name'], ["bash <<< {x,'git apply y'}", 'script'],
    ["X='git apply x'; $X", 'name'], ["X='git apply x'; \"$X\"", 'name'], ['X=g*; $X', 'name'], ['IFS=/; X=git/apply/x.diff; $X', 'name'],
    ['"${CARGO:-cargo}" test', 'name'], ['$(which cargo) test', 'name'], ['$HOME/.cargo/bin/cargo test', 'name'], ['$PWD/target/debug/a', 'name'],
    ['python3 -c "$X"', 'code'], ['node -e "$(cat f)"', 'code'], ['perl -e "$X"', 'code'], ['perl -ne "$X" f', 'code'], ['ruby -e "$X"', 'code'],
    ['php -r "$X"', 'code'], ['deno eval "$X"', 'code'],
  ]) assert.equal(o(cmd)?.kind, kind, cmd);
  for (const cmd of [
    "find crates -name '*.rs' | head", 'cargo test -- --nocapture', "jq '{a:.b}' x.json", "git log --format='%H' -5", "rg 'foo\\{' crates",
    'rg foo\\{ crates', 'find crates -name x -exec grep -n unwrap {} +', 'git show HEAD@{1} --stat', "eval 'git status'", 'ls crates/*/src',
    '"$HOME/.cargo/bin/cargo" test', '"$PWD/target/debug/a" --help', '~/.cargo/bin/cargo test',
    '"$(git rev-parse --show-toplevel)/scripts/check.sh"', "python3 -c 'import json, sys; print(json.load(sys.stdin))' < x.json",
    'python3 -c "print(1)"', "node -e 'console.log(1)'", "perl -ne 'print if /unwrap/' x.rs",
  ]) assert.equal(o(cmd, { top: root }), null, cmd);
});

// #151 review 7 (wi-0e226b): temper's variable model is not bash (`read`, `printf -v`, arrays, an
// assignment that never runs, ...), so for the reviewer a command name is never read from a tracked
// assignment: only a literal, or (review 8, wi-ec88b9) quoted expansions followed by a literal
// `/name`, which name the command whatever their value. The words of a wrapper before it are read the same way.
test('shell: opaquePatch never reads a command name from a variable', () => {
  const o = (cmd) => parseCommands(cmd, { cwd: root, top: root, untracked: true }).map(opaquePatch).find(Boolean) ?? null;
  for (const cmd of [
    // A value from an assignment: never read, whatever temper's model says it is.
    'X=cargo; $X test', 'X=cargo; export X; $X test', 'X=git; $X status', "X='git'; X=\"${X}\"; $X status",
    // Builtins that write a variable temper does not follow, and values it would read wrong.
    'X=cargo; read X <<< git; $X apply x.diff', 'X=cargo; printf -v X git; $X apply x.diff', 'X=cargo; mapfile -t X <<< git; $X apply x.diff',
    'Y=git; X=cargo; declare -n X=Y; $X apply x.diff', 'X=cargo; X[0]=git; $X apply x.diff', 'X=cargo; X=(git); $X apply x.diff',
    'X=cargo; ${X/cargo/git} apply x.diff', 'X=GIT; ${X,,} apply x.diff', 'Y=git; X=Y; ${!X} apply x.diff', 'X=cargo; unset X; $X git apply x.diff',
    'X=; $X git apply x.diff', "X=''; $X git apply x.diff", '$X git apply x.diff', 'set -- git; "$@" apply x.diff', 'X=cargo; : ${X:=git}; $X test',
    // Assignments that never run, or run elsewhere.
    'X=git; false && X=cargo; $X apply x.diff', 'X=git; if false; then X=cargo; fi; $X apply x.diff', 'X=git; f() { X=cargo; }; $X apply x.diff',
    'X=git; { X=cargo; } | cat; $X apply x.diff', 'X=git; { X=cargo; } & $X apply x.diff', 'X=git\nX=cargo &\n$X apply x.diff',
    'X=git; case a in b) X=cargo;; esac; $X apply x.diff', 'X=git; local X=cargo 2>/dev/null; $X apply x.diff', 'X=cargo; time X=git; $X apply x.diff',
    'X=cargo; X=git eval :; $X apply x.diff', 'X=cargo; X=git export Y; $X apply x.diff', 'X=cargo; source ./set.sh; $X apply x.diff',
    'X=gitxapply; IFS=x; $X x.diff', 'X=gitxapply; read IFS <<< x; $X x.diff', "X=cargo; trap 'X=git' DEBUG; $X apply x.diff",
    // A name the model would read as cd, export or a wrapper, and a wrapper's word built from an expansion.
    'X=cd; $X apply x.diff', 'X=export; $X apply x.diff', 'X=nice; $X git apply x.diff', 'timeout $T apply x.diff', 'nice -n $N apply x.diff',
    'timeout "$T" 5 git apply x.diff', 'env A=$X apply x.diff', 'watch -n $N apply x.diff',
    "alias cargo='git apply'", 'hash -p /usr/bin/git cargo',
    // An unquoted expansion, or one not followed by a literal /name.
    'export HOME=/x; $HOME/cargo test', 'unset HOME; $HOME/cargo test', ': ${HOME:=/x}; $HOME/cargo test', 'for HOME in /x; do $HOME/cargo test; done',
    'IFS=/; $HOME/cargo test', 'read IFS <<< /; $HOME/cargo test', '$HOME/cargo test', '$PWD/x',
    '$HOME', '"$HOME"', '"$HOME/"', '"$HOME/x$Y"', '$HOME/{x,y}', '$(git rev-parse --show-toplevel)/x',
  ]) assert.equal(o(cmd)?.kind, 'name', cmd);
  for (const cmd of [
    '"$HOME/.cargo/bin/cargo" test', '"${HOME}"/.cargo/bin/cargo test',
    '"$PWD/target/debug/a" --help', '"$(git rev-parse --show-toplevel)/scripts/check.sh"', '~/.cargo/bin/cargo test',
    // Review 8 (wi-ec88b9): quoted expansions followed by a literal /name, whatever the line writes, name that command.
    'X=/usr/bin; "$X/cargo" test', 'read HOME <<< /x; "$HOME/cargo" test', 'printf -v HOME /x; "$HOME/cargo" test', 'HOME=/x; "$HOME/cargo" test',
    'declare -n R=HOME; "$HOME/cargo" test', 'Y=ME; read HO$Y <<< /x; "$HOME/cargo" test', "mapfile -t HO''ME <<< /x; \"$HOME/cargo\" test",
    'cd /tmp; "$PWD/x"', 'pushd /tmp; "$PWD/x"', 'source ./set.sh; "$HOME/cargo" test', '(( HOME = 1 )); "$HOME/cargo" test',
    '"$HOME$PWD/x"', '"$TMPDIR/x"', '"$OLDPWD/x"', '"$(git rev-parse --show-toplevel)$X/x"', '"${HOME:-/x}/cargo" test', '"$(git rev-parse --git-dir)/x"',
    'echo "$HOME"; "$HOME/.cargo/bin/cargo" test', 'X=1; echo $X; "$HOME/.cargo/bin/cargo" test', 'cd crates && cargo test', 'timeout 60 cargo test',
    'nice -n 5 cargo test', 'env A="$X" cargo test', 'X=crates; ls $X', '"$HOME/My Tools/x" --help',
    // An interpreter's -c or -e after its script or module is the script's own argument (#151 review 7).
    // python's, that is: review 8 reads node's, ruby's and the others' code options in every argument.
    'python3 tool.py -c "$CFG"', 'python3 -m pytest -p "$PLUGIN"', 'python3 -u tool.py -c "$CFG"',
    'deno run x.ts -e "$X"', 'python3 -c "import sys; print(sys.argv)" "$X"',
    // A shell's or source's script operand from quoted expansions followed by a literal path.
    'source "$HOME/.cargo/env"', 'bash "$CLAUDE_PLUGIN_ROOT/x.sh"',
    'PATH+=":$HOME/.cargo/bin"; cargo test',
  ]) assert.equal(o(cmd), null, cmd);
  // A script operand temper cannot name exactly may be /dev/stdin.
  for (const cmd of ['F=/dev/stdin; bash "$F" < cmd.sh', 'read HOME <<< /dev; bash "$HOME/stdin" < cmd.sh', 'bash "$X"', '. "$X"']) {
    assert.equal(o(cmd)?.kind, 'script', cmd);
  }
  // Code before the script is still read: an option's value is not the script.
  for (const cmd of ['python3 -W ignore -c "$X"', 'python3 -X dev -c "$X"', 'node -r ./x.js -e "$X"', 'node --require ./x.js -e "$X"', 'perl -pi -e "$X" x',
    'python3 $O "$X"', 'python3 "$O" "$X"', 'python3 -E -c "$X"', 'python3 -Ic "$X"', 'perl -W -e "$X"', 'ruby -W -e "$X"', 'perl -I lib -e "$X"',
    // perl's -W is a flag, so the code after it is read.
    `perl -W -e 'system("git apply x.diff")'`]) {
    assert.equal(o(cmd)?.kind, 'code', cmd);
  }
});

// #151 review 8 (wi-ec88b9): a command name, wrapper word or script operand is read when every
// expansion in it is double-quoted and it ends in a literal `/name` segment. Bash looks up no PATH,
// alias or function for a name with a slash, so that segment is the name. An unquoted expansion is never read.
test('shell: a quoted expansion followed by a literal /name is read as that name', () => {
  const ps = (cmd) => parseCommands(cmd, { cwd: root, top: root, untracked: true });
  const o = (cmd) => ps(cmd).map(opaquePatch).find(Boolean) ?? null;
  for (const [cmd, tool] of [
    ['"$X/git" apply x.diff', 'git apply'], ['"$X/git-apply" x.diff', 'git apply'], ['"$X/patch" -p1 < x.diff', 'patch'],
    ['"$X"/../git am x.mbox', 'git am'], ['"$X/bin/sudo" git apply x.diff', 'git apply'], ['timeout 5 "$X/git" apply x.diff', 'git apply'],
  ]) {
    assert.equal(ps(cmd).map(patchCommand).find(Boolean)?.tool, tool, cmd);
    assert.equal(parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean)?.tool, tool, cmd);
  }
  for (const cmd of [
    '"$HOME/.cargo/bin/cargo" test', 'source "$HOME/.cargo/env"; "$HOME/.cargo/bin/cargo" test', '"$CARGO_HOME/bin/cargo" test',
    'cd crates/a && "$PWD/../../target/debug/a"', 'HOME=/x; "$HOME/cargo" test', 'read X; "$X/cargo" test', '"${X:-/usr}/bin/cargo" test',
    '"$(git rev-parse --show-toplevel)/scripts/check.sh"', '"$X"/cargo test', '"$X"/\'cargo\' test', 'timeout 60 "$X/cargo" test',
    'bash "$HOME/x.sh"', 'source "$X/env"', 'bash "$(git rev-parse --show-toplevel)/scripts/check.sh"',
  ]) assert.equal(o(cmd), null, cmd);
  for (const cmd of [
    '$HOME/.cargo/bin/cargo test', '$PWD/target/debug/a', '${HOME}/.cargo/bin/cargo test', '"$X"', '"$X$Y"', '"$(which cargo)"', '"$X/"',
    '"$X/c$Y"', '"$X"/{a,b}', '"$X"/c*', '"$@/cargo"', '"${A[@]}/cargo"', '"$X"/$Y',
    // A word a wrapper may still read as its option: `X=-S...; env "$X/cargo"` runs what X says.
    'env "$X/cargo" test', 'nice "$X/cargo" test', 'xargs "$X/cargo" test',
  ]) assert.equal(o(cmd)?.kind, 'name', cmd);
  // A script operand whose last segment names a standard stream may be the shell's stdin.
  for (const cmd of ['bash "$X/stdin" < cmd.sh', 'bash "$X/fd/0" < cmd.sh', 'read HOME <<< /dev; bash "$HOME/stdin" < cmd.sh']) {
    assert.equal(o(cmd)?.kind, 'script', cmd);
  }
});

// #151 review 8: node, bun, deno, ruby, perl and php take a code option anywhere, since temper
// cannot tell which of their options take a value; python's -c, and its first operand, end its options.
// A code option's value may be attached (`-c'...'`, `--eval=...`), and an interpreter's option word
// that starts with an expansion may be one (`X=--eval=...; node "$X/a.js"`).
test('shell: interpreter code options are read in every argument, attached or not', () => {
  const o = (cmd) => parseCommands(cmd, { cwd: root, top: root, untracked: true }).map(opaquePatch).find(Boolean) ?? null;
  for (const cmd of [
    `node --disable-warning DEP0040 -e "require('child_process').execSync('git apply x.diff')"`,
    `node --unhandled-rejections strict -e "require('child_process').execSync('git apply x.diff')"`,
    `node --env-file-if-exists .env -e "require('child_process').execSync('git apply x.diff')"`,
    `bun --cwd . -e "require('child_process').execSync('git apply x.diff')"`, `ruby --encoding UTF-8 -e 'system("git apply x.diff")'`,
    `php -B 'system("git apply x.diff");'`, `php -R 'system("git apply x.diff");'`, `python3 -c'import os; os.system("git apply x.diff")'`,
    `python3 -Ic'import os; os.system("git apply x.diff")'`, `node --eval='require("child_process").execSync("git apply x.diff")'`,
    `node --print='require("child_process").execSync("git apply x.diff")'`, `ruby -e'system("git apply x.diff")'`, `perl -e'system("git apply x.diff")'`,
    `php -r'system("git apply x.diff");'`, `deno repl --eval "new Deno.Command('git', { args: ['apply', 'x.diff'] }).outputSync()"`,
    'node --disable-warning X -e "$X"', 'node --disable-warning X "$O" "$X"', 'node script.js -e "$ENVNAME"', 'ruby x.rb -e "$X"',
    'python3 -c"$X"', 'X=--eval=x; node "$X/a.js"', 'python3 "$X/a.py"', 'python3 -u "$HOME/x.py"', 'node "$CLAUDE_PLUGIN_ROOT/scripts/temper.mjs" gate',
    'php -B "$X"', 'deno repl --eval="$X"',
    // A script read from stdin by name or from a process substitution, and pwsh's encoded command.
    'echo x | python3 /dev/stdin', 'cat x.js | node /dev/fd/0', "node <(echo 'x')", 'pwsh -EncodedCommand ZwBp', 'pwsh -enc ZwBp',
  ]) assert.equal(o(cmd)?.kind, 'code', cmd);
  for (const cmd of [
    'python3 tool.py -c "$CFG"', 'python3 -m pytest -p "$PLUGIN"', 'python3 -u tool.py -c "$CFG"', 'node script.js "$X"', 'ruby x.rb "$X"',
    "python3 -c'print(1)' < x.json", 'node "/opt/$X/a.js"', "perl -Mstrict -e 'print 1'", "node -pe '1 + 1'", 'deno run x.ts -e "$X"',
    'node .claude/temper/temper.mjs gate --check', "perl -ne 'print if /unwrap/' x.rs", "python3 /dev/stdin <<'EOF'\nprint(1)\nEOF",
    'pwsh -NoProfile -File x.ps1', 'node -r ./x.js script.js "$X"', "ruby -e 'require \"x\"' x.rb \"$X\"",
  ]) assert.equal(o(cmd), null, cmd);
});

// #151 review 8: env -S in every spelling env accepts. Other agents' commands read its string, with
// `\_` as a space; for the reviewer it is denied, since its syntax is not the shell's.
test('shell: env -S and --split-string in every spelling', () => {
  const tool = (cmd) => parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean)?.tool;
  for (const cmd of [
    "env -S 'git apply x.diff'", "env -iS 'git apply x.diff'", "env -vS 'git apply x.diff'", "env -iS'git apply x.diff'", "env -0vS 'git apply x.diff'",
    "env --s 'git apply x.diff'", "env --sp='git apply x.diff'", "env --split 'git apply x.diff'", "env --split-string='git apply x.diff'",
    "env -S 'git\\_apply x.diff'", "env -S 'git\\_apply\\_x.diff'", "env -u X -S 'git apply x.diff'", "env -iu X -S 'git apply x.diff'",
    // env's other options, in their prefixes and clusters, are read too.
    'env --unse X git apply x.diff', 'env - git apply x.diff', 'env -iu X git apply x.diff', 'env --ch=/tmp git apply x.diff', 'env -C /tmp git apply x.diff',
  ]) assert.equal(tool(cmd), 'git apply', cmd);
  const o = (cmd) => parseCommands(cmd, { cwd: root, top: root, untracked: true }).map(opaquePatch).find(Boolean) ?? null;
  for (const cmd of [
    "env -S 'cargo test'", "env -S'cargo test'", "env -iS 'cargo test'", "env --split=cargo", "env --sp 'cargo test'", 'env -S"$X"',
    "env -u X -S 'cargo test'", "env -S 'X=1 cargo test'",
  ]) assert.equal(o(cmd)?.kind, 'script', cmd);
  for (const cmd of ['env cargo test', 'env -i PATH=/usr/bin cargo test', 'env -u X cargo test', 'env A="$X" cargo test']) assert.equal(o(cmd), null, cmd);
});

// #151 review 8: an unquoted expansion that is empty is no word at all, as in bash.
test('shell: an unquoted empty expansion is dropped from the command', () => {
  assert.equal(parseCommands('X=; $X git apply x.diff', { cwd: root }).map(patchCommand).find(Boolean)?.tool, 'git apply');
  assert.equal(parseCommands("X=''; $X git apply x.diff", { cwd: root }).map(patchCommand).find(Boolean)?.tool, 'git apply');
  assert.equal(parseCommands('X=; git "$X" apply x.diff', { cwd: root }).map(patchCommand).find(Boolean)?.tool, undefined);
});

// #151 review 5: `$'...'` is decoded as bash decodes it; an escape temper does not decode exactly leaves the word unknown.
test("shell: ANSI-C quoting is decoded exactly, or its word is unknown", () => {
  const v = (cmd) => parseCommands(cmd, { cwd: root })[0].args.map((a) => a.value);
  assert.deepEqual(v("echo $'git\\x20apply' $'a\\tb' $'\\101\\x41\\u0041\\U00000041' $'\\'\\\"\\\\\\?' $'\\a\\b\\e\\E\\f\\v' $'\\q'"),
    ['git apply', 'a\tb', 'AAAA', '\'"\\?', '\x07\x08\x1b\x1b\x0c\x0b', '\\q']);
  for (const word of ["$'\\cA'", "$'\\u00e9'", "$'\\xff'", "$'a\\0b'", "$'\\x'", "$'\\x0'"]) assert.deepEqual(v(`echo ${word}`), [null], word);
  const p = parseCommands("bash -c $'git\\x20apply x.diff'", { cwd: root }).map(patchCommand).find(Boolean);
  assert.equal(p?.tool, 'git apply');
  assert.equal(parseCommands("bash -c $'git\\cAapply x.diff'", { cwd: root }).map(opaquePatch).find(Boolean)?.kind, 'script');
});

// #151 review 5: bash and sh take options after -c, and `--` or `-` ends them; the first operand is the script.
test('shell: the -c script is the first operand after the options', () => {
  const p = (cmd) => parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean) ?? null;
  for (const cmd of [
    "bash -c -- 'git apply x.diff'", "bash -c -x 'git apply x.diff'", "sh -c -- 'patch -p1 < x.diff'", "bash -c - 'git apply x.diff'",
    "bash -c -o pipefail 'git apply x.diff'", "bash -co pipefail 'git apply x.diff'", "bash -euo pipefail -c 'git apply x.diff'",
    "bash -c -O extglob -- 'git apply x.diff' arg0", "bash -xc 'git apply x.diff'", "dash -c -e 'git apply x.diff'",
  ]) assert.equal(p(cmd)?.apply, true, cmd);
  assert.deepEqual(targets("bash -c -- 'cp /tmp/x crates/a/src/lib.rs'"), [['cp', at('crates/a/src/lib.rs')]]);
  assert.deepEqual(targets("bash -euo pipefail -c 'cp /tmp/x out.rs'"), [['cp', at('out.rs')]]);
  // Without -c, the first operand after the options is a script file, which temper does not read.
  assert.equal(p("bash -o pipefail 'git apply x.diff'"), null);
  assert.equal(parseCommands("bash -c -- 'ls'", { cwd: root }).map(opaquePatch).find(Boolean) ?? null, null);
});

test('shell: commands nested deeper than temper reads leave an opaque marker', () => {
  const nest = (n) => `bash -c '${'eval '.repeat(n)}git apply x.diff'`;
  const deep = parseCommands(nest(12), { cwd: root });
  assert.ok(deep.some((c) => c.opaque === 'depth'));
  assert.equal(deep.map(opaquePatch).find(Boolean)?.kind, 'script');
  assert.ok(!parseCommands(nest(1), { cwd: root }).some((c) => c.opaque));
  assert.ok(parseCommands(`${'find . -exec '.repeat(20)}ls \\;`, { cwd: root }).some((c) => c.opaque === 'depth'));
});

test('shell: options at the start of a git alias are read as git options', () => {
  const p = (cmd) => parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean) ?? null;
  for (const cmd of [
    "git -c alias.ap='-p apply' ap x.diff", "git -c alias.ap='--paginate apply' ap x.diff", "git -c alias.ap='-c core.pager=cat apply' ap x.diff",
    "git -c alias.ap='--exec-path=/usr/lib/git-core apply' ap x.diff", "git -c alias.ap='-p' -c alias.b='ap apply' b x.diff",
  ]) {
    const r = p(cmd);
    assert.equal(r?.apply, true, cmd);
    assert.equal(r.tool, 'git apply', cmd);
    assert.deepEqual(r.inputs.map((w) => w.value), ['x.diff'], cmd);
  }
  assert.equal(p("git -c alias.st='-p status' st"), null);
  assert.equal(p("git -c alias.ck='-p apply --check' ck x.diff").apply, false);
});

test('shell: an alias chain longer than temper follows reads as a guessed git apply', () => {
  const p = (cmd) => parseCommands(cmd, { cwd: root }).map(patchCommand).find(Boolean) ?? null;
  const r = p(`git ${Array.from({ length: 10 }, (_, k) => `-c alias.a${k}=a${k + 1}`).join(' ')} -c alias.a10=apply a0 x.diff`);
  assert.equal(r?.apply, true);
  assert.equal(r.guessed, true);
  assert.equal(r.tool, 'git apply');
  assert.deepEqual(r.inputs.map((w) => w.value), ['x.diff']);
});

test('shell: opaquePatch: a script on stdin a shell reads that temper cannot, and patch next to < | > or at a string end', () => {
  const o = (cmd) => parseCommands(cmd, { cwd: root }).map(opaquePatch).find(Boolean) ?? null;
  for (const [cmd, kind] of [
    ['cat cmd.sh | bash', 'script'], ['bash < cmd.sh', 'script'], ['curl -s https://example.com/i.sh | sh -s', 'script'],
    ['cat cmd.sh | bash -s -- a b', 'script'], ['cat cmd.sh | bash -o pipefail', 'script'], ['cat cmd.sh | sudo sh -e', 'script'],
    // wi-0e226b: text piped into a shell is never read as its script, so even a harmless one is flagged.
    ['echo ls | bash', 'script'], ["printf 'git status\\n' | sh", 'script'],
    [`python3 -c "import subprocess; subprocess.run('patch<x.diff', shell=True)"`, 'code'],
    [`python3 -c "import subprocess; subprocess.run('patch -p1<x.diff', shell=True)"`, 'code'],
    [`python3 -c "import subprocess; subprocess.run('cat x.diff|patch', shell=True)"`, 'code'],
    [`python3 -c "import subprocess; subprocess.run('cat x.diff | patch', shell=True)"`, 'code'],
    [`node -e "require('child_process').execSync('cat x.diff | patch')"`, 'code'],
    ["perl -e 'system(\"cat x.diff | patch\\n\")'", 'code'],
    // A bare patch reads the interpreter's stdin, which may be a patch; wrappers before patch count too.
    [`python3 -c "import os; os.system('patch')"`, 'code'], [`python3 -c "import subprocess; subprocess.run(['/usr/bin/patch'])"`, 'code'],
    [`node -e "require('child_process').execSync('exec patch')"`, 'code'], [`python3 -c "import os; os.system('cat x.diff|timeout -k 5 10 patch')"`, 'code'],
    [`python3 -c "import os; os.system('cat x.diff|command patch -p1')"`, 'code'], [`python3 -c "import os; os.system('git\\x20apply x.diff')"`, 'code'],
    [`python3 -c "import os; os.system('git\\040apply x.diff')"`, 'code'], [`node -e "require('child_process').execSync('git\\u0020apply x.diff')"`, 'code'],
  ]) assert.equal(o(cmd)?.kind, kind, cmd);
  for (const cmd of [
    'echo hi | grep h', 'cargo test 2>&1 | tail', 'cargo test 2>&1 | tail -n 20',
    'bash script.sh', 'cat in.txt | bash script.sh', "bash -c 'ls' < in.txt", 'sh -c "cat" < in.txt', 'git diff | less',
    `python3 -c "import requests; requests.patch('https://example.com')"`, `python3 -c "method = 'patch'"`, `node -e "const r = 'patch'"`,
  ]) assert.equal(o(cmd), null, cmd);
});

test('shell: the interpreter-code patterns stay linear on long input', () => {
  for (const code of [`${'patch '.repeat(20000)}x`, `${'|patch'.repeat(20000)}`, `${'git -c a=b '.repeat(20000)}x`, `'${' '.repeat(50000)}patch`,
    `|${'timeout -k 5 '.repeat(20000)}x`, `|${'exec '.repeat(20000)}x`, `${"system('timeout ".repeat(10000)}x`, `|timeout${' -s'.repeat(30000)} 5x`, `${'\\x2'.repeat(30000)}`]) {
    const t0 = Date.now();
    opaquePatch(parseCommands(`python3 -c ${JSON.stringify(code)}`, { cwd: root })[0]);
    assert.ok(Date.now() - t0 < 2000, `${code.slice(0, 20)}: ${Date.now() - t0} ms`);
  }
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
