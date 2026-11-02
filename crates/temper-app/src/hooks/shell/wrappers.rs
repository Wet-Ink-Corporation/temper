//! Wrappers and the commands that run commands (1.x `shell.mjs`: the wrapper loop of
//! `simple`, `envOption`, `shellScript`, `trapAction`, `awkPrograms`, `AWK_SYSTEM`,
//! `awkString`, and the `bash -c`, `eval`, `trap`, awk and `pwsh -Command` branches).

use std::borrow::Cow;

use super::walk::{Inner, Meta, Walker};
use super::words::{Dir, Word, name_of, resolve_path};

/// What the wrappers leave: the index of the command word, its directory, and xargs.
pub(super) struct Run {
    pub(super) at: usize,
    pub(super) cwd: Option<Dir>,
    /// Run under xargs: `Some(Some(r))` with `-I r`, `Some(None)` reading its input.
    pub(super) xargs: Option<Option<String>>,
    pub(super) git_config: bool,
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
    pub(super) fn wrappers(
        &mut self,
        w: &[Word],
        k: usize,
        mut git_config: bool,
        depth: usize,
    ) -> Option<Run> {
        let mut cwd = self.env.cwd.clone();
        let mut i = k;
        let mut xargs: Option<Option<String>> = None;
        while let Some(word) = w.get(i) {
            let Some(name) = name_of(word) else {
                break;
            };
            let Some(valued) = wrapper_values(&name) else {
                break;
            };
            i = i.saturating_add(1);
            let next = |i: usize| w.get(i);
            if name == "command"
                && next(i).is_some_and(|x| {
                    x.raw
                        .strip_prefix('-')
                        .is_some_and(|f| !f.is_empty() && f.bytes().all(|b| b == b'v' || b == b'V'))
                })
            {
                // A lookup, not a run.
                return None;
            }
            if name == "xargs" {
                xargs = Some(None);
            }
            let env = name == "env";
            let mut watch_exec = false;
            while let Some(x) = next(i) {
                let option = x.raw.starts_with('-') && (x.raw != "-" || env);
                if !(option || (env && x.assign > 0)) {
                    break;
                }
                let a = x.raw.as_str();
                i = i.saturating_add(1);
                if env && x.assign > 0 {
                    git_config |= x.assign().is_some_and(|n| n.starts_with("GIT_CONFIG"));
                    continue;
                }
                if env {
                    let Some(o) = env_option(a) else {
                        continue;
                    };
                    if o.opt == 'S' {
                        // env -S splits its string into the command and runs it; env reads
                        // `\_`, `\t`, `\n`, `\v`, `\f` and `\r` in it as whitespace.
                        let v = match o.value {
                            Some(v) => v,
                            None => {
                                let v = next(i).map_or("", |x| x.text.as_str());
                                i = i.saturating_add(1);
                                v
                            }
                        };
                        let mut script = env_split_spaces(v);
                        for x in w.get(i..).unwrap_or(&[]) {
                            script.push(' ');
                            script.push_str(&x.text);
                        }
                        self.sub_in(&script, cwd, false, depth.saturating_add(1));
                        return None;
                    }
                    let v: Option<Cow<'_, str>> = match o.value {
                        Some(v) => Some(Cow::Borrowed(v)),
                        None => {
                            let v = next(i).and_then(Word::value).map(Cow::Borrowed);
                            i = i.saturating_add(1);
                            v
                        }
                    };
                    if o.opt == 'C' {
                        cwd = resolve_path(self.opts.paths, v.as_deref(), cwd.as_deref())
                            .map(Dir::from);
                    }
                    continue;
                }
                if name == "xargs"
                    && let Some(rest) = xargs_replace(a)
                {
                    let replace = if !rest.is_empty() {
                        rest.to_owned()
                    } else if a == "-I" || a == "--replace" {
                        let replace = next(i).map_or("{}", |x| x.raw.as_str()).to_owned();
                        i = i.saturating_add(1);
                        replace
                    } else {
                        "{}".to_owned()
                    };
                    xargs = Some(Some(replace));
                    continue;
                }
                if name == "sudo" && (a == "-C" || a == "--chdir") {
                    let value = next(i).and_then(Word::value);
                    cwd = resolve_path(self.opts.paths, value, cwd.as_deref()).map(Dir::from);
                    i = i.saturating_add(1);
                    continue;
                }
                if name == "watch" {
                    if a == "--exec" || cluster_reaches(a, "x") {
                        watch_exec = true;
                    }
                    // -n 2, -dn 2
                    let ends_nq = a.strip_prefix('-').is_some_and(|f| {
                        f.ends_with(['n', 'q']) && f.bytes().all(|b| b.is_ascii_alphabetic())
                    });
                    if ends_nq {
                        i = i.saturating_add(1);
                        continue;
                    }
                }
                if valued.contains(&a) {
                    i = i.saturating_add(1);
                }
            }
            if name == "watch" && !watch_exec {
                if let Some(rest) = w.get(i..).filter(|r| !r.is_empty()) {
                    let script = join_texts(rest);
                    self.sub_in(&script, cwd, false, depth.saturating_add(1));
                }
                return None;
            }
            // timeout reads no option after its duration.
            if name == "timeout" && i < w.len() {
                i = i.saturating_add(1);
            }
            if name == "nice"
                && next(i).is_some_and(|x| {
                    x.raw
                        .strip_prefix('-')
                        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
                })
            {
                i = i.saturating_add(1);
            }
        }
        // A `GIT_CONFIG*` assignment is a cheap flag; `xargs` holds at most its replace string.
        Some(Run {
            at: i,
            cwd,
            xargs,
            git_config,
        })
    }

    /// What `name` runs inside it, from its arguments and stdin.
    pub(super) fn inner_script(&self, name: &str, args: &[Word], meta: &Meta) -> Option<Inner> {
        match name {
            "bash" | "sh" | "zsh" | "dash" | "ksh" => {
                // The script is walked after the command is pushed, which takes its words.
                let script = match shell_script(args) {
                    Script::Arg(word) => word.map(|w| w.text.clone()),
                    Script::Stdin => meta
                        .stdin
                        .as_ref()
                        .and_then(|s| s.body())
                        .map(str::to_owned),
                    Script::File | Script::None => None,
                };
                script
                    .filter(|s| !s.is_empty())
                    .map(|src| Inner::Sub { src, ps: false })
            }
            "eval" => {
                let args = match args.first() {
                    Some(first) if first.value() == Some("--") => args.get(1..).unwrap_or(&[]),
                    _ => args,
                };
                Some(Inner::Run(join_texts(args)))
            }
            "trap" => trap_action(args)
                .filter(|a| !a.text.is_empty())
                .map(|a| Inner::Run(a.text.clone())),
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
                Some(Inner::Sub {
                    src: join_texts(rest),
                    ps: true,
                })
            }
            _ => None,
        }
    }
}

/// The words' texts, joined by spaces.
fn join_texts(words: &[Word]) -> String {
    let mut out = String::new();
    for (k, w) in words.iter().enumerate() {
        if k > 0 {
            out.push(' ');
        }
        out.push_str(&w.text);
    }
    out
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
    File,
    /// `-s` or no operand.
    Stdin,
    /// `--version` and `--help`.
    None,
}

/// 1.x `shellScript(args)`. `--` or `-` ends the options, and `-o` or `-O`, alone or at the
/// end of a cluster, takes the next word.
pub(super) fn shell_script(args: &[Word]) -> Script<'_> {
    let (mut stdin, mut cmd) = (false, false);
    let mut i = 0;
    while let Some(a) = args.get(i) {
        match a.value() {
            Some("--version" | "--help") => return Script::None,
            Some("--" | "-") => return operand(args.get(i.saturating_add(1)), cmd, stdin),
            Some(v) if a.raw.starts_with(['-', '+']) => {
                if ["-o", "+o", "-O", "+O", "--rcfile", "--init-file"].contains(&v) {
                    i = i.saturating_add(2);
                    continue;
                }
                if !v.starts_with("--") {
                    cmd |= cluster_reaches(v, "c");
                    stdin |= cluster_reaches(v, "s");
                    let values = v.get(1..).unwrap_or("").matches(['o', 'O']).count();
                    i = i.saturating_add(values);
                }
            }
            _ => return operand(Some(a), cmd, stdin),
        }
        i = i.saturating_add(1);
    }
    if cmd {
        Script::Arg(None)
    } else {
        Script::Stdin
    }
}

/// 1.x `operand(w)` of `shellScript`.
fn operand(w: Option<&Word>, cmd: bool, stdin: bool) -> Script<'_> {
    match w {
        _ if cmd => Script::Arg(w),
        Some(_) if !stdin => Script::File,
        _ => Script::Stdin,
    }
}

/// 1.x `trapAction(args)`: the action word of `trap action signal...`, or `None` for
/// `trap -p`, `trap -l`, `trap - SIG` and a lone argument.
fn trap_action(args: &[Word]) -> Option<&Word> {
    let a = match args.first() {
        Some(first) if first.value() == Some("--") => args.get(1..).unwrap_or(&[]),
        _ => args,
    };
    let first = a.first()?;
    let listing = first
        .raw
        .strip_prefix('-')
        .is_some_and(|f| f.starts_with(['l', 'p']));
    (a.len() >= 2 && first.value() != Some("-") && !listing).then_some(first)
}

/// An awk program: its text, and whether its word is literal.
struct AwkProgram<'w> {
    literal: bool,
    text: Cow<'w, str>,
}

/// 1.x `awkPrograms(args).progs`: each `-e`/`--source` value, else the first operand when no
/// `-f` or `-E` names a program file.
fn awk_programs(args: &[Word]) -> Vec<AwkProgram<'_>> {
    let mut progs = Vec::new();
    let mut main = false;
    let mut k = 0;
    fn prog<'w>(w: &'w Word, text: Cow<'w, str>) -> AwkProgram<'w> {
        AwkProgram {
            literal: w.literal,
            text,
        }
    }
    while let Some(a) = args.get(k) {
        // awk sees the word's value: `'-f/dev/stdin'` is still `-f`.
        let r = if a.literal {
            a.text.as_str()
        } else {
            a.raw.as_str()
        };
        if r == "--" {
            k = k.saturating_add(1);
            break;
        }
        if let Some((key, value)) = r.strip_prefix("--").and_then(|l| l.split_once('='))
            && [
                "source",
                "file",
                "exec",
                "include",
                "load",
                "field-separator",
                "assign",
            ]
            .contains(&key)
        {
            match key {
                "source" => progs.push(prog(a, Cow::Borrowed(value))),
                "file" | "exec" => main = true,
                _ => {}
            }
            k = k.saturating_add(1);
            if key == "exec" {
                break;
            }
            continue;
        }
        let long = r.strip_prefix("--").is_some_and(|l| {
            [
                "source",
                "file",
                "exec",
                "include",
                "load",
                "field-separator",
                "assign",
            ]
            .contains(&l)
        });
        let short = r.len() == 2 && r.strip_prefix('-').is_some_and(|f| "evfFEil".contains(f));
        if long || short {
            k = k.saturating_add(1);
            if let Some(v) = args.get(k)
                && (r == "-e" || r == "--source")
            {
                progs.push(prog(v, Cow::Borrowed(v.text.as_str())));
            }
            main |= ["-f", "-E", "--file", "--exec"].contains(&r);
            k = k.saturating_add(1);
            if r == "-E" || r == "--exec" {
                break;
            }
            continue;
        }
        let mut chars = r.chars();
        if let (Some('-'), Some(letter)) = (chars.next(), chars.next())
            && "evfFEil".contains(letter)
            && !chars.as_str().is_empty()
        {
            main |= letter == 'f' || letter == 'E';
            if letter == 'e' {
                progs.push(prog(a, Cow::Borrowed(chars.as_str())));
            }
            k = k.saturating_add(1);
            if letter == 'E' {
                break;
            }
            continue;
        }
        if r.starts_with('-') && r != "-" {
            k = k.saturating_add(1);
            continue;
        }
        break;
    }
    if progs.is_empty()
        && !main
        && let Some(a) = args.get(k)
    {
        progs.push(prog(a, Cow::Borrowed(a.text.as_str())));
    }
    progs
}

/// 1.x `AWK_SYSTEM`, `/\bsystem\s*\(\s*"((?:[^"\\]|\\.)*)"\s*\)/g`: the string of each
/// `system("...")` call with a literal alone as its argument. A string's scan ends at its
/// first unescaped quote, so the scans from successive calls do not overlap.
fn awk_systems(text: &str) -> Vec<String> {
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
        out.push(body.get(..close).unwrap_or("").to_owned());
        from = text.len().saturating_sub(tail.len());
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
