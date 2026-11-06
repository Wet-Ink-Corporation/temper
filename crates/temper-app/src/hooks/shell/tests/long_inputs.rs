//! Long inputs: 2^15 to 2^17 repeats of each shape a scan starts at, with the decisions 1.x
//! makes on the same input (`node` on `scripts/lib/shell.mjs`, recorded in the port plan's
//! Phase 2e progress). A scan that read back over its input from each start would take
//! minutes here; each of these reads it forward once per level.

use super::{
    CODE, Command, Dir, ParseOpts, command_deletes, command_writes, cx, parse_commands, parse_with,
    path_of,
};
use temper_domain::PathStyle;

use crate::hooks::shell::walk::TooLong;
use crate::hooks::shell::{Guard, PathCx, Word};

const N15: usize = 1 << 15;
const N17: usize = 1 << 17;

/// What 1.x decides on a command line: how many commands, how many writes, the first
/// write's path and how many deletes.
#[derive(Debug, PartialEq, Eq)]
struct Decision {
    commands: usize,
    writes: usize,
    first: Option<String>,
    deletes: usize,
}

fn decide(cmd: &str) -> Decision {
    let opts = ParseOpts {
        cwd: Some("/r"),
        ..super::opts()
    };
    let commands = parse_with(cmd, opts);
    let writes: Vec<_> = commands
        .iter()
        .flat_map(|c| command_writes(c, cx(), &CODE))
        .collect();
    Decision {
        commands: commands.len(),
        writes: writes.len(),
        first: writes.first().and_then(path_of),
        deletes: commands
            .iter()
            .map(|c| command_deletes(c, cx()).len())
            .sum(),
    }
}

/// `shape` `n` times, then `tail`, reads as `(commands, writes, first)`.
fn expect(shape: &str, n: usize, tail: &str, want: (usize, usize, Option<&str>)) {
    expect_in(("", tail), shape, n, want);
}

/// `around.0`, then `shape` `n` times, then `around.1`, reads as `(commands, writes, first)`.
fn expect_in(around: (&str, &str), shape: &str, n: usize, want: (usize, usize, Option<&str>)) {
    let (commands, writes, first) = want;
    let (head, tail) = around;
    let got = decide(&[head, shape.repeat(n).as_str(), tail].concat());
    let want = Decision {
        commands,
        writes,
        first: first.map(str::to_owned),
        deletes: 0,
    };
    assert_eq!(got, want, "{head:?}, {shape:?} x {n}, {tail:?}");
}

const N16: usize = 1 << 16;
const O: Option<&str> = Some("/r/o");

#[test]
fn unclosed_quotes_and_expansions_repeated_read_as_1x_reads_them() {
    expect("'", N17, " > o", (1, 1, O));
    expect("\"", N17, " > o", (1, 1, O));
    expect("$(", N17, "> o", (6, 0, None));
    expect("${", N17, "> o", (1, 0, None));
    expect("`", N17, "> o", (1, 1, O));
    expect("<<", N17, "> o", (1, 1, O));
    expect(">&", N17, " o", (1, 1, O));
    expect("(", N17, "> o", (0, 0, None));
    expect("\\", N17, "> o", (1, 1, O));
    expect("$'\\x41", N17, "' > o", (1, 0, None));
    expect("$\"", N17, "\" > o", (1, 0, None));
    expect("<(", N17, "> o", (6, 0, None));
    expect("$((", N17, "> o", (1, 0, None));
    expect("&", N17, "> o", (1, 1, O));
    expect(";", N17, "> o", (1, 1, O));
}

#[test]
fn closed_words_and_substitutions_repeated_read_as_1x_reads_them() {
    expect("'a' ", N17, "> o", (1, 1, O));
    expect("\"a\" ", N17, "> o", (1, 1, O));
    expect("$(x) ", N17, "> o", (N17 + 1, 1, O));
    expect("${x} ", N17, "> o", (1, 1, O));
    expect("`x` ", N17, "> o", (N17 + 1, 1, O));
    expect("cat <<E\n", N17, "E\n> o", (2, 1, O));
    expect("echo >&2 ", N17, "> o", (1, 1, O));
    expect("( ", N17, ") > o", (1, 1, O));
    expect("\\a ", N17, "> o", (1, 1, O));
    expect("# x\n", N17, "> o", (1, 1, O));
    expect("{ ", N17, "> o; }", (1, 1, O));
    expect("if true; then ", N17, "> o; fi", (N17 + 1, 1, O));
    expect("a | ", N17, "tee o", (N17 + 1, 1, O));
    expect("cp a ", N17, "o", (1, 1, O));
}

#[test]
fn wrappers_and_inner_scripts_repeated_read_as_1x_reads_them() {
    expect("sudo ", N17, "cp a o", (1, 1, O));
    expect("env -u x ", N17, "cp a o", (1, 1, O));
    expect("xargs ", N17, "cp a o", (1, 1, None));
    expect("git -c alias.x=y ", N17, "apply o.diff", (1, 0, None));
    expect("find . -exec ", N17, "cp a o \\;", (6, 0, None));
    expect(
        "awk 'BEGIN{system(\"",
        N17,
        "cp a o\")}'",
        (43_693, 0, None),
    );
    expect("system(\"", N17, "", (65_537, 0, None));
    expect("bash -c '", N17, "cp a o'", (3, 0, None));
    expect("eval ", N17, "cp a o", (6, 0, None));
}

#[test]
fn variables_and_git_paths_repeated_read_as_1x_reads_them() {
    expect(
        "echo $(git rev-parse --git-path ",
        N17,
        "x) > o",
        (6, 0, None),
    );
    expect("--git-path ", N17, "> o", (1, 1, O));
    expect("X=$X$X; ", N17, "echo > o", (1, 1, O));
    expect("X=a; ", N17, "echo > $X", (1, 1, Some("/r/a")));
    expect("popd; ", N17, "> o", (1, 1, None));
}

/// A scope keeps one old value per name, so a value grown inside `(`, `$(` or `bash -c` costs
/// its final length, not the sum of every earlier one (#201 F1: 4,197 MB at 2^16 before).
#[test]
fn a_variable_grown_inside_a_scope_reads_as_1x_reads_it() {
    let grow = "a=$a/b; ";
    let deep = ["/r/x", &"/b".repeat(N16)].concat();
    expect_in(("( a=x; ", ") > o"), grow, N16, (1, 1, O));
    expect_in(("echo $( a=x; ", ") > o"), grow, N16, (1, 1, O));
    expect_in(("bash -c 'a=x; ", "' > o"), grow, N16, (1, 1, O));
    expect_in(("( a=x; ", "echo > $a)"), grow, N16, (1, 1, Some(&deep)));
    expect_in(("( a=x; ", ") ; echo > $a"), grow, N16, (1, 1, None));
}

#[test]
fn long_rm_operand_lists_are_each_a_delete() {
    let got = decide(&"rm -rf a ".repeat(N17));
    assert_eq!((got.commands, got.writes, got.deletes), (1, 0, 2 * N17 - 1));
}

/// `X=$X$X` doubles the value: 28 doublings make a 2^28-char path, which 1.x reads; the 29th
/// passes V8's longest string, where 1.x throws (`RangeError: Invalid string length`).
#[test]
fn a_value_doubled_past_v8s_longest_string_stops_the_reader_as_1x_throws() {
    let doubled = |k: usize| ["X=a; ", &"X=$X$X; ".repeat(k), "echo > $X"].concat();
    let opts = || ParseOpts {
        cwd: Some("/r"),
        ..super::opts()
    };
    // The target's value, not its resolved path: normalising 2^28 chars is slow unoptimised.
    let commands = parse_commands(&doubled(28), opts()).map(|c| {
        let w: Vec<_> = c
            .iter()
            .flat_map(|c| command_writes(c, cx(), &CODE))
            .collect();
        w.first().and_then(|t| t.word.value()).map(str::len)
    });
    assert_eq!(commands, Ok(Some(1 << 28)));
    assert_eq!(
        parse_commands(&doubled(29), opts()).map(|c| c.len()),
        Err(TooLong)
    );
    // `+=` appends, and is held to the same bound.
    let appended = ["X=a; ", &"X+=$X; ".repeat(29), "echo > $X"].concat();
    assert_eq!(
        parse_commands(&appended, opts()).map(|c| c.len()),
        Err(TooLong)
    );
}

/// Each `tail`, after `$X` is set to 2^27 chars, stops the reader with `TooLong`.
fn too_long_after_x27(tails: &[&str]) {
    let x27 = ["X=a; ", &"X=$X$X; ".repeat(27)].concat();
    for tail in tails {
        let opts = ParseOpts {
            cwd: Some("/r"),
            ..super::opts()
        };
        let got = parse_commands(&[x27.as_str(), tail].concat(), opts).map(|c| c.len());
        assert_eq!(got, Err(TooLong), "{tail}");
    }
}

/// A directory past V8's longest string stops the reader too, where 1.x's `path.resolve`
/// throws (#201 F3). `$X` is 2^27 chars; four or five of them pass it.
#[test]
fn a_directory_past_v8s_longest_string_stops_the_reader_as_1x_throws() {
    too_long_after_x27(&[
        "cd $X; cd $X; cd $X; cd $X; cd $X; > clippy.toml",
        "pushd $X; pushd $X; pushd $X; pushd $X; pushd $X; > clippy.toml",
        "(cd $X; cd $X; cd $X; cd $X; cd $X); > clippy.toml",
        "cd $X; cd $X; cd $X; sudo -C $X ls; > clippy.toml",
        "cd $X; cd $X; cd $X; env -C $X ls; > clippy.toml",
    ]);
}

/// So does an inner script joined from words, where 1.x's `join(' ')` throws (#201 F3).
#[test]
fn a_joined_inner_script_past_v8s_longest_string_stops_the_reader_as_1x_throws() {
    too_long_after_x27(&[
        "eval $X $X $X $X $X; > clippy.toml",
        "watch $X $X $X $X $X; > clippy.toml",
        "env -S $X $X $X $X $X; > clippy.toml",
        "pwsh -c $X $X $X $X $X; > clippy.toml",
    ]);
}

/// The directory grows with each `cd`, so these cost the directory's length per `cd`; 1.x
/// takes 46 s to 66 s on 2^15 of them.
#[test]
fn cd_and_pushd_repeated_land_the_write_under_every_directory() {
    let deep = ["/r", &"/a".repeat(N15), "/o"].concat();
    expect("cd a; ", N15, "cp a o", (1, 1, Some(&deep)));
    expect("cd a && ", N15, "> o", (1, 1, Some(&deep)));
    expect("pushd a; ", N15, "> o", (1, 1, Some(&deep)));
}

/// A `cd` of several names, or with `.` in it, steps from the directory too (#201 F2: 17.9 s
/// for `cd a/b; ` x 2^15 before; 1.x takes 99 s and 46 s on these).
#[test]
fn a_cd_of_several_names_repeated_lands_the_write_under_every_one() {
    let ab = ["/r", &"/a/b".repeat(N15), "/o"].concat();
    let a = ["/r", &"/a".repeat(N15), "/o"].concat();
    expect("cd a/b; ", N15, "> o", (1, 1, Some(&ab)));
    expect("cd ./a; ", N15, "> o", (1, 1, Some(&a)));
}

/// The commands' directories between them, each shared node counted once.
fn held(commands: &[Command]) -> usize {
    Dir::held_bytes(commands.iter().filter_map(|c| c.cwd.as_ref()))
}

/// Each directory holds its parent and the names it adds, so a line of `pushd` or `cd`
/// holds each name once (port plan item 50). As whole strings, as in 1.x, the first shape
/// held 1,161 MB and the second 2,763 MB (7.2 s), and `cd $d; ` x 4000 took 113 s.
#[test]
fn a_line_of_cds_holds_each_name_once_however_deep_it_goes() {
    let r = || ParseOpts {
        cwd: Some("/r"),
        ..super::opts()
    };
    let pushed = parse_with(
        &["pushd a; : ; ".repeat(N15), "> o".to_owned()].concat(),
        r(),
    );
    assert_eq!(pushed.len(), N15.saturating_add(1));
    let deep = ["/r", &"/a".repeat(N15)].concat();
    assert_eq!(dir_of(pushed.last()), Some(deep));
    assert!(held(&pushed) < 4 * N15, "{} bytes", held(&pushed));

    let d = ["d=a; ", &"d=$d/a; ".repeat(4000)].concat();
    let d_value = ["a", &"/a".repeat(4000)].concat();
    let line = [d.as_str(), &"cd $d; : ; ".repeat(600), "> o"].concat();
    let ran = parse_with(&line, r());
    assert_eq!(ran.len(), 601);
    let want = ["/r", &["/", d_value.as_str()].concat().repeat(600)].concat();
    assert_eq!(dir_of(ran.last()), Some(want));
    assert!(held(&ran) < 601 * 8100, "{} bytes", held(&ran));

    let line = [d.as_str(), &"cd $d; ".repeat(4000), "> o"].concat();
    assert!(line.len() < 64 * 1024);
    let ran = parse_with(&line, r());
    let want = ["/r", &["/", d_value.as_str()].concat().repeat(4000), "/o"].concat();
    let first = ran
        .iter()
        .flat_map(|c| command_writes(c, cx(), &CODE))
        .next();
    assert_eq!(first.and_then(|t| t.path(cx())), Some(want));
}

/// A `..` takes a name off without copying the directory, and several in one value step up
/// together, so these land where the full resolve does.
#[test]
fn a_line_of_cd_up_steps_back_through_the_shared_names() {
    let r = || ParseOpts {
        cwd: Some("/r"),
        ..super::opts()
    };
    let up = [
        "cd ",
        &"a/".repeat(N15),
        "; ",
        &"cd ..; ".repeat(N15 - 1),
        "> o",
    ]
    .concat();
    assert_eq!(
        first_write(&parse_with(&up, r())),
        Some("/r/a/o".to_owned())
    );
    // `$u` climbs the 4001 names `$d` adds, so each pair comes back to `/r`.
    let d = [
        "d=a; ",
        &"d=$d/a; ".repeat(4000),
        "u=..; ",
        &"u=$u/..; ".repeat(4000),
    ]
    .concat();
    let line = [d.as_str(), &"cd $d; cd $u; ".repeat(2000), "cd a; > o"].concat();
    assert_eq!(
        first_write(&parse_with(&line, r())),
        Some("/r/a/o".to_owned())
    );
}

/// The text of a command's directory.
fn dir_of(c: Option<&Command>) -> Option<String> {
    c.and_then(|c| c.cwd.as_ref())
        .map(|d| d.text().into_owned())
}

/// The path of a line's first write.
fn first_write(commands: &[Command]) -> Option<String> {
    let first = commands
        .iter()
        .flat_map(|c| command_writes(c, cx(), &CODE))
        .next();
    first.and_then(|t| t.path(cx()))
}

/// Past its budget of directory text built for the rules, the line stops as at V8's longest
/// string. Only text built counts: a root's is borrowed, and costs nothing (#202 F1).
#[test]
fn the_directory_text_a_line_builds_is_held_to_a_budget() {
    let name = "a".repeat(1 << 20);
    let root = Dir::resolved(["/", name.as_str()].concat());
    let Some(deeper) = Dir::resolved("/").step(PathStyle::Posix, &name) else {
        panic!("a name steps from the root");
    };
    let budget = 64 << 20;
    let guard = Guard::with_budget(budget);
    // A root is free however often it is read: 1,024 reads of 1 MiB, past the budget.
    for _ in 0..(1 << 10) {
        assert_eq!(guard.dir_text(&root).as_deref(), Some(root.text().as_ref()));
    }
    let calls = budget / deeper.len();
    for _ in 0..calls {
        assert_eq!(
            guard.dir_text(&deeper).as_deref(),
            Some(root.text().as_ref())
        );
    }
    assert!(!guard.overflowed());
    assert_eq!(guard.dir_text(&deeper), None);
    assert!(guard.overflowed());
    assert_eq!(Dir::BUDGET, 2 << 30);
}

/// #202 F2: a word that is one expansion shares its value, as V8 shares a string, so a line
/// holds `$X` once however often it names it. Each line held a copy per word: with `$X` of
/// 2^24 chars, `: $X; ` x 2000 was OOM-killed at 10.9 GB, and `$PWD` and `$OLDPWD` the same.
#[test]
fn a_word_of_one_expansion_shares_the_value_however_often_a_line_names_it() {
    let r = || ParseOpts {
        cwd: Some("/r"),
        ..super::opts()
    };
    let x = ["X=a; ", &"X=$X$X; ".repeat(20)].concat();
    let shapes = [
        [x.as_str(), &": $X; ".repeat(2000)].concat(),
        [x.as_str(), "cd /; cd $X; ", &": \"$PWD\"; ".repeat(2000)].concat(),
        [x.as_str(), "cd $X; cd /; ", &": $OLDPWD; ".repeat(2000)].concat(),
    ];
    for line in &shapes {
        let ran = parse_with(line, r());
        let texts: Vec<&str> = ran.iter().flat_map(|c| &c.args).map(Word::text).collect();
        assert_eq!(texts.len(), 2000);
        let Some(first) = texts.first() else {
            panic!("the line has words");
        };
        assert!(first.len() >= 1 << 20, "{}", first.len());
        let shared = texts
            .iter()
            .all(|t| std::ptr::eq(t.as_ptr(), first.as_ptr()));
        let shape = line.get(line.len().saturating_sub(12)..).unwrap_or("");
        assert!(shared, "each word of {shape:?} holds its own copy");
    }
}

/// #202 re-review M2: a command name is read from the end of its word, and the targets'
/// paths from words read once, so neither keeps a joined copy: `$X/a; ` x 2000 with a
/// 2^24-char `$X` held 12 GB, a copy per command word.
#[test]
fn command_names_and_target_paths_keep_no_joined_copy_of_their_words() {
    let x = ["X=a; ", &"X=$X$X; ".repeat(16)].concat();
    let line = [
        x.as_str(),
        &"$X/a; ".repeat(200),
        &"echo > $X/b; ".repeat(200),
    ]
    .concat();
    let ran = parse_with(&line, super::opts());
    let names: Vec<_> = ran.iter().filter_map(|c| c.name.as_deref()).collect();
    assert_eq!(names.iter().filter(|&&n| n == "a").count(), 200);
    let paths: Vec<_> = ran
        .iter()
        .flat_map(|c| command_writes(c, cx(), &CODE))
        .filter_map(|t| path_of(&t))
        .collect();
    assert_eq!(paths.len(), 200);
    assert!(
        paths
            .iter()
            .all(|p| p.ends_with("a/b") && p.len() > 1 << 16)
    );
    let joined = ran
        .iter()
        .flat_map(|c| std::iter::once(&c.argv0).chain(c.redirects.iter().map(|r| &r.word)))
        .filter(|w| w.text.joined())
        .count();
    assert_eq!(joined, 0, "words kept a joined copy");
}

/// #202 F2: a word that holds an expansion among other text (`$X/a`) shares the value too,
/// and its text is joined only when a rule reads it, as V8 joins strings: 1.x answers
/// `: $X/a; ` x 140 with a 2^24-char `$X` in 0.16 s, where each word copied it (2.1 GB). No
/// budget is spent on it until a word is read: the budget has room for the two walks and the
/// one read below (#202 re-review M2 charges that read), and none for a copy per word.
#[test]
fn a_word_with_an_expansion_in_it_joins_its_text_only_when_read() {
    let guard = Guard::with_budget(3 << 20);
    let opts = ParseOpts {
        paths: PathCx {
            guard: Some(&guard),
            ..cx()
        },
        ..super::opts()
    };
    let x = ["X=a; ", &"X=$X$X; ".repeat(20)].concat();
    let line = [x.as_str(), &": $X/a; ".repeat(2000), "> o"].concat();
    let ran = parse_commands(&line, opts).map(|c| c.len());
    assert_eq!(ran, Ok(2001));
    let Ok(ran) = parse_commands(&line, opts) else {
        panic!("the line is read");
    };
    let words: Vec<&Word> = ran.iter().flat_map(|c| &c.args).collect();
    assert_eq!(words.len(), 2000);
    let joined = words.iter().filter(|w| w.text.joined()).count();
    assert_eq!(joined, 0, "words joined unread");
    let Some(last) = words.last() else {
        panic!("the line has words");
    };
    assert_eq!(last.text().len(), (1 << 20) + 2);
    assert!(last.text().ends_with("a/a"));
    assert!(last.text.joined());
}

/// #202 re-review M2: an inner script (`bash -c`, `eval`, `watch`, `env -S`, `pwsh -c`) is
/// read from its words once, charged and not kept, and its walk is charged the text its
/// words copy, so a line of them is held to the budget: `bash -c $X/a; ` x 200 with a
/// 2^24-char `$X` passed 3.7 GB. Here each of 40 scripts is 2^16 + 2 bytes: read alone they
/// cost 2.5 MiB, read and walked 5 MiB.
#[test]
fn an_inner_script_is_charged_its_read_and_its_walk_and_keeps_no_join() {
    let x = ["X=a; ", &"X=$X$X; ".repeat(16)].concat();
    let within = |line: &str, budget: usize| {
        let guard = Guard::with_budget(budget);
        let opts = ParseOpts {
            paths: PathCx {
                guard: Some(&guard),
                ..cx()
            },
            ..super::opts()
        };
        parse_commands(line, opts).map(|ran| {
            let kept = ran
                .iter()
                .flat_map(|c| std::iter::once(&c.argv0).chain(&c.args))
                .filter(|w| w.text.joined())
                .count();
            (ran.len(), kept)
        })
    };
    for (each, commands) in [
        ("bash -c $X/a; ", 80),
        ("eval $X/a; ", 80),
        // `watch` and `env -S` leave only the command they run.
        ("watch $X/a; ", 40),
        ("env -S $X/a; ", 40),
        ("pwsh -c $X/a; ", 80),
    ] {
        let line = [x.as_str(), &each.repeat(40), "true"].concat();
        assert_eq!(within(&line, 6 << 20), Ok((commands + 1, 0)), "{each}");
        assert_eq!(within(&line, 4 << 20), Err(TooLong), "{each}");
    }
}
