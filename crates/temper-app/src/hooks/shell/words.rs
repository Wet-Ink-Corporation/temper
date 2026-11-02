//! Words as the rules see them (1.x `shell.mjs` `makeWord`, `partValue`, `gitRevParse`,
//! `mktemp`), and the paths they name (`msysPath`, `resolvePath`, `wordPath`).

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use temper_domain::{PathStyle, js_trim};

use super::lex::{Part, PartKind, name_len};

/// V8's `String::kMaxLength` on a 64-bit host, in UTF-16 units: 1.x throws `RangeError:
/// Invalid string length` when a word's text would grow past it (`X=$X$X` repeated), and its
/// rule then falls back.
const JS_MAX_STRING: usize = (1 << 29) - 24;

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
use crate::paths::{self, PathEnv};
use crate::project::is_js_space;

/// A word: `raw` (quotes removed, expansions as written), `text` (known expansions replaced,
/// unknown ones as written) and whether every part is known, which makes `text` its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Word {
    pub(super) raw: String,
    pub(super) text: String,
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
            text: s.to_owned(),
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
        &self.text
    }

    /// The value, or `None` when a part is unknown.
    pub(crate) fn value(&self) -> Option<&str> {
        self.known.then_some(self.text.as_str())
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
/// time proportional to them (1.x copies the whole `Map` at each `(` and `$(`). A change made
/// while no scope is open is never undone, so it is not journaled: `a=$a/b` repeated at the
/// top level keeps one value, not every earlier one.
#[derive(Debug, Default)]
pub(super) struct Vars {
    map: HashMap<String, String>,
    journal: Vec<(String, Option<String>)>,
    /// How many marks are open. A scope the line leaves unclosed is never rolled back, so its
    /// mark stays open and only costs journaling.
    open: usize,
    /// How many names start with `GIT_CONFIG`, which git reads as configuration.
    git_config: usize,
}

impl Vars {
    pub(super) fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }

    /// 1.x `setVar`: `None` deletes.
    pub(super) fn set(&mut self, name: &str, value: Option<String>) {
        let old = self.put(name, value);
        if self.open > 0 {
            self.journal.push((name.to_owned(), old));
        }
    }

    fn put(&mut self, name: &str, value: Option<String>) -> Option<String> {
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

    /// Opens a scope: where the journal is now, for [`Self::rollback`], which closes it.
    pub(super) fn mark(&mut self) -> usize {
        self.open = self.open.saturating_add(1);
        self.journal.len()
    }

    /// Undoes every change since `mark` and closes its scope.
    pub(super) fn rollback(&mut self, mark: usize) {
        while self.journal.len() > mark {
            if let Some((name, old)) = self.journal.pop() {
                self.put(&name, old);
            }
        }
        self.open = self.open.saturating_sub(1);
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
    pub(super) cwd: Option<&'e str>,
    pub(super) prev: Option<&'e str>,
    pub(super) tmp: Option<&'e str>,
    pub(super) top: Option<&'e str>,
    pub(super) git_dir: &'e str,
    pub(super) vars: &'e Vars,
    pub(super) style: PathStyle,
    /// Set when a word grows past [`JS_MAX_STRING`].
    pub(super) overflow: &'e Cell<bool>,
}

/// 1.x `makeWord(parts, env)`.
pub(super) fn make_word(parts: &[Part], env: &WordEnv<'_>) -> Word {
    let mut raw = String::new();
    let mut text = String::new();
    let mut units = 0;
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
        let shown = if !part.is_lit() {
            part_value(part, env).unwrap_or_else(|| {
                known = false;
                Cow::Borrowed(s)
            })
        } else if !part.quoted && k == 0 {
            tilde(s, env.home).unwrap_or_else(|| {
                known = false;
                Cow::Borrowed(s)
            })
        } else {
            Cow::Borrowed(s)
        };
        if part.is_lit() && !part.quoted {
            glob |= shown.contains(['*', '?']) || brace_list(&shown);
            if bracket.is_none()
                && let Some(at) = s.find('[')
            {
                bracket = Some(raw.len().saturating_sub(s.len()).saturating_add(at));
            }
        }
        if !push_js(&mut text, &mut units, &shown, env.overflow) {
            known = false;
            break;
        }
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
        text,
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
fn part_value<'e>(part: &Part, env: &WordEnv<'e>) -> Option<Cow<'e, str>> {
    match &part.kind {
        PartKind::Var(name) => {
            if let Some(value) = env.vars.get(name) {
                return Some(Cow::Borrowed(value));
            }
            match name.as_str() {
                "HOME" => Some(env.home),
                "PWD" => env.cwd,
                "OLDPWD" => env.prev,
                _ if is_temp_var(name) => env.tmp,
                _ => None,
            }
            .map(Cow::Borrowed)
        }
        PartKind::Sub(src) => git_rev_parse(src, env)
            .or_else(|| mktemp(src, env))
            .map(Cow::Owned),
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
        return Some(paths::resolve(style, cx.at.cwd, &t));
    }
    let base = base.filter(|b| !b.is_empty())?;
    Some(resolve_from(cx.at, base, &t))
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
pub(crate) fn word_path(cx: PathCx<'_>, word: &Word, cwd: Option<&str>) -> Option<String> {
    if word.glob() {
        return None;
    }
    resolve_path(cx, word.value(), cwd)
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
    word.value().map(command_name)
}

/// A shared directory, as the walker carries it.
pub(super) type Dir = Rc<str>;

/// `^\$\w+$`: a PowerShell variable as a whole word.
pub(super) fn is_ps_var(raw: &str) -> bool {
    raw.strip_prefix('$').is_some_and(|name| {
        !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

#[cfg(test)]
mod tests {
    use super::{Vars, brace_list, git_path, without_path_format};

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
