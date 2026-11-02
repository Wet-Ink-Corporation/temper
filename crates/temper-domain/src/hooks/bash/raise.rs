//! Commands that raise the lint baseline or restore older verification configuration (1.x
//! `hooks.mjs` `unquotedSegments`, `baselineCall`, `baselineRaise` and `claudeRestore`).

use regex::Regex;

use crate::hooks::setup::{PolicySetupError, compile};
use crate::js::{JsFlags, is_js_space, js_trim};

/// The flag of a `temper baseline` call that records every current finding as accepted debt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiseFlag {
    /// `--init`.
    Init,
    /// `--update`.
    Update,
}

impl RaiseFlag {
    /// The flag as written, `--init` or `--update`.
    #[must_use]
    pub const fn flag(self) -> &'static str {
        match self {
            Self::Init => "--init",
            Self::Update => "--update",
        }
    }
}

/// The compiled patterns of the raise and restore checks.
#[derive(Debug)]
pub(super) struct RaisePatterns {
    separators: Regex,
    quoted: Regex,
    cli: Regex,
    rest: Regex,
    raise_flag: Regex,
    restore: Regex,
    claude_path: Regex,
    show_into_claude: Regex,
}

impl RaisePatterns {
    /// Compiles the patterns.
    pub(super) fn new() -> Result<Self, PolicySetupError> {
        let plain = |name, source: &str| compile(name, source, JsFlags::Plain);
        Ok(Self {
            separators: plain("raise:separators", r"&&|\|\||[;|\n]")?,
            quoted: plain("raise:quoted", r#""((?:[^"\\]|\\.)*)"|'([^']*)'"#)?,
            // 1.x `BASELINE_CLI`, plus D16(g): `temper.exe`, and `temper` by an absolute path.
            cli: plain(
                "raise:cli",
                r"(?:^|[\s=])(?:[^\s=]*temper\.mjs|\$\{?\w+\}?|(?:\/|[A-Za-z]:[\\/]|\\\\)[^\s=]*[\\/]temper(?:\.exe)?|temper(?:\.exe)?)\s+(?:-\S+\s+)*",
            )?,
            // 1.x `BASELINE_REST`, sticky: anchored, and matched on the text where `cli` ended.
            rest: plain("raise:rest", r"^baseline\b(.*)$")?,
            // 1.x `(?![\w-])`, consumed: only the group is read.
            raise_flag: plain("raise:flag", r"(?:^|\s)--?(init|update)(?:[^\w-]|$)")?,
            restore: plain(
                "raise:restore",
                r"\bgit\s+(?:(?:-C|-c)\s+\S+\s+)*(checkout|restore|stash)\b(.*)$",
            )?,
            claude_path: plain(
                "raise:claude-path",
                r"(?:^|[\s=:/])\.claude(?:\/|\s|$)|temper-baseline\.json",
            )?,
            show_into_claude: plain(
                "raise:show",
                r"\bgit\s+(?:-C\s+\S+\s+)*show\b[^>]*>\s*\S*(?:^|\/|\s)?\.claude\/",
            )?,
        })
    }

    /// 1.x `unquotedSegments(cmd)`: the simple commands, each with its quotes removed.
    fn unquoted_segments(&self, cmd: &str) -> Vec<String> {
        self.separators
            .split(cmd)
            .map(|seg| {
                self.quoted
                    .replace_all(seg, |caps: &regex::Captures<'_>| {
                        caps.get(1)
                            .or_else(|| caps.get(2))
                            .map_or(String::new(), |g| g.as_str().to_owned())
                    })
                    .into_owned()
            })
            .collect()
    }

    /// 1.x `baselineCall(seg)`: the text after the first `<cli> [options] baseline` in `seg`.
    /// A start inside an option chain reaches the same end as the chain, so the scan resumes
    /// at the chain's last char rather than at each start (#155).
    fn baseline_call<'s>(&self, seg: &'s str) -> Option<&'s str> {
        let mut from = 0;
        while let Some(cli) = self.cli.find_at(seg, from) {
            // The sticky match reads only from `cli`'s end, so a miss costs a few bytes.
            let after = seg.get(cli.end()..).unwrap_or("");
            if after.starts_with("baseline")
                && let Some(call) = self.rest.captures(after)
            {
                return call.get(1).map(|rest| rest.as_str());
            }
            let after_start = next_char(seg, cli.start());
            let last_char = prev_char(seg, cli.end());
            from = after_start.max(last_char);
        }
        None
    }

    /// 1.x `baselineRaise(cmd)`: `temper baseline --init` or `--update`, through any spelling
    /// of the CLI; quotes do not hide a flag.
    pub(super) fn baseline_raise(&self, cmd: &str) -> Option<RaiseFlag> {
        self.unquoted_segments(cmd).iter().find_map(|seg| {
            let rest = self.baseline_call(seg)?;
            let flag = self.raise_flag.captures(rest)?;
            match flag.get(1)?.as_str() {
                "init" => Some(RaiseFlag::Init),
                _ => Some(RaiseFlag::Update),
            }
        })
    }

    /// 1.x `claudeRestore(cmd)`: the trimmed segment of a git command that puts an older
    /// `.claude/` back: checkout, restore or stash of a `.claude` path from a revision other
    /// than `HEAD` or `@`, or `git show ... >` into `.claude/`.
    pub(super) fn claude_restore(&self, cmd: &str) -> Option<String> {
        self.unquoted_segments(cmd).into_iter().find_map(|seg| {
            let restores = self.restore.captures(&seg).is_some_and(|m| {
                let (verb, args) = (
                    m.get(1).map_or("", |g| g.as_str()),
                    m.get(2).map_or("", |g| g.as_str()),
                );
                self.claude_path.is_match(args)
                    && match source(verb, args) {
                        Source::Index => false,
                        Source::Undefined => true,
                        Source::Rev(rev) => rev != "HEAD" && rev != "@",
                    }
            });
            (restores || self.show_into_claude.is_match(&seg)).then(|| js_trim(&seg).to_owned())
        })
    }
}

/// Where a checkout, restore or stash takes its files from, as 1.x reads its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source<'a> {
    /// No revision: the index (1.x `null`).
    Index,
    /// `-s` as the last argument: 1.x reads `args[s + 1]`, which is `undefined`.
    Undefined,
    /// A revision, or `stash`.
    Rev(&'a str),
}

/// 1.x `claudeRestore`'s source of `verb` with the arguments `args`.
fn source<'a>(verb: &str, args: &'a str) -> Source<'a> {
    let args: Vec<&str> = js_trim(args)
        .split(is_js_space)
        .filter(|a| !a.is_empty())
        .collect();
    match verb {
        "checkout" => {
            let before = match args.iter().position(|a| *a == "--") {
                Some(dd) => args.get(..dd),
                None => args.get(..args.len().saturating_sub(1)),
            };
            before
                .unwrap_or(&[])
                .iter()
                .find(|a| !a.starts_with('-'))
                .map_or(Source::Index, |rev| Source::Rev(rev))
        }
        "restore" => {
            let short = |a: &str| a.strip_prefix("-s").is_some_and(|v| !v.is_empty());
            let Some(s) = args.iter().position(|a| {
                *a == "-s" || *a == "--source" || a.starts_with("--source=") || short(a)
            }) else {
                return Source::Index;
            };
            let flag = args.get(s).copied().unwrap_or("");
            if let Some((_, rev)) = flag.split_once('=') {
                Source::Rev(rev)
            } else if short(flag) {
                Source::Rev(flag.get(2..).unwrap_or(""))
            } else {
                args.get(s.saturating_add(1))
                    .map_or(Source::Undefined, |rev| Source::Rev(rev))
            }
        }
        _ => Source::Rev("stash"),
    }
}

/// The byte offset of the char after the one at `at`.
fn next_char(s: &str, at: usize) -> usize {
    s.get(at..)
        .and_then(|rest| rest.chars().next())
        .map_or(s.len(), |c| at.saturating_add(c.len_utf8()))
}

/// The byte offset of the char before `at`.
fn prev_char(s: &str, at: usize) -> usize {
    s.get(..at)
        .and_then(|head| head.char_indices().next_back())
        .map_or(0, |(i, _)| i)
}
