//! The code an interpreter runs (1.x `shell.mjs` `INTERPRETERS`, `CODE_OPTION`, `codeWords`,
//! `interpreterCode` and `unreadCode`). The patterns that read the code are the domain's
//! [`temper_domain::InterpreterCode`].

use super::walk::{Command, Stdin};
use super::words::Word;

/// 1.x `INTERPRETERS`: `python[\d.]*`, `py`, `node`, `nodejs`, `deno`, `bun`, `ruby`, `perl`,
/// `php`, `pwsh` and `powershell`.
pub(super) fn is_interpreter(name: &str) -> bool {
    let python = name
        .strip_prefix("python")
        .is_some_and(|v| v.bytes().all(|b| b.is_ascii_digit() || b == b'.'));
    python
        || matches!(
            name,
            "py" | "node"
                | "nodejs"
                | "deno"
                | "bun"
                | "ruby"
                | "perl"
                | "php"
                | "pwsh"
                | "powershell"
        )
}

/// 1.x `/^(?:python[\d.]*|py)$/`.
fn is_python(name: &str) -> bool {
    name == "py"
        || name
            .strip_prefix("python")
            .is_some_and(|v| v.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
}

/// 1.x `CODE_OPTION(r)`: an option whose next word is code in any interpreter's arguments
/// (`-c`, `-e`, `-E`, `-p`, `-r`, `--eval`, `--print`, `-Command`).
fn code_option(r: &str) -> bool {
    let cluster = r.strip_prefix('-').is_some_and(|rest| {
        rest.ends_with(['c', 'e', 'E', 'p']) && rest.bytes().all(|b| b.is_ascii_alphabetic())
    });
    cluster
        || ["-r", "--eval", "--print", "-command"]
            .iter()
            .any(|o| o.eq_ignore_ascii_case(r))
}

/// 1.x `codeWords(c, every)`: the word after each code option. Without `every`, a word an
/// option took as its code is not read as an option itself; with it, it is
/// (`perl -p -e '...'`), for the reviewer's check, which may read too much but not too little.
pub(super) fn code_words(c: &Command, every: bool) -> Vec<&Word> {
    let a = &c.args;
    let mut code = Vec::new();
    let mut j = 0;
    while let Some(word) = a.get(j) {
        if code_option(&word.raw)
            && let Some(next) = a.get(j.saturating_add(1))
        {
            code.push(next);
            if !every {
                j = j.saturating_add(1);
            }
        }
        j = j.saturating_add(1);
    }
    code
}

/// 1.x `c.stdin?.body`, when it is not empty.
pub(super) fn stdin_body(c: &Command) -> Option<&str> {
    c.stdin
        .as_ref()
        .and_then(Stdin::body)
        .filter(|b| !b.is_empty())
}

/// 1.x `interpreterCode(c)`: the code an interpreter runs from stdin and its code options.
pub(super) fn interpreter_code(c: &Command) -> Vec<&str> {
    if !c.name.as_deref().is_some_and(is_interpreter) {
        return Vec::new();
    }
    stdin_body(c)
        .into_iter()
        .chain(code_words(c, false).into_iter().map(|w| w.text.as_str()))
        .collect()
}

/// 1.x `unreadCode(c)`: the code word right after a code option that is not literal (#151,
/// wi-9792d6): python's `-c` (alone or ending a cluster of flags, before its script or module;
/// `-W` and `-X` take a value), and node's, bun's, ruby's, perl's and php's `-e`, `-p`, `-r`,
/// `--eval` and `--print`.
///
/// # Errors
///
/// [`Threw`] where 1.x throws: a python option with a line terminator in it, which its
/// `/^-[bB...]*([cmWX]?)(.*)$/` does not match.
pub(super) fn unread_code(c: &Command) -> Result<Option<&Word>, Threw> {
    match c.name.as_deref() {
        Some(name) if is_python(name) => python_unread(&c.args),
        Some("node" | "nodejs" | "bun" | "ruby" | "perl" | "php") => Ok(script_unread(&c.args)),
        _ => Ok(None),
    }
}

/// [`unread_code`] for python: its flags, then the option that ends the cluster.
fn python_unread(args: &[Word]) -> Result<Option<&Word>, Threw> {
    let mut at = 0;
    while let Some(word) = args.get(at) {
        let raw = word.raw.as_str();
        if raw == "--" || raw == "-" || !raw.starts_with('-') {
            return Ok(None);
        }
        if raw.starts_with("--") {
            if raw == "--check-hash-based-pycs" {
                at = at.saturating_add(1);
            }
            at = at.saturating_add(1);
            continue;
        }
        if raw.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
            return Err(Threw);
        }
        let flags = raw
            .get(1..)
            .unwrap_or("")
            .trim_start_matches(|letter: char| "bBdEhiIOPqRsSuvVx".contains(letter));
        let (opt, rest) = match flags.chars().next() {
            Some(letter @ ('c' | 'm' | 'W' | 'X')) => (Some(letter), flags.get(1..).unwrap_or("")),
            _ => (None, flags),
        };
        let next = args.get(at.saturating_add(1));
        match opt {
            Some('c') => return Ok(next.filter(|code| rest.is_empty() && !code.is_literal())),
            Some('m') => return Ok(None),
            // `-W` and `-X` take the next word.
            Some(_) if rest.is_empty() => at = at.saturating_add(1),
            _ => {}
        }
        at = at.saturating_add(1);
    }
    Ok(None)
}

/// [`unread_code`] for node, bun, ruby, perl and php: each code option's next word, even one
/// an earlier option took (`perl -p -e "$X"`).
fn script_unread(args: &[Word]) -> Option<&Word> {
    args.windows(2)
        .find(|pair| match pair {
            [option, word] => {
                let flag = option.raw.as_str();
                let short = flag.strip_prefix('-').is_some_and(|rest| {
                    rest.ends_with(['e', 'p']) && rest.bytes().all(|b| b.is_ascii_alphabetic())
                });
                !word.is_literal() && (short || matches!(flag, "-r" | "--eval" | "--print"))
            }
            _ => false,
        })
        .and_then(|pair| pair.get(1))
}

/// Where 1.x's reader throws on a command, and the rule that called it falls back
/// (`readerRule`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Threw;
