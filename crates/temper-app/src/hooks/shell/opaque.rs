//! Why temper cannot rule out that a command applies a patch (1.x `shell.mjs` `opaquePatch`,
//! `unreadAwk`, `unreadScript`, `readStdin` and `configWritesAlias`), for the reviewer's
//! fail-closed check. Patterns, not a parse: read with [`super::ParseOpts::untracked`].

use temper_domain::{InterpreterCode, OpaqueKind, PathStyle};

use super::code::{Threw, code_words, is_interpreter, stdin_body, unread_code};
use super::git::{dashed_git, git_command, is_builtin};
use super::walk::{Command, Opaque, Stdin};
use super::words::{PathCx, Word};
use super::wrappers::{Script, awk_code, shell_script, trap_action, without_awk_systems};
use crate::paths;

/// 1.x `opaquePatch(c)`'s answer: the kind, why, and a hint of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpaquePatch {
    pub(crate) kind: OpaqueKind,
    pub(crate) why: String,
    pub(crate) hint: Option<&'static str>,
}

impl OpaquePatch {
    fn new(kind: OpaqueKind, why: impl Into<String>) -> Option<Self> {
        Some(Self {
            kind,
            why: why.into(),
            hint: None,
        })
    }
}

/// 1.x `AWKS`.
const AWKS: [&str; 4] = ["awk", "gawk", "mawk", "nawk"];
/// 1.x `SHELLS`.
const SHELLS: [&str; 5] = ["bash", "sh", "zsh", "dash", "ksh"];
/// 1.x `SOURCES`.
const SOURCES: [&str; 2] = ["source", "."];

/// 1.x `opaquePatch(c)`: why temper cannot rule out that `c` applies a patch, or `None`.
///
/// # Errors
///
/// [`Threw`] where 1.x's check throws on the command, and `readerRule` makes it `null`.
pub(crate) fn opaque_patch(
    c: &Command,
    cx: PathCx<'_>,
    code: &InterpreterCode,
) -> Result<Option<OpaquePatch>, Threw> {
    match c.opaque {
        Some(Opaque::Depth) => {
            return Ok(OpaquePatch::new(
                OpaqueKind::Script,
                "commands nested deeper than temper reads",
            ));
        }
        Some(Opaque::Split) => {
            return Ok(Some(OpaquePatch {
                kind: OpaqueKind::Script,
                why: "an `env -S` string, whose syntax is not the shell's".to_owned(),
                hint: Some("Run the command directly, without `env -S`, so temper can read it."),
            }));
        }
        Some(Opaque::Name) | None => {}
    }
    if let Some(dashed) = dashed_git(c).filter(|d| d != "apply" && d != "am") {
        return Ok(OpaquePatch::new(
            OpaqueKind::Git,
            format!("a dashed git command (git-{dashed})"),
        ));
    }
    let git = git_command(c, cx);
    let names_patch = names_git_or_patch(&c.argv0.raw)
        || c.args
            .iter()
            .any(|x| matches!(x.value(), Some("apply" | "am")));
    if git.is_none() && c.name.is_none() && names_patch {
        return Ok(OpaquePatch::new(
            OpaqueKind::Name,
            "an unresolved command name",
        ));
    }
    // A command name that is not a literal word, or an eval argument built from an
    // expansion, runs text temper cannot read exactly (wi-0e226b).
    if c.opaque == Some(Opaque::Name) {
        return Ok(OpaquePatch::new(
            OpaqueKind::Name,
            "a command name or wrapper word built from an expansion",
        ));
    }
    if !c.named() {
        return Ok(OpaquePatch::new(
            OpaqueKind::Name,
            "a command name built from an expansion",
        ));
    }
    let name = c.name.as_deref().unwrap_or("");
    if let Some(found) = named_opaque(c, name) {
        return Ok(Some(found));
    }
    if let Some(git) = git {
        if git.dynamic || (!git.sub.is_empty() && !is_builtin(&git.sub)) {
            return Ok(OpaquePatch::new(
                OpaqueKind::Git,
                "a git subcommand temper cannot resolve",
            ));
        }
        if git.sub == "config" && config_writes_alias(&git.args) {
            return Ok(OpaquePatch::new(
                OpaqueKind::Alias,
                "a git config write to an alias",
            ));
        }
        return Ok(None);
    }
    if SHELLS.contains(&name) || SOURCES.contains(&name) {
        return Ok(unread_script(c, name).and_then(|why| {
            OpaquePatch::new(
                OpaqueKind::Script,
                format!("a {name} script {why} that temper cannot read"),
            )
        }));
    }
    if !is_interpreter(name) {
        return Ok(None);
    }
    interpreter_opaque(c, name, code)
}

/// The checks of names that run other commands: aliases, `hash -p`, `eval`, `trap` and awk.
fn named_opaque(c: &Command, name: &str) -> Option<OpaquePatch> {
    // An alias or `hash -p` makes a name run another command.
    let alias = name == "alias" && c.args.iter().any(|x| x.raw.contains('='));
    let hash = name == "hash" && c.args.iter().any(|x| hash_p(&x.raw));
    if alias || hash {
        let what = if alias { "an alias" } else { "a hash -p" };
        return OpaquePatch::new(
            OpaqueKind::Name,
            format!("{what}, which makes a command name run another command"),
        );
    }
    if name == "eval" && c.args.iter().any(|x| !x.is_literal()) {
        return OpaquePatch::new(
            OpaqueKind::Script,
            "an eval argument built from an expansion",
        );
    }
    if name == "trap" && trap_action(&c.args).is_some_and(|a| !a.is_literal()) {
        return OpaquePatch::new(OpaqueKind::Script, "a trap action built from an expansion");
    }
    if AWKS.contains(&name)
        && let Some(why) = unread_awk(&c.args)
    {
        return OpaquePatch::new(OpaqueKind::Script, format!("an awk program that {why}"));
    }
    None
}

/// The interpreter's half of [`opaque_patch`]: code from an expansion, code that runs a
/// patch, and a script from a process substitution or from stdin temper cannot read.
fn interpreter_opaque(
    c: &Command,
    name: &str,
    code: &InterpreterCode,
) -> Result<Option<OpaquePatch>, Threw> {
    if unread_code(c)?.is_some() {
        return Ok(OpaquePatch::new(
            OpaqueKind::Code,
            format!("{name} code built from an expansion"),
        ));
    }
    let mut texts = stdin_body(c)
        .into_iter()
        .chain(code_words(c, true).into_iter().map(Word::text));
    if texts.any(|text| code.runs_patch(text)) {
        return Ok(OpaquePatch::new(
            OpaqueKind::Code,
            format!("{name} code that runs git apply, git am or patch"),
        ));
    }
    // A script from a process substitution, or on stdin that temper cannot read: `node <(...)`,
    // `cat x.py | python3`, `python3 - < x.py`, `python3 <<< "$X"`, `python3 /dev/stdin < x.py`.
    if c.args.iter().any(Word::has_process) {
        return Ok(OpaquePatch::new(
            OpaqueKind::Code,
            format!("a {name} script from a process substitution"),
        ));
    }
    let stdin_arg = c.args.iter().any(|x| {
        x.value()
            .is_some_and(|v| stdin_path(&paths::posix_normalize(v)))
    });
    let script_arg =
        c.args.iter().any(|x| !x.raw.starts_with('-')) || !code_words(c, false).is_empty();
    if c.stdin.as_ref().is_some_and(|s| !read_stdin(s)) && (!script_arg || stdin_arg) {
        return Ok(OpaquePatch::new(
            OpaqueKind::Code,
            format!("a {name} script on stdin that temper cannot read"),
        ));
    }
    Ok(None)
}

/// 1.x `/\b(?:git|g?patch)\b/`.
fn names_git_or_patch(raw: &str) -> bool {
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let bytes = raw.as_bytes();
    ["git", "gpatch", "patch"].iter().any(|name| {
        raw.match_indices(name).any(|(at, _)| {
            let end = at.saturating_add(name.len());
            let before = at
                .checked_sub(1)
                .and_then(|b| bytes.get(b))
                .is_some_and(|&b| word(b));
            let after = bytes.get(end).is_some_and(|&b| word(b));
            !before && !after
        })
    })
}

/// 1.x `/^-\w*p/`.
fn hash_p(raw: &str) -> bool {
    raw.strip_prefix('-').is_some_and(|rest| {
        rest.bytes()
            .take_while(|&b| b.is_ascii_alphanumeric() || b == b'_')
            .any(|b| b == b'p')
    })
}

/// 1.x `CONFIG_VERBS`.
const CONFIG_VERBS: [&str; 4] = ["set", "unset", "rename-section", "remove-section"];
/// 1.x `CONFIG_WRITE_FLAGS`.
const CONFIG_WRITE_FLAGS: [&str; 6] = [
    "--add",
    "--unset",
    "--unset-all",
    "--replace-all",
    "--rename-section",
    "--remove-section",
];

/// 1.x `configWritesAlias(args)`: `git config <args>` sets, unsets or renames an `alias.*`
/// key (`git config alias.x` alone reads it).
fn config_writes_alias(args: &[Word]) -> bool {
    let mut pos: Vec<&Word> = args.iter().filter(|x| !x.raw.starts_with('-')).collect();
    let verb = pos
        .first()
        .is_some_and(|first| CONFIG_VERBS.contains(&first.raw.as_str()));
    if verb {
        pos.remove(0);
    }
    let alias = |x: &&Word| {
        x.value()
            .unwrap_or(&x.raw)
            .get(..6)
            .is_some_and(|head| head.eq_ignore_ascii_case("alias."))
    };
    let Some(key) = pos.iter().position(alias) else {
        return false;
    };
    verb || pos.len() > key.saturating_add(1)
        || args.iter().any(|x| {
            let flag = x.raw.split('=').next().unwrap_or("");
            CONFIG_WRITE_FLAGS.contains(&flag)
        })
}

/// 1.x `STDIN_PATH`: a path that names the shell's own stdin.
fn stdin_path(p: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if matches!(p, "/dev/stdin" | "/dev/stdout" | "/dev/stderr") {
        return true;
    }
    if let Some(fd) = p.strip_prefix("/dev/fd/") {
        return digits(fd);
    }
    p.strip_prefix("/proc/")
        .and_then(|rest| rest.split_once('/'))
        .and_then(|(pid, rest)| (!pid.is_empty()).then_some(rest))
        .and_then(|rest| rest.strip_prefix("fd/"))
        .is_some_and(digits)
}

/// 1.x `readStdin(s)`: whether temper read a command's stdin exactly: a literal heredoc or
/// here-string body, or one a plain `cat` passed on.
fn read_stdin(s: &Stdin) -> bool {
    match s {
        Stdin::Pipe { body, exact } => *exact && body.is_some(),
        Stdin::Heredoc { literal, .. } | Stdin::Str { literal, .. } => *literal,
        Stdin::File(_) => false,
    }
}

/// 1.x `unreadScript(c)`: where a shell, or `source` and `.`, reads a script temper cannot
/// read exactly (#151, wi-0e226b), or `None`.
fn unread_script(c: &Command, name: &str) -> Option<String> {
    let src = if SOURCES.contains(&name) {
        let k = usize::from(c.args.first().and_then(Word::value) == Some("--"));
        c.args.get(k).map_or(Script::Stdin, Script::File)
    } else {
        shell_script(&c.args)
    };
    let xargs = c.with.xargs;
    match src {
        Script::None => None,
        Script::Arg(Some(word)) if word.is_literal() => None,
        Script::Arg(word) if xargs && word.is_none_or(|w| w.value().is_none()) => {
            Some("from xargs input".to_owned())
        }
        Script::Arg(Some(_)) => Some("built from an expansion".to_owned()),
        Script::Arg(None) => Some("after -c".to_owned()),
        Script::File(word) => unread_file(word, xargs),
        Script::Stdin => (!c.stdin.as_ref().is_some_and(read_stdin)).then(|| "on stdin".to_owned()),
    }
}

/// [`unread_script`] for a script operand.
fn unread_file(w: &Word, xargs: bool) -> Option<String> {
    if w.raw.contains("<(") || w.raw.contains(">(") {
        return Some("from a process substitution".to_owned());
    }
    let Some(value) = w.value() else {
        let why = if xargs {
            "from xargs input"
        } else {
            "built from an expansion"
        };
        return Some(why.to_owned());
    };
    // A glob or brace list is an expansion too (`bash /dev/std*`).
    if w.glob() || (!w.is_literal() && !w.expanded()) {
        return Some("built from an expansion".to_owned());
    }
    // An operand built from an expansion whose last segment names a stream may be stdin
    // (`"$HOME/stdin"` after `read HOME`).
    let base = paths::basename(PathStyle::Posix, value);
    let stream = matches!(base, "stdin" | "stdout" | "stderr")
        || (!base.is_empty() && base.bytes().all(|b| b.is_ascii_digit()));
    if !w.is_literal() && stream {
        return Some("on what may be stdin".to_owned());
    }
    stdin_path(&paths::posix_normalize(value)).then(|| format!("from {value}"))
}

/// 1.x `unreadAwk(args)`: why an awk command runs something temper cannot read: a program
/// from stdin or a non-literal word, `system()` of a non-literal, or a pipe to or from a
/// command (#155).
fn unread_awk(args: &[Word]) -> Option<&'static str> {
    let awk = awk_code(args);
    for f in &awk.files {
        let stream = f.text == "-"
            || f.text.starts_with("/dev/stdin")
            || f.text.starts_with("/dev/fd/")
            || f.text.starts_with("/proc/");
        if !f.literal || stream {
            return Some("reads its program from stdin or a word temper cannot read");
        }
    }
    for p in &awk.progs {
        if !p.literal {
            return Some("is built from an expansion");
        }
        // A string literal is text: `FS = "|"` pipes nothing. An unclosed one runs to the
        // end, so a run of `"\` does not rescan from every quote.
        let code = blank_awk_strings(&without_awk_systems(&p.text));
        if calls_system(&code) {
            return Some("runs system() on text temper cannot read");
        }
        if lone_pipe(&code) {
            return Some("pipes to or from a command");
        }
    }
    None
}

/// 1.x `code.replace(/"(?:[^"\\]|\\[\s\S])*(?:"|\\?$)/g, '""')`: each string literal emptied;
/// one that never closes runs to the end.
fn blank_awk_strings(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut chars = code.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            out.push(c);
            continue;
        }
        out.push_str("\"\"");
        loop {
            match chars.next() {
                Some('"') | None => break,
                Some('\\') => {
                    chars.next();
                }
                Some(_) => {}
            }
        }
    }
    out
}

/// 1.x `/\bsystem\s*\(/`.
fn calls_system(code: &str) -> bool {
    let bytes = code.as_bytes();
    code.match_indices("system").any(|(at, _)| {
        let before = at
            .checked_sub(1)
            .and_then(|b| bytes.get(b))
            .is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_');
        let rest = code.get(at.saturating_add(6)..).unwrap_or("");
        !before
            && rest
                .trim_start_matches(crate::project::is_js_space)
                .starts_with('(')
    })
}

/// 1.x `/(?<!\|)\|(?!\|)/`: a `|` with no `|` on either side.
fn lone_pipe(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.iter().enumerate().any(|(at, &b)| {
        b == b'|'
            && at.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'|')
            && bytes.get(at.saturating_add(1)) != Some(&b'|')
    })
}
