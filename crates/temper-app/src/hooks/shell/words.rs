//! Words as the rules see them (1.x `shell.mjs` `makeWord`, `partValue`, `gitRevParse`,
//! `mktemp`), and the paths they name (`msysPath`, `resolvePath`, `wordPath`).

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use temper_domain::{PathStyle, js_trim};

pub(crate) use super::dir::{Dir, Guard};
use super::lex::{Part, PartKind, name_len};
use super::text::Text;
use crate::paths::{self, PathEnv};
use crate::project::is_js_space;

/// V8's `String::kMaxLength` on a 64-bit host, in UTF-16 units: 1.x throws `RangeError:
/// Invalid string length` when a word's text would grow past it (`X=$X$X` repeated), and its
/// rule then falls back.
pub(super) const JS_MAX_STRING: usize = (1 << 29) - 24;

/// Appends `more` to `text`, whose length in UTF-16 units is `units`, unless the result
/// would be longer than a JS string can be: then it marks `overflow` and leaves `text`.
pub(super) fn push_js(
    text: &mut String,
    units: &mut usize,
    more: &str,
    overflow: &Cell<bool>,
) -> bool {
    let total = units.saturating_add(js_len(more));
    if total > JS_MAX_STRING {
        overflow.set(true);
        return false;
    }
    *units = total;
    text.push_str(more);
    true
}

/// A string's length in UTF-16 units, as JS counts it.
pub(crate) fn js_len(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.encode_utf16().count()
    }
}

/// A word: `raw` (quotes removed, expansions as written), `text` (known expansions replaced,
/// unknown ones as written) and whether every part is known, which makes `text` its value.
///
/// The text shares each value it expands rather than copying it, and is joined only when a
/// rule reads it ([`Text`]), as V8 joins strings: `: $X; ` or `: $X/a; ` repeated holds `$X`
/// once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Word {
    pub(super) raw: String,
    pub(super) text: Text,
    pub(super) known: bool,
    pub(super) quoted: bool,
    /// The length of the name a `NAME=` or `NAME+=` word assigns, or 0.
    pub(super) assign: usize,
    pub(super) form: Form,
}

/// What the shell does to a word before it passes it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Form {
    /// No expansion, brace list or glob at all: the text is exactly what the shell passes.
    Literal,
    /// Anything else: whether it is an unquoted `*`, `?`, `[...]` or brace list (`glob`), and
    /// whether some part is an expansion (`expanded`).
    Made { glob: bool, expanded: bool },
}

/// [`Form::Made`] with neither a glob nor an expansion.
const MADE: Form = Form::Made {
    glob: false,
    expanded: false,
};

impl Word {
    /// 1.x `literalWord(s)`.
    pub(super) fn literal(s: &str) -> Self {
        Self {
            raw: s.to_owned(),
            text: Text::whole(Rc::from(s)),
            known: true,
            quoted: false,
            assign: 0,
            form: Form::Literal,
        }
    }

    /// 1.x `unknownWord(raw)`: xargs input, find's `{}`.
    pub(super) fn unknown(raw: &str) -> Self {
        Self {
            known: false,
            form: MADE,
            ..Self::literal(raw)
        }
    }

    /// A word cut from `self` whose raw, text and value are all `s` (1.x's `{ ...w, raw: s,
    /// text: s, value: w.value === null ? null : s }`).
    pub(super) fn derived(&self, s: &str) -> Self {
        Self {
            known: self.known,
            quoted: self.quoted,
            form: self.form,
            ..Self::literal(s)
        }
    }

    /// A plain word `{ raw: s, text: s, value: s, glob: false }` (1.x builds these for option
    /// values cut from a cluster).
    pub(super) fn plain(s: &str) -> Self {
        Self {
            form: MADE,
            ..Self::literal(s)
        }
    }

    /// No expansion, brace list or glob at all: the text is exactly what the shell passes.
    pub(super) fn is_literal(&self) -> bool {
        self.form == Form::Literal
    }

    /// Some part is an expansion.
    pub(super) fn expanded(&self) -> bool {
        matches!(self.form, Form::Made { expanded: true, .. })
    }

    pub(crate) fn raw(&self) -> &str {
        &self.raw
    }

    pub(crate) fn text(&self) -> &str {
        self.text.get()
    }

    /// The value, or `None` when a part is unknown.
    pub(crate) fn value(&self) -> Option<&str> {
        self.known.then(|| self.text.get())
    }

    /// What a `NAME=value` or `NAME+=value` word stores, read without keeping a join on the
    /// word; `None` when it is no assignment.
    pub(super) fn assigned(&self) -> Option<Assigned<'_>> {
        let name = self.assign()?.len();
        if !self.known {
            return Some(Assigned::Unknown);
        }
        let head = self.text.head(name.saturating_add(2));
        if !head.get(name..).is_some_and(|op| op.starts_with("+=")) {
            return Some(Assigned::Set(name.saturating_add(1)));
        }
        let after = name.saturating_add(2);
        let more = match self.text.unkept() {
            Cow::Borrowed(all) => all.get(after..).map(Cow::Borrowed),
            Cow::Owned(all) => all.get(after..).map(|more| Cow::Owned(more.to_owned())),
        };
        Some(more.map_or(Assigned::Unknown, Assigned::Append))
    }

    /// The value after its first `skip` bytes, which are ASCII (a name and `=`, so as many
    /// UTF-16 units), as one shared string for a variable to keep: the piece itself when the
    /// rest is one, else joined once through `scratch`. `None` when a part is unknown.
    fn shared_after(&self, skip: usize, scratch: &mut String) -> Option<Shared> {
        if !self.known {
            return None;
        }
        let text = self.text.tail_shared(skip, scratch)?;
        Some(Shared::new(text, self.text.units().saturating_sub(skip)))
    }

    /// Whether every part's value is known, so [`Self::value`] is `Some`.
    pub(crate) fn value_is_known(&self) -> bool {
        self.known
    }

    /// The text for one read, which keeps no joined copy: a read that copies the pieces is
    /// charged to the line's guard, and is `None` once its budget is spent, which marks the
    /// overflow.
    pub(crate) fn text_read(&self) -> Option<Cow<'_, str>> {
        self.text.read()
    }

    /// Whether the value is `s` (`value() == Some(s)`), without joining a text that cannot
    /// be it.
    pub(super) fn value_is(&self, s: &str) -> bool {
        self.known && self.text.is(s)
    }

    /// An unquoted `*`, `?`, `[...]` or brace list.
    pub(crate) fn glob(&self) -> bool {
        matches!(self.form, Form::Made { glob: true, .. })
    }

    /// 1.x `w.assign`: the name a `NAME=value` word assigns.
    pub(super) fn assign(&self) -> Option<&str> {
        (self.assign > 0)
            .then(|| self.raw.get(..self.assign))
            .flatten()
    }
}

/// Shell variables, with a journal so a subshell's or a substitution's changes are undone in
/// time proportional to them (1.x copies the whole `Map` at each `(` and `$(`). Each open scope
/// journals a name once, with its value at the mark, which is all its rollback needs: `a=$a/b`
/// repeated inside `( ... )` keeps one old value, not every earlier one. A change made while
/// no scope is open is never undone, so it is not journaled.
///
/// The values it keeps alive are charged to the line's budget, net of those it drops: each
/// value set is charged, and refunded once it is freed. A dropped value
/// a word still shares (the `$a` in `a=$a/b`, until that word is gone) waits in `dropped`,
/// which is swept every so many releases, so a name grown in place holds its last few, and
/// many names holding long values stop at the budget.
#[derive(Debug, Default)]
pub(super) struct Vars {
    map: HashMap<String, Shared>,
    journal: Vec<(String, Option<Shared>)>,
    /// Per open mark, innermost last, the names it has journaled. A scope the line leaves
    /// unclosed is closed by the rollback of the one outside it, or never.
    saved: Vec<HashSet<String>>,
    /// How many names start with `GIT_CONFIG`, which git reads as configuration.
    git_config: usize,
    /// The line's budget; `None` charges nothing.
    meter: Option<Guard>,
    /// Values dropped while something else still held them, still charged.
    dropped: Vec<Rc<str>>,
    /// How many values were released since `dropped` was last swept.
    since: usize,
    /// How many the last sweep left in `dropped`.
    kept: usize,
    /// Where a value of several pieces is joined before it is shared, kept so its pages are
    /// reused; its capacity is charged as it grows.
    scratch: String,
}

/// Where [`Vars::mark`] opened a scope: the journal's length and the open marks' count then.
#[derive(Debug, Clone, Copy)]
pub(super) struct Mark {
    journal: usize,
    depth: usize,
}

/// What an assignment word stores ([`Word::assigned`]).
#[derive(Debug)]
pub(super) enum Assigned<'w> {
    /// A part's value is unknown, so the name is deleted.
    Unknown,
    /// `NAME=value`: the value is the word's text after this many bytes ([`Vars::set_word`]).
    Set(usize),
    /// `NAME+=more`: the text to append to a known value.
    Append(Cow<'w, str>),
}

/// A value words may share: its text and its length in UTF-16 units, counted once.
#[derive(Debug, Clone)]
pub(super) struct Shared {
    text: Rc<str>,
    units: usize,
}

impl Shared {
    fn new(text: Rc<str>, units: usize) -> Self {
        Self { text, units }
    }

    /// A copy of `s`, counted.
    fn of(s: &str) -> Self {
        Self::new(Rc::from(s), js_len(s))
    }
}

impl Vars {
    pub(super) fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(|v| &*v.text)
    }

    /// The value of `name`, for a word to share.
    fn shared(&self, name: &str) -> Option<&Shared> {
        self.map.get(name)
    }

    /// Variables whose values are charged to `guard`, the line's budget.
    pub(super) fn metered(guard: &Guard) -> Self {
        Self {
            meter: Some(guard.clone()), // a shared handle on the line's budget
            ..Self::default()
        }
    }

    /// 1.x `setVar`: `None` deletes. Past the budget the guard is stopped, and the line
    /// falls back.
    pub(super) fn set(&mut self, name: &str, value: Option<String>) {
        let value = value.map(|text| {
            let units = js_len(&text);
            Shared::new(Rc::from(text), units)
        });
        self.set_shared(name, value);
    }

    /// [`Self::set`] with `word`'s value after its first `skip` bytes (ASCII): the piece it
    /// names shared when the value is one, else joined once; deleted when it is unknown.
    pub(super) fn set_word(&mut self, name: &str, word: &Word, skip: usize) {
        let held = self.scratch.capacity();
        let value = word.shared_after(skip, &mut self.scratch);
        let grown = self.scratch.capacity().saturating_sub(held);
        if let Some(meter) = self.meter.as_ref().filter(|_| grown > 0) {
            meter.charge(grown);
        }
        self.set_shared(name, value);
    }

    /// [`Self::set`] with a value already shared, whose units are known.
    fn set_shared(&mut self, name: &str, value: Option<Shared>) {
        if let (Some(meter), Some(value)) = (&self.meter, &value) {
            // `charge` stops the guard when the budget is spent; the walk then stops.
            meter.charge(value.text.len());
        }
        let old = self.put(name, value);
        if let Some(saved) = self.saved.last_mut()
            && !saved.contains(name)
        {
            saved.insert(name.to_owned());
            self.journal.push((name.to_owned(), old));
        } else {
            self.release(old);
        }
    }

    /// Refunds a value no longer held here once nothing else holds it either.
    fn release(&mut self, old: Option<Shared>) {
        let Some(old) = old else {
            return;
        };
        if Rc::strong_count(&old.text) > 1 {
            self.dropped.push(old.text);
        } else {
            self.refund(&old.text);
        }
        // Swept once per twice as many releases as the last sweep left held, and at least
        // 16: O(1) per release, amortised.
        self.since = self.since.saturating_add(1);
        if self.since > self.kept.saturating_mul(2).max(16) {
            self.sweep();
        }
    }

    /// Refunds every dropped value that nothing else holds now.
    fn sweep(&mut self) {
        let mut dropped = std::mem::take(&mut self.dropped);
        dropped.retain(|text| {
            let held = Rc::strong_count(text) > 1;
            if !held {
                self.refund(text);
            }
            held
        });
        self.since = 0;
        self.kept = dropped.len();
        self.dropped = dropped;
    }

    fn refund(&self, text: &str) {
        if let Some(meter) = &self.meter {
            meter.refund(text.len());
        }
    }

    fn put(&mut self, name: &str, value: Option<Shared>) -> Option<Shared> {
        let old = match value {
            Some(value) => self.map.insert(name.to_owned(), value),
            None => self.map.remove(name),
        };
        if name.starts_with("GIT_CONFIG") {
            let now = self.map.contains_key(name);
            match (old.is_some(), now) {
                (false, true) => self.git_config = self.git_config.saturating_add(1),
                (true, false) => self.git_config = self.git_config.saturating_sub(1),
                _ => {}
            }
        }
        old
    }

    /// Opens a scope, for [`Self::rollback`], which closes it.
    pub(super) fn mark(&mut self) -> Mark {
        let mark = Mark {
            journal: self.journal.len(),
            depth: self.saved.len(),
        };
        self.saved.push(HashSet::new());
        mark
    }

    /// Undoes every change since `mark` and closes its scope, with any opened inside it. The
    /// journal holds each name's value at the mark of the scope that first changed it, so
    /// undoing newest first leaves each name as it was at `mark`.
    pub(super) fn rollback(&mut self, mark: Mark) {
        while self.journal.len() > mark.journal {
            if let Some((name, old)) = self.journal.pop() {
                let undone = self.put(&name, old);
                self.release(undone);
            }
        }
        self.saved.truncate(mark.depth);
    }

    /// Some variable set earlier is a `GIT_CONFIG*` one.
    pub(super) fn has_git_config(&self) -> bool {
        self.git_config > 0
    }
}

/// What a word's expansions read: the walker's state and the parse options.
#[derive(Debug)]
pub(super) struct WordEnv<'e> {
    pub(super) ps: bool,
    pub(super) home: &'e str,
    pub(super) cwd: Option<&'e Dir>,
    pub(super) prev: Option<&'e Dir>,
    pub(super) tmp: Option<&'e str>,
    pub(super) top: Option<&'e str>,
    pub(super) git_dir: &'e str,
    pub(super) vars: &'e Vars,
    pub(super) style: PathStyle,
    /// Set when a word grows past [`JS_MAX_STRING`].
    pub(super) overflow: &'e Cell<bool>,
    /// The line's budget of text built, which a directory's text is charged to the first time
    /// a word expands `$PWD` or `$OLDPWD` to it.
    pub(super) guard: Option<&'e Guard>,
}

/// What one part of a word shows, as a piece of its text: an expansion's value, shared with
/// what holds it; a leading `~` as the home directory; or the part as written. With it, its
/// length in UTF-16 units and whether it is the part's value.
fn part_piece(part: &Part, first: bool, env: &WordEnv<'_>) -> (Shared, bool) {
    let s = part.text.as_str();
    let found = if !part.is_lit() {
        part_value(part, env)
    } else if !part.quoted && first {
        tilde(s, env.home).map(|t| Shared::of(&t))
    } else {
        Some(Shared::of(s))
    };
    found.map_or_else(|| (Shared::of(s), false), |piece| (piece, true))
}

/// 1.x `makeWord(parts, env)`.
pub(super) fn make_word(parts: &[Part], env: &WordEnv<'_>) -> Word {
    let mut raw = String::new();
    let mut pieces = Vec::new();
    let mut units = 0_usize;
    let mut known = true;
    let mut glob = false;
    // Where an unquoted `[` is in raw: a glob when a `]` follows it.
    let mut bracket: Option<usize> = None;
    for (k, part) in parts.iter().enumerate() {
        if part.kind == PartKind::Escape && !env.ps {
            // bash removes the escaping backslash.
            continue;
        }
        raw.push_str(&part.text);
        let s = part.text.as_str();
        let (piece, part_known) = part_piece(part, k == 0, env);
        known &= part_known;
        if part.is_lit() && !part.quoted {
            glob |= piece.text.contains(['*', '?']) || brace_list(&piece.text);
            if bracket.is_none()
                && let Some(at) = s.find('[')
            {
                bracket = Some(raw.len().saturating_sub(s.len()).saturating_add(at));
            }
        }
        // 1.x's `+=` throws past V8's longest string; the text stops before it.
        let more = units.saturating_add(piece.units);
        if more > JS_MAX_STRING {
            env.overflow.set(true);
            known = false;
            break;
        }
        units = more;
        pieces.push(piece.text);
    }
    if let Some(at) = bracket
        && raw
            .get(at.saturating_add(1)..)
            .is_some_and(|r| r.contains(']'))
    {
        glob = true;
    }
    // An unquoted `{` may open a brace list, one that spans a quoted part too.
    let expanded = parts.iter().any(|p| !p.is_lit());
    let literal = !expanded && !glob && !parts.iter().any(|p| !p.quoted && p.text.contains('{'));
    Word {
        raw,
        text: Text::of(pieces, units, env.guard),
        known,
        quoted: parts.iter().any(|p| p.quoted),
        assign: assign_len(parts),
        form: if literal {
            Form::Literal
        } else {
            Form::Made { glob, expanded }
        },
    }
}

/// The unquoted first part `s` with a leading `~` or `~/` read as the home directory; `None`
/// for `~user`, or `~` without a home, which temper does not know.
fn tilde<'s>(s: &'s str, home: &str) -> Option<Cow<'s, str>> {
    let home_tilde = s
        .strip_prefix('~')
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '\\']));
    if home_tilde && !home.is_empty() {
        Some(Cow::Owned([home, s.get(1..).unwrap_or("")].concat()))
    } else if s.starts_with('~') {
        None
    } else {
        Some(Cow::Borrowed(s))
    }
}

/// The length of the name a `NAME=` or `NAME+=` word assigns, or 0.
fn assign_len(parts: &[Part]) -> usize {
    let Some(first) = parts
        .first()
        .filter(|first| first.kind == PartKind::Lit && !first.quoted)
    else {
        return 0;
    };
    let len = name_len(&first.text);
    let after = first.text.get(len..).unwrap_or("");
    if len > 0 && (after.starts_with('=') || after.starts_with("+=")) {
        len
    } else {
        0
    }
}

/// 1.x `braceList(s)`: an unquoted `{a,b}` or `{1..3}`, found in one pass.
fn brace_list(s: &str) -> bool {
    let mut open = false;
    let (mut comma, mut dots, mut prev_dot) = (false, false, false);
    for c in s.chars() {
        match c {
            '{' => {
                open = true;
                (comma, dots, prev_dot) = (false, false, false);
            }
            '}' => {
                if open && (comma || dots) {
                    return true;
                }
                open = false;
            }
            _ if open => {
                comma |= c == ',';
                dots |= prev_dot && c == '.';
                prev_dot = c == '.';
            }
            _ => {}
        }
    }
    false
}

/// 1.x `TEMP_VARS`, case-insensitive: the variables that name the temp directory.
fn is_temp_var(name: &str) -> bool {
    [
        "TMPDIR",
        "TMP",
        "TEMP",
        "RUNNER_TEMP",
        "env:TMP",
        "env:TEMP",
    ]
    .iter()
    .any(|v| v.eq_ignore_ascii_case(name))
}

/// 1.x `partValue(p, env)`: an expansion's value, or `None` when temper does not know it.
/// A variable's value is shared with the variable, and a directory's with the directory.
fn part_value(part: &Part, env: &WordEnv<'_>) -> Option<Shared> {
    let dir_value = |dir: &Dir| {
        let text = dir.shared_text(env.guard)?;
        Some(Shared::new(text, dir.units()))
    };
    match &part.kind {
        PartKind::Var(name) => {
            if let Some(value) = env.vars.shared(name) {
                return Some(value.clone()); // a shared handle on the value
            }
            match name.as_str() {
                "HOME" => Some(Shared::of(env.home)),
                "PWD" => env.cwd.and_then(dir_value),
                "OLDPWD" => env.prev.and_then(dir_value),
                _ if is_temp_var(name) => env.tmp.map(Shared::of),
                _ => None,
            }
        }
        PartKind::Sub(src) => git_rev_parse(src, env)
            .or_else(|| mktemp(src, env))
            .map(|value| Shared::of(&value)),
        PartKind::Lit
        | PartKind::Escape
        | PartKind::Param
        | PartKind::Arith
        | PartKind::Proc(_) => None,
    }
}

/// The text after a run of JS spaces at the start of `s`.
fn skip_spaces(s: &str) -> &str {
    s.trim_start_matches(is_js_space)
}

/// `s` starts with `word` followed by a `\b` (a non-word char or the end).
fn word_then_boundary<'s>(s: &'s str, word: &str) -> Option<&'s str> {
    let rest = s.strip_prefix(word)?;
    let boundary = rest
        .bytes()
        .next()
        .is_none_or(|b| !(b.is_ascii_alphanumeric() || b == b'_'));
    boundary.then_some(rest)
}

/// `\s+word\b` at the start of `s`: the text after it.
fn spaced_word<'s>(s: &'s str, word: &str) -> Option<&'s str> {
    let after = skip_spaces(s);
    (after.len() < s.len()).then_some(())?;
    word_then_boundary(after, word)
}

/// 1.x `gitRevParse(src, env)`: what `git rev-parse --git-dir` and friends name inside
/// `$(...)`, or `None`.
pub(super) fn git_rev_parse(src: &str, env: &WordEnv<'_>) -> Option<String> {
    // `/^\s*git\b(?:\s+-C\s+\S+)?\s+rev-parse\b(.*)$/s`
    let after_git = word_then_boundary(skip_spaces(src), "git")?;
    let with_dir = skip_spaces(after_git)
        .strip_prefix("-C")
        .filter(|_| after_git.starts_with(is_js_space))
        .and_then(|rest| {
            let value = skip_spaces(rest);
            (value.len() < rest.len()).then_some(value)
        })
        .and_then(|value| {
            let run = value.find(is_js_space).unwrap_or(value.len());
            (run > 0).then(|| value.get(run..)).flatten()
        })
        .and_then(|rest| spaced_word(rest, "rev-parse"));
    let rest = with_dir.or_else(|| spaced_word(after_git, "rev-parse"))?;
    let args = without_path_format(rest);
    let args = js_trim(&args);
    if let Some(name) = git_path(args) {
        return Some([env.git_dir, "/", name].concat());
    }
    match args {
        "--git-common-dir" | "--git-dir" | "--absolute-git-dir" => Some(env.git_dir.to_owned()),
        "--show-toplevel" => env.top.map(str::to_owned),
        _ => None,
    }
}

/// 1.x `.replace(/(?:^|\s)--path-format=(?:absolute|relative)(?=\s|$)/g, ' ')`.
fn without_path_format(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(c) = s.get(i..).and_then(|r| r.chars().next()) {
        let (lead, from) = if i == 0 && !is_js_space(c) {
            (0, 0)
        } else if is_js_space(c) {
            (c.len_utf8(), i.saturating_add(c.len_utf8()))
        } else {
            out.push(c);
            i = i.saturating_add(c.len_utf8());
            continue;
        };
        let token = s.get(from..).and_then(|r| {
            ["--path-format=absolute", "--path-format=relative"]
                .iter()
                .find(|t| {
                    r.strip_prefix(**t)
                        .is_some_and(|tail| tail.is_empty() || tail.starts_with(is_js_space))
                })
        });
        match token {
            Some(token) => {
                out.push(' ');
                i = from.saturating_add(token.len());
            }
            None if lead == 0 => {
                out.push(c);
                i = i.saturating_add(c.len_utf8());
            }
            None => {
                out.push(c);
                i = from;
            }
        }
    }
    out
}

/// 1.x ``/--git-path\s+["']?([^\s"'`)]+)["']?\s*$/``: the name after the first `--git-path`
/// whose value ends the text. A value starts after a space or a quote, both of which end a
/// value, so the values read are disjoint and each is read once.
fn git_path(args: &str) -> Option<&str> {
    let stops = |c: char| is_js_space(c) || "\"'`)".contains(c);
    let mut from = 0;
    while let Some(found) = args.get(from..).and_then(|r| r.find("--git-path")) {
        let at = from.saturating_add(found);
        from = at.saturating_add(1);
        let after = args.get(at.saturating_add(10)..).unwrap_or("");
        let value = skip_spaces(after);
        if value.len() == after.len() {
            continue;
        }
        let start = args.len().saturating_sub(value.len());
        let start = start.saturating_add(usize::from(value.starts_with(['"', '\''])));
        let rest = args.get(start..).unwrap_or("");
        let end = start.saturating_add(rest.find(stops).unwrap_or(rest.len()));
        if end == start {
            continue;
        }
        let tail = args.get(end..).unwrap_or("");
        let tail = tail.strip_prefix(['"', '\'']).unwrap_or(tail);
        if tail.chars().all(is_js_space) {
            return args.get(start..end);
        }
    }
    None
}

/// 1.x `mktemp(src, env)`: the temp file a `$(mktemp [-d] [-t])` makes, or `None` for one in
/// the current directory.
fn mktemp(src: &str, env: &WordEnv<'_>) -> Option<String> {
    // `/^\s*mktemp((?:\s+(?:-[dqut]+|--(?:directory|quiet|dry-run|tmpdir)))*)(?:\s+([^\s/\\]+))?\s*$/`
    let rest = skip_spaces(src).strip_prefix("mktemp")?;
    if !rest.is_empty() && !rest.starts_with(is_js_space) {
        return None;
    }
    let words: Vec<&str> = rest.split(is_js_space).filter(|w| !w.is_empty()).collect();
    let is_option = |w: &str| {
        w.strip_prefix('-')
            .is_some_and(|f| !f.is_empty() && f.bytes().all(|b| b"dqut".contains(&b)))
            || ["--directory", "--quiet", "--dry-run", "--tmpdir"].contains(&w)
    };
    // The options are taken greedily, so a last word is the template only when it is not one.
    let (options, template): (&[&str], Option<&str>) = match words.split_last() {
        None => (&[], None),
        Some((last, _)) if is_option(last) => (&words, None),
        Some((last, head)) => (head, Some(*last)),
    };
    if !options.iter().all(|w| is_option(w)) {
        return None;
    }
    if template.is_some_and(|t| t.contains(['/', '\\'])) {
        return None;
    }
    let in_tmp = options.iter().any(|w| {
        *w == "--tmpdir"
            || w.strip_prefix('-').is_some_and(|f| {
                f.bytes().filter(|b| *b == b't').count() == 1
                    && f.bytes().all(|b| b"dqut".contains(&b))
            })
    });
    // A template without -t or --tmpdir is made in the current directory.
    if template.is_some() && !in_tmp {
        return None;
    }
    env.tmp
        .map(|tmp| paths::join(env.style, tmp, "tmp.XXXXXXXXXX"))
}

/// 1.x `msysPath(t)`: a git-bash path (`/c/Users/...`, `/tmp/...`) as the Windows path it
/// names; other paths, and every path on other platforms, unchanged.
pub(crate) fn msys_path<'t>(t: &'t str, style: PathStyle, os_tmp: &str) -> Cow<'t, str> {
    if style != PathStyle::Windows {
        return Cow::Borrowed(t);
    }
    // JS `.` stops at line ends, so a rest with one in it matches neither form.
    let line_end = |s: &str| s.contains(['\n', '\r', '\u{2028}', '\u{2029}']);
    let mut chars = t.chars();
    if let (Some('/'), Some(drive)) = (chars.next(), chars.next())
        && drive.is_ascii_alphabetic()
    {
        let rest = chars.as_str();
        if rest.is_empty() {
            return Cow::Owned(format!("{}:/", drive.to_ascii_uppercase()));
        }
        if rest.starts_with('/') && !line_end(rest) {
            return Cow::Owned(format!("{}:{rest}", drive.to_ascii_uppercase()));
        }
    }
    if let Some(rest) = t.strip_prefix("/tmp")
        && (rest.is_empty() || (rest.starts_with('/') && !line_end(rest)))
    {
        return Cow::Owned([os_tmp.replace('\\', "/").as_str(), rest].concat());
    }
    Cow::Borrowed(t)
}

/// Where paths are resolved: the platform's rules, the process's working directory and
/// Node's `os.tmpdir()`, which a git-bash `/tmp` names on Windows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PathCx<'a> {
    pub(crate) at: PathEnv<'a>,
    pub(crate) os_tmp: &'a str,
    /// What stops the line's reading where 1.x throws. Every rule passes its own; `None`
    /// (the reader's unit tests) records nothing and sets no budget.
    pub(crate) guard: Option<&'a Guard>,
}

impl PathCx<'_> {
    /// A directory's whole path, for a resolve (see [`Guard::dir_text`]).
    pub(crate) fn dir_text<'d>(&self, dir: Option<&'d Dir>) -> Option<Cow<'d, str>> {
        let dir = dir?;
        match self.guard {
            Some(guard) => guard.dir_text(dir),
            None => Some(dir.text()),
        }
    }

    /// Whether `a` and `b` joined with `extra` more units fit in a JS string; when not, marks
    /// the overflow where 1.x's `+`, `path.resolve` or `path.join` throws.
    pub(crate) fn joins(&self, a: &str, b: &str, extra: usize) -> bool {
        self.fits(js_len(a).saturating_add(js_len(b)).saturating_add(extra))
    }

    /// Whether the line has gone past a bound, where 1.x has thrown: the rule reads no more.
    pub(crate) fn stopped(&self) -> bool {
        self.guard.is_some_and(Guard::overflowed)
    }

    /// Whether `units` UTF-16 units fit in a JS string; when not, marks the overflow.
    pub(crate) fn fits(&self, units: usize) -> bool {
        match self.guard {
            Some(guard) => guard.fits(units),
            None => units <= JS_MAX_STRING,
        }
    }
}

/// 1.x `resolvePath(value, base)`: the absolute path `value` names from `base`, or `None`
/// when it is unknown, empty, or relative to an unknown directory.
pub(crate) fn resolve_path(
    cx: PathCx<'_>,
    value: Option<&str>,
    base: Option<&str>,
) -> Option<String> {
    let value = value.filter(|v| !v.is_empty())?;
    let style = cx.at.style;
    let t = msys_path(value, style, cx.os_tmp);
    if paths::is_absolute(style, &t) {
        // Node's resolve builds `${t}/` first.
        return cx
            .joins(&t, "", 1)
            .then(|| paths::resolve(style, cx.at.cwd, &t));
    }
    let base = base.filter(|b| !b.is_empty())?;
    // Then `${base}/${t}/`, which throws past V8's longest string.
    cx.joins(base, &t, 2).then(|| resolve_from(cx.at, base, &t))
}

/// Node's `path.resolve(base, t)` for an absolute `base`: a drive-relative `t` on another
/// drive than `base`'s is resolved from the process's working directory, as Node does.
pub(crate) fn resolve_from(at: PathEnv<'_>, base: &str, t: &str) -> String {
    if at.style == PathStyle::Windows {
        let device = |p: &str| {
            let mut c = p.chars();
            match (c.next(), c.next()) {
                (Some(d), Some(':')) if d.is_ascii_alphabetic() => Some(d.to_ascii_lowercase()),
                _ => None,
            }
        };
        if let Some(d) = device(t)
            && device(base) != Some(d)
        {
            return paths::resolve(at.style, at.cwd, t);
        }
    }
    paths::resolve(at.style, base, t)
}

/// 1.x `wordPath(w, cwd)`: the absolute path a word names when the command runs in `cwd`;
/// `None` for a glob or an unknown word.
pub(crate) fn word_path(cx: PathCx<'_>, word: &Word, cwd: Option<&Dir>) -> Option<String> {
    if word.glob() {
        return None;
    }
    // A relative name steps from a directory in normal form, as `cd` does ([`Dir::step`]):
    // the same path as the full resolve, for the cost of a copy.
    let style = cx.at.style;
    // Read once, and charged when that copies the pieces.
    let read = if word.known && word.text.len() > 0 {
        Some(word.text_read()?)
    } else {
        None
    };
    let value = read.as_deref();
    let relative = cwd.zip(value).and_then(|(dir, value)| {
        let t = msys_path(value, style, cx.os_tmp);
        (dir.is_normal() && !paths::is_absolute(style, &t)).then_some((dir, t))
    });
    if let Some((dir, t)) = relative {
        // 1.x's `${base}/${t}/` past V8's longest string throws.
        let units = dir.units().saturating_add(js_len(&t)).saturating_add(2);
        if !cx.fits(units) {
            return None;
        }
        if let Some(to) = dir.step(style, &t) {
            // The path is built here, and that is what the budget counts.
            let charged = cx.guard.is_none_or(|guard| guard.charge(to.len()));
            return charged.then(|| to.text().into_owned());
        }
    }
    if names_absolute(cx, word) {
        return resolve_path(cx, value, None); // no directory to read
    }
    let base = cx.dir_text(cwd);
    resolve_path(cx, value, base.as_deref())
}

/// Whether `word` names an absolute path, which Node's `path.resolve` reads without the
/// directory: a rule then need not build it.
pub(crate) fn names_absolute(cx: PathCx<'_>, word: &Word) -> bool {
    let style = cx.at.style;
    // The first three bytes decide it (`C:\`, `\\`, `/`), and a git-bash `/c/x` or `/tmp`
    // stays absolute, so the value is not joined.
    word.known
        && word.text.len() > 0
        && !word.glob()
        && paths::is_absolute(style, &msys_path(&word.text.head(3), style, cx.os_tmp))
}

/// Whether [`word_path`] would stop the line, where 1.x's `wordPath` throws past V8's
/// longest string, without building the path: 1.x makes every target's path before it
/// checks one, so this marks the overflow for each, and a rule builds only the paths it reads.
pub(super) fn word_path_fits(cx: PathCx<'_>, word: &Word, cwd: Option<&Dir>) -> bool {
    if !word.known || word.text.len() == 0 || word.glob() {
        return true;
    }
    let style = cx.at.style;
    // A git-bash path changes length on Windows only; elsewhere the units are known
    // without joining the value.
    let units = match style {
        PathStyle::Posix => word.text.units(),
        PathStyle::Windows => match word.text.read() {
            Some(value) => js_len(&msys_path(&value, style, cx.os_tmp)),
            None => return false, // past the line's budget, which stops the rule
        },
    };
    if names_absolute(cx, word) {
        return cx.fits(units.saturating_add(1)); // `${t}/`
    }
    // `${base}/${t}/`, for the full resolve and the step alike.
    cwd.is_none_or(|dir| cx.fits(dir.units().saturating_add(units).saturating_add(2)))
}

/// The directory `word` names from `cwd`, as a `cd`, `git -C` or `patch -d` enters it: a
/// relative name steps from a directory in normal form, which builds no path, and any other
/// is resolved whole.
pub(super) fn word_dir(cx: PathCx<'_>, word: &Word, cwd: Option<&Dir>) -> Option<Dir> {
    let style = cx.at.style;
    let step = cwd.zip(word.value().filter(|v| !v.is_empty() && !word.glob()));
    if let Some((base, value)) = step {
        let t = msys_path(value, style, cx.os_tmp);
        if !word_path_fits(cx, word, cwd) {
            return None;
        }
        if let Some(dir) = base.step(style, &t) {
            return Some(dir);
        }
    }
    word_path(cx, word, cwd).map(Dir::resolved)
}

/// 1.x `commandName(v)`: the basename, lower case, without `.exe`; `gpatch` is `patch`.
pub(super) fn command_name(value: &str) -> String {
    let last = value.rsplit(['/', '\\']).next().unwrap_or(value);
    let lower = last.to_lowercase();
    let name = lower.strip_suffix(".exe").unwrap_or(&lower);
    if name == "gpatch" {
        "patch".to_owned()
    } else {
        name.to_owned()
    }
}

/// 1.x `nameOf(w)`: the command name a word runs as, or `None` when it is unknown.
pub(super) fn name_of(word: &Word) -> Option<String> {
    // The name is the value's last segment, read from the end without joining the value.
    word.known.then(|| command_name(&word.text.last_segment()))
}

/// `^\$\w+$`: a PowerShell variable as a whole word.
pub(super) fn is_ps_var(raw: &str) -> bool {
    raw.strip_prefix('$').is_some_and(|name| {
        !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

#[cfg(test)]
mod tests {
    use super::{Guard, Vars, brace_list, git_path, without_path_format};

    /// A top-level assignment keeps no undo record; a scope's are undone, nested or not.
    #[test]
    fn variables_journal_only_inside_a_scope_and_roll_back_to_its_mark() {
        let mut vars = Vars::default();
        for value in ["x", "x/b", "x/b/b"] {
            vars.set("a", Some(value.to_owned()));
        }
        assert_eq!(vars.journal.len(), 0);
        let outer = vars.mark();
        vars.set("a", Some("y".to_owned()));
        let inner = vars.mark();
        vars.set("a", None);
        vars.set("GIT_CONFIG_GLOBAL", Some("/x".to_owned()));
        assert!(vars.has_git_config());
        vars.rollback(inner);
        assert_eq!(vars.get("a"), Some("y"));
        assert!(!vars.has_git_config());
        vars.rollback(outer);
        assert_eq!(vars.get("a"), Some("x/b/b"));
        vars.set("a", Some("z".to_owned()));
        assert_eq!(vars.journal.len(), 0);
    }

    /// A scope keeps one undo record per name, the value at its mark, however often the name
    /// changes in it: `( a=$a/b; a=$a/b; ...)` keeps one old value, not every one (#201 F1).
    #[test]
    fn a_scope_journals_each_name_once_and_rolls_back_to_its_value_at_the_mark() {
        let mut vars = Vars::default();
        vars.set("a", Some("x".to_owned()));
        let outer = vars.mark();
        for value in ["x/b", "x/b/b", "x/b/b/b"] {
            vars.set("a", Some(value.to_owned()));
        }
        assert_eq!(vars.journal.len(), 1);
        let inner = vars.mark();
        vars.set("a", Some("y".to_owned()));
        vars.set("a", Some("y/b".to_owned()));
        vars.set("c", Some("1".to_owned()));
        assert_eq!(vars.journal.len(), 3);
        vars.rollback(inner);
        assert_eq!((vars.get("a"), vars.get("c")), (Some("x/b/b/b"), None));
        // A mark left open (an unclosed `(`) is closed by the rollback of the one outside it.
        let _unclosed = vars.mark();
        vars.set("a", Some("z".to_owned()));
        vars.set("a", Some("z/b".to_owned()));
        vars.rollback(outer);
        assert_eq!(vars.get("a"), Some("x"));
        assert_eq!(vars.journal.len(), 0);
        vars.set("a", Some("w".to_owned()));
        assert_eq!(vars.journal.len(), 0);
    }

    /// #202 third review F2: the guard is charged each value set and refunded each value the
    /// variables drop that nothing else holds; a journaled value stays charged until its
    /// scope closes, and one a word still shares until the word is gone.
    #[test]
    fn variables_charge_the_values_they_keep_alive_net_of_those_they_drop() {
        let guard = Guard::with_budget(1 << 20);
        let mut vars = Vars::metered(&guard);
        let long = "x".repeat(1000);
        vars.set("a", Some(long.clone()));
        assert_eq!(guard.spent(), 1000);
        vars.set("a", Some("y".to_owned()));
        assert_eq!(guard.spent(), 1, "the old value was freed");
        let mark = vars.mark();
        vars.set("a", Some(long.clone()));
        vars.set("b", Some(long.clone()));
        assert_eq!(guard.spent(), 2001, "the journal holds `y`");
        vars.rollback(mark);
        assert_eq!(guard.spent(), 1, "both long values freed");
        vars.set("c", Some(long));
        let shared = vars.shared("c").map(|s| std::rc::Rc::clone(&s.text));
        vars.set("c", None);
        assert_eq!(guard.spent(), 1001, "a word still holds it");
        drop(shared);
        vars.set("d", Some("z".to_owned()));
        for i in 0..64 {
            vars.set("e", Some(i.to_string()));
        }
        assert!(
            guard.spent() < 1000,
            "freed once nothing holds it: {}",
            guard.spent()
        );
        assert!(!guard.overflowed());
    }

    #[test]
    fn helpers_read_as_1x_regexes_do() {
        assert!(brace_list("{a,b}"));
        assert!(brace_list("x{1..3}"));
        assert!(brace_list("{a{b,c}"));
        assert!(!brace_list("{a}{b}"));
        assert!(!brace_list("{a,b"));
        assert_eq!(
            without_path_format("--path-format=absolute --git-dir"),
            "  --git-dir"
        );
        assert_eq!(
            without_path_format("--git-dir --path-format=relative"),
            "--git-dir "
        );
        assert_eq!(
            without_path_format("--path-format=absolutex"),
            "--path-format=absolutex"
        );
        assert_eq!(git_path("--git-path 'temper/x' "), Some("temper/x"));
        assert_eq!(git_path("--git-path a b"), None);
        assert_eq!(git_path("--git-path x--git-path y"), Some("y"));
    }
}
