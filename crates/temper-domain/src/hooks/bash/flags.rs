//! Flags that weaken verification on a command line (1.x `hooks.mjs` `segments`, `isCargo`,
//! `skipsGitHooks` with `GIT_COMMIT_N`, and `lowersLints`).

use regex::{NoExpand, Regex};

use crate::hooks::setup::{PolicySetupError, compile};
use crate::js::{JsFlags, is_js_space};

/// The compiled patterns of the flag checks.
#[derive(Debug)]
pub(super) struct FlagPatterns {
    quoted: Regex,
    separators: Regex,
    cargo: Regex,
    ignore_rust_version: Regex,
    no_verify: Regex,
    git: Regex,
    commit_n_tail: Regex,
    cargo_word: Regex,
    lint_tool: Regex,
    lint_flag: Regex,
    rustflags: Regex,
}

impl FlagPatterns {
    /// Compiles the patterns.
    pub(super) fn new() -> Result<Self, PolicySetupError> {
        let plain = |name, source: &str| compile(name, source, JsFlags::Plain);
        Ok(Self {
            quoted: plain("flags:quoted", r#"'[^']*'|"(?:[^"\\]|\\.)*""#)?,
            separators: plain("flags:separators", r"&&|\|\||[;|\n]")?,
            cargo: plain("flags:cargo", r"(?:^|\s)cargo(?:\s|$)")?,
            // 1.x `(?![\w-])`, consumed: only `.test` reads it.
            ignore_rust_version: plain(
                "flags:ignore-rust-version",
                r"(?:^|\s)--ignore-rust-version(?:[^\w-]|$)",
            )?,
            no_verify: plain("flags:no-verify", r"--no-verify\b")?,
            git: plain("flags:git", r"\bgit\s+")?,
            commit_n_tail: plain("flags:commit-n", r#"\s["']?-[aehiopqsvz]*n"#)?,
            cargo_word: plain("flags:cargo-word", r"\bcargo\b")?,
            lint_tool: plain("flags:lint-tool", r"\b(?:clippy|rustc)\b")?,
            lint_flag: plain(
                "flags:lint-flag",
                r#"\s["']?-A|\s["']?--allow(?:[^\w-]|$)|--cap-lints"#,
            )?,
            // 1.x `RUST(?:DOC)?FLAGS=(?:"[^"]*?|'[^']*?|\S*?)(?<=[=\s"'\x1f]|\\x1f)(?:-A|--cap-lints)`:
            // the look-behind sees the value's last char, or the `=` when the value is empty, so
            // each kind of value ends in the chars the look-behind allows inside it.
            rustflags: plain(
                "flags:rustflags",
                r#"RUST(?:DOC)?FLAGS=(?:"(?:[^"]*(?:[=\s'\x1f]|\\x1f))?|'(?:[^']*(?:[=\s"\x1f]|\\x1f))?|(?:\S*(?:[="'\x1f]|\\x1f))?)(?:-A|--cap-lints)"#,
            )?,
        })
    }

    /// 1.x `segments(cmd)`: the command split into simple commands, quoted strings blanked to
    /// `''` first so flags inside messages do not count.
    pub(super) fn segments(&self, cmd: &str) -> Vec<String> {
        let blanked = self.quoted.replace_all(cmd, NoExpand("''"));
        self.separators.split(&blanked).map(str::to_owned).collect()
    }

    /// `--ignore-rust-version` in a cargo segment.
    pub(super) fn ignores_rust_version(&self, cmd: &str) -> bool {
        self.segments(cmd)
            .iter()
            .any(|seg| self.cargo.is_match(seg) && self.ignore_rust_version.is_match(seg))
    }

    /// 1.x `skipsGitHooks(text)`: `--no-verify` anywhere, or `-n` in a `git commit` short-flag
    /// cluster before any valued flag.
    pub(super) fn skips_git_hooks(&self, text: &str) -> bool {
        self.no_verify.is_match(text) || self.git_commit_n(text)
    }

    /// 1.x `GIT_COMMIT_N.test(text)`:
    /// `\bgit\s+(?:-[cC]\s+(?:${QUOTED}|\S+)\s+|--?(?![cC]\s)\w[\w-]*(?:=(?:${QUOTED}|\S*))?\s+)*commit(?=\s|$)(?:(?!\s--\s)[^|;&\n])*\s["']?-[aehiopqsvz]*n`.
    /// The option loop is walked by hand over every position it can reach, since a quoted
    /// value can also be read as `\S+`; the tempered tail is a search bounded by the first
    /// separator or ` -- `.
    fn git_commit_n(&self, text: &str) -> bool {
        self.git.find_iter(text).any(|git| {
            let mut seen = vec![git.end()];
            let mut todo = vec![git.end()];
            while let Some(at) = todo.pop() {
                let rest = text.get(at..).unwrap_or("");
                if let Some(after) = rest.strip_prefix("commit")
                    && after.starts_with(is_js_space)
                    && self.commit_tail(text, text.len().saturating_sub(after.len()))
                {
                    return true;
                }
                for next in option_ends(text, at) {
                    if !seen.contains(&next) {
                        seen.push(next);
                        todo.push(next);
                    }
                }
            }
            false
        })
    }

    /// 1.x `(?:(?!\s--\s)[^|;&\n])*\s["']?-[aehiopqsvz]*n` from byte `at`: the cluster may
    /// start anywhere up to the first `|`, `;`, `&`, newline or ` -- `.
    fn commit_tail(&self, text: &str, at: usize) -> bool {
        let rest = text.get(at..).unwrap_or("");
        let stop = rest
            .char_indices()
            .find(|&(i, c)| matches!(c, '|' | ';' | '&' | '\n') || starts_dashes(rest, i))
            .map_or(rest.len(), |(i, _)| i);
        self.commit_n_tail
            .find_at(text, at)
            .is_some_and(|m| m.start() <= at.saturating_add(stop))
    }

    /// 1.x `lowersLints(text)`: `cargo clippy` or `rustc` with `-A`, `--allow` or
    /// `--cap-lints`, or a `*RUSTFLAGS` assignment holding one.
    pub(super) fn lowers_lints(&self, text: &str) -> bool {
        text.split(['|', ';', '&', '\n']).any(|seg| {
            let Some(cargo) = self.cargo_word.find(seg) else {
                return false;
            };
            let from_cargo = seg.get(cargo.start()..).unwrap_or("");
            self.lint_tool.find(from_cargo).is_some_and(|tool| {
                self.lint_flag
                    .is_match(from_cargo.get(tool.start()..).unwrap_or(""))
            })
        }) || self.rustflags.is_match(text)
    }
}

/// Does a JS `\s--\s` start at byte `i` of `text`?
fn starts_dashes(text: &str, i: usize) -> bool {
    let mut chars = text.get(i..).unwrap_or("").chars();
    matches!(
        (chars.next(), chars.next(), chars.next(), chars.next()),
        (Some(a), Some('-'), Some('-'), Some(b)) if is_js_space(a) && is_js_space(b)
    )
}

/// Where each way of reading one option of `GIT_COMMIT_N`'s loop at byte `at` ends:
/// `-[cC]\s+(?:QUOTED|\S+)\s+` or `--?(?![cC]\s)\w[\w-]*(?:=(?:QUOTED|\S*))?\s+`.
fn option_ends(text: &str, at: usize) -> Vec<usize> {
    let rest = text.get(at..).unwrap_or("");
    let end = |tail: &str| text.len().saturating_sub(tail.len());
    let mut ends = Vec::new();
    let Some(dashed) = rest.strip_prefix('-') else {
        return ends;
    };
    // `-c value` and `-C dir`.
    if let Some(flag) = dashed.strip_prefix(['c', 'C']) {
        let value = flag.trim_start_matches(is_js_space);
        if value.len() < flag.len() {
            for after in values(value, false) {
                if let Some(next) = spaces(after) {
                    ends.push(end(next));
                }
            }
        }
    }
    // `--name[=value]`, the second `-` optional; the name is no `c` or `C` before a space.
    let named = dashed.strip_prefix('-').unwrap_or(dashed);
    let mut chars = named.chars();
    let first = chars.next();
    let short_c = matches!(first, Some('c' | 'C')) && chars.next().is_some_and(is_js_space);
    if first.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') && !short_c {
        let after_name =
            named.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        let afters = match after_name.strip_prefix('=') {
            Some(value) => values(value, true),
            None => vec![after_name],
        };
        for after in afters {
            if let Some(next) = spaces(after) {
                ends.push(end(next));
            }
        }
    }
    ends
}

/// The text after each way of reading a value at the start of `text`: a quoted string, then
/// `\S+` (or `\S*` when `empty_ok`), whole, since a space must follow it.
fn values(text: &str, empty_ok: bool) -> Vec<&str> {
    let mut out = Vec::new();
    if let Some(after) = quoted_prefix(text) {
        out.push(after);
    }
    let bare = text.trim_start_matches(|c: char| !is_js_space(c));
    if empty_ok || bare.len() < text.len() {
        out.push(bare);
    }
    out
}

/// The text after `\s+`, or `None` without a space.
fn spaces(text: &str) -> Option<&str> {
    let after = text.trim_start_matches(is_js_space);
    (after.len() < text.len()).then_some(after)
}

/// The text after a JS `'[^']*'|"(?:[^"\\]|\\.)*"` at the start of `text`.
fn quoted_prefix(text: &str) -> Option<&str> {
    if let Some(body) = text.strip_prefix('\'') {
        return body
            .find('\'')
            .and_then(|end| body.get(end.saturating_add(1)..));
    }
    let body = text.strip_prefix('"')?;
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return body.get(i.saturating_add(1)..),
            // `\\.`: JS `.` is no line terminator.
            '\\' => match chars.next() {
                Some((_, '\n' | '\r' | '\u{2028}' | '\u{2029}')) | None => return None,
                Some(_) => {}
            },
            _ => {}
        }
    }
    None
}
