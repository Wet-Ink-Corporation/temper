//! Long inputs: 2^15 to 2^17 repeats of each shape a scan starts at, with the decisions 1.x
//! makes on the same input (`node` on `scripts/lib/shell.mjs`, recorded in the port plan's
//! Phase 2e progress). A scan that read back over its input from each start would take
//! minutes here; each of these reads it forward once per level.

use super::{ParseOpts, command_deletes, command_writes, cx, parse_commands, parse_with, path_of};
use crate::hooks::shell::walk::TooLong;

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
        .flat_map(|c| command_writes(c, cx()))
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

fn expect(shape: &str, n: usize, tail: &str, commands: usize, writes: usize, first: Option<&str>) {
    let got = decide(&[shape.repeat(n).as_str(), tail].concat());
    let want = Decision {
        commands,
        writes,
        first: first.map(str::to_owned),
        deletes: 0,
    };
    assert_eq!(got, want, "{shape:?} x {n} then {tail:?}");
}

const O: Option<&str> = Some("/r/o");

#[test]
fn unclosed_quotes_and_expansions_repeated_read_as_1x_reads_them() {
    expect("'", N17, " > o", 1, 1, O);
    expect("\"", N17, " > o", 1, 1, O);
    expect("$(", N17, "> o", 6, 0, None);
    expect("${", N17, "> o", 1, 0, None);
    expect("`", N17, "> o", 1, 1, O);
    expect("<<", N17, "> o", 1, 1, O);
    expect(">&", N17, " o", 1, 1, O);
    expect("(", N17, "> o", 0, 0, None);
    expect("\\", N17, "> o", 1, 1, O);
    expect("$'\\x41", N17, "' > o", 1, 0, None);
    expect("$\"", N17, "\" > o", 1, 0, None);
    expect("<(", N17, "> o", 6, 0, None);
    expect("$((", N17, "> o", 1, 0, None);
    expect("&", N17, "> o", 1, 1, O);
    expect(";", N17, "> o", 1, 1, O);
}

#[test]
fn closed_words_and_substitutions_repeated_read_as_1x_reads_them() {
    expect("'a' ", N17, "> o", 1, 1, O);
    expect("\"a\" ", N17, "> o", 1, 1, O);
    expect("$(x) ", N17, "> o", N17 + 1, 1, O);
    expect("${x} ", N17, "> o", 1, 1, O);
    expect("`x` ", N17, "> o", N17 + 1, 1, O);
    expect("cat <<E\n", N17, "E\n> o", 2, 1, O);
    expect("echo >&2 ", N17, "> o", 1, 1, O);
    expect("( ", N17, ") > o", 1, 1, O);
    expect("\\a ", N17, "> o", 1, 1, O);
    expect("# x\n", N17, "> o", 1, 1, O);
    expect("{ ", N17, "> o; }", 1, 1, O);
    expect("if true; then ", N17, "> o; fi", N17 + 1, 1, O);
    expect("a | ", N17, "tee o", N17 + 1, 1, O);
    expect("cp a ", N17, "o", 1, 1, O);
}

#[test]
fn wrappers_and_inner_scripts_repeated_read_as_1x_reads_them() {
    expect("sudo ", N17, "cp a o", 1, 1, O);
    expect("env -u x ", N17, "cp a o", 1, 1, O);
    expect("xargs ", N17, "cp a o", 1, 1, None);
    expect("git -c alias.x=y ", N17, "apply o.diff", 1, 0, None);
    expect("find . -exec ", N17, "cp a o \\;", 6, 0, None);
    expect("awk 'BEGIN{system(\"", N17, "cp a o\")}'", 43_693, 0, None);
    expect("system(\"", N17, "", 65_537, 0, None);
    expect("bash -c '", N17, "cp a o'", 3, 0, None);
    expect("eval ", N17, "cp a o", 6, 0, None);
}

#[test]
fn variables_and_git_paths_repeated_read_as_1x_reads_them() {
    expect(
        "echo $(git rev-parse --git-path ",
        N17,
        "x) > o",
        6,
        0,
        None,
    );
    expect("--git-path ", N17, "> o", 1, 1, O);
    expect("X=$X$X; ", N17, "echo > o", 1, 1, O);
    expect("X=a; ", N17, "echo > $X", 1, 1, Some("/r/a"));
    expect("popd; ", N17, "> o", 1, 1, None);
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
        let w: Vec<_> = c.iter().flat_map(|c| command_writes(c, cx())).collect();
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

/// The directory grows with each `cd`, so these cost the directory's length per `cd`; 1.x
/// takes 46 s to 66 s on 2^15 of them.
#[test]
fn cd_and_pushd_repeated_land_the_write_under_every_directory() {
    let deep = ["/r", &"/a".repeat(N15), "/o"].concat();
    expect("cd a; ", N15, "cp a o", 1, 1, Some(&deep));
    expect("cd a && ", N15, "> o", 1, 1, Some(&deep));
    expect("pushd a; ", N15, "> o", 1, 1, Some(&deep));
}
