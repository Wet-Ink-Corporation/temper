//! Interpreter code read as text (1.x `shell.mjs` `codeWriteTargets` and `codePatches`): the
//! files python, node, ruby, perl or PowerShell code writes, and whether it runs a patch.
//!
//! 1.x's patterns hold three look-arounds that `regex` has no syntax for, each emulated here:
//! - `ARG`'s `(?<![\w.])` before a variable. Only patterns 4 and 5 start with `ARG`; in the
//!   others a `(`, a `,` or a space always comes before it. A match that starts with a bare
//!   variable after a word char or a dot is a match 1.x does not make there: its search goes on
//!   one char later. Every start inside the same run of word chars and dots fails the same way,
//!   so the search resumes after the run, and the starts inside it are tried with a second
//!   pattern that lacks the bare-variable branch ([`Pattern::inner`]).
//! - `ARG`'s `(?=(?<sp>\s*))\k<sp>`, whitespace taken whole: plain `\s*` matches the same text,
//!   since what follows it never starts with whitespace.
//! - `MODE`'s `\b(?!\s*[(.\[])` after a variable. When it fails, 1.x backtracks: past a
//!   `mode=` its only other way is to read `mode` itself as the variable; without one the
//!   match fails at that start ([`Pattern::judge`]).
//!
//! `vars`' `(?!=)` after `=` is implied by what must follow it, and so is dropped.
//!
//! `codePatches`' patterns only answer yes or no, so a look-ahead at a pattern's end is
//! consumed instead, except the list form's `[^\]\n]{0,160}?`, which counts UTF-16 units and
//! is read by hand ([`patch_file_follows`]).

use std::borrow::Cow;
use std::collections::HashMap;

use regex::{Captures, Regex};

use crate::hooks::setup::{PolicySetupError, compile};
use crate::js::{JsFlags, is_js_space};

/// 1.x `LIT_PREFIX`.
const LIT_PREFIX: &str = "[rbfuRBFU]{0,2}";
/// 1.x `PATH_WRAP`.
const PATH_WRAP: &str = r"(?:(?:pathlib\s*\.\s*)?Path\s*\(\s*)?";

/// How a pattern's mode decides whether its match writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeTest {
    /// No mode: every match writes.
    None,
    /// 1.x `/[wax+]/`.
    Write,
    /// 1.x `/^\s*(?:\+?>>?|\+<)/`: perl's three-argument open.
    PerlOpen,
}

/// One of 1.x's `CODE_WRITES` patterns, compiled.
#[derive(Debug)]
struct Pattern {
    /// The pattern without its look-arounds.
    full: Regex,
    /// For a pattern that starts with `ARG`, the same pattern without `ARG`'s bare-variable
    /// branch: what 1.x can match where that branch's look-behind fails.
    inner: Option<Regex>,
    mode: ModeTest,
}

/// What one match names: the argument and the mode, as indexes into the literals or as
/// variable names.
#[derive(Debug, Clone, Copy)]
struct Found<'t> {
    arg: Ref<'t>,
    mode: Option<Ref<'t>>,
    end: usize,
}

/// A literal (`__L<n>__`) or a variable.
#[derive(Debug, Clone, Copy)]
enum Ref<'t> {
    Lit(&'t str),
    Var(&'t str),
}

/// Why 1.x makes no match at a start the look-around-free pattern matches at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Miss {
    /// `ARG`'s look-behind failed: so does every start up to the end of this run of `[\w.]`.
    Behind,
    /// `MODE`'s look-ahead failed with no `mode=` to fall back on.
    Ahead,
}

/// The literals of `code` and the code with each replaced by `__L<n>__` (1.x `stringLiterals`).
#[derive(Debug)]
struct Literals<'c> {
    text: String,
    lits: Vec<&'c str>,
}

impl<'c> Literals<'c> {
    /// 1.x `stringLiterals(code, lits)`: one pass; a quote that never closes stays as it is,
    /// and so does every later quote of its kind.
    fn of(code: &'c str) -> Self {
        let bytes = code.as_bytes();
        let mut out = String::with_capacity(code.len());
        let mut lits = Vec::new();
        let mut open = [true; 3];
        let mut i = 0;
        while let Some(&q) = bytes.get(i) {
            let kind = match q {
                b'"' => Some(0),
                b'\'' => Some(1),
                b'`' => Some(2),
                _ => None,
            };
            let Some(kind) = kind.filter(|&k| open.get(k).copied().unwrap_or(false)) else {
                // Plain text runs to the next quote of any kind.
                let from = i.saturating_add(1);
                let end = code
                    .get(from..)
                    .and_then(|rest| rest.find(['"', '\'', '`']))
                    .map_or(code.len(), |k| from.saturating_add(k));
                out.push_str(code.get(i..end).unwrap_or(""));
                i = end;
                continue;
            };
            let mut j = i.saturating_add(1);
            while let Some(&b) = bytes.get(j).filter(|&&b| b != q) {
                j = j.saturating_add(if b == b'\\' { 2 } else { 1 });
            }
            if j >= bytes.len() {
                if let Some(slot) = open.get_mut(kind) {
                    *slot = false;
                }
                out.push(char::from(q));
                i = i.saturating_add(1);
                continue;
            }
            out.push_str("__L");
            out.push_str(&lits.len().to_string());
            out.push_str("__");
            lits.push(code.get(i.saturating_add(1)..j).unwrap_or(""));
            i = j.saturating_add(1);
        }
        Self { text: out, lits }
    }

    /// 1.x `lits[Number(n)]`.
    fn get(&self, n: &str) -> Option<&'c str> {
        n.parse::<usize>()
            .ok()
            .and_then(|n| self.lits.get(n).copied())
    }
}

/// Interpreter code's compiled patterns.
///
/// # Examples
///
/// ```
/// use temper_domain::InterpreterCode;
///
/// let code = InterpreterCode::new()?;
/// let targets = code.write_targets("open('w.txt', 'w'); open('r.txt').read()");
/// assert_eq!(targets, ["w.txt"]);
/// # Ok::<(), temper_domain::PolicySetupError>(())
/// ```
#[derive(Debug)]
pub struct InterpreterCode {
    /// 1.x's assignment of a literal to a variable.
    vars: Regex,
    writes: Vec<Pattern>,
    /// Perl's two-argument open: `open(FH, '>file')`.
    perl_two: Regex,
    patches: PatchPatterns,
}

/// 1.x `codePatches`' patterns. Each is a yes-or-no test, so a look-ahead at a pattern's end
/// is consumed instead, and `$` under `m` consumes the line terminator it stands before.
#[derive(Debug)]
struct PatchPatterns {
    /// 1.x `decodeEscapes`' pattern.
    escapes: Regex,
    /// 1.x `GIT_PATCH_CODE`.
    git: Regex,
    /// 1.x `PATCH_CMD_CODE` without its list-form branch.
    command: Regex,
    /// 1.x `PATCH_CMD_CODE`'s list-form branch up to its look-ahead, which
    /// [`patch_file_follows`] reads, since its `{0,160}` counts UTF-16 units.
    listed: Regex,
    /// 1.x `PATCH_END_CODE`.
    end: Regex,
    /// 1.x `PATCH_CALL_CODE`.
    call: Regex,
}

/// 1.x `GIT_OPTS`: git's options before the subcommand, the ones that take a value first.
const GIT_OPTS: &str = r"(?:(?:-C|-c|--git-dir|--work-tree|--namespace|--config-env)\s+\S+\s+|--?[\w-]+(?:=\S*)?\s+){0,8}";
/// 1.x `PATCH_WRAP`: the wrappers that run the patch after them.
const PATCH_WRAP: &str =
    r"(?:(?:busybox|exec|command|nohup|timeout(?:\s+-\S+)*(?:\s+\d[\w.]*){1,2})\s+)";
/// 1.x `PATCH_CMD_CODE`'s start: a string, `;`, `|`, `&`, a directory or a wrapper.
const PATCH_START: &str = r#"(?:^|['"`(\[;|&/]\s*|\b"#;

impl PatchPatterns {
    fn new() -> Result<Self, PolicySetupError> {
        let re = |source: &str| compile(RULE, source, JsFlags::Plain);
        let lines = |source: &str| compile(RULE, source, JsFlags::Multiline);
        let start = format!("{PATCH_START}{PATCH_WRAP})g?patch");
        Ok(Self {
            escapes: re(
                r"\\(?:x([0-9a-fA-F]{2})|u\{([0-9a-fA-F]{1,6})\}|u([0-9a-fA-F]{4})|([0-7]{1,3})|[tnrfv])",
            )?,
            git: re(&format!(
                r"\bgit\s+(?:args\s*[:=]\s*)?{GIT_OPTS}(?:apply|am)(?:[^\w./-]|$)|\bgit-(?:apply|am)\b"
            ))?,
            command: lines(&format!(
                r#"{start}(?:[<|>]|\s+[-<\w./~$"']|['"],\s*['"]-)"#
            ))?,
            listed: lines(&format!(r#"{start}['"],"#))?,
            end: re(&format!(
                r#"[;|&]\s*{PATCH_WRAP}{{0,3}}(?:[\w.~-]*/)*g?patch\s*(?:['"`;&|)\n\r\u2028\u2029]|$)"#
            ))?,
            call: re(&format!(
                r#"\b(?:system|popen|run|call|check_call|check_output|Popen|exec\w*|spawn\w*)\s*\(\s*(?:\[\s*)?['"`]\s*{PATCH_WRAP}{{0,3}}(?:[\w.~-]*/)*g?patch\s*['"`]"#
            ))?,
        })
    }

    /// 1.x `decodeEscapes(code)`: `\x20`, `\040`, `\u0020` and `\u{20}` decoded, and `\t`,
    /// `\n`, `\r`, `\f`, `\v` as spaces. A surrogate code point, which JS keeps as a lone
    /// surrogate, becomes U+FFFD: neither is a space, a word char or in any class these
    /// patterns name, and both are one UTF-16 unit.
    fn decode<'c>(&self, code: &'c str) -> Cow<'c, str> {
        self.escapes.replace_all(code, |caps: &Captures<'_>| {
            let whole = caps.get(0).map_or("", |m| m.as_str());
            let hex = (1..=3).find_map(|group| caps.get(group));
            let (digits, radix) = match (hex, caps.get(4)) {
                (Some(hex), _) => (hex.as_str(), 16),
                (None, Some(octal)) => (octal.as_str(), 8),
                (None, None) => return " ".to_owned(),
            };
            match u32::from_str_radix(digits, radix) {
                Ok(n) if n <= 0x10_FFFF => char::from_u32(n).unwrap_or('\u{FFFD}').to_string(),
                _ => whole.to_owned(),
            }
        })
    }

    /// 1.x `codePatches(code)`.
    fn test(&self, code: &str) -> bool {
        let text = self.decode(code);
        let blanked: String = text
            .chars()
            .map(|c| if "'\"`,[]{}()+".contains(c) { ' ' } else { c })
            .collect();
        self.git.is_match(&blanked)
            || self.command.is_match(&text)
            || self
                .listed
                .find_iter(&text)
                .any(|m| patch_file_follows(text.get(m.end()..).unwrap_or("")))
            || self.end.is_match(&text)
            || self.call.is_match(&text)
    }
}

/// 1.x `(?=,[^\]\n]{0,160}?\.(?:diff|patch)['"])` after its `,`: within 160 UTF-16 units
/// that hold no `]` or newline, a `.diff` or `.patch` name ends in a quote.
fn patch_file_follows(rest: &str) -> bool {
    let named = |at: &str| {
        [".diff'", ".diff\"", ".patch'", ".patch\""]
            .iter()
            .any(|end| at.starts_with(end))
    };
    let mut units = 0_usize;
    for (at, c) in rest.char_indices() {
        if named(rest.get(at..).unwrap_or("")) {
            return true;
        }
        units = units.saturating_add(c.len_utf16());
        if units > 160 || c == ']' || c == '\n' {
            return false;
        }
    }
    false
}

/// The name these patterns fail under, if one does not compile.
const RULE: &str = "code-write-targets";

impl InterpreterCode {
    /// Compiles the patterns.
    ///
    /// # Errors
    ///
    /// [`PolicySetupError`] when a built-in pattern does not compile, a bug a unit test guards.
    pub fn new() -> Result<Self, PolicySetupError> {
        let re = |source: &str| compile(RULE, source, JsFlags::Plain);
        let lit = format!(r"{LIT_PREFIX}__L(?<a>\d+)__");
        let var = r"\$?(?<v>[A-Za-z_]\w*)";
        let arg = format!(r"{PATH_WRAP}(?:{lit}|{var})\s*\)?");
        // ARG without its bare variable: a wrapped literal or variable, or a bare literal.
        let inner = format!(
            r"(?:(?:pathlib\s*\.\s*)?Path\s*\(\s*(?:{lit}|{var})|{LIT_PREFIX}__L(?<a2>\d+)__)\s*\)?"
        );
        let mode =
            format!(r"(?<kw>mode\s*=\s*)?(?:{LIT_PREFIX}__L(?<m>\d+)__|(?<mv>[A-Za-z_]\w*)\b)");
        let open = |arg: &str| format!(r"\bopen\s*\(\s*{arg}\s*,\s*{mode}");
        let perl = |arg: &str| {
            format!(r"\bopen\s*(?:\(\s*)?(?:my\s+)?\$?\w+\s*,\s*__L(?<m>\d+)__\s*,\s*{arg}")
        };
        let os_open = |arg: &str| {
            format!(r"\bos\s*\.\s*open\s*\(\s*{arg}\s*,[^)]*O_(?:WRONLY|RDWR|CREAT|APPEND|TRUNC)")
        };
        let write_text = |arg: &str| format!(r"{arg}\s*\.\s*(?:write_text|write_bytes|touch)\b");
        let path_open = |arg: &str| format!(r"{arg}\s*\.\s*open\s*\(\s*{mode}");
        let writers = |arg: &str| {
            format!(
                r"\b(?:(?:write|append|output)File(?:Sync)?|writeJson(?:Sync)?|createWriteStream|truncate(?:Sync)?|(?:File|IO)\s*\.\s*(?:bin)?write|(?:Write|Append)All(?:Text|Bytes|Lines))\s*\(\s*{arg}"
            )
        };
        let copies = |arg: &str| {
            format!(
                r"\b(?:copy(?:file|2|tree|File(?:Sync)?)?|copy_file|move|mv|rename(?:Sync)?|replace|cp(?:Sync)?|(?:sym)?link(?:Sync)?)\s*\([^,()]*(?:\([^()]*\)[^,()]*)?,\s*{arg}"
            )
        };
        let plain = |source: String, mode| -> Result<Pattern, PolicySetupError> {
            Ok(Pattern {
                full: re(&source)?,
                inner: None,
                mode,
            })
        };
        let leading = |build: &dyn Fn(&str) -> String, mode| -> Result<Pattern, PolicySetupError> {
            Ok(Pattern {
                full: re(&build(&arg))?,
                inner: Some(re(&build(&inner))?),
                mode,
            })
        };
        Ok(Self {
            vars: re(&format!(
                r"(?:^|[^\w$.])\$?(?<n>[A-Za-z_]\w*)\s*=\s*{PATH_WRAP}{LIT_PREFIX}__L(?<l>\d+)__"
            ))?,
            writes: vec![
                plain(open(&arg), ModeTest::Write)?,
                plain(perl(&arg), ModeTest::PerlOpen)?,
                plain(os_open(&arg), ModeTest::None)?,
                leading(&write_text, ModeTest::None)?,
                leading(&path_open, ModeTest::Write)?,
                plain(writers(&arg), ModeTest::None)?,
                plain(copies(&arg), ModeTest::None)?,
            ],
            perl_two: re(r"\bopen\s*(?:\(\s*)?(?:my\s+)?\$?\w+\s*,\s*__L(?<m>\d+)__\s*\)")?,
            patches: PatchPatterns::new()?,
        })
    }

    /// 1.x `codePatches(code)`: whether the code, with its string escapes decoded, mentions
    /// `git apply`, `git am` (`['git', 'apply']` too) or a patch command: `patch` followed by
    /// an option, a file or `<` in a string or after `;`, `|`, `&`, a directory or a wrapper,
    /// or handed bare to a call that runs a command.
    #[must_use]
    pub fn runs_patch(&self, code: &str) -> bool {
        self.patches.test(code)
    }

    /// 1.x `codeWriteTargets(code)`: the files `open(p, 'w')`, `Path(p).write_text`,
    /// `writeFileSync(p)`, `File.write(p)`, `shutil.copy(src, p)`, perl's `open(my $fh, '>', p)`,
    /// `[IO.File]::WriteAllText(p)` and their kin write, where `p` is a string literal or a
    /// variable the code assigned one, without repeats, in 1.x's order.
    #[must_use]
    pub fn write_targets(&self, code: &str) -> Vec<String> {
        let lits = Literals::of(code);
        let text = lits.text.as_str();
        let mut vars: HashMap<&str, Option<&str>> = HashMap::new();
        for caps in self.vars.captures_iter(text) {
            if let (Some(name), Some(n)) = (caps.name("n"), caps.name("l")) {
                vars.insert(name.as_str(), lits.get(n.as_str()));
            }
        }
        let value = |r: Ref<'_>| match r {
            Ref::Lit(n) => lits.get(n),
            Ref::Var(name) => vars.get(name).copied().flatten(),
        };
        let mut out: Vec<String> = Vec::new();
        let mut push = |lit: &str| {
            if !lit.is_empty() && !out.iter().any(|o| o == lit) {
                out.push(lit.to_owned());
            }
        };
        for pattern in &self.writes {
            for found in pattern.matches(text) {
                let mode = found.mode.and_then(value).unwrap_or("");
                let writes = match pattern.mode {
                    ModeTest::None => true,
                    ModeTest::Write => mode.contains(['w', 'a', 'x', '+']),
                    ModeTest::PerlOpen => perl_write_mode(mode),
                };
                if writes && let Some(lit) = value(found.arg) {
                    push(lit);
                }
            }
        }
        for caps in self.perl_two.captures_iter(text) {
            let lit = caps.name("m").and_then(|n| lits.get(n.as_str()));
            if let Some(file) = lit.and_then(perl_two_target) {
                push(file);
            }
        }
        out
    }
}

impl Pattern {
    /// 1.x `text.matchAll(re)` with the look-arounds this pattern lost (see the module doc).
    fn matches<'t>(&self, text: &'t str) -> Vec<Found<'t>> {
        let mut out = Vec::new();
        let mut at = 0;
        // The inner pattern's last search: where it started and what it found.
        let mut inner_seen: Option<(usize, Option<Captures<'t>>)> = None;
        while let Some(caps) = self.full.captures_at(text, at) {
            let Some(whole) = caps.get(0) else { break };
            let start = whole.start();
            match self.judge(text, &caps) {
                Ok(found) => {
                    at = found.end;
                    out.push(found);
                }
                Err(Miss::Ahead) => at = start.saturating_add(1),
                Err(Miss::Behind) => {
                    let (last, resume) = behind_run(text, start);
                    at = resume;
                    let mut from = start.saturating_add(1);
                    while from <= last {
                        let Some(inner) = self.inner.as_ref() else {
                            break;
                        };
                        let stale = inner_seen.as_ref().is_none_or(|(searched, seen)| {
                            *searched > from
                                || seen
                                    .as_ref()
                                    .and_then(|c| c.get(0))
                                    .is_some_and(|m| m.start() < from)
                        });
                        if stale {
                            inner_seen = Some((from, inner.captures_at(text, from)));
                        }
                        let Some(caps) = inner_seen.as_ref().and_then(|(_, seen)| seen.as_ref())
                        else {
                            break;
                        };
                        let Some(m) = caps.get(0).filter(|m| m.start() <= last) else {
                            break;
                        };
                        match self.judge(text, caps) {
                            Ok(found) => {
                                at = found.end;
                                out.push(found);
                                break;
                            }
                            Err(_) => from = m.start().saturating_add(1),
                        }
                    }
                }
            }
        }
        out
    }

    /// The match 1.x makes at this start, from the look-around-free one, or why it makes none.
    fn judge<'t>(&self, text: &'t str, caps: &Captures<'t>) -> Result<Found<'t>, Miss> {
        let whole = caps.get(0).ok_or(Miss::Ahead)?;
        let start = whole.start();
        let lit = caps.name("a").or_else(|| caps.name("a2"));
        let arg = match (lit, caps.name("v")) {
            (Some(n), _) => Ref::Lit(n.as_str()),
            (None, Some(v)) => {
                // A bare variable at the start: `ARG`'s look-behind.
                let bytes = text.as_bytes();
                let bare = v.start() == start
                    || (v.start() == start.saturating_add(1) && bytes.get(start) == Some(&b'$'));
                let after_word = start
                    .checked_sub(1)
                    .and_then(|before| bytes.get(before))
                    .is_some_and(|&b| word_or_dot(b));
                if self.inner.is_some() && bare && after_word {
                    return Err(Miss::Behind);
                }
                Ref::Var(v.as_str())
            }
            (None, None) => return Err(Miss::Ahead),
        };
        let mut end = whole.end();
        let mode = match (caps.name("m"), caps.name("mv")) {
            (Some(n), _) => Some(Ref::Lit(n.as_str())),
            (None, Some(mv)) if !called_after(text, mv.end()) => Some(Ref::Var(mv.as_str())),
            (None, Some(_)) => {
                // `MODE`'s look-ahead failed: past `mode=`, `mode` is the variable.
                let kw = caps.name("kw").ok_or(Miss::Ahead)?;
                let name_end = kw.start().saturating_add("mode".len());
                end = name_end;
                Some(Ref::Var(text.get(kw.start()..name_end).unwrap_or("")))
            }
            (None, None) => None,
        };
        Ok(Found { arg, mode, end })
    }
}

/// Is `b` in `[\w.]` (ASCII, as JS reads `\w` without `u`)?
fn word_or_dot(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

/// After a bare variable's look-behind fails at `start`: the last start where it fails the
/// same way (each start up to it follows a char of `start`'s run of `[\w.]`), and the next
/// char boundary past it, where the full pattern's search resumes.
fn behind_run(text: &str, start: usize) -> (usize, usize) {
    let run = text.as_bytes().get(start..).map_or(0, |rest| {
        rest.iter().take_while(|&&b| word_or_dot(b)).count()
    });
    let last = start.saturating_add(run);
    let width = text
        .get(last..)
        .and_then(|rest| rest.chars().next())
        .map_or(1, char::len_utf8);
    (last, last.saturating_add(width))
}

/// 1.x `(?!\s*[(.\[])` at `at`: whitespace, then a call, a member or an index.
fn called_after(text: &str, at: usize) -> bool {
    text.get(at..)
        .and_then(|rest| rest.chars().find(|&c| !is_js_space(c)))
        .is_some_and(|c| matches!(c, '(' | '.' | '['))
}

/// 1.x `/^\s*(?:\+?>>?|\+<)/`.
fn perl_write_mode(mode: &str) -> bool {
    let rest = mode.trim_start_matches(is_js_space);
    rest.starts_with('>') || rest.starts_with("+>") || rest.starts_with("+<")
}

/// Is `c` a JS line terminator, which `.` does not match?
fn line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// 1.x `/^\s*\+?>>?\s*(\S.*)$/.exec(lit)[1]`: the file of perl's `open(FH, '>file')`.
fn perl_two_target(lit: &str) -> Option<&str> {
    let rest = lit.trim_start_matches(is_js_space);
    let rest = rest.strip_prefix('+').unwrap_or(rest);
    let after_one = rest.strip_prefix('>')?;
    // `>>?` takes the second `>` first, then gives it back.
    if let Some(after_two) = after_one.strip_prefix('>') {
        let file = after_two.trim_start_matches(is_js_space);
        if !file.is_empty() && !file.contains(line_terminator) {
            return Some(file);
        }
        return (!after_one.contains(line_terminator)).then_some(after_one);
    }
    let file = after_one.trim_start_matches(is_js_space);
    (!file.is_empty() && !file.contains(line_terminator)).then_some(file)
}

#[cfg(test)]
mod tests;
