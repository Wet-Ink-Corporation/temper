//! Wrappers and the commands that run commands (1.x `shell.mjs`: the wrapper loop of
//! `simple`, `envOption`, `shellScript`, `trapAction`, `awkPrograms`, `AWK_SYSTEM`,
//! `awkString`, and the `bash -c`, `eval`, `trap`, awk and `pwsh -Command` branches).

use std::borrow::Cow;
use std::cell::Cell;
use std::ops::Range;

use super::walk::{Command, Inner, Meta, Opaque, Walker};
use super::words::{Dir, Word, js_len, name_of, push_js, resolve_path};

/// What the wrappers leave: the words, the index of the command word in them, its directory,
/// and xargs.
pub(super) struct Run {
    pub(super) words: Vec<Word>,
    pub(super) at: usize,
    pub(super) cwd: Option<Dir>,
    /// Run under xargs.
    pub(super) xargs: Option<Xargs>,
    /// A `GIT_CONFIG*` variable is set for the command.
    pub(super) git_config: bool,
}

/// How xargs hands its input to the command.
pub(super) enum Xargs {
    /// After its words.
    Input,
    /// `-I r`: in place of `r`.
    Replace(String),
}

/// A chain of wrapper words, and the indices of the wrappers' own command words in it.
#[derive(Clone, Copy)]
struct Chain<'a> {
    words: &'a [Word],
    cmds: &'a [usize],
}

impl<'a> Chain<'a> {
    fn new(words: &'a [Word], cmds: &'a [usize]) -> Self {
        Self { words, cmds }
    }
}

/// A wrapper: its name, and its options that take a separate value.
#[derive(Clone, Copy)]
struct Wrapper<'n> {
    name: &'n str,
    valued: &'static [&'static str],
}

/// 1.x `WRAPPERS`: the wrappers that run the command after them, with their options that
/// take a separate value.
fn wrapper_values(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "sudo" => &[
            "-u", "-g", "-C", "-D", "-h", "-p", "-r", "-t", "-U", "-T", "--user", "--group",
            "--chdir", "--prompt",
        ],
        "doas" => &["-u", "-C"],
        "exec" => &["-a"],
        "time" => &["-f", "-o", "--format", "--output"],
        "nice" => &["-n", "--adjustment"],
        "ionice" => &["-c", "-n", "-p"],
        "stdbuf" => &["-i", "-o", "-e"],
        "timeout" => &["-s", "-k", "--signal", "--kill-after"],
        "xargs" => &[
            "-a",
            "-d",
            "-E",
            "-I",
            "-L",
            "-n",
            "-P",
            "-s",
            "--arg-file",
            "--delimiter",
            "--max-args",
            "--max-procs",
            "--replace",
        ],
        // Without -x, watch runs its words with sh -c.
        "watch" => &["-n", "--interval", "-q", "--equexit"],
        "env" | "command" | "builtin" | "nohup" | "chronic" | "unbuffer" | "setsid" | "busybox" => {
            &[]
        }
        _ => return None,
    })
}

/// An env option that takes a value: its short letter, and the value when it is attached.
struct EnvOption<'a> {
    opt: char,
    value: Option<&'a str>,
}

/// 1.x `ENV_LONG`: env's long options, by the short option of those that take a value.
const ENV_LONG: [(&str, Option<char>); 13] = [
    ("ignore-environment", None),
    ("null", None),
    ("unset", Some('u')),
    ("chdir", Some('C')),
    ("split-string", Some('S')),
    ("block-signal", None),
    ("default-signal", None),
    ("ignore-signal", None),
    ("list-signal-handling", None),
    ("debug", None),
    ("argv0", Some('a')),
    ("help", None),
    ("version", None),
];

/// 1.x `envOption(a)`: getopt takes any unambiguous prefix of a long option; in a short
/// cluster, the first letter that takes a value takes the rest.
fn env_option(a: &str) -> Option<EnvOption<'_>> {
    if let Some(long) = a.strip_prefix("--") {
        let (key, value) = match long.split_once('=') {
            Some((key, value)) => (key, Some(value)),
            None => (long, None),
        };
        let mut hits = ENV_LONG.iter().filter(|(name, _)| name.starts_with(key));
        let exact = ENV_LONG.iter().find(|(name, _)| *name == key);
        let full = match (exact, hits.next(), hits.next()) {
            (Some(exact), _, _) => exact,
            (None, Some(only), None) => only,
            _ => return None,
        };
        return full.1.map(|opt| EnvOption { opt, value });
    }
    let rest = a.get(1..).unwrap_or("");
    rest.char_indices()
        .find(|(_, c)| "uCSa".contains(*c))
        .map(|(at, opt)| {
            let after = rest.get(at.saturating_add(1)..).unwrap_or("");
            EnvOption {
                opt,
                value: (!after.is_empty()).then_some(after),
            }
        })
}

/// 1.x `/^(?:-I|--replace=?|-i)(.*)$/` on an xargs option: what follows the option, when the
/// option is one that sets the replace string. JS `.` stops at line ends, so a rest with one
/// in it does not match.
fn xargs_replace(a: &str) -> Option<&str> {
    let rest = a
        .strip_prefix("-I")
        .or_else(|| {
            a.strip_prefix("--replace")
                .map(|r| r.strip_prefix('=').unwrap_or(r))
        })
        .or_else(|| a.strip_prefix("-i"))?;
    (!rest.contains(['\n', '\r', '\u{2028}', '\u{2029}'])).then_some(rest)
}

/// `^-[a-zA-Z]*<c>`: a short-option cluster that reaches one of `letters` before any other
/// char than a letter.
pub(super) fn cluster_reaches(a: &str, letters: &str) -> bool {
    a.strip_prefix('-').is_some_and(|rest| {
        rest.chars()
            .take_while(char::is_ascii_alphabetic)
            .any(|c| letters.contains(c))
    })
}

impl Walker<'_> {
    /// 1.x's wrapper loop: `sudo`, `env`, `timeout 5`, `xargs`, ... before the command,
    /// walked with an index so a long chain stays linear. `None` when the wrappers end the
    /// command: `command -v`, `watch` and `env -S` (whose script is walked here).
    pub(super) fn wrappers(&mut self, words: Vec<Word>, k: usize) -> Option<Run> {
        let git_config = words
            .iter()
            .take(k)
            .any(|a| a.assign().is_some_and(|n| n.starts_with("GIT_CONFIG")));
        let mut run = Run {
            words: Vec::new(),
            at: k,
            cwd: self.env.cwd.clone(), // a shared `Rc` handle
            xargs: None,
            git_config,
        };
        // The wrappers' own command words, which are read only when literal.
        let mut cmds: Vec<usize> = Vec::new();
        while let Some(word) = words.get(run.at) {
            let Some(name) = name_of(word) else {
                break;
            };
            let Some(valued) = wrapper_values(&name) else {
                break;
            };
            cmds.push(run.at);
            run.at = run.at.saturating_add(1);
            if name == "command" && is_lookup(words.get(run.at)) {
                // A lookup, not a run.
                return None;
            }
            if name == "xargs" {
                run.xargs = Some(Xargs::Input);
            }
            let wrapper = Wrapper {
                name: &name,
                valued,
            };
            let watch_exec = self.options(wrapper, &words, &mut run)?;
            if name == "watch" && !watch_exec {
                self.unread(Chain::new(&words, &cmds), k..run.at, None);
                if let Some(rest) = words.get(run.at..).filter(|r| !r.is_empty())
                    && let Some(script) = join_texts(rest, &self.overflow)
                {
                    let cwd = run.cwd.take();
                    self.deeper(|walker| walker.script_in(&script, cwd, false));
                }
                return None;
            }
            // timeout reads no option after its duration.
            if (name == "timeout" && run.at < words.len())
                || (name == "nice" && is_niceness(words.get(run.at)))
            {
                run.at = run.at.saturating_add(1);
            }
        }
        self.unread(Chain::new(&words, &cmds), k..run.at, Some(run.at));
        // A `GIT_CONFIG*` assignment is a cheap flag; `xargs` holds at most its replace string.
        run.words = words;
        Some(run)
    }

    /// 1.x `unread(from, to, name)`: under `untracked`, the first wrapper word in `span` that
    /// temper does not read exactly (a command word that is not literal, or an option or
    /// assignment that is neither literal nor [`Word::fixed`]), else the command name at
    /// `name` when it is not literal, is reported as an [`Opaque::Name`] marker. The command
    /// is still read after it.
    fn unread(&mut self, chain: Chain<'_>, span: Range<usize>, name: Option<usize>) {
        if !self.opts.untracked {
            return;
        }
        let Chain { words, cmds } = chain;
        let Range { start, end } = span;
        let wrapper = words.get(start..end).and_then(|ws| {
            ws.iter().zip(start..).find_map(|(y, j)| {
                // `cmds` is in order, so a long chain is searched, not scanned.
                (!y.is_literal() && (cmds.binary_search(&j).is_ok() || !y.fixed())).then_some(y)
            })
        });
        let named = name
            .and_then(|at| words.get(at))
            .filter(|w| !w.is_literal());
        if let Some(x) = wrapper.or(named) {
            // The marker holds its own copy: the command after it still reads the word.
            self.out.push(Command::marker(Opaque::Name, x.clone()));
        }
    }

    /// The options after `wrapper`: whether they include watch's `--exec`, or `None` when
    /// they end the command (`env -S`, whose script is walked here).
    fn options(&mut self, wrapper: Wrapper<'_>, words: &[Word], run: &mut Run) -> Option<bool> {
        let (name, env) = (wrapper.name, wrapper.name == "env");
        let mut watch_exec = false;
        while let Some(word) = words.get(run.at) {
            let option = word.raw.starts_with('-') && (word.raw != "-" || env);
            if !(option || (env && word.assign > 0)) {
                break;
            }
            let opt = word.raw.as_str();
            run.at = run.at.saturating_add(1);
            if env {
                self.env_flag(word, words, run)?;
                continue;
            }
            if name == "xargs"
                && let Some(rest) = xargs_replace(opt)
            {
                run.xargs = Some(Xargs::Replace(replace_string(
                    opt,
                    rest,
                    words,
                    &mut run.at,
                )));
                continue;
            }
            if name == "sudo" && (opt == "-C" || opt == "--chdir") {
                let value = words.get(run.at).and_then(Word::value);
                run.cwd = self.option_dir(value, run.cwd.as_ref());
                run.at = run.at.saturating_add(1);
                continue;
            }
            if name == "watch" {
                watch_exec |= opt == "--exec" || cluster_reaches(opt, "x");
                // -n 2, -dn 2
                let ends_nq = opt.strip_prefix('-').is_some_and(|f| {
                    f.ends_with(['n', 'q']) && f.bytes().all(|b| b.is_ascii_alphabetic())
                });
                if ends_nq {
                    run.at = run.at.saturating_add(1);
                    continue;
                }
            }
            if wrapper.valued.contains(&opt) {
                run.at = run.at.saturating_add(1);
            }
        }
        Some(watch_exec)
    }

    /// One word of env's options, `word`: an assignment, `-C dir`, or `-S string`, which
    /// ends the command (`None`).
    fn env_flag(&mut self, word: &Word, words: &[Word], run: &mut Run) -> Option<()> {
        if word.assign > 0 {
            run.git_config |= word.assign().is_some_and(|n| n.starts_with("GIT_CONFIG"));
            return Some(());
        }
        let Some(flag) = env_option(&word.raw) else {
            return Some(());
        };
        // Each of env's valued options takes the next word when nothing is attached.
        if flag.opt == 'S' && self.opts.untracked {
            // Its quoting and escapes are not the shell's, so the fail-closed parse does
            // not read it.
            self.out
                .push(Command::marker(Opaque::Split, Word::unknown("")));
            return None;
        }
        if flag.opt == 'S' {
            // env -S splits its string into the command and runs it; env reads `\_`, `\t`,
            // `\n`, `\v`, `\f` and `\r` in it as whitespace.
            // Each word is read once, charged and not kept.
            let text = if let Some(value) = flag.value {
                Cow::Borrowed(value)
            } else {
                run.at = run.at.saturating_add(1);
                match words.get(run.at.saturating_sub(1)) {
                    Some(next) => next.text_read()?,
                    None => Cow::Borrowed(""),
                }
            };
            let mut script = env_split_spaces(&text);
            let mut units = js_len(&script);
            for rest in words.get(run.at..).unwrap_or(&[]) {
                let rest = rest.text_read()?;
                let fits = push_js(&mut script, &mut units, " ", &self.overflow)
                    && push_js(&mut script, &mut units, &rest, &self.overflow);
                if !fits {
                    return None;
                }
            }
            let cwd = run.cwd.take();
            self.deeper(|walker| walker.script_in(&script, cwd, false));
            return None;
        }
        let value = flag.value.or_else(|| {
            run.at = run.at.saturating_add(1);
            words.get(run.at.saturating_sub(1)).and_then(Word::value)
        });
        if flag.opt == 'C' {
            run.cwd = self.option_dir(value, run.cwd.as_ref());
        }
        Some(())
    }

    /// The directory a wrapper's `-C value` runs its command in, from `cwd`.
    fn option_dir(&self, value: Option<&str>, cwd: Option<&Dir>) -> Option<Dir> {
        let paths = self.opts.paths;
        let base = paths.dir_text(cwd);
        self.capped(resolve_path(paths, value, base.as_deref()).map(Dir::resolved))
    }
}

/// `command -v` and `-V`: a lookup.
fn is_lookup(word: Option<&Word>) -> bool {
    word.is_some_and(|x| {
        x.raw
            .strip_prefix('-')
            .is_some_and(|f| !f.is_empty() && f.bytes().all(|b| b == b'v' || b == b'V'))
    })
}

/// `nice -5`: a niceness.
fn is_niceness(word: Option<&Word>) -> bool {
    word.is_some_and(|x| {
        x.raw
            .strip_prefix('-')
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// xargs's replace string for the option `opt`, with `rest` attached to it: the next word
/// after a bare `-I` or `--replace` (at `at`, which moves past it), else `{}`.
fn replace_string(opt: &str, rest: &str, words: &[Word], at: &mut usize) -> String {
    if !rest.is_empty() {
        rest.to_owned()
    } else if opt == "-I" || opt == "--replace" {
        let replace = words.get(*at).map_or("{}", |x| x.raw.as_str()).to_owned();
        *at = at.saturating_add(1);
        replace
    } else {
        "{}".to_owned()
    }
}

/// What `name` runs inside it, from its arguments and stdin; a script joined past V8's
/// longest string marks `overflow`.
pub(super) fn inner_script(
    name: &str,
    args: &[Word],
    meta: &Meta,
    overflow: &Cell<bool>,
) -> Option<Inner> {
    match name {
        "bash" | "sh" | "zsh" | "dash" | "ksh" => {
            // The script is walked after the command is pushed, which takes its words.
            let script = match shell_script(args) {
                Script::Arg(word) => word.and_then(Word::text_read).map(Cow::into_owned),
                Script::Stdin => meta
                    .stdin
                    .as_ref()
                    .and_then(|s| s.body())
                    .map(str::to_owned),
                Script::File(_) | Script::None => None,
            };
            script
                .filter(|s| !s.is_empty())
                .map(|src| Inner::Sub { src, ps: false })
        }
        "eval" => {
            let args = match args.first() {
                Some(first) if first.value_is("--") => args.get(1..).unwrap_or(&[]),
                _ => args,
            };
            join_texts(args, overflow).map(Inner::Run)
        }
        "trap" => trap_action(args)
            .and_then(Word::text_read) // read once, charged and not kept
            .filter(|action| !action.is_empty())
            .map(|action| Inner::Run(action.into_owned())),
        "awk" | "gawk" | "mawk" | "nawk" => {
            let systems: Vec<String> = awk_programs(args)
                .into_iter()
                .filter(|p| p.literal)
                .flat_map(|p| awk_systems(&p.text))
                .map(|s| awk_string(&s))
                .collect();
            (!systems.is_empty()).then_some(Inner::Awk(systems))
        }
        "pwsh" | "powershell" => {
            let flag = args.iter().position(|a| {
                a.raw.eq_ignore_ascii_case("-c") || a.raw.eq_ignore_ascii_case("-command")
            })?;
            let rest = args
                .get(flag.saturating_add(1)..)
                .filter(|r| !r.is_empty())?;
            join_texts(rest, overflow).map(|src| Inner::Sub { src, ps: true })
        }
        _ => None,
    }
}

/// The words' texts, joined by spaces, or `None` (marking `overflow`) when that is longer
/// than a JS string can be, where 1.x's `join(' ')` throws.
fn join_texts(words: &[Word], overflow: &Cell<bool>) -> Option<String> {
    let mut out = String::new();
    let mut units = 0;
    for (k, w) in words.iter().enumerate() {
        let sep = if k > 0 { " " } else { "" };
        // Read once, charged and not kept; `None` past the budget.
        let text = w.text_read()?;
        let fits = push_js(&mut out, &mut units, sep, overflow)
            && push_js(&mut out, &mut units, &text, overflow);
        if !fits {
            return None;
        }
    }
    Some(out)
}

/// 1.x `v.replace(/\\[_tnvfr]/g, ' ')`.
fn env_split_spaces(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some_and(|n| "_tnvfr".contains(*n)) {
            chars.next();
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Where a shell reads its script (1.x `shellScript(args)`).
pub(super) enum Script<'w> {
    /// `-c`: the first operand after the options, if any.
    Arg(Option<&'w Word>),
    /// A script operand, which temper does not read.
    File(&'w Word),
    /// `-s` or no operand.
    Stdin,
    /// `--version` and `--help`.
    None,
}

/// 1.x `shellScript(args)`. `--` or `-` ends the options, and `-o` or `-O`, alone or at the
/// end of a cluster, takes the next word.
pub(super) fn shell_script(args: &[Word]) -> Script<'_> {
    let mut flags = ShellFlags::default();
    let mut i = 0;
    while let Some(a) = args.get(i) {
        // Compared without joining a value that cannot match, and read whole only for an
        // option, so the script operand keeps no joined copy.
        if a.value_is("--version") || a.value_is("--help") {
            return Script::None;
        }
        if a.value_is("--") || a.value_is("-") {
            return flags.operand(args.get(i.saturating_add(1)));
        }
        let option = if a.raw.starts_with(['-', '+']) {
            a.value()
        } else {
            None
        };
        match option {
            Some(v) => {
                if ["-o", "+o", "-O", "+O", "--rcfile", "--init-file"].contains(&v) {
                    i = i.saturating_add(2);
                    continue;
                }
                if !v.starts_with("--") {
                    flags.cmd |= cluster_reaches(v, "c");
                    flags.stdin |= cluster_reaches(v, "s");
                    let values = v.get(1..).unwrap_or("").matches(['o', 'O']).count();
                    i = i.saturating_add(values);
                }
            }
            _ => return flags.operand(Some(a)),
        }
        i = i.saturating_add(1);
    }
    if flags.cmd {
        Script::Arg(None)
    } else {
        Script::Stdin
    }
}

/// The `-c` and `-s` a shell's options set.
#[derive(Debug, Default, Clone, Copy)]
struct ShellFlags {
    cmd: bool,
    stdin: bool,
}

impl ShellFlags {
    /// 1.x `operand(w)` of `shellScript`.
    fn operand(self, w: Option<&Word>) -> Script<'_> {
        match w {
            _ if self.cmd => Script::Arg(w),
            Some(word) if !self.stdin => Script::File(word),
            _ => Script::Stdin,
        }
    }
}

/// 1.x `trapAction(args)`: the action word of `trap action signal...`, or `None` for
/// `trap -p`, `trap -l`, `trap - SIG` and a lone argument.
pub(super) fn trap_action(args: &[Word]) -> Option<&Word> {
    let a = match args.first() {
        Some(first) if first.value_is("--") => args.get(1..).unwrap_or(&[]),
        _ => args,
    };
    let first = a.first()?;
    let listing = first
        .raw
        .strip_prefix('-')
        .is_some_and(|f| f.starts_with(['l', 'p']));
    (a.len() >= 2 && !first.value_is("-") && !listing).then_some(first)
}

/// An awk program: its text, and whether its word is literal.
pub(super) struct AwkProgram<'w> {
    pub(super) literal: bool,
    pub(super) text: Cow<'w, str>,
}

/// 1.x `awkPrograms(args).progs`: each `-e`/`--source` value, else the first operand when no
/// `-f` or `-E` names a program file.
fn awk_programs(args: &[Word]) -> Vec<AwkProgram<'_>> {
    awk_code(args).progs
}

/// 1.x `awkPrograms(args)`: the programs, and the files awk reads code from: each `-f`, `-E`,
/// `-i` and `-l` value, a word or the text of `-fFILE` or `--file=FILE`.
pub(super) fn awk_code(args: &[Word]) -> AwkArgs<'_> {
    let mut found = AwkArgs {
        progs: Vec::new(),
        files: Vec::new(),
        main: false,
    };
    let mut k = 0;
    let operand = loop {
        match found.option(args, k) {
            AwkStep::Next(next) => k = next,
            AwkStep::Stop(at) => break at,
        }
    };
    if found.progs.is_empty()
        && !found.main
        && let Some(a) = args.get(operand)
    {
        found.progs.push(AwkProgram::of(a, a.text()));
    }
    found
}

/// awk's long options that take a value.
const AWK_LONG: [&str; 7] = [
    "source",
    "file",
    "exec",
    "include",
    "load",
    "field-separator",
    "assign",
];

impl<'w> AwkProgram<'w> {
    fn of(word: &'w Word, text: &'w str) -> Self {
        Self {
            literal: word.is_literal(),
            text: Cow::Borrowed(text),
        }
    }
}

/// What awk's options name: its `-e` programs, the files it reads code from, and whether a
/// program file stands in for the operand.
pub(super) struct AwkArgs<'w> {
    pub(super) progs: Vec<AwkProgram<'w>>,
    pub(super) files: Vec<AwkProgram<'w>>,
    main: bool,
}

/// Where awk's option scan goes on.
enum AwkStep {
    /// The next option is at this index.
    Next(usize),
    /// The options end, and the program operand is at this index.
    Stop(usize),
}

impl<'w> AwkArgs<'w> {
    /// The option at `k`, if it is one.
    fn option(&mut self, args: &'w [Word], k: usize) -> AwkStep {
        let Some(a) = args.get(k) else {
            return AwkStep::Stop(k);
        };
        // awk sees the word's value: `'-f/dev/stdin'` is still `-f`.
        let r = if a.is_literal() {
            a.text()
        } else {
            a.raw.as_str()
        };
        let next = k.saturating_add(1);
        if r == "--" {
            return AwkStep::Stop(next);
        }
        if let Some(step) = self.long_assign(a, r, next) {
            return step;
        }
        let long = r.strip_prefix("--").is_some_and(|l| AWK_LONG.contains(&l));
        let short = r.len() == 2 && r.strip_prefix('-').is_some_and(|f| "evfFEil".contains(f));
        if long || short {
            self.valued(args.get(next), r);
            return stop_if(r == "-E" || r == "--exec", next.saturating_add(1));
        }
        let mut chars = r.chars();
        if let (Some('-'), Some(letter)) = (chars.next(), chars.next())
            && "evfFEil".contains(letter)
            && !chars.as_str().is_empty()
        {
            if "fEil".contains(letter) {
                self.files.push(AwkProgram::of(a, chars.as_str()));
            }
            self.main |= letter == 'f' || letter == 'E';
            if letter == 'e' {
                self.progs.push(AwkProgram::of(a, chars.as_str()));
            }
            return stop_if(letter == 'E', next);
        }
        if r.starts_with('-') && r != "-" {
            AwkStep::Next(next)
        } else {
            AwkStep::Stop(k)
        }
    }

    /// A long option with its value after `=`, `--file=x`, in word `a` read as `r`.
    fn long_assign(&mut self, a: &'w Word, r: &'w str, next: usize) -> Option<AwkStep> {
        let (key, value) = r.strip_prefix("--").and_then(|l| l.split_once('='))?;
        if !AWK_LONG.contains(&key) {
            return None;
        }
        match key {
            "source" => self.progs.push(AwkProgram::of(a, value)),
            "field-separator" | "assign" => {}
            _ => {
                self.files.push(AwkProgram::of(a, value));
                self.main |= key == "file" || key == "exec";
            }
        }
        Some(stop_if(key == "exec", next))
    }

    /// Option `r`, whose value is the next word, `value`.
    fn valued(&mut self, value: Option<&'w Word>, r: &str) {
        if let Some(v) = value
            && (r == "-e" || r == "--source")
        {
            self.progs.push(AwkProgram::of(v, v.text()));
        }
        let file = [
            "-f",
            "-E",
            "-i",
            "-l",
            "--file",
            "--exec",
            "--include",
            "--load",
        ];
        if file.contains(&r) {
            self.files.push(value.map_or(
                AwkProgram {
                    literal: true,
                    text: Cow::Borrowed(""),
                },
                |v| AwkProgram::of(v, v.text()),
            ));
        }
        self.main |= ["-f", "-E", "--file", "--exec"].contains(&r);
    }
}

/// Stop at `at` when `stop`, else read the option there.
fn stop_if(stop: bool, at: usize) -> AwkStep {
    if stop {
        AwkStep::Stop(at)
    } else {
        AwkStep::Next(at)
    }
}

/// 1.x `AWK_SYSTEM`, `/\bsystem\s*\(\s*"((?:[^"\\]|\\.)*)"\s*\)/g`: the string of each
/// `system("...")` call with a literal alone as its argument. A string's scan ends at its
/// first unescaped quote, so the scans from successive calls do not overlap.
fn awk_systems(text: &str) -> Vec<String> {
    awk_system_calls(text)
        .into_iter()
        .map(|call| text.get(call.body).unwrap_or("").to_owned())
        .collect()
}

/// 1.x `text.replace(AWK_SYSTEM, '')`.
pub(super) fn without_awk_systems(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for call in awk_system_calls(text) {
        out.push_str(text.get(at..call.whole.start).unwrap_or(""));
        at = call.whole.end;
    }
    out.push_str(text.get(at..).unwrap_or(""));
    out
}

/// One match of 1.x `AWK_SYSTEM`: the whole call, and its string's text.
struct SystemCall {
    whole: Range<usize>,
    body: Range<usize>,
}

/// The matches of 1.x `AWK_SYSTEM`, in order (see [`awk_systems`]).
fn awk_system_calls(text: &str) -> Vec<SystemCall> {
    use crate::project::is_js_space;
    let line_end = |c: char| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}');
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = text.get(from..).and_then(|r| r.find("system")) {
        let at = from.saturating_add(found);
        from = at.saturating_add(1);
        let boundary = text
            .get(..at)
            .and_then(|b| b.bytes().next_back())
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || b == b'_'));
        if !boundary {
            continue;
        }
        let rest = text.get(at.saturating_add(6)..).unwrap_or("");
        let Some(body) = rest
            .trim_start_matches(is_js_space)
            .strip_prefix('(')
            .map(|r| r.trim_start_matches(is_js_space))
            .and_then(|r| r.strip_prefix('"'))
        else {
            continue;
        };
        let mut chars = body.char_indices();
        let close = loop {
            match chars.next() {
                Some((i, '"')) => break Some(i),
                Some((_, '\\')) => match chars.next() {
                    Some((_, c)) if !line_end(c) => {}
                    _ => break None,
                },
                Some(_) => {}
                None => break None,
            }
        };
        let Some(close) = close else {
            continue;
        };
        let after = body.get(close.saturating_add(1)..).unwrap_or("");
        let Some(tail) = after.trim_start_matches(is_js_space).strip_prefix(')') else {
            continue;
        };
        let body_at = text.len().saturating_sub(body.len());
        from = text.len().saturating_sub(tail.len());
        out.push(SystemCall {
            whole: at..from,
            body: body_at..body_at.saturating_add(close),
        });
    }
    out
}

/// 1.x `awkString(s)`: an awk string literal's text, its escapes (octal `\ddd` among them)
/// decoded.
fn awk_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(&e) = chars.peek() else {
            out.push(c);
            continue;
        };
        if e.is_digit(8) {
            let mut code = 0_u32;
            for _ in 0..3 {
                match chars.peek().and_then(|d| d.to_digit(8)) {
                    Some(d) => {
                        code = code.saturating_mul(8).saturating_add(d);
                        chars.next();
                    }
                    None => break,
                }
            }
            out.extend(char::from_u32(code));
            continue;
        }
        chars.next();
        out.push(match e {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'a' => '\x07',
            'b' => '\x08',
            'f' => '\x0c',
            'v' => '\x0b',
            other => other,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{awk_string, awk_systems, env_option, xargs_replace};

    #[test]
    fn helpers_read_as_1x_does() {
        assert_eq!(
            env_option("--sp=x").map(|o| (o.opt, o.value)),
            Some(('S', Some("x")))
        );
        assert_eq!(env_option("--s").map(|o| o.opt), Some('S'));
        assert_eq!(env_option("--d").map(|o| o.opt), None);
        assert_eq!(env_option("--").map(|o| o.opt), None);
        assert_eq!(
            env_option("-iS'x'").map(|o| (o.opt, o.value)),
            Some(('S', Some("'x'")))
        );
        assert_eq!(env_option("--null").map(|o| o.opt), None);
        assert_eq!(xargs_replace("--replace=x"), Some("x"));
        assert_eq!(xargs_replace("-Ix\ny"), None);
        assert_eq!(
            awk_systems(r#"BEGIN{system("cp x k1"); system ( "a\"b" ) }"#),
            ["cp x k1", r#"a\"b"#]
        );
        assert_eq!(awk_systems(r#"mysystem("x")"#), Vec::<String>::new());
        assert_eq!(awk_string(r"cp\040x\ k1\n"), "cp x k1\n");
    }
}
