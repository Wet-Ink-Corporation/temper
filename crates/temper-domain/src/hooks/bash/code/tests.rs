//! Interpreter code against 1.x: every expected value was printed by 1.x's exported
//! `codeWriteTargets` on the same input.

use std::sync::LazyLock;

use super::InterpreterCode;

static CODE: LazyLock<InterpreterCode> =
    LazyLock::new(|| InterpreterCode::new().expect("the code patterns compile"));

fn targets(table: &[(&str, &[&str])]) {
    for &(code, want) in table {
        assert_eq!(CODE.write_targets(code), want, "{code:?}");
    }
}

/// 1.x `shell.test.mjs`: "codeWriteTargets finds literals in write positions only".
#[test]
fn code_write_targets_finds_literals_in_write_positions_only() {
    targets(&[
        ("open('w.txt', 'w'); open('r.txt').read()", &["w.txt"]),
        (
            "from pathlib import Path\np = Path('p.txt')\np.write_text('x')",
            &["p.txt"],
        ),
        ("shutil.copy('a', 'dst')", &["dst"]),
        ("open(my $fh, \">\", \"perl.txt\")", &["perl.txt"]),
        (
            "fs.writeFileSync('/tmp/o', fs.readFileSync('in'))",
            &["/tmp/o"],
        ),
    ]);
}

/// 1.x `shell.test.mjs`: "an interpreter write whose mode is a variable assigned a literal
/// counts".
#[test]
fn an_interpreter_write_whose_mode_is_a_variable_assigned_a_literal_counts() {
    targets(&[
        ("m='w'; open('f1', m).write('{}')", &["f1"]),
        ("mode = 'a'\nopen('f2', mode=mode)", &["f2"]),
        ("m='r'; open('f3', m).read()", &[]),
        ("open(path, m)", &[]),
    ]);
}

/// 1.x `shell.test.mjs`, "code and word scans stay roughly linear on pathological input": the
/// literal pass still reads escapes and both quote kinds.
#[test]
fn the_literal_pass_reads_escapes_and_every_quote_kind() {
    targets(&[
        ("open(\"a\\\"b\", 'w'); open('c', \"w\")", &["a\\\"b", "c"]),
        ("'unclosed open('a', 'w')", &[]),
        ("\"open('q', 'w')\"", &[]),
        ("`t` open('bt', 'w')", &["bt"]),
        ("open(__L0__, 'w')", &["w"]),
        ("__L0__ = 'x'; open(__L0__, 'w')", &["x"]),
        (
            "open('a', 'w'); open('a', 'w'); open('b', 'a')",
            &["a", "b"],
        ),
    ]);
}

/// `ARG`'s look-behind: a variable after a word char or a dot is no argument, so `obj.p` and
/// `9p` name nothing; `$p` and a variable after a non-ASCII char do. Red against the pattern
/// with the look-behind dropped: it read `obj.p.write_text` as `p.write_text` and gave `["f"]`.
#[test]
fn a_variable_after_a_word_char_or_a_dot_is_no_argument() {
    targets(&[
        ("p = 'f'; obj.p.write_text('x')", &[]),
        ("p = 'f'; 9p.touch()", &[]),
        ("p = 'f'; $p.touch()", &["f"]),
        ("p = 'f'; a$p.touch()", &["f"]),
        ("p='f'; m='w'; x.p.open(m)", &[]),
        ("p='f'; m='w'; p.open(m)", &["f"]),
        ("ä = 'u'; open(ä, 'w'); é.p.touch()", &[]),
        ("p='uu'; é.p.touch(); ép.touch(); .p.touch()", &["uu"]),
        ("x = Path('h'); x.touch()", &["h"]),
        ("p='f'; Path(p).touch()", &["f"]),
        // A literal or `Path(` later in the same run still matches there.
        (".xr'a'.touch()", &["a"]),
        ("q.xb'c'.open('w')", &["c"]),
        ("o.x__L9__ ; .yPath('d').touch()", &["d"]),
        (".zz$p.touch()", &[]),
    ]);
}

/// `MODE`'s look-ahead: a mode variable that is called, indexed or a member is no mode, and
/// past `mode=` 1.x reads `mode` itself as the variable. Red against the pattern with the
/// look-ahead dropped: `open('f', m(1))` gave `["f"]`, and `open('f', mode=g(x))` gave `[]`.
#[test]
fn a_called_mode_variable_is_no_mode_and_mode_equals_falls_back_to_mode() {
    targets(&[
        ("m='w'; open('f', m(1))", &[]),
        ("m='w'; open('f', m .x)", &[]),
        ("m='w'; open('f', m [0])", &[]),
        ("m='w'; open('f', m)", &["f"]),
        ("mode='w'; open('f', mode=g(x))", &["f"]),
        ("mode='r'; open('f', mode=g(x))", &[]),
        ("mode='w'; Path('f').open(mode=g(x))", &["f"]),
        ("m='w'; Path('f').open(m(1))", &[]),
        ("m='w'; Path('f').open(m)", &["f"]),
        ("m='w'; Path('f').open(mode = m)", &["f"]),
        ("mode='w'; open('mm', mode=mode(1))", &["mm"]),
        ("m='w'; open('nb', m)x", &["nb"]),
    ]);
}

/// Each writer of `CODE_WRITES`, perl's opens and the assignments, as 1.x reads them.
#[test]
fn every_writer_and_assignment_reads_as_in_1x() {
    targets(&[
        ("Path('f').open('w')", &["f"]),
        ("pathlib.Path('g').write_bytes(b'')", &["g"]),
        ("open('j', 'w+')", &["j"]),
        ("open('k', 'rb')", &[]),
        ("os.open('l', os.O_WRONLY)", &["l"]),
        ("os.open('l2', os.O_RDONLY)", &[]),
        ("open(my $fh, '>>', 'p2.txt')", &["p2.txt"]),
        ("open(my $fh, '<', 'p3.txt')", &[]),
        ("File.write('rb.txt', 'x')", &["rb.txt"]),
        ("IO.binwrite('rb2', x)", &["rb2"]),
        ("[IO.File]::WriteAllText('ps.txt', 'x')", &["ps.txt"]),
        ("fs.renameSync(a, 'ren')", &["ren"]),
        ("os.rename(f(a), 'ren2')", &["ren2"]),
        ("shutil.copyfile(open(x).name, 'cf')", &["cf"]),
        ("os.symlink('t', 'lnk')", &["lnk"]),
        ("fs.createWriteStream('ws')", &["ws"]),
        ("fs.appendFileSync(\"ap\", 'x')", &["ap"]),
        ("writeJson('wj', {})", &["wj"]),
        ("x = ''; open(x, 'w')", &[]),
        ("open('', 'w')", &[]),
        ("p = rb'raw'; open(p, 'w')", &["raw"]),
        ("open(f'fs', 'w')", &["fs"]),
        ("m = 'w'\nm = 'r'\nopen('z', m)", &[]),
        ("m == 'w'; open('z2', m)", &[]),
        ("m='w' ; open('z3', $m)", &[]),
        ("$m='w'; open('z4', m)", &["z4"]),
        ("x.m='w'; open('z5', m)", &[]),
        ("open( 'sp' ,   'w' )", &["sp"]),
        ("open('a' 'w')", &[]),
        ("Path ( 'pw' ) . write_text('x')", &["pw"]),
        ("pathlib . Path('pl').touch()", &["pl"]),
        ("open(Path('op'), 'w')", &["op"]),
        ("x.open('o', 'w')", &["o"]),
    ]);
}

/// Perl's two-argument open, `open(FH, '>file')`: 1.x's `^\s*\+?>>?\s*(\S.*)$`, which gives
/// the second `>` back when nothing follows it.
#[test]
fn perls_two_argument_open_reads_as_in_1x() {
    targets(&[
        ("open(FH, '>out.txt')", &["out.txt"]),
        ("open(FH, '>>out2.txt')", &["out2.txt"]),
        ("open(FH, '+< out3.txt')", &[]),
        ("open(FH, '>')", &[]),
        ("open(FH, '>>')", &[">"]),
        ("open(FH, '>> ')", &["> "]),
        ("open(FH, '>>\n')", &[]),
        ("open(FH, '>\nfile')", &["file"]),
        ("open(FH, '>file\nx')", &[]),
        ("open(FH, 'out4.txt')", &[]),
    ]);
}

fn patches(table: &[(&str, bool)]) {
    for &(code, want) in table {
        assert_eq!(CODE.runs_patch(code), want, "{code:?}");
    }
}

/// 1.x `codePatches`: git apply and am, past git's options, in a list too; a patch command in
/// a string, after an operator, a directory or a wrapper, or handed to a call; escapes
/// decoded. Red against the patterns with their look-aheads dropped: `git apply.x` and
/// `['patch', 'a]b.diff']` read as patches.
#[test]
fn code_that_runs_git_apply_git_am_or_patch_is_spotted() {
    patches(&[
        (
            "import subprocess; subprocess.run(['git', 'apply', 'x.diff'])",
            true,
        ),
        ("os.system('git apply x.diff')", true),
        ("os.system('git -C dir apply x')", true),
        ("os.system('git --grep apply x')", true),
        ("os.system('git -c a=b am x')", true),
        ("os.system('git apply.x')", false),
        ("os.system('git applyx')", false),
        ("os.system('git-apply x')", true),
        ("os.system('git-am')", true),
        ("git args: apply", true),
        ("os.system('patch -p1 < x.diff')", true),
        ("os.system('patch<x.diff')", true),
        ("x = 'patch'", false),
        ("method = 'patch'", false),
        ("requests.patch('https://example.com')", false),
        ("run(['patch', '-p1'])", true),
        ("run(['patch', 'x.diff'])", true),
        ("run(['patch', 'a', 'x.patch'])", true),
        ("run(['patch', 'a', 'x.txt'])", true),
        ("run(['/usr/bin/patch'])", true),
        ("os.system('patch')", true),
        ("os.system(' timeout 5 patch ')", true),
        ("os.system('exec  patch')", true),
        ("'cat x.diff|patch'", true),
        ("'cat x.diff|exec patch'", true),
        ("'cat x.diff|timeout -k 5 10 patch'", true),
        ("x;patch\ny", true),
        ("x;patch", true),
        ("x | /usr/bin/gpatch;", true),
        ("nohup patch -p1", true),
        ("busybox patch -p1", true),
        ("command gpatch -R", true),
        ("echo patching", false),
        ("dispatch -x", false),
        ("x/patch -x", true),
        ("(patch -p1)", true),
        ("[patch -x]", true),
        ("patch'", false),
        ("\"patch\", \"-p1\"", true),
        ("spawnSync('patch', ['x'])", true),
        ("execFileSync(\"gpatch\")", true),
        ("check_output([ 'patch' ])", true),
        ("Popen(['git','apply'])", true),
        ("git  am", true),
        ("git --work-tree=x apply", true),
        (
            "git -C a -C b -C c -C d -C e -C f -C g -C h -C i apply",
            false,
        ),
        ("x = ['patch', 'a]b.diff']", false),
        ("x = ['patch', 'a\nb.diff']", false),
    ]);
}

/// 1.x `decodeEscapes` before the patterns: hex, `\u`, `\u{}` and octal escapes, and the
/// whitespace letters as spaces; a code point past U+10FFFF stays as written, and a
/// surrogate one reads as JS's lone surrogate does.
#[test]
fn string_escapes_are_decoded_before_the_patch_patterns() {
    patches(&[
        ("\\x67it apply x", true),
        ("os.system('\\x70atch -p1')", true),
        ("os.system('\\u0070atch -p1')", true),
        ("os.system('\\u{70}atch -p1')", true),
        ("os.system('\\160atch -p1')", true),
        ("os.system('patch\\t-p1')", true),
        ("os.system('git\\napply')", true),
        ("os.system('\\u{110000}patch -p1')", false),
        ("\\ud800 git -C \\ud800 apply", true),
    ]);
}

/// 1.x `PATCH_CMD_CODE`'s `[^\]\n]{0,160}?` counts UTF-16 units, so 80 astral chars are past
/// it and 150 `é` are not. Red against `regex`'s `{0,160}`, which counts chars: the 80 astral
/// chars read as a patch.
#[test]
fn a_listed_patch_file_is_looked_for_within_160_utf16_units() {
    let listed = |name: &str| format!("x = ['patch', '{name}.diff']");
    for (name, want) in [
        ("😀".repeat(70), true),
        ("😀".repeat(80), false),
        ("é".repeat(150), true),
        ("e".repeat(160), false),
    ] {
        assert_eq!(CODE.runs_patch(&listed(&name)), want, "{name:?}");
    }
}

/// 1.x `shell.test.mjs`, "the interpreter-code patterns stay linear on long input", at 2^15
/// repeats of each shape (OC-4: decisions, no clock).
#[test]
fn the_patch_patterns_stay_linear_on_long_input() {
    let n = 1 << 15;
    for (code, want) in [
        (format!("{}x", "patch ".repeat(n)), true),
        ("|patch".repeat(n), true),
        (format!("{}x", "git -c a=b ".repeat(n)), false),
        (format!("'{}patch", " ".repeat(n * 4)), false),
        (format!("|{}x", "timeout -k 5 ".repeat(n)), false),
        (format!("|{}x", "exec ".repeat(n)), false),
        (format!("{}x", "system('timeout ".repeat(n)), false),
        (format!("|timeout{} 5x", " -s".repeat(n * 2)), false),
        ("\\x2".repeat(n * 2), false),
        (format!("x = ['patch', {}", "'a'".repeat(n)), false),
        ("['patch',".repeat(n), false),
    ] {
        assert_eq!(CODE.runs_patch(&code), want, "{:?}", code.get(..20));
    }
}

/// 1.x `shell.test.mjs`, "code and word scans stay roughly linear on pathological input",
/// at 2^17 chars, with the shapes the look-around emulation retries on: a long run before a
/// bare variable, and starts whose mode is called, 2^15 times (OC-4: decisions, no clock).
#[test]
fn code_scans_stay_linear_on_pathological_input() {
    let n = 1 << 17;
    let r = 1 << 15;
    let long = [
        (format!("copy( {}", "x ".repeat(n / 2)), Vec::<&str>::new()),
        (format!("cp({}", "a".repeat(n)), vec![]),
        (format!("copy({}", " ".repeat(n)), vec![]),
        (format!("\"\"\"{}", "\\\"".repeat(n / 2)), vec![]),
        (format!("'{}", "a".repeat(n)), vec![]),
        (format!("\"\"\"{}", "x".repeat(n)), vec![]),
        (format!("{}x", "\"".repeat(n)), vec![]),
        (format!("p='f'; .{}.touch()", "a".repeat(n)), vec![]),
        (format!("p='f'; m='w'; .{}.open(m)", "a".repeat(n)), vec![]),
        (format!("p='f'; {}", ".p.touch() ".repeat(r)), vec![]),
        ("x.Path('a').touch() ".repeat(r), vec!["a"]),
        (format!("m='w'; {}", "open('f', m(".repeat(r)), vec![]),
        (format!("m='w'; {}", "x.open(m(".repeat(r)), vec![]),
        (
            format!("mode='w'; {}", "open('f', mode=g(".repeat(r)),
            vec!["f"],
        ),
        (format!("p='f'; {}", "a$p.touch() ".repeat(r)), vec!["f"]),
        (format!("open({}p, 'w')", " ".repeat(n)), vec![]),
        (format!("{}.touch()", "__L0__".repeat(n / 6)), vec![]),
    ];
    for (code, want) in &long {
        assert_eq!(&CODE.write_targets(code), want, "{:?}", code.get(..30));
    }
}
