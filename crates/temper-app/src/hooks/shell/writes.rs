//! What a command writes (1.x `shell.mjs` `commandWrites`, `copyArgs`, `psParams`,
//! `psCopyArgs`, `optionValues`, `inPlace`, `inPlaceTargets`, `gitRestore`, `landing`,
//! `pathspecPattern`).
//!
//! Interpreter code and PowerShell expressions that write files (1.x `codeWriteTargets`) are
//! read in port Phase 2f, with the patches and copies onto `.rs` files that read them too.

use std::borrow::Cow;

use temper_domain::PathStyle;

use super::git::git_command;
use super::walk::Command;
use super::words::{Dir, PathCx, Word};
use super::wrappers::cluster_reaches;

/// How a target is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verb<'c> {
    /// A redirection.
    Redirect,
    /// A command named for what it does: `tee`, `cp`, `curl`, `Copy-Item`, ...
    Named(&'c str),
    /// `sed -i` and `perl -i`.
    InPlace(&'c str),
    /// `git mv`.
    GitMv,
    /// `git checkout` or `git restore`, with the revision (`None`: the index).
    GitRestore {
        checkout: bool,
        source: Option<String>,
    },
}

/// A copy's sources and how its destination takes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CopyOp<'c> {
    pub(crate) sources: Vec<Cow<'c, Word>>,
    /// `Some(true)` for `-t` or several sources, `Some(false)` for `-T`, and `None` when it
    /// depends on whether the destination is a directory.
    pub(crate) into_dir: Option<bool>,
    pub(crate) recursive: bool,
}

impl CopyOp<'_> {
    fn into_owned<'x>(self) -> CopyOp<'x> {
        CopyOp {
            sources: self
                .sources
                .into_iter()
                .map(|w| Cow::Owned(w.into_owned()))
                .collect(),
            into_dir: self.into_dir,
            recursive: self.recursive,
        }
    }
}

/// One file a command writes: the word that names it and the directory it is relative to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target<'c> {
    pub(crate) verb: Verb<'c>,
    pub(crate) word: Cow<'c, Word>,
    pub(crate) cwd: Option<Dir>,
    /// For a copy, the destination's sources.
    pub(crate) copy: Option<CopyOp<'c>>,
}

/// 1.x `commandWrites(c)`: the files one command writes, in the order the shell would.
pub(crate) fn command_writes<'c>(c: &'c Command, cx: PathCx<'_>) -> Vec<Target<'c>> {
    let mut out = Vec::new();
    let cwd = || c.cwd.clone();
    let mut add =
        |verb: Verb<'c>, word: Cow<'c, Word>, copy: Option<CopyOp<'c>>, cwd: Option<Dir>| {
            out.push(Target {
                verb,
                word,
                cwd,
                copy,
            });
        };
    for r in &c.redirects {
        if r.op.writes() {
            add(Verb::Redirect, Cow::Borrowed(&r.word), None, cwd());
        }
    }
    let Some(name) = c.name.as_deref().filter(|n| !n.is_empty()) else {
        return out;
    };
    let args = c.args.as_slice();
    let named = |w: Cow<'c, Word>| (Verb::Named(name), w);
    match name {
        "tee" | "sponge" => {
            for a in args.iter().filter(|a| !a.raw.starts_with('-')) {
                let (verb, word) = named(Cow::Borrowed(a));
                add(verb, word, None, cwd());
            }
        }
        "truncate" => {
            let mut k = 0;
            while let Some(a) = args.get(k) {
                if ["-s", "--size", "-r", "--reference"].contains(&a.raw.as_str()) {
                    k = k.saturating_add(1);
                } else if !a.raw.starts_with('-') {
                    let (verb, word) = named(Cow::Borrowed(a));
                    add(verb, word, None, cwd());
                }
                k = k.saturating_add(1);
            }
        }
        "sed" | "perl" if in_place(name == "sed", args) => {
            for a in in_place_targets(args, name == "sed") {
                add(Verb::InPlace(name), Cow::Borrowed(a), None, cwd());
            }
        }
        "curl" => {
            for a in option_values(args, Some('o'), &["--output"], CURL_VALUES) {
                if a.raw != "-" {
                    add(Verb::Named(name), a, None, cwd());
                }
            }
            for a in option_values(args, None, &["--output-dir"], "") {
                add(Verb::Named(name), a, None, cwd());
            }
        }
        "wget" => {
            for a in option_values(args, Some('O'), &["--output-document"], WGET_VALUES) {
                if a.raw != "-" {
                    add(Verb::Named(name), a, None, cwd());
                }
            }
            for a in option_values(args, Some('P'), &["--directory-prefix"], WGET_VALUES) {
                add(Verb::Named(name), a, None, cwd());
            }
        }
        "dd" => {
            for a in args.iter().filter(|a| a.raw.starts_with("of=")) {
                add(Verb::Named(name), Cow::Owned(a.sliced(3, 3)), None, cwd());
            }
        }
        "tar" => {
            let extract = args.iter().enumerate().any(|(j, a)| {
                let r = a.raw.as_str();
                r == "--extract"
                    || r == "--get"
                    || r.strip_prefix('-').is_some_and(letters_with_x)
                    || (j == 0 && letters_with_x(r))
            });
            if extract {
                for a in option_values(args, Some('C'), &["--directory"], "") {
                    add(Verb::Named(name), a, None, cwd());
                }
            }
        }
        "unzip" => {
            for a in option_values(args, Some('d'), &[], "") {
                add(Verb::Named(name), a, None, cwd());
            }
        }
        "cp" | "mv" | "install" | "ln" | "rsync" | "scp" => {
            if let Some((dest, op)) = copy_args(name, args) {
                add(Verb::Named(name), dest, Some(op), cwd());
            }
        }
        _ if is_ps_copy(name) => {
            let moves = matches!(name, "move-item" | "mi" | "move");
            if let Some((dest, op)) = ps_copy_args(moves, args) {
                add(Verb::Named(name), dest, Some(op), cwd());
            }
        }
        _ if is_ps_content(name) => {
            let params = PsParams::new(args, ps_switch);
            if !params.dry_run() {
                let word = match params.get(&["path", "literalpath", "filepath"]) {
                    Some(Param::Word(w)) => Some(w.clone()),
                    Some(Param::Switch | Param::Missing) | None => params.pos.first().cloned(),
                };
                if let Some(word) = word {
                    add(Verb::Named(name), word, None, cwd());
                }
            }
        }
        "git" => git_writes(c, cx, &mut add),
        _ => {}
    }
    out
}

/// The `git mv`, `git checkout` and `git restore` targets of a git command.
fn git_writes<'c>(
    c: &'c Command,
    cx: PathCx<'_>,
    add: &mut impl FnMut(Verb<'c>, Cow<'c, Word>, Option<CopyOp<'c>>, Option<Dir>),
) {
    let Some(g) = git_command(c, cx) else {
        return;
    };
    match g.args {
        Cow::Borrowed(args) => git_targets(&g.sub, args, g.cwd.as_ref(), add),
        // An alias expansion's words live here, so its targets own theirs.
        Cow::Owned(args) => git_targets(
            &g.sub,
            &args,
            g.cwd.as_ref(),
            &mut |verb, word, copy, cwd| {
                let verb = match verb {
                    Verb::GitRestore { checkout, source } => Verb::GitRestore { checkout, source },
                    _ => Verb::GitMv,
                };
                add(
                    verb,
                    Cow::Owned(word.into_owned()),
                    copy.map(CopyOp::into_owned),
                    cwd,
                );
            },
        ),
    }
}

/// The targets of `git <sub> args`.
fn git_targets<'a>(
    sub: &str,
    args: &'a [Word],
    cwd: Option<&Dir>,
    add: &mut impl FnMut(Verb<'a>, Cow<'a, Word>, Option<CopyOp<'a>>, Option<Dir>),
) {
    match sub {
        "mv" => {
            if let Some((dest, mut op)) = copy_args("git", args) {
                op.recursive = true;
                add(Verb::GitMv, dest, Some(op), cwd.cloned());
            }
        }
        "checkout" | "restore" => {
            let restore = git_restore(sub, args);
            for p in restore.paths {
                let verb = Verb::GitRestore {
                    checkout: sub == "checkout",
                    source: restore.source.map(str::to_owned),
                };
                // `Dir` is a shared handle.
                add(verb, Cow::Borrowed(p), None, cwd.cloned());
            }
        }
        _ => {}
    }
}

/// `[a-zA-Z]*x[a-zA-Z]*` as a whole: a letter cluster holding `x`.
fn letters_with_x(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphabetic()) && s.contains('x')
}

/// 1.x `PS_COPY`.
fn is_ps_copy(name: &str) -> bool {
    matches!(
        name,
        "copy-item" | "move-item" | "cpi" | "mi" | "copy" | "move"
    )
}

/// 1.x `PS_CONTENT`.
fn is_ps_content(name: &str) -> bool {
    matches!(
        name,
        "set-content" | "add-content" | "out-file" | "new-item" | "tee-object" | "sc" | "ac" | "ni"
    )
}

/// Short options of curl that take a value, so a cluster stops at them (1.x `CURL_VALUES`).
const CURL_VALUES: &str = "AbcCdDeEFHKmoPQrtTuUwxXyYz";
/// The same for wget (1.x `WGET_VALUES`).
const WGET_VALUES: &str = "aeiBtOoTwQPUlARDIX";

impl Word {
    /// 1.x `{ ...a, raw: a.raw.slice(r), text: a.text.slice(t), value: ... }`.
    pub(super) fn sliced(&self, raw_from: usize, text_from: usize) -> Self {
        let mut word = self.derived(self.raw.get(raw_from..).unwrap_or(""));
        self.text
            .get(text_from..)
            .unwrap_or("")
            .clone_into(&mut word.text);
        word
    }

    /// The part after the first `=`, cut from raw and from text each at its own `=`.
    pub(super) fn after_eq(&self) -> Self {
        let raw = self.raw.find('=').map_or(0, |e| e.saturating_add(1));
        let text = self.text.find('=').map_or(0, |e| e.saturating_add(1));
        self.sliced(raw, text)
    }
}

/// 1.x `optionValues(args, short, long, valueLetters)`: the values of `-o`/`--output`-style
/// options (`-o f`, `-sSLo f`, `-of`, `--output f`, `--output=f`).
fn option_values<'c>(
    args: &'c [Word],
    short: Option<char>,
    long: &[&str],
    value_letters: &str,
) -> Vec<Cow<'c, Word>> {
    let mut out = Vec::new();
    let mut k = 0;
    while let Some(arg) = args.get(k) {
        let a = arg.raw.as_str();
        let eq = long
            .iter()
            .find(|l| a.strip_prefix(**l).is_some_and(|r| r.starts_with('=')));
        if let Some(l) = eq {
            out.push(Cow::Owned(
                arg.derived(a.get(l.len().saturating_add(1)..).unwrap_or("")),
            ));
        } else if long.contains(&a) {
            if let Some(next) = args.get(k.saturating_add(1)) {
                out.push(Cow::Borrowed(next));
                k = k.saturating_add(1);
            }
        } else if let Some(short) = short
            && let Some(cluster) = a
                .strip_prefix('-')
                .filter(|c| c.starts_with(|ch: char| ch.is_ascii_alphabetic()))
        {
            for (j, ch) in cluster.char_indices() {
                let rest = cluster.get(j.saturating_add(ch.len_utf8())..).unwrap_or("");
                if ch == short {
                    if !rest.is_empty() {
                        out.push(Cow::Owned(arg.derived(rest)));
                    } else if let Some(next) = args.get(k.saturating_add(1)) {
                        out.push(Cow::Borrowed(next));
                        k = k.saturating_add(1);
                    }
                    break;
                }
                if value_letters.contains(ch) {
                    if rest.is_empty() {
                        k = k.saturating_add(1);
                    }
                    break;
                }
                if !ch.is_ascii_alphabetic() {
                    break;
                }
            }
        }
        k = k.saturating_add(1);
    }
    out
}

/// 1.x `inPlace(sed, args)`: `sed -i` / `perl -i`, a cluster that reaches `i` before an option
/// that takes a value (`-Mstrict` is not -i).
fn in_place(sed: bool, args: &[Word]) -> bool {
    let flags = if sed { "nErsuz" } else { "nplawsTtUWXc" };
    args.iter().any(|a| {
        let r = a.raw.as_str();
        let short = r.strip_prefix('-').is_some_and(|rest| {
            let next = rest.trim_start_matches(|c: char| flags.contains(c));
            next.starts_with('i') || (sed && next.starts_with('I'))
        });
        short
            || r.strip_prefix("--in-place")
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
    })
}

/// A line end, which JS `.` does not match.
fn is_line_end(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `^-<c>.`: the option `-<c>` with something attached.
fn attached(r: &str, c: char) -> bool {
    let mut chars = r.chars();
    chars.next() == Some('-')
        && chars.next() == Some(c)
        && chars.next().is_some_and(|n| !is_line_end(n))
}

/// 1.x `inPlaceTargets(args, sed)`: the operands after the script, unless -e/-f gave it.
fn in_place_targets(args: &[Word], sed: bool) -> Vec<&Word> {
    let mut files = Vec::new();
    let mut script = false;
    let mut k = 0;
    while let Some(arg) = args.get(k) {
        let a = arg.raw.as_str();
        let next = args.get(k.saturating_add(1));
        if sed && (a == "-i" || a == "-I") && next.is_some_and(|n| n.raw.is_empty() && n.quoted) {
            // BSD `-i ''`.
            k = k.saturating_add(2);
            continue;
        }
        if a.is_empty() {
            k = k.saturating_add(1);
            continue;
        }
        let script_flag = a
            .strip_prefix('-')
            .is_some_and(|f| f.ends_with(['e', 'f']) && f.bytes().all(|b| b.is_ascii_alphabetic()));
        if script_flag || a == "--expression" || a == "--file" {
            script = true;
            k = k.saturating_add(2);
            continue;
        }
        if a.starts_with("--expression=") || a.starts_with("--file=") || attached(a, 'e') {
            script = true;
        } else if !a.starts_with('-') {
            files.push(arg);
        }
        k = k.saturating_add(1);
    }
    if script {
        files
    } else {
        files.into_iter().skip(1).collect()
    }
}

/// What `git checkout <tree-ish> [--] paths` and `git restore [--source=<tree-ish>] paths`
/// restore, and from where (`None`: the index).
struct Restore<'w> {
    source: Option<&'w str>,
    paths: Vec<&'w Word>,
}

/// 1.x `gitRestore(sub, args)`.
fn git_restore<'w>(sub: &str, args: &'w [Word]) -> Restore<'w> {
    if sub == "checkout" {
        let dd = args.iter().position(|a| a.raw == "--");
        let head = args.get(..dd.unwrap_or(args.len())).unwrap_or(&[]);
        let mut before = Vec::new();
        let mut k = 0;
        while let Some(a) = head.get(k) {
            if ["-b", "-B", "--orphan", "--conflict"].contains(&a.raw.as_str()) {
                k = k.saturating_add(2);
                continue;
            }
            if !a.raw.starts_with('-') {
                before.push(a);
            }
            k = k.saturating_add(1);
        }
        let source = before.first().map(|w| w.raw.as_str());
        let paths = match dd {
            None => before.into_iter().skip(1).collect(),
            Some(dd) => args
                .get(dd.saturating_add(1)..)
                .unwrap_or(&[])
                .iter()
                .collect(),
        };
        return Restore { source, paths };
    }
    let staged = args.iter().any(|a| a.raw == "--staged" || a.raw == "-S");
    let worktree = args.iter().any(|a| a.raw == "--worktree" || a.raw == "-W");
    if staged && !worktree {
        return Restore {
            source: None,
            paths: Vec::new(),
        };
    }
    let mut source = None;
    let mut paths = Vec::new();
    let mut k = 0;
    while let Some(arg) = args.get(k) {
        let a = arg.raw.as_str();
        if a == "-s" || a == "--source" {
            k = k.saturating_add(1);
            source = args.get(k).map(|w| w.raw.as_str());
        } else if let Some(rev) = a.strip_prefix("--source=") {
            source = Some(rev);
        } else if attached(a, 's') {
            source = a.get(2..);
        } else if a != "--" && !a.starts_with('-') {
            paths.push(arg);
        }
        k = k.saturating_add(1);
    }
    Restore { source, paths }
}

/// 1.x `INSTALL_VALUES`, which every copy command skips the value of.
const INSTALL_VALUES: [&str; 6] = [
    "--suffix",
    "--mode",
    "--owner",
    "--group",
    "--strip-program",
    "--context",
];
/// 1.x `RSYNC_VALUES`.
const RSYNC_VALUES: [&str; 29] = [
    "-e",
    "--rsh",
    "-f",
    "--filter",
    "--exclude",
    "--include",
    "--exclude-from",
    "--include-from",
    "--files-from",
    "--chmod",
    "--chown",
    "--log-file",
    "--backup-dir",
    "--suffix",
    "--partial-dir",
    "-T",
    "--temp-dir",
    "--compare-dest",
    "--copy-dest",
    "--link-dest",
    "--max-size",
    "--min-size",
    "--bwlimit",
    "--timeout",
    "--port",
    "-M",
    "--remote-option",
    "-B",
    "--block-size",
];

fn rsync_value(k: &str) -> bool {
    RSYNC_VALUES.contains(&k)
}

/// 1.x `remote(w)`: `host:path`, not a drive path.
fn remote(w: &Word) -> bool {
    let Some(v) = w.value() else {
        return false;
    };
    let host = v
        .find(':')
        .is_some_and(|at| at > 0 && !v.get(..at).is_some_and(|h| h.contains(['/', '\\'])));
    let mut chars = v.chars();
    let drive = matches!((chars.next(), chars.next()), (Some(d), Some(':')) if d.is_ascii_alphabetic())
        && chars.next().is_none_or(|c| c == '\\' || c == '/');
    host && !drive
}

/// 1.x `copyArgs(name, args)`: cp, mv, install, ln, git mv, rsync and scp arguments, as the
/// destination and the copy, or `None` when nothing is copied (`install -d`, a dry run, a
/// remote destination).
fn copy_args<'c>(name: &str, args: &'c [Word]) -> Option<(Cow<'c, Word>, CopyOp<'c>)> {
    let rsync = name == "rsync" || name == "scp";
    let value_letters = match name {
        "install" => "tSmogZ",
        _ if rsync => "",
        _ => "tS",
    };
    let mut target: Option<Cow<'c, Word>> = None;
    let mut no_target = false;
    let mut recursive = name == "mv" || name == "git";
    let mut pos: Vec<Cow<'c, Word>> = Vec::new();
    let mut opts = true;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let r = a.raw.as_str();
        if opts && r == "--" {
            opts = false;
        } else if opts && r.starts_with("--") {
            let (k, v) = match r.find('=') {
                Some(eq) => (r.get(..eq).unwrap_or(r), Some(a.after_eq())),
                None => (r, None),
            };
            if k == "--target-directory" && !rsync {
                target = match v {
                    Some(v) => Some(Cow::Owned(v)),
                    None => {
                        i = i.saturating_add(1);
                        args.get(i).map(Cow::Borrowed)
                    }
                };
                target.as_ref()?;
            } else if k == "--no-target-directory" {
                no_target = true;
            } else if k == "--recursive" || k == "--archive" {
                recursive = true;
            } else if k == "--directory" || k == "--dry-run" || k == "--list-only" {
                return None;
            } else if v.is_none() && (INSTALL_VALUES.contains(&k) || (rsync && rsync_value(k))) {
                i = i.saturating_add(1);
            }
        } else if opts && r.len() > 1 && r.starts_with('-') && !r.starts_with("--") {
            if rsync {
                if rsync_value(r) {
                    i = i.saturating_add(1);
                }
                recursive |= cluster_reaches(r, "ra");
                if cluster_reaches(r, "n") {
                    // --dry-run
                    return None;
                }
            } else {
                let cluster = r.get(1..).unwrap_or("");
                for (j, ch) in cluster.char_indices() {
                    if value_letters.contains(ch) {
                        let rest = cluster.get(j.saturating_add(ch.len_utf8())..).unwrap_or("");
                        let v = if rest.is_empty() {
                            i = i.saturating_add(1);
                            args.get(i).map(Cow::Borrowed)
                        } else {
                            Some(Cow::Owned(Word::plain(rest)))
                        };
                        if ch == 't' {
                            target = v;
                            target.as_ref()?;
                        }
                        break;
                    }
                    match ch {
                        'T' => no_target = true,
                        'r' | 'R' | 'a' => recursive = true,
                        'd' if name == "install" => return None,
                        // git mv -n is a dry run.
                        'n' if name == "git" => return None,
                        _ => {}
                    }
                }
            }
        } else {
            pos.push(Cow::Borrowed(a));
        }
        i = i.saturating_add(1);
    }
    let (mut sources, dest, into_dir) = match target {
        Some(target) => (pos, target, Some(true)),
        None => {
            if pos.len() < 2 {
                return None;
            }
            let into = if no_target {
                Some(false)
            } else if pos.len() > 2 {
                Some(true)
            } else {
                None
            };
            let dest = pos.pop()?;
            (pos, dest, into)
        }
    };
    if rsync {
        if remote(&dest) {
            return None;
        }
        for s in &mut sources {
            if remote(s) {
                *s = Cow::Owned(Word::unknown(&s.raw));
            }
        }
    }
    Some((
        dest,
        CopyOp {
            sources,
            into_dir,
            recursive,
        },
    ))
}

/// A PowerShell parameter's value.
#[derive(Debug, Clone)]
pub(super) enum Param<'c> {
    /// A switch, which takes no value.
    Switch,
    /// `-Name value` or `-Name:value`.
    Word(Cow<'c, Word>),
    /// `-Name` at the end, with no value after it.
    Missing,
}

/// 1.x `psParams(args, switches)`: named parameters, in the order first given (a JS `Map`),
/// and positional ones.
pub(super) struct PsParams<'c> {
    named: Vec<(String, Param<'c>)>,
    pub(super) pos: Vec<Cow<'c, Word>>,
}

/// 1.x `psParams`' default switches.
fn ps_switch(key: &str) -> bool {
    matches!(
        key,
        "force"
            | "append"
            | "noclobber"
            | "nonewline"
            | "recurse"
            | "passthru"
            | "whatif"
            | "confirm"
            | "container"
            | "asbytestream"
    )
}

/// `Remove-Item`'s switches: `-Recurse` may be shortened to any prefix (`-r`, `-rec`).
pub(super) fn ps_remove_switch(key: &str) -> bool {
    matches!(key, "force" | "whatif" | "confirm") || (!key.is_empty() && "recurse".starts_with(key))
}

impl<'c> PsParams<'c> {
    pub(super) fn new(args: &'c [Word], switch: fn(&str) -> bool) -> Self {
        let mut named: Vec<(String, Param<'c>)> = Vec::new();
        let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut pos = Vec::new();
        let mut j = 0;
        while let Some(a) = args.get(j) {
            // `^-([A-Za-z]+)(?::(.*))?$`, with `.` matching anything (`s`).
            let parsed = a.raw.strip_prefix('-').and_then(|rest| {
                let letters = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
                let tail = rest.get(letters..)?;
                (letters > 0 && (tail.is_empty() || tail.starts_with(':')))
                    .then(|| (rest.get(..letters).unwrap_or(""), tail.strip_prefix(':')))
            });
            let Some((key, value)) = parsed.filter(|_| !a.quoted) else {
                pos.push(Cow::Borrowed(a));
                j = j.saturating_add(1);
                continue;
            };
            let key = key.to_lowercase();
            let param = match value {
                Some(v) => Param::Word(Cow::Owned(Word::plain(v))),
                None if switch(&key) => Param::Switch,
                None => {
                    j = j.saturating_add(1);
                    args.get(j)
                        .map_or(Param::Missing, |w| Param::Word(Cow::Borrowed(w)))
                }
            };
            match index.get(&key) {
                Some(&at) => {
                    if let Some(slot) = named.get_mut(at) {
                        slot.1 = param;
                    }
                }
                None => {
                    index.insert(key.clone(), named.len());
                    named.push((key, param));
                }
            }
            j = j.saturating_add(1);
        }
        Self { named, pos }
    }

    /// 1.x `named.get(key)`.
    pub(super) fn exact(&self, key: &str) -> Option<&Param<'c>> {
        self.named.iter().find(|(k, _)| k == key).map(|(_, p)| p)
    }

    /// 1.x `named.has(key)`.
    pub(super) fn has(&self, key: &str) -> bool {
        self.exact(key).is_some()
    }

    /// 1.x `get(...full)`: the first parameter given whose name (two letters or more) is a
    /// prefix of one of `full`.
    pub(super) fn get(&self, full: &[&str]) -> Option<&Param<'c>> {
        self.named
            .iter()
            .find(|(k, _)| k.len() > 1 && full.iter().any(|f| f.starts_with(k.as_str())))
            .map(|(_, p)| p)
    }

    /// The names given, in order.
    pub(super) fn keys(&self) -> impl Iterator<Item = &str> {
        self.named.iter().map(|(k, _)| k.as_str())
    }

    /// 1.x `psDryRun(named)`: only a bare `-WhatIf` or `-WhatIf:$true` is a dry run.
    pub(super) fn dry_run(&self) -> bool {
        match self.exact("whatif") {
            Some(Param::Switch) => true,
            Some(Param::Word(w)) => w.raw.eq_ignore_ascii_case("$true"),
            Some(Param::Missing) | None => false,
        }
    }

    /// The paths a copy or delete names: each word's value split at `,`, each unknown when
    /// `-Filter`, `-Include` or `-Exclude` picks among them.
    pub(super) fn split_paths(&self, paths: Vec<Cow<'c, Word>>) -> Vec<Cow<'c, Word>> {
        let filtered = ["filter", "include", "exclude"].iter().any(|k| self.has(k));
        let mut out = Vec::new();
        for p in paths {
            let Some(value) = p.value() else {
                out.push(p);
                continue;
            };
            for t in value.split(',') {
                out.push(Cow::Owned(if filtered {
                    Word::unknown(t)
                } else {
                    p.derived(t)
                }));
            }
        }
        out
    }
}

/// A named parameter's word, if it has one.
pub(super) fn param_word<'c>(p: Option<&Param<'c>>) -> Option<Cow<'c, Word>> {
    match p {
        Some(Param::Word(w)) => Some(w.clone()),
        Some(Param::Switch | Param::Missing) | None => None,
    }
}

/// 1.x `psCopyArgs(move, args)`: `Copy-Item` and `Move-Item` and their aliases.
fn ps_copy_args<'c>(moves: bool, args: &'c [Word]) -> Option<(Cow<'c, Word>, CopyOp<'c>)> {
    let mut params = PsParams::new(args, ps_switch);
    if params.dry_run() {
        return None;
    }
    let mut paths: Vec<Cow<'c, Word>> = [
        params.get(&["path"]),
        params.get(&["literalpath"]),
        params.get(&["pspath"]),
        params.exact("lp"),
    ]
    .into_iter()
    .filter_map(param_word)
    .collect();
    if paths.is_empty() && !params.pos.is_empty() {
        paths.push(params.pos.remove(0));
    }
    let mut dest = param_word(params.get(&["destination"]));
    if dest.is_none() && !params.pos.is_empty() {
        dest = Some(params.pos.remove(0));
    }
    let dest = dest?;
    if paths.is_empty() {
        return None;
    }
    let recursive = moves || params.has("recurse");
    let sources = params.split_paths(paths);
    let into_dir = (sources.len() > 1).then_some(true);
    Some((
        dest,
        CopyOp {
            sources,
            into_dir,
            recursive,
        },
    ))
}

/// 1.x `landing(t, src, destIsDir)`: where one source of a copy lands, given the destination's
/// absolute path: the destination, or the source's name inside it when the copy goes into a
/// directory (`-t`, several sources, a trailing `/`, or `dest_is_dir`). rsync's `src/` copies
/// the contents. `None` when the source's name is unknown.
pub(crate) fn landing(
    cx: PathCx<'_>,
    t: &Target<'_>,
    dest: &str,
    src: &Word,
    dest_is_dir: bool,
) -> Option<String> {
    let slash_end = |s: &str| s.ends_with(['/', '\\']);
    if t.verb == Verb::Named("rsync") && slash_end(&src.raw) {
        return Some(dest.to_owned());
    }
    let into = t
        .copy
        .as_ref()
        .and_then(|op| op.into_dir)
        .unwrap_or_else(|| dest_is_dir || slash_end(&t.word.raw));
    if !into {
        return Some(dest.to_owned());
    }
    if src.glob {
        return None;
    }
    let value = src.value()?;
    let name = node_basename(cx.at.style, value.trim_end_matches(['/', '\\']));
    Some(super::words::resolve_from(cx.at, dest, name))
}

/// Node's `path.basename(p)` of any path: the last name, after a Windows drive.
pub(crate) fn node_basename(style: PathStyle, p: &str) -> &str {
    match style {
        PathStyle::Posix => {
            let p = p.trim_end_matches('/');
            p.rsplit('/').next().unwrap_or(p)
        }
        PathStyle::Windows => {
            let mut chars = p.chars();
            let p = match (chars.next(), chars.next()) {
                (Some(d), Some(':')) if d.is_ascii_alphabetic() => chars.as_str(),
                _ => p,
            };
            let p = p.trim_end_matches(['/', '\\']);
            p.rsplit(['/', '\\']).next().unwrap_or(p)
        }
    }
}

/// 1.x `pathspecPattern(w)`: a git pathspec that is a pattern, not a path: a wildcard (`*`,
/// `?`, `[`) or magic (`:(glob)...`, `:/x`, `:!x`).
pub(crate) fn pathspec_pattern(raw: &str) -> bool {
    raw.contains(['*', '?', '['])
        || raw
            .strip_prefix(':')
            .is_some_and(|r| r.starts_with(['(', '/', '!', '^']))
}
