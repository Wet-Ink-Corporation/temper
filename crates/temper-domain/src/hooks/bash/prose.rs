//! Prose blanking for the `--no-verify` and lint-lowering checks (1.x `hooks.mjs` `blankProse`,
//! `HEREDOC`, `FEEDS_SHELL`, `MESSAGE_VALUE`, `CAT_HEREDOC`, `runsCode` and `SEARCH`).
//!
//! Those checks match the raw text, so a command handed to `sh -c`, `rebase -x` or
//! `powershell -Command` is still read; only text that is clearly prose is blanked first.

use regex::{NoExpand, Regex};

use crate::hooks::setup::{PolicySetupError, compile};
use crate::js::{JsFlags, Replacement, is_js_space, replace_all};

/// 1.x `QUOTED`.
const QUOTED: &str = r#"'[^']*'|"(?:[^"\\]|\\.)*""#;

/// The compiled patterns of [`ProsePatterns::blank`].
#[derive(Debug)]
pub(super) struct ProsePatterns {
    continuation: Regex,
    head_split: Regex,
    feeds_shell: Regex,
    message_value: Regex,
    search_head: Regex,
    search_tail: Regex,
    quoted: Regex,
    after_dashes: Regex,
}

impl ProsePatterns {
    /// Compiles the patterns.
    pub(super) fn new() -> Result<Self, PolicySetupError> {
        let plain = |name, source: &str| compile(name, source, JsFlags::Plain);
        // 1.x `[^\s;&|(\x60]`.
        let word = r"[^\s;&|(\x60]";
        Ok(Self {
            continuation: plain("prose:continuation", r"\\\r?\n")?,
            head_split: plain("prose:head-split", r"&&|\|\||;|\(")?,
            // 1.x ends with `(?=\s|$)`; consumed, as only `.test` reads it.
            feeds_shell: compile(
                "prose:feeds-shell",
                r#"\||(?:^|[\s(])["']?(?:\S*[\\/])?(?:(?:ba|da|z|k|pw)?sh|powershell|cmd|eval|source|xargs|python[\d.]*|py|node|perl|ruby)(?:\.exe)?["']?(?:\s|$)"#,
                JsFlags::IgnoreCase,
            )?,
            // 1.x `(?:${QUOTED}|(?!--)[^\s'"]+)`: a value that does not start with `--`. A
            // value of a lone `-` ends where the class does, and that char is the last group,
            // which the match does not keep.
            message_value: plain(
                "prose:message-value",
                &format!(
                    r#"((?:^|\s)(?:--(?:message|body|title|notes|description)=?|-[a-zA-Z]*m|-[bt])\s*)(?:{QUOTED}|[^\s'"-][^\s'"]*|-[^\s'"-][^\s'"]*|-([\s'"]|$))"#
                ),
            )?,
            // 1.x `SEARCH` up to its `(?=\s|$)`, which is consumed; group 1 ends where the
            // look-ahead stood, and `search_tail` runs from there.
            search_head: plain(
                "prose:search",
                &format!(
                    r"((?:^|[;&|(\x60\n])[ \t]*(?:\w+={word}*[ \t]+|(?:sudo|env|time|xargs)[ \t]+)*(?:{word}*[\\/])?(?:git[ \t]+grep|[ef]?grep|rg|ag|ack)(?:\.exe)?)(?:\s|$)"
                ),
            )?,
            search_tail: plain("prose:search-tail", &format!(r#"(?:{QUOTED}|[^'"|;&\n])*"#))?,
            quoted: plain("prose:quoted", QUOTED)?,
            after_dashes: plain("prose:after-dashes", r"\s--\s.*")?,
        })
    }

    /// 1.x `blankProse(cmd)`: here-document bodies, message option values and search patterns
    /// blanked.
    pub(super) fn blank(&self, cmd: &str) -> String {
        // A backslash-newline continues the line.
        let s = self.continuation.replace_all(cmd, NoExpand(" "));
        let s = replace_all(&s, |from| self.heredoc(&s, from));
        let s = replace_all(&s, |from| self.message_value(&s, from));
        replace_all(&s, |from| self.search(&s, from))
    }

    /// The next here-document at or after `from`: its body is text, unless a shell reads it or
    /// an unquoted body runs `$(...)`.
    fn heredoc(&self, s: &str, from: usize) -> Option<Replacement> {
        let mut at = from;
        loop {
            let start = at.saturating_add(s.get(at..)?.find("<<")?);
            if let Some(doc) = heredoc_at(s, start) {
                let line_start = s
                    .get(..start)
                    .and_then(|h| h.rfind('\n'))
                    .map_or(0, |nl| nl.saturating_add(1));
                let before = s.get(line_start..start).unwrap_or("");
                let last = self
                    .head_split
                    .find_iter(before)
                    .last()
                    .map_or(0, |m| m.end());
                let head = format!("{}{}", before.get(last..).unwrap_or(""), doc.rest);
                let whole = s.get(start..doc.end).unwrap_or("");
                let keep = self.feeds_shell.is_match(&head)
                    || (doc.quote.is_empty() && (whole.contains("$(") || whole.contains('`')));
                let with = if keep {
                    whole.to_owned()
                } else {
                    let (q, tag, rest) = (doc.quote, doc.tag, doc.rest);
                    format!("<<{q}{tag}{q}{rest}\n{tag}")
                };
                return Some(Replacement {
                    start,
                    end: doc.end,
                    with,
                });
            }
            at = start.saturating_add(1);
        }
    }

    /// The next message option value at or after `from`; one that holds a command
    /// substitution runs, so it is never blanked.
    fn message_value(&self, s: &str, from: usize) -> Option<Replacement> {
        let caps = self.message_value.captures_at(s, from)?;
        let whole = caps.get(0)?;
        let end = caps
            .get(2)
            .map_or_else(|| whole.end(), |trailer| trailer.start());
        let matched = s.get(whole.start()..end).unwrap_or("");
        let with = if runs_code(matched) {
            matched.to_owned()
        } else {
            format!("{}''", caps.get(1).map_or("", |pre| pre.as_str()))
        };
        Some(Replacement {
            start: whole.start(),
            end,
            with,
        })
    }

    /// The next search command at or after `from`, with its quoted patterns and everything
    /// after a `--` blanked, unless it runs code.
    fn search(&self, s: &str, from: usize) -> Option<Replacement> {
        let caps = self.search_head.captures_at(s, from)?;
        let start = caps.get(0)?.start();
        let head_end = caps.get(1)?.end();
        let end = self
            .search_tail
            .find_at(s, head_end)
            .filter(|tail| tail.start() == head_end)
            .map_or(head_end, |tail| tail.end());
        let matched = s.get(start..end).unwrap_or("");
        let with = if runs_code(matched) {
            matched.to_owned()
        } else {
            let unquoted = self.quoted.replace_all(matched, NoExpand("''"));
            self.after_dashes
                .replacen(&unquoted, 1, NoExpand(" -- "))
                .into_owned()
        };
        Some(Replacement { start, end, with })
    }
}

/// A here-document 1.x's `HEREDOC` matches.
#[derive(Debug)]
struct Heredoc<'s> {
    /// Where the match ends: after the closing tag.
    end: usize,
    /// Group 1: the quote around the tag, or empty.
    quote: &'s str,
    /// Group 2: the tag.
    tag: &'s str,
    /// Group 3: the rest of the opening line.
    rest: &'s str,
}

/// 1.x `(?<!<)<<-?[ \t]*(['"]?)([\w.-]+)\1([^\n]*)\n[\s\S]*?\n\t*\2(?=\n|$)` at byte `at`, in
/// JS's backtracking order: the `-` taken, then not; for an unquoted tag, the longest tag that
/// has a closing line, and the first such line.
fn heredoc_at(s: &str, at: usize) -> Option<Heredoc<'_>> {
    let bytes = s.as_bytes();
    if !s.get(at..)?.starts_with("<<")
        || at.checked_sub(1).and_then(|b| bytes.get(b)) == Some(&b'<')
    {
        return None;
    }
    let after = at.saturating_add(2);
    let dashes: &[usize] = if bytes.get(after) == Some(&b'-') {
        &[1, 0]
    } else {
        &[0]
    };
    dashes.iter().find_map(|&dash| {
        let mut p = after.saturating_add(dash);
        while matches!(bytes.get(p), Some(b' ' | b'\t')) {
            p = p.saturating_add(1);
        }
        let quote = match bytes.get(p) {
            Some(b'\'' | b'"') => s.get(p..p.saturating_add(1))?,
            _ => "",
        };
        let tag_start = p.saturating_add(quote.len());
        let run = s.get(tag_start..)?;
        let run = run.get(..run.find(|c: char| !is_tag_char(c)).unwrap_or(run.len()))?;
        if run.is_empty() {
            return None;
        }
        let shortest = if quote.is_empty() { 1 } else { run.len() };
        (shortest..=run.len()).rev().find_map(|len| {
            let tag = run.get(..len)?;
            let rest_start = tag_start.saturating_add(len);
            let rest_start = if quote.is_empty() {
                rest_start
            } else {
                s.get(rest_start..)?.strip_prefix(quote)?;
                rest_start.saturating_add(quote.len())
            };
            let line_end = rest_start.saturating_add(s.get(rest_start..)?.find('\n')?);
            let end = closing_line(s, line_end.saturating_add(1), tag)?;
            Some(Heredoc {
                end,
                quote,
                tag,
                rest: s.get(rest_start..line_end)?,
            })
        })
    })
}

/// JS `[\w.-]`.
fn is_tag_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// The end of the first `\n\t*<tag>` at or after `from` that a newline or the end follows.
fn closing_line(s: &str, from: usize, tag: &str) -> Option<usize> {
    let mut at = from;
    loop {
        let nl = at.saturating_add(s.get(at..)?.find('\n')?);
        let line = s.get(nl.saturating_add(1)..)?;
        let body = line.trim_start_matches('\t');
        if let Some(after) = body.strip_prefix(tag)
            && (after.is_empty() || after.starts_with('\n'))
        {
            return Some(s.len().saturating_sub(after.len()));
        }
        at = nl.saturating_add(1);
    }
}

/// 1.x `runsCode(text)`: does `text` hold `$(` or a backtick once each bare
/// `$(cat <<'EOF' ... EOF)`, whose body is already blanked, is removed?
fn runs_code(text: &str) -> bool {
    let rest = replace_all(text, |from| {
        let mut at = from;
        loop {
            let start = at.saturating_add(text.get(at..)?.find("$(")?);
            if let Some(end) = cat_heredoc_at(text, start) {
                return Some(Replacement {
                    start,
                    end,
                    with: String::new(),
                });
            }
            at = start.saturating_add(1);
        }
    });
    rest.contains("$(") || rest.contains('`')
}

/// 1.x `CAT_HEREDOC`, `\$\(\s*cat\s+<<-?(['"]?)([\w.-]+)\1[ \t]*\n\2\s*\)`, at byte `at`: the
/// end of the match. The tag is the whole run, since `[ \t]*\n` must follow it; the `-` is
/// tried taken, then as part of the tag.
fn cat_heredoc_at(s: &str, at: usize) -> Option<usize> {
    let rest = s.get(at..)?.strip_prefix("$(")?;
    let rest = rest.trim_start_matches(is_js_space).strip_prefix("cat")?;
    let after_cat = rest.trim_start_matches(is_js_space);
    if after_cat.len() == rest.len() {
        return None;
    }
    let opened = after_cat.strip_prefix("<<")?;
    let forms: &[&str] = match opened.strip_prefix('-') {
        Some(undashed) => &[undashed, opened],
        None => &[opened],
    };
    forms.iter().find_map(|form| {
        let (quote, body) = match form.chars().next() {
            Some(q @ ('\'' | '"')) => (form.get(..1)?, form.get(q.len_utf8()..)?),
            _ => ("", *form),
        };
        let tag = body.get(..body.find(|c: char| !is_tag_char(c)).unwrap_or(body.len()))?;
        if tag.is_empty() {
            return None;
        }
        let after = body.get(tag.len()..)?.strip_prefix(quote)?;
        let after = after.trim_start_matches([' ', '\t']).strip_prefix('\n')?;
        let after = after
            .strip_prefix(tag)?
            .trim_start_matches(is_js_space)
            .strip_prefix(')')?;
        Some(s.len().saturating_sub(after.len()))
    })
}
