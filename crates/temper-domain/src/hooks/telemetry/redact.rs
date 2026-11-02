//! What a telemetry row keeps of a shell command (1.x `project.mjs` `redactCommand`,
//! `dropQuoted`, `REDACTIONS` and `REDACT_CAP`).
//!
//! The JS patterns are kept as sources and translated, except where `regex` has no look-around
//! or back-reference (port plan 2-16, wi-31f653): each rewrite carries its 1.x source in a
//! comment, and `tests` holds an input on which dropping the look-around gives another answer.

use regex::{Captures, Regex};

use crate::hooks::setup::{PolicySetupError, compile};
use crate::js::{JsFlags, Replacement, is_js_space, next_boundary, replace_all, truncate_utf16};

/// How much of a command's first line is redacted at most, in UTF-16 units (1.x `REDACT_CAP`).
const REDACT_CAP: usize = 2048;
/// How much of the first line quotes are dropped from, in UTF-16 units.
const QUOTE_SCAN_CAP: usize = 16 * REDACT_CAP;
/// How many chars (code points) a row keeps (`redactCommand`'s default `max`).
const ROW_CHARS: usize = 200;

/// A name that looks like it holds a secret (1.x `SECRET_WORD`), without its last alternative,
/// `auth(?![a-z])`, which each pattern spells out with the run that follows it.
const SECRET_WORD: &str = r"(?:token|secret|passw(?:or)?d|passphrase|api[-_]?key|private[-_]?key|access[-_]?key|credential|cookie)";
/// 1.x `SECRET_SUFFIX`.
const SECRET_SUFFIX: &str = r"(?:\w{0,64}_)?(?:key|pass|passwd|pwd|pw)";
/// 1.x `QUOTE`.
const QUOTE: &str = r#"\\?["']"#;
/// 1.x `SECRET_VALUE`.
const SECRET_VALUE: &str = r#"(?:"[^"]*"|'[^']*'|\S+)"#;
/// 1.x `(?!\*\*\*)(?:"[^"]*"|'[^']*'|[^\s;&|]+)`: a value that does not start with `***`. A
/// value of one or two `*`s ends where the class does, and that char is the last group, which
/// the match does not keep.
const UNSTARRED_VALUE: &str = r#"(?:"[^"]*"|'[^']*'|[^\s;&|*][^\s;&|]*|\*[^\s;&|*][^\s;&|]*|\*\*[^\s;&|*][^\s;&|]*|\*{1,2}([\s;&|]|$))"#;

/// 1.x `[\w-]{0,64}${SECRET_WORD}[\w-]{0,64}` with the run after a word drawn from `run`
/// (`[\w-]`, `[\w.-]`, `\w`, ...) up to `max` times. `auth(?![a-z])` becomes `auth` followed by
/// nothing or by a run whose first char is no letter (`first`): what follows the name in every
/// pattern is no letter either. `extra` adds alternatives after `auth`, as `signature`.
fn secret_name(run: &str, max: usize, first: &str, extra: &str) -> String {
    let rest = max.saturating_sub(1);
    format!(
        "{run}{{0,{max}}}(?:{SECRET_WORD}{run}{{0,{max}}}|auth(?:{first}{run}{{0,{rest}}})?{extra})"
    )
}

/// How a rule's matches are replaced.
#[derive(Debug)]
enum To {
    /// A `$n` template.
    Template(&'static str),
    /// A `$n` template; the last group, when it took part, is a consumed look-ahead char that
    /// the match does not keep.
    Trailer(&'static str),
    /// 1.x `(head)(QUOTE)(?:(?!\2)[^\n])*\2` → `$1$2***$2`: the regex finds the head and the
    /// opening quote, and the value runs to the next copy of that quote on the line.
    QuotedValue,
}

/// One of 1.x's `REDACTIONS`, compiled.
#[derive(Debug)]
struct Rule {
    re: Regex,
    to: To,
}

/// 1.x `REDACTIONS`, in order: name, flags (`true` for `gi`), JS source, replacement.
fn sources() -> Vec<(&'static str, bool, String, To)> {
    let name_dash = secret_name(r"[\w-]", 64, r"[\d_-]", "");
    let name_word = secret_name(r"\w", 64, r"[\d_]", "");
    vec![
        // scheme://user:password@host. 1.x `[a-z]` under `i`, spelled out, since the
        // translation folds letters but not ranges.
        ("redact:url-userinfo", true, r#"\b([abcdefghijklmnopqrstuvwxyz][\w+.-]{0,31}:\/\/)[^\s/'"]+@"#.to_owned(), To::Template("${1}***@")),
        // Authorization: Bearer x. 1.x `(?:\b|(?<=-H))`: the `-H` is taken into the match and
        // kept; a match from `-H`'s `H` would end the same and print the same.
        ("redact:header", true, format!(r#"(?:\b|(-H))((?:proxy-)?authorization|{name_dash})(\s*:\s*)[^'"\n]+"#), To::Template("${1}${2}${3}***")),
        // "token": "x", its quotes escaped or not.
        ("redact:json-quoted", true, format!(r"({QUOTE}{name_dash}{QUOTE}\s*:\s*)({QUOTE})"), To::QuotedValue),
        // "pin": 1234. 1.x `(?![\s"'\\])[^\s,}]+`.
        ("redact:json-bare", true, format!(r#"({QUOTE}{name_dash}{QUOTE}\s*:\s*)[^\s,}}"'\\][^\s,}}]*"#), To::Template("${1}***")),
        // ?api_key=x&token=y&X-Amz-Signature=z
        ("redact:query", true, format!(r##"([?&](?:{}|(?:[\w.-]{{0,64}}[_.-])?(?:key|pass|passwd|pwd|pw)|sig|code|session(?:id)?)=)[^&\s'"#]*"##, secret_name(r"[\w.-]", 64, r"[\d_.-]", r"|signature[\w.-]{0,64}")), To::Template("${1}***")),
        // --token x, --password=x
        ("redact:flag", true, format!(r"(^|\s)(--?(?:{name_dash}|(?:[\w-]{{0,64}}[-_])?(?:key|pass|passwd|pwd|pw)))(=|\s+){SECRET_VALUE}"), To::Template("${1}${2}${3}***")),
        // GH_TOKEN=x, export API_KEY='x'
        ("redact:assignment", true, format!(r#"(^|[\s;&|("'=.:-])({name_word}|{SECRET_SUFFIX})={UNSTARRED_VALUE}"#), To::Trailer("${1}${2}=***")),
        // $env:GH_TOKEN = "x"
        ("redact:powershell", true, format!(r"(\$env:(?:{name_word}|{SECRET_SUFFIX}))(\s*=\s*){UNSTARRED_VALUE}"), To::Trailer("${1}${2}***")),
        // curl -u user:password
        ("redact:user", false, r#"(\s(?:-u|--user)(?:=|\s*))(?:"[^"\s]*:[^"]*"|'[^'\s]*:[^']*'|[^\s'"]*:\S*)"#.to_owned(), To::Template("${1}***")),
        // aws configure set aws_secret_access_key x. 1.x's `[^\s=]` run after `auth` starts
        // with no letter.
        ("redact:config", true, format!(r"(\bconfig(?:ure)?(?:\s+-[\w-]{{1,32}})*\s+(?:set\s+)?{})(=|\s+){SECRET_VALUE}", secret_name(r"[^\s=]", 128, r"[^\s=abcdefghijklmnopqrstuvwxyz]", "")), To::Template("${1}${2}***")),
        // .npmrc's _authToken=x and _password=x
        ("redact:npmrc", true, r#"(_auth(?:Token)?|_password)(\s*=\s*|\s+)(?:"[^"]*"|'[^']*'|\S+)"#.to_owned(), To::Template("${1}${2}***")),
        // mysql -pX, sshpass -p X, docker login -p X
        ("redact:short-p", false, r#"(\b(?:mysql\w{0,16}|mariadb\w{0,16}|sshpass|(?:docker|podman|nerdctl|helm\s+registry)\s+login)\b[^;&|\n]{0,256}?\s-p)(\s*)(?:"[^"]*"|'[^']*'|[^\s;&|]+)"#.to_owned(), To::Template("${1}${2}***")),
        ("redact:curl-cookie", false, r#"(\bcurl\b[^;&|\n]{0,256}?\s-b)(\s*)(?:"[^"]*"|'[^']*'|[^\s;&|]+)"#.to_owned(), To::Template("${1}${2}***")),
        ("redact:redis", false, r#"(\bredis-cli\b[^;&|\n]{0,256}?\s-a)(\s*)(?:"[^"]*"|'[^']*'|[^\s;&|]+)"#.to_owned(), To::Template("${1}${2}***")),
        ("redact:az-login", false, r#"(\baz\s+login\b[^;&|\n]{0,256}?\s-p)(\s*)(?:"[^"]*"|'[^']*'|[^\s;&|]+)"#.to_owned(), To::Template("${1}${2}***")),
        ("redact:cargo-login", false, r"(\bcargo\s+login)\s+[^;&|\n]*[^\s;&|]".to_owned(), To::Template("${1} ***")),
        ("redact:openssl", false, r#"(\s-pass(?:in|out)?\s+pass:)(?:"[^"]*"|'[^']*'|[^\s;&|]+)"#.to_owned(), To::Template("${1}***")),
        ("redact:gh-secret", false, r#"(\bgh\s+secret\s+set\b[^;&|\n]{0,256}?\s(?:-b|--body)(?:=|\s*))(?:"[^"]*"|'[^']*'|[^\s;&|]+)"#.to_owned(), To::Template("${1}***")),
        ("redact:private-key", false, r"-----BEGIN[A-Z ]{0,32}PRIVATE KEY-----.*?(?:-----END[A-Z ]{0,32}PRIVATE KEY-----|$)".to_owned(), To::Template("***")),
        ("redact:webhook", true, r"\b(hooks\.slack\.com\/services\/|discord(?:app)?\.com\/api\/webhooks\/)[\w/-]+".to_owned(), To::Template("${1}***")),
        ("redact:token-shape", false, r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_\w{20,}|glpat-[\w-]{20,}|sk-[\w-]{20,}|[sr]k_(?:live|test)_[A-Za-z0-9]{10,}|npm_[A-Za-z0-9]{20,}|AIza[\w-]{30,}|xox[abprs]-[\w-]{10,}|(?:AKIA|ASIA)[0-9A-Z]{16}|eyJ[\w-]{10,}\.[\w-]{10,}\.[\w-]+|hf_[A-Za-z0-9]{30,}|pypi-AgEI[\w-]{20,}|glsa_\w{20,}|dop_v1_[a-f0-9]{20,}|SG\.[\w-]{16,}\.[\w-]{16,}|shpat_[a-fA-F0-9]{20,})\b".to_owned(), To::Template("***")),
    ]
}

/// The compiled redaction rules (1.x `redactCommand`).
#[derive(Debug)]
pub(crate) struct Redactor {
    rules: Vec<Rule>,
}

impl Redactor {
    /// Compiles 1.x's `REDACTIONS`.
    ///
    /// # Errors
    ///
    /// [`PolicySetupError`] when a pattern does not compile, which is a bug in temper.
    pub(crate) fn new() -> Result<Self, PolicySetupError> {
        let rules = sources()
            .into_iter()
            .map(|(name, fold, source, to)| {
                let flags = if fold {
                    JsFlags::IgnoreCase
                } else {
                    JsFlags::Plain
                };
                Ok(Rule {
                    re: compile(name, &source, flags)?,
                    to,
                })
            })
            .collect::<Result<_, PolicySetupError>>()?;
        Ok(Self { rules })
    }

    /// The command as a telemetry row keeps it, clipped to 200 chars.
    pub(crate) fn redact(&self, cmd: &str) -> String {
        self.redact_to(cmd, ROW_CHARS)
    }

    /// 1.x `redactCommand(cmd, max)`: the first line, up to any heredoc or here-string, with
    /// quoted text dropped and likely secrets replaced by `***`, then clipped to `max` chars;
    /// `…` marks a command cut short. The UTF-16 cuts never split a surrogate pair (D16(o)).
    pub(crate) fn redact_to(&self, cmd: &str, max: usize) -> String {
        let line = cmd
            .find(['\r', '\n'])
            .map_or(cmd, |eol| cmd.get(..eol).unwrap_or(""));
        let first = truncate_utf16(line, QUOTE_SCAN_CAP);
        let (mut s, mut cut) = drop_quoted(first);
        cut |= first.len() < cmd.len();
        if let Some(here) = s.find("<<")
            && here.saturating_add(2) < s.len()
        {
            s.truncate(here.saturating_add(2));
            cut = true;
        }
        if s.encode_utf16().count() > REDACT_CAP {
            // Cut back to the last whole word (`.replace(/\S*$/, '')`).
            let head = truncate_utf16(&s, REDACT_CAP);
            s = head.trim_end_matches(|c| !is_js_space(c)).to_owned();
            cut = true;
        }
        for rule in &self.rules {
            s = rule.apply(&s);
        }
        if let Some((at, _)) = s.char_indices().nth(max) {
            s.truncate(at);
            cut = true;
        }
        if cut {
            s.push('…');
        }
        s
    }
}

impl Rule {
    /// `text.replace(re, to)`.
    fn apply(&self, text: &str) -> String {
        match self.to {
            To::Template(template) => self.re.replace_all(text, template).into_owned(),
            To::Trailer(template) => replace_all(text, |from| {
                let caps = self.re.captures_at(text, from)?;
                let whole = caps.get(0)?;
                let end = trailer_start(&caps).unwrap_or_else(|| whole.end());
                let mut with = String::new();
                caps.expand(template, &mut with);
                Some(Replacement {
                    start: whole.start(),
                    end,
                    with,
                })
            }),
            To::QuotedValue => replace_all(text, |from| quoted_value(&self.re, text, from)),
        }
    }
}

/// Where the last group starts, when it took part: the consumed char of a rewritten look-ahead.
fn trailer_start(caps: &Captures<'_>) -> Option<usize> {
    caps.get(caps.len().saturating_sub(1))
        .filter(|_| caps.len() > 1)
        .map(|trailer| trailer.start())
}

/// The leftmost 1.x `(head)(QUOTE)(?:(?!\2)[^\n])*\2` at or after `from`: `head` finds the
/// head and the opening quote (groups 1 and 2), and the value runs to the next copy of the
/// opening quote before a newline. Without one, JS fails at that start and tries the next.
fn quoted_value(head: &Regex, text: &str, from: usize) -> Option<Replacement> {
    let mut at = from;
    loop {
        let caps = head.captures_at(text, at)?;
        let whole = caps.get(0)?;
        let quote = caps.get(2)?.as_str();
        let rest = text.get(whole.end()..).unwrap_or("");
        let line = rest
            .find('\n')
            .map_or(rest, |nl| rest.get(..nl).unwrap_or(""));
        if let Some(close) = line.find(quote) {
            let name = caps.get(1).map_or("", |g| g.as_str());
            return Some(Replacement {
                start: whole.start(),
                end: whole
                    .end()
                    .saturating_add(close)
                    .saturating_add(quote.len()),
                with: format!("{name}{quote}***{quote}"),
            });
        }
        at = next_boundary(text, whole.start());
    }
}

/// 1.x `dropQuoted(line)`: the line with the contents of every quoted string (`'…'`, `"…"`,
/// `$'…'` and, outside quotes, `\"…\"`) replaced by `…`, and whether an unterminated quote,
/// a command substitution or parameter expansion inside `"…"`, or a bare quote inside `\"…\"`
/// cut the rest of the line.
fn drop_quoted(line: &str) -> (String, bool) {
    let bytes = line.as_bytes();
    let at = |i: usize| bytes.get(i).copied();
    let mut out = String::new();
    let mut plain = 0;
    let mut i = 0;
    while let Some(c) = at(i) {
        let open = match c {
            b'\\' if at(i.saturating_add(1)) == Some(b'"') => "\\\"",
            b'\\' => {
                // A backslash escapes the next char, whatever its width.
                i = next_boundary(line, i.saturating_add(1));
                continue;
            }
            b'$' if at(i.saturating_add(1)) == Some(b'\'') => "$'",
            b'\'' => "'",
            b'"' => "\"",
            _ => {
                i = i.saturating_add(1);
                continue;
            }
        };
        out.push_str(line.get(plain..i).unwrap_or(""));
        let from = i.saturating_add(open.len());
        let close = if open == "$'" { "'" } else { open };
        let Some(end) = closing(line, open, from) else {
            out.push_str(open);
            return (out, true);
        };
        out.push_str(open);
        if end > from {
            out.push('…');
        }
        out.push_str(close);
        i = end.saturating_add(close.len());
        plain = i;
    }
    out.push_str(line.get(plain..).unwrap_or(""));
    (out, false)
}

/// Where the string `open` opened, whose contents start at byte `from`, closes; `None` when it
/// does not, or when something inside it hides where it ends.
fn closing(line: &str, open: &str, from: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let at = |i: usize| bytes.get(i).copied();
    if open == "'" {
        return line
            .get(from..)
            .and_then(|rest| rest.find('\''))
            .map(|end| from.saturating_add(end));
    }
    let mut end = from;
    while let Some(c) = at(end) {
        let next = at(end.saturating_add(1));
        let stops = match open {
            // `\"…\"`: the closing `\"`; a bare quote before it may open a string that holds it.
            "\\\"" if c == b'\\' && next == Some(b'"') => return Some(end),
            "\\\"" => c == b'"' || c == b'\'',
            // `"…"`: `$(`, `` ` `` and `${` hold quotes of their own.
            "\"" if c == b'"' => return Some(end),
            "\"" => c == b'`' || (c == b'$' && matches!(next, Some(b'(' | b'{'))),
            // `$'…'`.
            _ if c == b'\'' => return Some(end),
            _ => false,
        };
        if stops {
            return None;
        }
        end = if c == b'\\' {
            next_boundary(line, end.saturating_add(1))
        } else {
            end.saturating_add(1)
        };
    }
    None
}

#[cfg(test)]
mod tests;
