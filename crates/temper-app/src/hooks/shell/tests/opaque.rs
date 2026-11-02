//! Ports of the `tests/shell.test.mjs` tests that are 1.x `opaquePatch`'s alone, in its
//! order: why temper cannot rule out that a command applies a patch, which the reviewer is
//! denied (#151).

use super::{opaque, opaque_with, opts, parse, topped, untracked};
use crate::hooks::shell::walk::Opaque;

/// Each command is flagged with the kind beside it.
fn flagged(table: &[(&str, &str)]) {
    for &(cmd, kind) in table {
        assert_eq!(opaque(cmd), Some(kind), "{cmd:?}");
    }
}

/// No command is flagged.
fn clear(cmds: &[&str]) {
    for &cmd in cmds {
        assert_eq!(opaque(cmd), None, "{cmd:?}");
    }
}

/// 1.x's `git -c alias.a0=a1 ... -c alias.a10=apply a0 x.diff`: a chain longer than temper
/// follows.
fn alias_chain() -> String {
    let links: Vec<String> = (0..10)
        .map(|k| format!("-c alias.a{k}=a{}", k + 1))
        .collect();
    format!("git {} -c alias.a10=apply a0 x.diff", links.join(" "))
}

#[test]
fn opaque_patch_flags_git_subcommands_and_interpreter_code_temper_cannot_resolve() {
    for cmd in [
        "git ${CMD:-apply} x.diff",
        "git $x x.diff",
        "git -C sub $SUB x.diff",
        "git -c \"$CFG\" ap x.diff",
        "git -c 'alias.x=!git apply x.diff' x",
        "git -c alias.ap=$A ap x.diff",
        "git -c $K=apply ap x.diff",
        "$GIT apply x.diff",
        "${P:-patch} -p1 < x.diff",
        r#"python3 -c "import subprocess; subprocess.run(['git', 'apply', 'x.diff'])""#,
        r#"python -c "import os; os.system('patch -p1 < x.diff')""#,
        r#"node -e "require('child_process').execSync('git apply x.diff')""#,
        r#"node --eval "require('child_process').execFileSync('git', ['am', 'x.mbox'])""#,
        r#"node -p "require('child_process').execSync('gpatch -p1 -i x.diff')""#,
        r#"perl -e 'system("git", "apply", "x.diff")'"#,
        r#"perl -E 'system("cd sub && patch -p1 < x.diff")'"#,
        r#"ruby -e 'system("git am x.mbox")'"#,
        "python3 - <<'EOF'\nimport subprocess\nsubprocess.run(['busybox', 'patch', '-i', 'x.diff'])\nEOF",
    ] {
        assert!(opaque(cmd).is_some(), "{cmd:?}");
    }
    clear(&[
        "git diff",
        "git apply --check x.diff",
        "git -c core.pager=cat log",
        "git -c color.ui=$C diff",
        "git log --grep 'git apply'",
        "git -c alias.ap=apply ap x.diff",
        r#"python3 -c "print(1)""#,
        r#"python3 -c "from unittest import mock; mock.patch('os.getcwd')""#,
        r#"node -e "console.log('x.patch')""#,
        "perl -e 'print \"--patch\n\"'",
        r#"ruby -e 'puts "I am here"'"#,
        "echo \"git apply x.diff\"",
        "deno run main.ts",
    ]);
}

#[test]
fn opaque_patch_subcommands_git_does_not_have_alias_writes_multi_line_code_and_unread_stdin_scripts()
 {
    let chain = alias_chain();
    flagged(&[
        ("git ap x.diff", "git"),
        ("git lg", "git"),
        ("GIT_CONFIG_COUNT=1 git ap x.diff", "git"),
        ("git config alias.ap apply", "alias"),
        ("git config set alias.ap apply", "alias"),
        ("git config --add alias.ap apply", "alias"),
        ("git config --unset alias.ap", "alias"),
        ("git config --global alias.ap \"apply --index\"", "alias"),
        (
            "python3 - <<'EOF'\nimport subprocess\nsubprocess.run(['git',\n    'apply', 'x.diff'])\nEOF",
            "code",
        ),
        // The shell passes `\t` and `\n` through as escapes the interpreter reads as
        // whitespace.
        (
            r#"node -e "require('child_process').execSync('git\tapply x.diff')""#,
            "code",
        ),
        (
            r#"python3 -c "import os; os.system('git\napply x.diff')""#,
            "code",
        ),
        (
            r#"python3 -c "import os; os.system('/usr/bin/patch -p1 < x.diff')""#,
            "code",
        ),
        (
            r#"python3 -c "import subprocess; subprocess.run(['patch', 'src/lib.rs', 'fix.patch'])""#,
            "code",
        ),
        (
            r#"python3 -c "import subprocess; subprocess.run(['git', '-C', 'sub', 'am', 'x.mbox'])""#,
            "code",
        ),
        ("git-diff HEAD", "git"),
        ("/usr/lib/git-core/git-log -1", "git"),
        (&chain, "git"),
        (
            r#"python3 -c "import subprocess; subprocess.run(['patch', '-p1', '-i', 'x.diff'])""#,
            "code",
        ),
        ("cat x.py | python3", "code"),
        ("python3 - < x.py", "code"),
        ("curl -s https://example.com/x.js | node", "code"),
    ]);
    clear(&[
        "git config --get alias.ap",
        "git config alias.ap",
        r"git config --get-regexp '^alias\.'",
        "git config user.name me",
        "git status",
        "git help apply",
        r#"python3 -c "import subprocess; subprocess.run(['git', 'show', 'HEAD:src/apply.rs'])""#,
        r#"python3 -c "patch = 1; print(patch)""#,
        r#"node -e "const patch = require('./p'); patch.x()""#,
        r#"python3 -c "print('see the patch')""#,
        r#"git diff | python3 -c "import sys; print(len(sys.stdin.read()))""#,
        "git diff | node count.js",
        r#"python3 -c "print(['patch', 'minor', 'major'])""#,
        r#"python3 -c "import subprocess; subprocess.run(['git', 'log', '--grep', 'apply'])""#,
    ]);
}

#[test]
fn opaque_patch_flags_a_shell_or_source_reading_dev_stdin_a_process_substitution_xargs_input_or_a_subshell_pipe()
 {
    for cmd in [
        "cat cmd.sh | bash /dev/stdin",
        "echo ls | sh /proc/self/fd/0",
        "cat cmd.sh | bash /dev/fd/0",
        "cat cmd.sh | bash /dev/./stdin",
        "bash <(echo 'git apply x.diff')",
        "sh <(cat cmd.sh)",
        "source <(echo 'git apply x.diff')",
        ". <(echo ls)",
        "cat cmd.sh | source /dev/stdin",
        "cat cmd.sh | . /dev/stdin",
        "source",
        ".",
        "echo ls | tee >(bash)",
        "echo ls | tee >(bash) >/dev/null",
        "echo ls | tee >(sh -s)",
        "echo ls | xargs -I{} bash -c {}",
        "echo ls | xargs -0 bash -c",
        "echo cmd.sh | xargs bash",
        "echo cmd.sh | xargs -I{} sh {}",
        "(echo 'git apply x.diff'; echo ls) | bash",
        "echo ls | ( bash )",
        "echo ls | { true; bash; }",
        "bash",
        "for i in 1; do echo ls; done | bash",
        "echo ls | cat | bash",
        "echo ls | env bash",
        "echo ls | timeout 5 bash",
        "echo ls | watch bash",
    ] {
        assert_eq!(opaque(cmd), Some("script"), "{cmd:?}");
    }
    clear(&[
        "bash script.sh",
        "source ./env.sh",
        ". ./env.sh",
        "bash -c 'ls'",
        "echo foo | sh -c cat",
        "git diff | cat",
        "echo ls | xargs bash -c 'echo $0'",
        "echo a | xargs bash script.sh",
        "bash --version",
        "sh --help",
        "bash -c 'cat' < in.txt",
    ]);
}

#[test]
fn opaque_patch_flags_a_c_script_here_string_heredoc_eval_argument_or_command_name_that_is_not_literal()
 {
    for cmd in [
        "bash -c \"$(cat cmd.sh)\"",
        "sh -c \"`cat x`\"",
        "bash -c \"$CMD\"",
        "X=ls; bash -c \"$X\"",
        "bash -c",
        "bash -c \"ls; $(cat cmd.sh)\"",
        "bash <<< \"$(cat cmd.sh)\"",
        "bash <<< \"$X\"",
        "bash <<< `cat x`",
        "X=ls; bash <<< \"$X\"",
        "bash <<EOF\n$(cat cmd.sh)\nEOF",
        "bash <<EOF\ngit $X\nEOF",
        "cat <<EOF | bash\n$X\nEOF",
        "cat <<< \"$X\" | bash",
        "eval `cat cmd.txt`",
        "eval \"$(cat cmd.txt)\"",
        "X=ls; eval \"$X\"",
        "xargs -0 -n1 eval < cmd.sh",
        "while read -r l; do eval \"$l\"; done < cmd.sh",
        // #155: a trap action, and an awk program that runs commands temper cannot read.
        "trap \"$X\" EXIT",
        "trap -- \"$(cat x)\" EXIT",
        "awk '{system($0)}' cmd.txt",
        "awk '{ print | \"sh\" }' cmd.txt",
        "echo x | awk -f /dev/stdin",
        "awk -f - < p.awk",
        "awk -f <(cat p.awk) f",
        "awk \"$P\" f",
        "awk 'BEGIN { \"cat x\" | getline l }'",
        "awk 'BEGIN { c = \"git apply x.diff\"; system(c) }'",
        "gawk -i /dev/stdin 1 f < lib.awk",
        "gawk -E /dev/fd/3 f",
        "gawk --exec=- f",
        "gawk -l \"$L\" 1 f",
        "gawk '-f/dev/stdin' f",
        "gawk -f '/dev/stdin' f",
        "gawk --load /proc/self/fd/0 1 f",
        "awk 'BEGIN { FS = \"|\" } { print $1 | \"sh\" }' f",
    ] {
        assert_eq!(opaque(cmd), Some("script"), "{cmd:?}");
    }
    clear(&[
        "trap 'rm -f tmp' EXIT",
        "awk '{print $1}' f",
        "awk -F: '$3 > 10 || $4 < 2 { print $1 }' f",
        "awk 'BEGIN{system(\"ls\")}'",
        "awk -f prog.awk f",
        "awk 'BEGIN { FS = \"|\"; OFS = \"a|b\" } { print $1 }' f",
        "gawk -E prog.awk f",
        "gawk -i lib.awk 1 f",
        r#"awk 'BEGIN { print "\"|" }'"#,
    ]);
    // A command name temper cannot read is its own kind (#151 review 6).
    for cmd in [
        "X=$(cat cmd.txt); $X",
        "$X x.diff",
        "\"$CMD\" x.diff",
        "${X:-ls}",
        "$(cat cmd.txt)",
        "X=ls; X+=\";git apply x.diff\"; $X",
    ] {
        assert_eq!(opaque(cmd), Some("name"), "{cmd:?}");
    }
    // An interpreter's stdin is read the same way.
    for cmd in [
        "python3 <<< \"$X\"",
        "python3 - <<EOF\nprint(\"$(cat x.py)\")\nEOF",
    ] {
        assert_eq!(opaque(cmd), Some("code"), "{cmd:?}");
    }
    clear(&[
        "bash -c 'ls'",
        "bash -lc 'cargo test 2>&1 | tail -n 20'",
        "bash -euo pipefail -c 'cargo test'",
        "bash -c -- 'cargo test'",
        "bash <<< 'cargo test'",
        "bash <<< ls",
        "bash <<'EOF'\ncargo test\nEOF",
        "bash <<EOF\ncargo test\nEOF",
        "cat <<'EOF' | bash\nls\nEOF",
        "eval 'ls'",
        "eval ls -la",
        "python3 - <<'EOF'\nprint(1)\nEOF",
        "echo \"$HOME\"",
        "~/.cargo/bin/cargo test",
        "cargo test -- \"$X\"",
        "find crates -name '*.rs' | head",
        "X=1; echo $X",
    ]);
}

#[test]
fn opaque_patch_brace_and_glob_words_command_names_temper_reads_and_interpreter_code_from_an_expansion()
 {
    for (cmd, kind) in [
        ("{git,apply,doc.diff}", "name"),
        ("eval {git,apply,doc.diff}", "script"),
        ("bash -c {'git apply doc.diff',x}", "script"),
        ("{bash,-c,'git apply doc.diff'}", "name"),
        ("{eval,'git apply doc.diff'}", "name"),
        ("{patch,-p1,-i,doc.diff}", "name"),
        ("eval git*", "script"),
        ("bash -c gi?' apply doc.diff'", "script"),
        ("eval {'git apply',x}", "script"),
        (r"eval {'git apply'\,x}", "script"),
        ("sh -c {x,'git apply doc.diff'}", "script"),
        ("git* apply doc.diff", "name"),
        ("gi? apply doc.diff", "name"),
        ("bash <<< {x,'git apply y'}", "script"),
        ("X='git apply x'; $X", "name"),
        ("X='git apply x'; \"$X\"", "name"),
        ("X=g*; $X", "name"),
        ("IFS=/; X=git/apply/x.diff; $X", "name"),
        ("\"${CARGO:-cargo}\" test", "name"),
        ("$(which cargo) test", "name"),
        ("$HOME/.cargo/bin/cargo test", "name"),
        ("$PWD/target/debug/a", "name"),
        ("python3 -c \"$X\"", "code"),
        ("node -e \"$(cat f)\"", "code"),
        ("perl -e \"$X\"", "code"),
        ("perl -ne \"$X\" f", "code"),
        ("ruby -e \"$X\"", "code"),
        ("php -r \"$X\"", "code"),
        ("node --eval \"$X\"", "code"),
        ("node -p \"$X\"", "code"),
        ("bun -e \"$X\"", "code"),
        ("python3 -Ic \"$X\"", "code"),
        // A value temper knows is not enough: the name must be literal (wi-9792d6).
        ("\"$HOME/.cargo/bin/cargo\" test", "name"),
        ("\"$PWD/target/debug/a\" --help", "name"),
        (
            "\"$(git rev-parse --show-toplevel)/scripts/check.sh\"",
            "name",
        ),
    ] {
        assert_eq!(opaque_with(cmd, topped()), Some(kind), "{cmd:?}");
    }
    for cmd in [
        "find crates -name '*.rs' | head",
        "cargo test -- --nocapture",
        "jq '{a:.b}' x.json",
        "git log --format='%H' -5",
        r"rg 'foo\{' crates",
        r"rg foo\{ crates",
        "find crates -name x -exec grep -n unwrap {} +",
        "git show HEAD@{1} --stat",
        "eval 'git status'",
        "ls crates/*/src",
        "~/.cargo/bin/cargo test",
        "python3 -c 'import json, sys; print(json.load(sys.stdin))' < x.json",
        "python3 -c \"print(1)\"",
        "node -e 'console.log(1)'",
        "perl -ne 'print if /unwrap/' x.rs",
    ] {
        assert_eq!(opaque_with(cmd, topped()), None, "{cmd:?}");
    }
}

/// The names 1.x's "never reads a command name from a variable" flags as `name`.
const NAMES_FROM_VARIABLES: &[&str] = &[
    // A value from an assignment: never read, whatever temper's model says it is.
    "X=cargo; $X test",
    "X=cargo; export X; $X test",
    "X=git; $X status",
    "X='git'; X=\"${X}\"; $X status",
    // Builtins that write a variable temper does not follow, and values it would read wrong.
    "X=cargo; read X <<< git; $X apply x.diff",
    "X=cargo; printf -v X git; $X apply x.diff",
    "X=cargo; mapfile -t X <<< git; $X apply x.diff",
    "Y=git; X=cargo; declare -n X=Y; $X apply x.diff",
    "X=cargo; X[0]=git; $X apply x.diff",
    "X=cargo; X=(git); $X apply x.diff",
    "X=cargo; ${X/cargo/git} apply x.diff",
    "X=GIT; ${X,,} apply x.diff",
    "Y=git; X=Y; ${!X} apply x.diff",
    "X=cargo; unset X; $X git apply x.diff",
    "X=; $X git apply x.diff",
    "X=''; $X git apply x.diff",
    "$X git apply x.diff",
    "set -- git; \"$@\" apply x.diff",
    "X=cargo; : ${X:=git}; $X test",
    // Assignments that never run, or run elsewhere.
    "X=git; false && X=cargo; $X apply x.diff",
    "X=git; if false; then X=cargo; fi; $X apply x.diff",
    "X=git; f() { X=cargo; }; $X apply x.diff",
    "X=git; { X=cargo; } | cat; $X apply x.diff",
    "X=git; { X=cargo; } & $X apply x.diff",
    "X=git\nX=cargo &\n$X apply x.diff",
    "X=git; case a in b) X=cargo;; esac; $X apply x.diff",
    "X=git; local X=cargo 2>/dev/null; $X apply x.diff",
    "X=cargo; time X=git; $X apply x.diff",
    "X=cargo; X=git eval :; $X apply x.diff",
    "X=cargo; X=git export Y; $X apply x.diff",
    "X=cargo; source ./set.sh; $X apply x.diff",
    "X=gitxapply; IFS=x; $X x.diff",
    "X=gitxapply; read IFS <<< x; $X x.diff",
    "X=cargo; trap 'X=git' DEBUG; $X apply x.diff",
    // A name the model would read as cd, export or a wrapper, and a wrapper's word built from
    // an expansion.
    "X=cd; $X apply x.diff",
    "X=export; $X apply x.diff",
    "X=nice; $X git apply x.diff",
    "timeout $T apply x.diff",
    "nice -n $N apply x.diff",
    "timeout \"$T\" 5 git apply x.diff",
    "env A=$X apply x.diff",
    "watch -n $N apply x.diff",
    "alias cargo='git apply'",
    "hash -p /usr/bin/git cargo",
    // An expansion in the name, quoted or not, whatever temper knows of its value (wi-9792d6).
    "export HOME=/x; $HOME/cargo test",
    "unset HOME; $HOME/cargo test",
    ": ${HOME:=/x}; $HOME/cargo test",
    "for HOME in /x; do $HOME/cargo test; done",
    "IFS=/; $HOME/cargo test",
    "read IFS <<< /; $HOME/cargo test",
    "$HOME/cargo test",
    "$PWD/x",
    "$HOME",
    "\"$HOME\"",
    "\"$HOME/\"",
    "\"$HOME/x$Y\"",
    "$HOME/{x,y}",
    "$(git rev-parse --show-toplevel)/x",
    "\"$HOME/.cargo/bin/cargo\" test",
    "\"${HOME}\"/.cargo/bin/cargo test",
    "\"$PWD/target/debug/a\" --help",
    "\"$(git rev-parse --show-toplevel)/scripts/check.sh\"",
    "X=/usr/bin; \"$X/cargo\" test",
    "read HOME <<< /x; \"$HOME/cargo\" test",
    "printf -v HOME /x; \"$HOME/cargo\" test",
    "HOME=/x; \"$HOME/cargo\" test",
    "declare -n R=HOME; \"$HOME/cargo\" test",
    "Y=ME; read HO$Y <<< /x; \"$HOME/cargo\" test",
    "mapfile -t HO''ME <<< /x; \"$HOME/cargo\" test",
    "cd /tmp; \"$PWD/x\"",
    "pushd /tmp; \"$PWD/x\"",
    "source ./set.sh; \"$HOME/cargo\" test",
    "(( HOME = 1 )); \"$HOME/cargo\" test",
    "\"$HOME$PWD/x\"",
    "\"$TMPDIR/x\"",
    "\"$OLDPWD/x\"",
    "\"$(git rev-parse --show-toplevel)$X/x\"",
    "\"${HOME:-/x}/cargo\" test",
    "\"$(git rev-parse --git-dir)/x\"",
    "echo \"$HOME\"; \"$HOME/.cargo/bin/cargo\" test",
    "X=1; echo $X; \"$HOME/.cargo/bin/cargo\" test",
    "\"$HOME/My Tools/x\" --help",
];

#[test]
fn opaque_patch_never_reads_a_command_name_from_a_variable() {
    for &cmd in NAMES_FROM_VARIABLES {
        assert_eq!(opaque_with(cmd, untracked()), Some("name"), "{cmd:?}");
    }
    for cmd in [
        "~/.cargo/bin/cargo test",
        "cd crates && cargo test",
        "timeout 60 cargo test",
        "nice -n 5 cargo test",
        "env A=\"$X\" cargo test",
        "X=crates; ls $X",
        // An interpreter's -c or -e after its script or module is the script's own argument
        // (#151 review 7): python's, that is; the others' code options are read anywhere.
        "python3 tool.py -c \"$CFG\"",
        "python3 -m pytest -p \"$PLUGIN\"",
        "python3 -u tool.py -c \"$CFG\"",
        "deno run x.ts -e \"$X\"",
        "python3 -c \"import sys; print(sys.argv)\" \"$X\"",
        // A shell's or source's script operand whose value temper knows.
        "source \"$HOME/.cargo/env\"",
        "bash \"$HOME/x.sh\"",
        "PATH+=\":$HOME/.cargo/bin\"; cargo test",
    ] {
        assert_eq!(opaque_with(cmd, untracked()), None, "{cmd:?}");
    }
    // A script operand temper cannot name exactly may be /dev/stdin.
    for cmd in [
        "F=/dev/stdin; bash \"$F\" < cmd.sh",
        "read HOME <<< /dev; bash \"$HOME/stdin\" < cmd.sh",
        "bash \"$X\"",
        ". \"$X\"",
        "bash \"$CLAUDE_PLUGIN_ROOT/x.sh\"",
    ] {
        assert_eq!(opaque_with(cmd, untracked()), Some("script"), "{cmd:?}");
    }
    // Code before the script is still read: an option's value is not the script.
    for cmd in [
        "python3 -W ignore -c \"$X\"",
        "python3 -X dev -c \"$X\"",
        "node -r ./x.js -e \"$X\"",
        "node --require ./x.js -e \"$X\"",
        "perl -pi -e \"$X\" x",
        "python3 -E -c \"$X\"",
        "python3 -Ic \"$X\"",
        "perl -W -e \"$X\"",
        "ruby -W -e \"$X\"",
        "perl -I lib -e \"$X\"",
        // perl's -W is a flag, so the code after it is read.
        r#"perl -W -e 'system("git apply x.diff")'"#,
    ] {
        assert_eq!(opaque_with(cmd, untracked()), Some("code"), "{cmd:?}");
    }
}

#[test]
fn interpreter_code_options_and_a_code_word_built_from_an_expansion() {
    for cmd in [
        r#"node --disable-warning DEP0040 -e "require('child_process').execSync('git apply x.diff')""#,
        r#"node --unhandled-rejections strict -e "require('child_process').execSync('git apply x.diff')""#,
        r#"node --env-file-if-exists .env -e "require('child_process').execSync('git apply x.diff')""#,
        r#"bun --cwd . -e "require('child_process').execSync('git apply x.diff')""#,
        r#"ruby --encoding UTF-8 -e 'system("git apply x.diff")'"#,
        r#"deno repl --eval "new Deno.Command('git', { args: ['apply', 'x.diff'] }).outputSync()""#,
        "node --disable-warning X -e \"$X\"",
        "node script.js -e \"$ENVNAME\"",
        "ruby x.rb -e \"$X\"",
        "php -r \"$X\"",
        "node -p \"$X\"",
        "perl -ne \"$X\"",
        "python3 -W ignore -c \"$X\"",
        "python3 -Ic \"$X\"",
        "perl -p -e \"$X\"",
        // A code option that a word before it would take as its value is read too
        // (`-Wignore` ends in e).
        r#"python3 -Wignore -c 'import os; os.system("git apply x.diff")'"#,
        r#"perl -p -e 'system("git apply x.diff")'"#,
        // A script read from stdin by name or from a process substitution.
        "echo x | python3 /dev/stdin",
        "cat x.js | node /dev/fd/0",
        "node <(echo 'x')",
    ] {
        assert_eq!(opaque_with(cmd, untracked()), Some("code"), "{cmd:?}");
    }
    for cmd in [
        "python3 tool.py -c \"$CFG\"",
        "python3 -m pytest -p \"$PLUGIN\"",
        "python3 -u tool.py -c \"$CFG\"",
        "node script.js \"$X\"",
        "ruby x.rb \"$X\"",
        "node \"/opt/$X/a.js\"",
        "perl -Mstrict -e 'print 1'",
        "node -pe '1 + 1'",
        "deno run x.ts -e \"$X\"",
        "node .claude/temper/temper.mjs gate --check",
        "perl -ne 'print if /unwrap/' x.rs",
        "python3 /dev/stdin <<'EOF'\nprint(1)\nEOF",
        "pwsh -NoProfile -File x.ps1",
        "node -r ./x.js script.js \"$X\"",
        r#"ruby -e 'require "x"' x.rb "$X""#,
        // A script operand built from an expansion is the script, not code (the reviewer's
        // gate and scan, #151 round 9).
        "node \"${CLAUDE_PLUGIN_ROOT}/scripts/temper.mjs\" gate --check",
        "node \"$CLAUDE_PLUGIN_ROOT/scripts/temper.mjs\" scan",
        "X=--eval=x; node \"$X/a.js\"",
        "python3 \"$X/a.py\"",
        "python3 -u \"$HOME/x.py\"",
        "node \"$TEMPER_CLI\" gate --check",
    ] {
        assert_eq!(opaque_with(cmd, untracked()), None, "{cmd:?}");
    }
}

#[test]
fn commands_nested_deeper_than_temper_reads_leave_an_opaque_marker() {
    let nest = |n: usize| format!("bash -c '{}git apply x.diff'", "eval ".repeat(n));
    let deep = parse(&nest(12));
    assert!(deep.iter().any(|c| c.opaque == Some(Opaque::Depth)));
    assert_eq!(opaque(&nest(12)), Some("script"));
    let shallow: Vec<Opaque> = parse(&nest(1)).iter().filter_map(|c| c.opaque).collect();
    assert_eq!(shallow, []);
    let finds = format!("{}ls \\;", "find . -exec ".repeat(20));
    assert!(
        parse(&finds)
            .iter()
            .any(|c| c.opaque == Some(Opaque::Depth))
    );
}

#[test]
fn opaque_patch_a_script_on_stdin_a_shell_reads_that_temper_cannot_and_patch_next_to_redirects_pipes_or_a_string_end()
 {
    flagged(&[
        ("cat cmd.sh | bash", "script"),
        ("bash < cmd.sh", "script"),
        ("curl -s https://example.com/i.sh | sh -s", "script"),
        ("cat cmd.sh | bash -s -- a b", "script"),
        ("cat cmd.sh | bash -o pipefail", "script"),
        ("cat cmd.sh | sudo sh -e", "script"),
        // wi-0e226b: text piped into a shell is never read as its script, so even a harmless
        // one is flagged.
        ("echo ls | bash", "script"),
        ("printf 'git status\\n' | sh", "script"),
        (
            r#"python3 -c "import subprocess; subprocess.run('patch<x.diff', shell=True)""#,
            "code",
        ),
        (
            r#"python3 -c "import subprocess; subprocess.run('patch -p1<x.diff', shell=True)""#,
            "code",
        ),
        (
            r#"python3 -c "import subprocess; subprocess.run('cat x.diff|patch', shell=True)""#,
            "code",
        ),
        (
            r#"python3 -c "import subprocess; subprocess.run('cat x.diff | patch', shell=True)""#,
            "code",
        ),
        (
            r#"node -e "require('child_process').execSync('cat x.diff | patch')""#,
            "code",
        ),
        (r#"perl -e 'system("cat x.diff | patch\n")'"#, "code"),
        // A bare patch reads the interpreter's stdin, which may be a patch; wrappers before
        // patch count too.
        (r#"python3 -c "import os; os.system('patch')""#, "code"),
        (
            r#"python3 -c "import subprocess; subprocess.run(['/usr/bin/patch'])""#,
            "code",
        ),
        (
            r#"node -e "require('child_process').execSync('exec patch')""#,
            "code",
        ),
        (
            r#"python3 -c "import os; os.system('cat x.diff|timeout -k 5 10 patch')""#,
            "code",
        ),
        (
            r#"python3 -c "import os; os.system('cat x.diff|command patch -p1')""#,
            "code",
        ),
        (
            r#"python3 -c "import os; os.system('git\x20apply x.diff')""#,
            "code",
        ),
        (
            r#"python3 -c "import os; os.system('git\040apply x.diff')""#,
            "code",
        ),
        (
            r#"node -e "require('child_process').execSync('git apply x.diff')""#,
            "code",
        ),
    ]);
    clear(&[
        "echo hi | grep h",
        "cargo test 2>&1 | tail",
        "cargo test 2>&1 | tail -n 20",
        "bash script.sh",
        "cat in.txt | bash script.sh",
        "bash -c 'ls' < in.txt",
        "sh -c \"cat\" < in.txt",
        "git diff | less",
        r#"python3 -c "import requests; requests.patch('https://example.com')""#,
        r#"python3 -c "method = 'patch'""#,
        r#"node -e "const r = 'patch'""#,
    ]);
}

/// 1.x's wall-clock bound is retired into the parity harness's per-step budget (OC-4 A); the
/// answers are 1.x's, probed on each code.
#[test]
fn the_interpreter_code_patterns_stay_linear_on_long_input() {
    let cases = [
        (
            ["patch ".repeat(20_000), "x".to_owned()].concat(),
            Some("code"),
        ),
        ("|patch".repeat(20_000), Some("code")),
        (
            ["git -c a=b ".repeat(20_000), "x".to_owned()].concat(),
            None,
        ),
        (["'", &" ".repeat(50_000), "patch"].concat(), None),
        (["|", &"timeout -k 5 ".repeat(20_000), "x"].concat(), None),
        (["|", &"exec ".repeat(20_000), "x"].concat(), None),
        (
            ["system('timeout ".repeat(10_000), "x".to_owned()].concat(),
            None,
        ),
        (["|timeout", &" -s".repeat(30_000), " 5x"].concat(), None),
        (r"\x2".repeat(30_000), None),
    ];
    for (code, kind) in cases {
        // 1.x's `JSON.stringify(code)`: no code here holds a `"`, so only `\` is escaped.
        let cmd = format!("python3 -c \"{}\"", code.replace('\\', r"\\"));
        let first = parse(&cmd).into_iter().next().expect("one command");
        let found = super::opaque_patch(&first, opts().paths, &super::CODE)
            .expect("1.x's opaquePatch does not throw here")
            .map(|o| super::kind_name(o.kind));
        assert_eq!(found, kind, "{:?}", &code[..20]);
    }
}
