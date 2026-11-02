//! The walker (1.x `shell.mjs` `parseCommands`, `run`, `sub`, `simple`): the simple commands
//! a command line runs, in order, each with the directory it runs in.
//!
//! Variables live in a journal ([`Vars`]) and directories are shared [`Dir`]s, so opening and
//! closing a subshell or a substitution costs the names it changes, not the variables before
//! it (1.x copies the variable map at every `(` and `$(`); a `cd` costs the value it reads,
//! never a copy or a re-read of the whole path ([`word_dir`]).

use std::cell::Cell;
use std::rc::Rc;

use temper_domain::js_trim;

use super::lex::{self, Item, PartKind, RawCommand, RedirectOp};
use super::words::{
    Dir, Guard, Mark, PathCx, Vars, Word, WordEnv, git_rev_parse, is_ps_var, js_len, make_word,
    name_of, push_js, word_dir,
};
use super::wrappers::{Run, Xargs, inner_script};

/// How deep temper follows commands inside commands (1.x `MAX_DEPTH`).
pub(super) const MAX_DEPTH: usize = 4;

/// Where a parse starts (1.x `parseCommands`' options).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParseOpts<'a> {
    pub(crate) paths: PathCx<'a>,
    /// The starting directory, absolute, or `None` when it is unknown.
    pub(crate) cwd: Option<&'a str>,
    /// `~` and `$HOME`.
    pub(crate) home: &'a str,
    /// What `$(git rev-parse --show-toplevel)` names.
    pub(crate) top: Option<&'a str>,
    /// What `$(git rev-parse --git-dir)` names: `.git` unless the caller knows better.
    pub(crate) git_dir: &'a str,
    /// The temp directory `$TMPDIR` and `$(mktemp)` name.
    pub(crate) tmp: Option<&'a str>,
}

/// A redirection, with its target as a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Redirect {
    pub(super) op: RedirectOp,
    pub(super) word: Word,
}

/// Where a command's stdin comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stdin {
    /// The stage before it in a pipeline, and the heredoc or here-string body that stage read.
    Pipe { body: Option<Rc<str>> },
    /// `< file`.
    File(Word),
    /// `<<EOF`: its body, and whether it has no expansion in it.
    Heredoc {
        body: Option<Rc<str>>,
        literal: bool,
    },
    /// `<<< word`: its text, and whether the word is literal.
    Str { body: Rc<str>, literal: bool },
}

impl Stdin {
    /// 1.x `stdin.body`: what a shell reading this stdin runs.
    pub(super) fn body(&self) -> Option<&str> {
        match self {
            Self::Pipe { body } | Self::Heredoc { body, .. } => body.as_deref(),
            Self::Str { body, .. } => Some(body),
            Self::File(_) => None,
        }
    }
}

/// One simple command a command line runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Command {
    /// The command name as the rules compare it; `Some("")` for redirections alone, `None`
    /// when the name is unknown.
    pub(crate) name: Option<String>,
    pub(crate) argv0: Word,
    pub(crate) args: Vec<Word>,
    pub(crate) redirects: Vec<Redirect>,
    pub(crate) stdin: Option<Stdin>,
    /// It reads a pipe.
    pub(crate) piped: bool,
    /// The directory it runs in, absolute, or `None` after a `cd` temper cannot follow.
    pub(crate) cwd: Option<Dir>,
    /// A `GIT_CONFIG*` variable is set for it, which git reads as configuration.
    pub(crate) git_config: bool,
}

impl Command {
    /// 1.x `depthMarker`: what stands for commands nested deeper than temper reads.
    fn depth_marker() -> Self {
        Self {
            name: None,
            argv0: Word::unknown(""),
            args: Vec::new(),
            redirects: Vec::new(),
            stdin: None,
            piped: false,
            cwd: None,
            git_config: false,
        }
    }
}

/// A word's text grew past the longest string V8 makes, where 1.x's reader throws and the
/// rule that called it falls back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooLong;

/// 1.x `parseCommands(cmd, opts)`.
pub(crate) fn parse_commands(cmd: &str, opts: ParseOpts<'_>) -> Result<Vec<Command>, TooLong> {
    let cwd = opts.cwd.map(|dir| {
        Dir::resolved(crate::paths::resolve(
            opts.paths.at.style,
            opts.paths.at.cwd,
            dir,
        ))
    });
    // A resolve past V8's longest string stops the walk whoever asked for it.
    let own = Guard::default();
    let guard = opts.paths.guard.unwrap_or(&own);
    let paths = PathCx {
        guard: Some(guard),
        ..opts.paths
    };
    let mut walker = Walker {
        opts: ParseOpts { paths, ..opts },
        overflow: Cell::new(false),
        env: Env {
            cwd,
            ..Env::default()
        },
        out: Vec::new(),
        depth: 0,
    };
    walker.run(cmd);
    if walker.overflow.get() || guard.overflowed() {
        return Err(TooLong);
    }
    Ok(walker.out)
}

/// What the commands so far have set.
#[derive(Debug, Default)]
pub(super) struct Env {
    pub(super) cwd: Option<Dir>,
    pub(super) prev: Option<Dir>,
    /// The commands are PowerShell, which reads a kept backslash as a separator.
    pub(super) ps: bool,
    pub(super) vars: Vars,
    /// The `pushd` stack, shared with a substitution as in 1.x.
    pub(super) dirs: Vec<Option<Dir>>,
    /// The bottom of the stack `popd` sees: `find -exec` starts a fresh one.
    pub(super) floor: usize,
}

/// What a simple command carries from its line.
#[derive(Debug, Default)]
pub(super) struct Meta {
    pub(super) redirects: Vec<Redirect>,
    pub(super) stdin: Option<Stdin>,
    pub(super) piped: bool,
    /// A pipeline stage or a background job, whose `cd` and assignments end with it.
    pub(super) isolated: bool,
}

pub(super) struct Walker<'o> {
    pub(super) opts: ParseOpts<'o>,
    /// A word, directory or joined script grew past V8's longest string (see [`TooLong`]).
    pub(super) overflow: Cell<bool>,
    pub(super) env: Env,
    pub(super) out: Vec<Command>,
    /// How deep in subshells, substitutions and inner scripts the walk is (1.x's `depth`).
    depth: usize,
}

/// What a `(` saves.
struct Scope {
    cwd: Option<Dir>,
    prev: Option<Dir>,
    dirs: usize,
    vars: Mark,
}

/// 1.x `KEYWORDS`.
const KEYWORDS: [&str; 13] = [
    "if", "then", "else", "elif", "fi", "do", "done", "while", "until", "!", "{", "}", "esac",
];

/// The index of the first word at or after `i` that is not a reserved word.
fn skip_keywords(words: &[Word], mut i: usize) -> usize {
    while words
        .get(i)
        .is_some_and(|w| !w.quoted && KEYWORDS.contains(&w.raw.as_str()))
    {
        i = i.saturating_add(1);
    }
    i
}

impl Walker<'_> {
    pub(super) fn word_env(&self, ps: bool) -> WordEnv<'_> {
        WordEnv {
            ps,
            home: self.opts.home,
            cwd: self.env.cwd.as_ref(),
            prev: self.env.prev.as_ref(),
            tmp: self.opts.tmp,
            top: self.opts.top,
            git_dir: self.opts.git_dir,
            vars: &self.env.vars,
            style: self.opts.paths.at.style,
            overflow: &self.overflow,
            guard: self.opts.paths.guard,
        }
    }

    /// `dir`, unless it is longer than a JS string can be: 1.x's `path.resolve` throws there,
    /// so this marks the overflow that stops the reader (see [`TooLong`]).
    pub(super) fn capped(&self, dir: Option<Dir>) -> Option<Dir> {
        dir.filter(|d| {
            let fits = self.opts.paths.fits(d.units());
            if !fits {
                self.overflow.set(true);
            }
            fits
        })
    }

    /// A string grew past V8's longest, where 1.x throws (see [`TooLong`]).
    fn stopped(&self) -> bool {
        self.overflow.get() || self.opts.paths.guard.is_some_and(Guard::overflowed)
    }

    /// Runs `walk` one level deeper (1.x's `depth + 1`).
    pub(super) fn deeper(&mut self, walk: impl FnOnce(&mut Self)) {
        self.depth = self.depth.saturating_add(1);
        walk(self);
        self.depth = self.depth.saturating_sub(1);
    }

    /// 1.x `run(src, env, out, depth)`.
    pub(super) fn run(&mut self, src: &str) {
        if self.depth > MAX_DEPTH {
            self.out.push(Command::depth_marker());
            return;
        }
        let mut scopes: Vec<Scope> = Vec::new();
        let mut prev_body: Option<Rc<str>> = None;
        for item in lex::lex(src) {
            // 1.x has thrown by now: nothing after it is read.
            if self.stopped() {
                return;
            }
            match item {
                Item::Open => scopes.push(Scope {
                    cwd: self.env.cwd.clone(),
                    prev: self.env.prev.clone(),
                    dirs: self.env.dirs.len(),
                    vars: self.env.vars.mark(),
                }),
                Item::Close => {
                    if let Some(scope) = scopes.pop() {
                        self.env.cwd = scope.cwd;
                        self.env.prev = scope.prev;
                        let keep = self.env.dirs.len().min(scope.dirs);
                        self.env.dirs.truncate(keep);
                        self.env.vars.rollback(scope.vars);
                    }
                }
                Item::Command(raw) => prev_body = self.item(&raw, prev_body.take()),
            }
        }
    }

    /// One simple command of a line: its substitutions first, then the command. Returns the
    /// heredoc or here-string body it read, which a pipe passes on.
    fn item(&mut self, raw: &RawCommand, prev_body: Option<Rc<str>>) -> Option<Rc<str>> {
        // Command substitutions run first, in a subshell.
        let words = raw
            .words
            .iter()
            .chain(raw.redirects.iter().map(|r| &r.word));
        for word in words {
            for part in &word.parts {
                let src = match &part.kind {
                    PartKind::Sub(src) if git_rev_parse(src, &self.word_env(false)).is_none() => {
                        src
                    }
                    PartKind::Proc(src) => src,
                    _ => continue,
                };
                if !js_trim(src).is_empty() {
                    self.deeper(|walker| walker.sub(src));
                }
            }
        }
        let words = self.command_words(raw);
        let wenv = self.word_env(self.env.ps || words.ps);
        let redirects: Vec<Redirect> = raw
            .redirects
            .iter()
            .map(|r| Redirect {
                op: r.op,
                word: make_word(&r.word.parts, &wenv),
            })
            .collect();
        let piped = raw.sep.is_pipe();
        let isolated = piped || raw.after.is_pipe() || raw.after == lex::Joint::Background;
        let mut stdin = piped.then_some(Stdin::Pipe { body: prev_body });
        for (redirect, lexed) in redirects.iter().zip(&raw.redirects) {
            stdin = match (redirect.op, &lexed.heredoc) {
                (RedirectOp::In, _) => Some(Stdin::File(redirect.word.clone())),
                (RedirectOp::Heredoc | RedirectOp::HeredocTabs, heredoc) => {
                    let body = heredoc.as_ref().and_then(|h| h.body.clone());
                    let quoted = heredoc.as_ref().is_some_and(|h| h.quoted);
                    let literal = body.as_deref().is_some_and(|b| quoted || !expands(b));
                    Some(Stdin::Heredoc { body, literal })
                }
                (RedirectOp::HereString, _) => Some(Stdin::Str {
                    body: Rc::clone(&redirect.word.text), // shared with the word
                    literal: redirect.word.is_literal(),
                }),
                _ => stdin,
            };
        }
        let passed = match &stdin {
            Some(Stdin::Heredoc { body, .. }) => body.clone(),
            Some(Stdin::Str { body, .. }) => Some(Rc::clone(body)),
            Some(Stdin::Pipe { .. } | Stdin::File(_)) | None => None,
        };
        let meta = Meta {
            redirects,
            stdin,
            piped,
            isolated,
        };
        self.simple(words.words, meta);
        passed
    }

    /// The command's words, made as PowerShell reads them when it is a PowerShell command, an
    /// unquoted empty expansion dropped (`X=; $X git apply x`).
    fn command_words(&self, raw: &RawCommand) -> CommandWords {
        let make = |ps: bool| -> Vec<Word> {
            let wenv = self.word_env(ps);
            raw.words
                .iter()
                .map(|w| make_word(&w.parts, &wenv))
                .collect()
        };
        let (words, ps) = if self.env.ps {
            (make(true), true)
        } else {
            let plain = make(false);
            if ps_command(&plain) {
                (make(true), true)
            } else {
                (plain, false)
            }
        };
        let words = words
            .into_iter()
            .filter(|w| !(w.value() == Some("") && !w.quoted && w.expanded()))
            .collect();
        CommandWords { words, ps }
    }

    /// 1.x `sub(src, env, out, depth)`: a subshell, whose changes end with it.
    pub(super) fn sub(&mut self, src: &str) {
        let (cwd, prev) = (self.env.cwd.clone(), self.env.prev.clone());
        let (dirs, vars) = (self.env.dirs.len(), self.env.vars.mark());
        self.run(src);
        self.env.cwd = cwd;
        self.env.prev = prev;
        let keep = self.env.dirs.len().min(dirs);
        self.env.dirs.truncate(keep);
        self.env.vars.rollback(vars);
    }

    /// 1.x `sub(src, { ...env, cwd, ps }, ...)`: a subshell in `cwd`, read as `ps` says.
    pub(super) fn sub_in(&mut self, src: &str, cwd: Option<Dir>, ps: bool) {
        let saved_cwd = std::mem::replace(&mut self.env.cwd, cwd);
        let saved_ps = std::mem::replace(&mut self.env.ps, ps);
        let saved_prev = self.env.prev.clone();
        self.sub(src);
        self.env.cwd = saved_cwd;
        self.env.ps = saved_ps;
        self.env.prev = saved_prev;
    }

    /// 1.x `simple(words, meta, { ...env, cwd, vars: new Map(env.vars), dirs: [] })`, as
    /// `find -exec` runs one: nothing it sets outlives it.
    pub(super) fn simple_apart(&mut self, words: Vec<Word>, meta: Meta, cwd: Option<Dir>) {
        let saved_cwd = std::mem::replace(&mut self.env.cwd, cwd);
        let saved_prev = self.env.prev.clone();
        let saved_ps = self.env.ps;
        let vars = self.env.vars.mark();
        let dirs = self.env.dirs.len();
        let floor = std::mem::replace(&mut self.env.floor, dirs);
        self.simple(words, meta);
        self.env.cwd = saved_cwd;
        self.env.prev = saved_prev;
        self.env.ps = saved_ps;
        self.env.vars.rollback(vars);
        self.env.dirs.truncate(dirs);
        self.env.floor = floor;
    }

    /// 1.x `assignTo(env, w)`: `NAME=value` sets, `NAME+=value` appends to a known value.
    pub(super) fn assign(&mut self, word: &Word) {
        let Some(name) = word.assign() else {
            return;
        };
        let value = word.value();
        let tail = value.and_then(|v| v.get(name.len()..));
        let value = match tail.and_then(|t| t.strip_prefix("+=")) {
            Some(more) => self.env.vars.get(name).and_then(|old| {
                let mut value = old.to_owned();
                let mut units = js_len(old);
                push_js(&mut value, &mut units, more, &self.overflow).then_some(value)
            }),
            None => tail.and_then(|t| t.get(1..)).map(str::to_owned),
        };
        let name = name.to_owned();
        self.env.vars.set(&name, value);
    }

    /// Pushes the command `run` leaves, named `name`.
    fn emit(&mut self, name: Option<String>, run: Run, meta: &mut Meta) {
        let mut words = run.words;
        let argv0 = if words.is_empty() {
            Word::literal("")
        } else {
            words.remove(0)
        };
        self.out.push(Command {
            name,
            argv0,
            args: words,
            redirects: std::mem::take(&mut meta.redirects),
            stdin: meta.stdin.clone(),
            piped: meta.piped,
            cwd: run.cwd,
            git_config: run.git_config || self.env.vars.has_git_config(),
        });
    }

    /// Pushes a line of redirections alone (`> file`, `exec > file`), when it has any.
    fn emit_redirects(&mut self, meta: &mut Meta, git_config: bool) {
        if meta.redirects.is_empty() {
            return;
        }
        let run = Run {
            at: 0,
            words: Vec::new(),
            cwd: self.env.cwd.clone(), // a shared `Rc` handle
            xargs: None,
            git_config,
        };
        self.emit(Some(String::new()), run, meta);
    }

    /// 1.x `simple(words, meta, env, out, depth)`: keywords, assignments and wrappers, then
    /// the command and what it runs.
    pub(super) fn simple(&mut self, words: Vec<Word>, mut meta: Meta) {
        let mut w = words;
        w.drain(..skip_keywords(&w, 0).min(w.len()));
        let Some(first) = w.first() else {
            self.emit_redirects(&mut meta, false);
            return;
        };
        if first.raw == "function" && !first.quoted {
            // function f { ...
            let from = skip_keywords(&w, 2).min(w.len());
            w.drain(..from);
            if w.is_empty() {
                return;
            }
        }
        if let Some(first) = w.first()
            && !first.quoted
            && ["for", "select", "case"].contains(&first.raw.as_str())
        {
            if first.raw != "case"
                && let Some(var) = w.get(1)
            {
                self.env.vars.set(&var.raw, None);
            }
            return;
        }
        // Assignments: alone they set shell variables; before a command they only set its
        // environment.
        let k = w.iter().take_while(|x| x.assign > 0).count();
        if k == w.len() {
            if !meta.isolated {
                for a in &w {
                    self.assign(a);
                }
            }
            self.emit_redirects(&mut meta, false);
            return;
        }
        // PowerShell: `$x = 'value'`.
        if let [var, eq, value] = w.as_slice()
            && is_ps_var(&var.raw)
            && eq.raw == "="
        {
            let name = var.raw.get(1..).unwrap_or("").to_owned();
            let value = value.value().map(str::to_owned);
            self.env.vars.set(&name, value);
            return;
        }
        let Some(run) = self.wrappers(w, k) else {
            return;
        };
        self.command(run, meta);
    }

    /// The command after its wrappers.
    fn command(&mut self, mut run: Run, mut meta: Meta) {
        let w = &mut run.words;
        w.drain(..run.at.min(w.len()));
        if w.is_empty() {
            // `exec > file`.
            self.emit_redirects(&mut meta, run.git_config);
            return;
        }
        match &run.xargs {
            Some(Xargs::Replace(replace)) => {
                for x in w.iter_mut() {
                    if x.raw.contains(replace.as_str()) {
                        *x = Word::unknown(&x.raw);
                    }
                }
            }
            Some(Xargs::Input) => w.push(Word::unknown("(xargs input)")),
            None => {}
        }
        let Some(mut name) = w.first().and_then(name_of) else {
            self.emit(None, run, &mut meta);
            return;
        };
        // A name with a slash in it runs that file, never a builtin.
        if is_builtin(&name)
            && let Some(value) = w.first().and_then(Word::value).filter(|v| v.contains('/'))
        {
            name = value.to_lowercase();
        }
        let args = w.get(1..).unwrap_or(&[]);
        if CD.contains(&name.as_str()) {
            if !meta.isolated {
                self.cd(&name, args);
            }
            return;
        }
        if DECLARE.contains(&name.as_str()) {
            if !meta.isolated {
                for a in args {
                    self.assign(a);
                }
            }
            return;
        }
        let inner = if name == "find" {
            find_execs(args, run.cwd.as_ref(), self.depth)
        } else {
            inner_script(&name, args, &meta, &self.overflow)
        };
        // The inner commands run in the command's directory; `Dir` is a shared handle.
        let cwd = run.cwd.clone();
        self.emit(Some(name), run, &mut meta);
        if let Some(inner) = inner {
            self.deeper(|walker| walker.inner(inner, cwd, &meta));
        }
    }

    /// What a command runs inside it, in `cwd`, one level deeper.
    fn inner(&mut self, inner: Inner, cwd: Option<Dir>, meta: &Meta) {
        match inner {
            Inner::Sub { src, ps } => self.sub_in(&src, cwd, ps),
            Inner::Run(src) => self.run(&src),
            Inner::Awk(srcs) => {
                for src in srcs {
                    self.sub_in(&src, cwd.clone(), false); // a shared `Rc` handle
                }
            }
            Inner::Find(execs) => {
                for exec in execs {
                    let FindExec::Run { words, cwd } = exec else {
                        self.out.push(Command::depth_marker());
                        break;
                    };
                    let inner = Meta {
                        redirects: Vec::new(),
                        stdin: None,
                        piped: meta.piped,
                        isolated: meta.isolated,
                    };
                    self.simple_apart(words, inner, cwd);
                }
            }
        }
    }

    /// 1.x's `cd`, `pushd`, `popd` and friends.
    fn cd(&mut self, name: &str, args: &[Word]) {
        let before = self.env.cwd.clone();
        if name == "popd" {
            self.env.cwd = if self.env.dirs.len() > self.env.floor {
                self.env.dirs.pop().flatten()
            } else {
                None
            };
        } else {
            let a: Vec<&Word> = args
                .iter()
                .filter(|x| x.value().is_none() || !is_cd_option(&x.raw))
                .collect();
            if name == "pushd" {
                let cwd = self.env.cwd.clone();
                self.env.dirs.push(cwd);
            }
            self.env.cwd = match a.as_slice() {
                [] if name == "pushd" => None,
                [] => Some(Dir::given(self.opts.home)),
                [only] if only.value() == Some("-") => self.env.prev.clone(),
                [only] => self.capped(self.cd_into(only)),
                _ => None,
            };
        }
        self.env.prev = before;
    }

    /// The directory `cd <word>` enters: Node's `path.resolve(cwd, value)`, stepping a relative
    /// value's names from a directory already in normal form ([`word_dir`]).
    fn cd_into(&self, word: &Word) -> Option<Dir> {
        word_dir(self.opts.paths, word, self.env.cwd.as_ref())
    }
}

/// A command's words, and whether they were made as PowerShell's.
struct CommandWords {
    words: Vec<Word>,
    ps: bool,
}

/// What a command runs inside it.
pub(super) enum Inner {
    /// A script in a subshell: a shell's `-c` or stdin, `watch`, `pwsh -Command`.
    Sub { src: String, ps: bool },
    /// `eval` and `trap`, in this shell.
    Run(String),
    /// Each `system("...")` of an awk program.
    Awk(Vec<String>),
    /// Each `find -exec`, `-execdir`, `-ok` and `-okdir` command.
    Find(Vec<FindExec>),
}

/// One command `find` runs.
pub(super) enum FindExec {
    /// Its words (`{}` unknown) and its directory (`None` for the `dir` forms).
    Run { words: Vec<Word>, cwd: Option<Dir> },
    /// Past the depth temper follows.
    TooDeep,
}

/// 1.x's `find` branch of `simple`: the commands `-exec` and its kin run, each up to its `;`
/// or `+`, scanned forward once (1.x searches from the first argument for each one).
fn find_execs(args: &[Word], cwd: Option<&Dir>, depth: usize) -> Option<Inner> {
    let mut execs = Vec::new();
    let mut k = 0;
    while let Some(arg) = args.get(k) {
        if !["-exec", "-execdir", "-ok", "-okdir"].contains(&arg.raw.as_str()) {
            k = k.saturating_add(1);
            continue;
        }
        if depth >= MAX_DEPTH {
            execs.push(FindExec::TooDeep);
            break;
        }
        let from = k.saturating_add(1);
        let end = args
            .get(from..)
            .unwrap_or(&[])
            .iter()
            .position(|a| matches!(a.value(), Some(";" | "+")))
            .map(|j| from.saturating_add(j));
        let words: Vec<Word> = args
            .get(from..end.unwrap_or(args.len()))
            .unwrap_or(&[])
            .iter()
            .map(|a| {
                if a.raw.contains("{}") {
                    Word::unknown(&a.raw)
                } else {
                    a.clone()
                }
            })
            .collect();
        if !words.is_empty() {
            // `Dir` is a shared handle.
            let cwd = if arg.raw.ends_with("dir") {
                None
            } else {
                cwd.cloned()
            };
            execs.push(FindExec::Run { words, cwd });
        }
        // 1.x resumes at the word after the terminator.
        k = end.unwrap_or(args.len()).saturating_add(1);
    }
    (!execs.is_empty()).then_some(Inner::Find(execs))
}

/// `/[$`]|\\[\\$`\n]/`: a heredoc body that expands something.
fn expands(body: &str) -> bool {
    let bytes = body.as_bytes();
    bytes.iter().enumerate().any(|(i, &b)| {
        b == b'$'
            || b == b'`'
            || (b == b'\\'
                && bytes
                    .get(i.saturating_add(1))
                    .is_some_and(|n| b"\\$`\n".contains(n)))
    })
}

/// 1.x `CD`.
const CD: [&str; 6] = ["cd", "pushd", "popd", "chdir", "set-location", "sl"];
/// 1.x `DECLARE`.
const DECLARE: [&str; 5] = ["export", "declare", "local", "readonly", "typeset"];

/// 1.x `BUILTINS`: the builtins whose effect temper models or checks.
fn is_builtin(name: &str) -> bool {
    CD.contains(&name)
        || DECLARE.contains(&name)
        || ["eval", "source", ".", "alias", "hash"].contains(&name)
}

/// `/^(?:--|-[LPe@]+|-path|-literalpath)$/i`: an option of `cd` and `Set-Location`.
fn is_cd_option(raw: &str) -> bool {
    raw == "--"
        || raw.eq_ignore_ascii_case("-path")
        || raw.eq_ignore_ascii_case("-literalpath")
        || raw
            .strip_prefix('-')
            .is_some_and(|f| !f.is_empty() && f.bytes().all(|b| b"lpeLPE@".contains(&b)))
}

/// 1.x `PS_NAME`: a cmdlet (a common verb, `-`, a lower-case noun) or an alias the rules read.
fn is_ps_name(name: &str) -> bool {
    const VERBS: [&str; 39] = [
        "get",
        "set",
        "new",
        "remove",
        "copy",
        "move",
        "add",
        "clear",
        "out",
        "write",
        "read",
        "invoke",
        "start",
        "stop",
        "test",
        "import",
        "export",
        "select",
        "where",
        "foreach",
        "rename",
        "join",
        "split",
        "resolve",
        "convert",
        "convertto",
        "convertfrom",
        "format",
        "measure",
        "tee",
        "push",
        "pop",
        "expand",
        "compress",
        "update",
        "enter",
        "sort",
        "group",
        "compare",
    ];
    const ALIASES: [&str; 12] = [
        "cpi", "mi", "sc", "ac", "ni", "sl", "copy", "move", "ri", "del", "erase", "rd",
    ];
    if ALIASES.contains(&name) {
        return true;
    }
    name.split_once('-').is_some_and(|(verb, noun)| {
        !verb.is_empty()
            && VERBS.contains(&verb)
            && !noun.is_empty()
            && noun.bytes().all(|b| b.is_ascii_lowercase())
    })
}

/// 1.x `psCommand(words)` over words made without PowerShell's reading: a cmdlet name,
/// `$x = ...` or a `[Type]::Method(...)` expression.
fn ps_command(words: &[Word]) -> bool {
    let Some(k) = words.iter().position(|w| w.assign == 0) else {
        return false;
    };
    let Some(first) = words.get(k) else {
        return false;
    };
    let type_call = first.raw.strip_prefix('[').is_some_and(|rest| {
        let len = rest
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.')
            .count();
        len > 0 && rest.get(len..).is_some_and(|r| r.starts_with("]::"))
    });
    let assignment =
        is_ps_var(&first.raw) && words.get(k.saturating_add(1)).is_some_and(|w| w.raw == "=");
    if type_call || assignment {
        return true;
    }
    name_of(first).is_some_and(|name| is_ps_name(&name))
}
