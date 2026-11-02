//! Patch commands and the files a patch names (1.x `shell.mjs` `patchCommand`, `patchFiles`).
//!
//! The protected-config ask reads them in port Phase 2e; the reviewer's patch denials and the
//! scan of patches onto `.rs` files read them from 2f.

use std::borrow::Cow;

use super::git::git_command;
use super::walk::Command;
use super::words::{Dir, PathCx, Word};

/// Which tool applies a patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatchTool {
    /// `patch` (and `gpatch`).
    Patch,
    /// `git apply`, or a subcommand read as one (see [`Patch::guessed`]).
    GitApply,
    /// `git am`.
    GitAm,
}

/// What a patch command does (1.x `patchCommand(c)`'s answer with `apply: true`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Patch<'c> {
    pub(crate) tool: PatchTool,
    /// The patch files named on the command line; with none, the patch comes from stdin.
    pub(crate) inputs: Vec<Cow<'c, Word>>,
    /// `-o` files, which it writes.
    pub(crate) outputs: Vec<Cow<'c, Word>>,
    /// `patch`'s file operand, which it writes.
    pub(crate) target: Option<Cow<'c, Word>>,
    /// Where it runs, after `git -C` and `patch -d`.
    pub(crate) cwd: Option<Dir>,
    /// `git am --continue` and the like, which apply patches temper cannot see.
    pub(crate) stored: bool,
    /// A subcommand git does not have, under configuration temper cannot read, read as
    /// `git apply`: only a patch file temper can read counts.
    pub(crate) guessed: bool,
}

/// A patch command: one that applies, or an inspecting form that changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PatchCommand<'c> {
    Applies(Patch<'c>),
    /// `--check`, `--stat` without `--apply`, `--dry-run`, `--help`, `--version`,
    /// `git am --abort`.
    Inspects(PatchTool),
}

/// 1.x `PATCH_VALUES`: patch's options that take a value.
const PATCH_VALUES: [&str; 24] = [
    "-p",
    "-i",
    "-o",
    "-d",
    "-D",
    "-F",
    "-B",
    "-Y",
    "-z",
    "-V",
    "-r",
    "-g",
    "--strip",
    "--input",
    "--output",
    "--directory",
    "--ifdef",
    "--fuzz",
    "--prefix",
    "--basename-prefix",
    "--suffix",
    "--version-control",
    "--reject-file",
    "--get",
];
/// 1.x `GIT_APPLY_VALUES`.
const GIT_APPLY_VALUES: [&str; 7] = [
    "-p",
    "-C",
    "--exclude",
    "--include",
    "--directory",
    "--whitespace",
    "--build-fake-ancestor",
];

fn patch_value(key: &str) -> bool {
    PATCH_VALUES.contains(&key) || key == "--quoting-style"
}

/// 1.x `patchCommand(c)`: git apply, git am and patch, or `None` for any other command.
pub(crate) fn patch_command<'c>(c: &'c Command, cx: PathCx<'_>) -> Option<PatchCommand<'c>> {
    if c.name.as_deref() == Some("patch") {
        let at = PatchAt {
            tool: PatchTool::Patch,
            cwd: c.cwd.clone(), // a shared `Rc` handle
            guessed: false,
        };
        return Some(read_patch(&c.args, at, cx));
    }
    let g = git_command(c, cx)?;
    let tool = if g.hidden_alias {
        PatchTool::GitApply
    } else {
        match g.sub.as_str() {
            "apply" => PatchTool::GitApply,
            "am" => PatchTool::GitAm,
            _ => return None,
        }
    };
    let at = PatchAt {
        tool,
        cwd: g.cwd,
        guessed: g.hidden_alias,
    };
    Some(match g.args {
        Cow::Borrowed(args) => read_patch(args, at, cx),
        // An alias expansion's words live in `g`, so the patch owns its.
        Cow::Owned(args) => match read_patch(&args, at, cx) {
            PatchCommand::Applies(p) => PatchCommand::Applies(p.into_owned()),
            PatchCommand::Inspects(tool) => PatchCommand::Inspects(tool),
        },
    })
}

impl Patch<'_> {
    fn into_owned<'x>(self) -> Patch<'x> {
        let own = |w: Cow<'_, Word>| Cow::Owned(w.into_owned());
        Patch {
            tool: self.tool,
            inputs: self.inputs.into_iter().map(own).collect(),
            outputs: self.outputs.into_iter().map(own).collect(),
            target: self.target.map(own),
            cwd: self.cwd,
            stored: self.stored,
            guessed: self.guessed,
        }
    }
}

/// A patch command's tool, where it runs, and whether it is a guessed alias.
struct PatchAt {
    tool: PatchTool,
    cwd: Option<Dir>,
    guessed: bool,
}

/// What one option of `git apply` or `git am` does.
enum GitOption {
    /// `--abort` and the like: nothing is applied.
    Inspects,
    /// `--check`, `--stat`, `--numstat`, `--summary`: the patch is read, unless `--apply`.
    Reads,
    /// `--apply`.
    Applies,
    /// `git am --continue` and the like: applies the patch git stored.
    Stored,
    /// An option whose value is the next word.
    Valued,
    Other,
}

fn git_option(tool: PatchTool, key: &str, attached: bool) -> GitOption {
    match key {
        "--check" | "--stat" | "--numstat" | "--summary" => GitOption::Reads,
        "--apply" => GitOption::Applies,
        "--abort" | "--quit" | "--show-current-patch" => GitOption::Inspects,
        "--continue" | "--skip" | "--resolved" | "-r" if tool == PatchTool::GitAm => {
            GitOption::Stored
        }
        _ if !attached && GIT_APPLY_VALUES.contains(&key) => GitOption::Valued,
        _ => GitOption::Other,
    }
}

/// A `patch` option `k` (the word `a` up to any `=`) that takes a value: its key, and the
/// value attached to it (`-ofile`, `--output=file`), if any.
fn patch_option<'a>(a: &'a Word, k: &'a str, eq: bool) -> Option<(&'a str, Option<Cow<'a, Word>>)> {
    let short = !k.starts_with("--")
        && k.strip_prefix('-')
            .is_some_and(|f| f.starts_with(|c: char| "pioDdFBYzVrg".contains(c)));
    let mut value: Option<Cow<'a, Word>> = eq.then(|| Cow::Owned(a.after_eq()));
    if short && k.len() > 2 {
        value = k.get(2..).map(|s| Cow::Owned(Word::plain(s)));
    }
    let key = if short { k.get(..2).unwrap_or(k) } else { k };
    patch_value(key).then_some((key, value))
}

/// `patch`'s first operand is its target and its second the patch; git's operands are
/// patches, `-` being stdin. Returns the target.
fn operands<'a>(
    tool: PatchTool,
    pos: Vec<&'a Word>,
    inputs: &mut Vec<Cow<'a, Word>>,
) -> Option<Cow<'a, Word>> {
    if tool != PatchTool::Patch {
        inputs.extend(pos.into_iter().filter(|w| w.raw != "-").map(Cow::Borrowed));
        return None;
    }
    if let Some(second) = pos.get(1) {
        inputs.push(Cow::Borrowed(*second));
    }
    pos.first().map(|w| Cow::Borrowed(*w))
}

/// The options and operands of a patch command.
fn read_patch<'a>(args: &'a [Word], mut at: PatchAt, cx: PathCx<'_>) -> PatchCommand<'a> {
    let tool = at.tool;
    // `--check`, `--stat` and the like, or `patch --dry-run`: the patch is only read.
    let (mut inspects, mut apply_too, mut stored) = (false, false, false);
    let mut inputs: Vec<Cow<'a, Word>> = Vec::new();
    let mut outputs = Vec::new();
    let mut pos: Vec<&'a Word> = Vec::new();
    let mut i = 0;
    while let Some(a) = args.get(i) {
        i = i.saturating_add(1);
        let r = a.raw.as_str();
        if a.value().is_none() || !r.starts_with('-') || r == "-" {
            pos.push(a);
            continue;
        }
        let eq = r.find('=');
        let k = eq.and_then(|e| r.get(..e)).unwrap_or(r);
        if ["-h", "--help", "--version"].contains(&k) || (tool == PatchTool::Patch && k == "-v") {
            return PatchCommand::Inspects(tool);
        }
        if tool != PatchTool::Patch {
            match git_option(tool, k, eq.is_some()) {
                GitOption::Inspects => return PatchCommand::Inspects(tool),
                GitOption::Reads => inspects = true,
                GitOption::Applies => apply_too = true,
                GitOption::Stored => stored = true,
                GitOption::Valued => i = i.saturating_add(1),
                GitOption::Other => {}
            }
            continue;
        }
        // `--dry-run` takes no value, so it ends here.
        inspects |= k == "--dry-run";
        let Some((key, attached)) = patch_option(a, k, eq.is_some()) else {
            continue;
        };
        let Some(value) = attached.or_else(|| {
            i = i.saturating_add(1);
            args.get(i.saturating_sub(1)).map(Cow::Borrowed)
        }) else {
            continue;
        };
        match key {
            "-i" | "--input" => inputs.push(value),
            "-o" | "--output" => outputs.push(value),
            "-d" | "--directory" => {
                at.cwd = super::words::word_dir(cx, &value, at.cwd.as_ref());
            }
            _ => {}
        }
    }
    if inspects && !apply_too {
        return PatchCommand::Inspects(tool);
    }
    PatchCommand::Applies(Patch {
        tool,
        // Before `inputs`, which the operands add to.
        target: operands(tool, pos, &mut inputs),
        inputs,
        outputs,
        cwd: at.cwd,
        stored,
        guessed: at.guessed,
    })
}

/// A JS line terminator, where `^` and `$` match under the `m` flag and `.` stops.
fn line_end(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `$` under `m` at byte `at`: the end, or a line terminator next.
fn at_line_end(text: &str, at: usize) -> bool {
    text.get(at..)
        .and_then(|r| r.chars().next())
        .is_none_or(line_end)
}

/// `\r?$` at `at`: the byte offset where it matches, or `None`.
fn cr_then_end(text: &str, at: usize) -> Option<usize> {
    if at_line_end(text, at) {
        return Some(at);
    }
    let after = at.saturating_add(1);
    (text.get(at..).is_some_and(|r| r.starts_with('\r')) && at_line_end(text, after))
        .then_some(after)
}

/// A `"[^"]*"` at `at` (which may run over lines): its end.
fn quoted_at(text: &str, at: usize) -> Option<usize> {
    let rest = text.get(at..)?.strip_prefix('"')?;
    rest.find('"')
        .map(|e| at.saturating_add(e).saturating_add(2))
}

/// One alternative's captures at a line start, and where the match ends.
type LineMatch<'t> = (Vec<&'t str>, usize);

/// 1.x `patchFiles`' first alternative: `(?:\+\+\+|---|\*\*\*)[ \t]+("[^"]*"|[^\t\r\n]*?)(?:\t[^\r\n]*)?\r?$`.
fn header_line(text: &str, at: usize) -> Option<LineMatch<'_>> {
    let rest = text.get(at..)?;
    let head = ["+++", "---", "***"]
        .iter()
        .find(|h| rest.starts_with(**h))?;
    let after = rest.get(head.len()..)?;
    let spaces = after
        .len()
        .saturating_sub(after.trim_start_matches([' ', '\t']).len());
    if spaces == 0 {
        return None;
    }
    let start = at.saturating_add(head.len()).saturating_add(spaces);
    // The tail: an optional tab and the rest of the line, then `\r?$`.
    let tail = |from: usize| -> Option<usize> {
        let r = text.get(from..)?;
        if let Some(tabbed) = r.strip_prefix('\t') {
            let line = tabbed.find(['\r', '\n']).unwrap_or(tabbed.len());
            let end = from.saturating_add(1).saturating_add(line);
            return cr_then_end(text, end);
        }
        cr_then_end(text, from)
    };
    if let Some(close) = quoted_at(text, start)
        && let Some(end) = tail(close)
    {
        return Some((vec![text.get(start..close)?], end));
    }
    // The lazy capture stops at the first place the tail matches: a tab, or the line's end.
    let mut at = start;
    loop {
        if let Some(end) = tail(at) {
            return Some((vec![text.get(start..at)?], end));
        }
        let c = text.get(at..)?.chars().next()?;
        if matches!(c, '\t' | '\r' | '\n') {
            return None;
        }
        at = at.saturating_add(c.len_utf8());
    }
}

/// `("[^"]*"|\S+)` at `at`: its end.
fn git_name_at(
    text: &str,
    at: usize,
    then: impl Fn(usize) -> Option<usize>,
) -> Option<(usize, usize)> {
    use crate::project::is_js_space;
    if let Some(close) = quoted_at(text, at)
        && let Some(end) = then(close)
    {
        return Some((close, end));
    }
    let rest = text.get(at..)?;
    let run = rest.find(is_js_space).unwrap_or(rest.len());
    if run == 0 {
        return None;
    }
    let close = at.saturating_add(run);
    then(close).map(|end| (close, end))
}

/// 1.x `patchFiles`' second alternative: `diff --git ("[^"]*"|\S+) ("[^"]*"|\S+)\r?$`.
fn diff_git_line(text: &str, at: usize) -> Option<LineMatch<'_>> {
    let start = at.saturating_add("diff --git ".len());
    text.get(at..)?.starts_with("diff --git ").then_some(())?;
    let second = |from: usize| -> Option<usize> {
        let b = from.saturating_add(1);
        text.get(from..)?.starts_with(' ').then_some(())?;
        git_name_at(text, b, |close| cr_then_end(text, close)).map(|(_, end)| end)
    };
    let (close_a, _) = git_name_at(text, start, second)?;
    let b = close_a.saturating_add(1);
    let (close_b, end) = git_name_at(text, b, |close| cr_then_end(text, close))?;
    Some((vec![text.get(start..close_a)?, text.get(b..close_b)?], end))
}

/// 1.x `patchFiles`' last alternatives: `(?:rename|copy) (?:from|to) (.+?)\r?$` and
/// `Index: (.+?)\r?$`.
fn named_line(text: &str, at: usize) -> Option<LineMatch<'_>> {
    let rest = text.get(at..)?;
    let lead = [
        "rename from ",
        "rename to ",
        "copy from ",
        "copy to ",
        "Index: ",
    ]
    .iter()
    .find(|l| rest.starts_with(**l))?;
    let start = at.saturating_add(lead.len());
    let line = text
        .get(start..)?
        .find(line_end)
        .unwrap_or_else(|| text.len().saturating_sub(start));
    if line == 0 {
        return None;
    }
    // `.` stops at every line end, so the lazy capture is the rest of the line.
    let close = start.saturating_add(line);
    let end = cr_then_end(text, close)?;
    Some((vec![text.get(start..close)?], end))
}

/// The first line start at or after `from`: `from` itself when a line ends just before it,
/// else the char after the next line end.
fn line_start_from(text: &str, from: usize) -> Option<usize> {
    let before = text.get(..from)?;
    if from == 0 || before.chars().next_back().is_some_and(line_end) {
        return Some(from);
    }
    let rest = text.get(from..)?;
    let (at, c) = rest.char_indices().find(|(_, c)| line_end(*c))?;
    Some(from.saturating_add(at).saturating_add(c.len_utf8()))
}

/// 1.x `patchFiles(text)`: the files a unified, context or git diff names, with prefixes left
/// on. A `---`/`+++`/`***` header may end in a tab or space and a timestamp, and lines may
/// end in CR. As the global regex does, each search starts where the last match ended, and
/// only a line start can begin a match, so each line is tried once.
pub(crate) fn patch_files(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(at) = line_start_from(text, cursor).filter(|&at| at < text.len()) {
        let found = header_line(text, at)
            .or_else(|| diff_git_line(text, at))
            .or_else(|| named_line(text, at));
        cursor = if let Some((captures, end)) = found {
            out.extend(
                captures
                    .into_iter()
                    .filter(|c| !c.is_empty())
                    .map(clean_name),
            );
            end.max(at.saturating_add(1))
        } else {
            // The next start is after the next line end.
            let c_len = text
                .get(at..)
                .and_then(|r| r.chars().next())
                .map_or(1, char::len_utf8);
            at.saturating_add(c_len)
        };
        if !text.is_char_boundary(cursor) {
            break;
        }
    }
    out
}

/// 1.x's `.replace(/^"|"$/g, '').replace(/\s+\d{4}-\d\d-\d\d[ \t].*$/, '').trim()`.
fn clean_name(name: &str) -> String {
    use crate::project::is_js_space;
    let name = name.strip_prefix('"').unwrap_or(name);
    let name = name.strip_suffix('"').unwrap_or(name);
    let mut cut = name.len();
    let mut i = 0;
    while let Some(c) = name.get(i..).and_then(|r| r.chars().next()) {
        if !is_js_space(c) {
            i = i.saturating_add(c.len_utf8());
            continue;
        }
        let run_start = i;
        let rest = name.get(i..).unwrap_or("");
        let run_end = i.saturating_add(
            rest.len()
                .saturating_sub(rest.trim_start_matches(is_js_space).len()),
        );
        let after = name.get(run_end..).unwrap_or("");
        let b = after.as_bytes();
        let digit = |k: usize| b.get(k).is_some_and(u8::is_ascii_digit);
        let date = (0..4).all(digit)
            && b.get(4) == Some(&b'-')
            && digit(5)
            && digit(6)
            && b.get(7) == Some(&b'-')
            && digit(8)
            && digit(9)
            && matches!(b.get(10), Some(b' ' | b'\t'))
            && !after.get(11..).unwrap_or("").contains(line_end);
        if date {
            cut = run_start;
            break;
        }
        i = run_end.max(i.saturating_add(c.len_utf8()));
    }
    temper_domain::js_trim(name.get(..cut).unwrap_or(name)).to_owned()
}
