//! Shell commands that write a file of a given name (1.x `hooks.mjs` `shellWrites` and
//! `fromFirst`).
//!
//! 1.x appends `(?![\w.])` to each target. For a yes-or-no test, a look-ahead at the end of a
//! pattern is the same as consuming one char of its complement or the end, so it becomes
//! `(?:[^\w.]|$)`; where a quote, a space or a terminator must follow anyway, it is dropped.
//! `fromFirst`'s sticky `rest` with a leading look-ahead becomes two searches of the segment
//! after the key, since a segment holds no separator for `[^|;&]*` to stop at.

use regex::Regex;

use crate::hooks::setup::{PolicySetupError, compile};
use crate::js::JsFlags;

/// A file name `shellWrites` looks for.
#[derive(Debug, Clone, Copy)]
pub(super) struct Target {
    name: &'static str,
    /// 1.x `${target}(?![\w.])`, as a search.
    ends: &'static str,
    /// 1.x `${target}` where a quote, `,` or a terminator follows.
    bare: &'static str,
    /// Whether the destination of `cp`, `mv` and `install` counts (1.x `{ copies: true }`).
    copies: bool,
}

/// `.rs` (1.x `SHELL_WRITE_RS`).
pub(super) const RUST_SOURCE: Target = Target {
    name: "shell-write:rs",
    ends: r"\.rs(?:[^\w.]|$)",
    bare: r"\.rs",
    copies: false,
};
/// 1.x `SHELL_WRITE_LOCK`.
pub(super) const LOCKFILE: Target = Target {
    name: "shell-write:lock",
    ends: r"Cargo\.lock(?:[^\w.]|$)",
    bare: r"Cargo\.lock",
    copies: true,
};
/// 1.x `SHELL_WRITE_TOML`.
pub(super) const MANIFEST: Target = Target {
    name: "shell-write:toml",
    ends: r"Cargo\.toml(?:[^\w.]|$)",
    bare: r"Cargo\.toml",
    copies: true,
};
/// 1.x `SHELL_WRITE_BASELINE`.
pub(super) const BASELINE: Target = Target {
    name: "shell-write:baseline",
    ends: r"temper-baseline\.json(?:[^\w.]|$)",
    bare: r"temper-baseline\.json",
    copies: true,
};
/// 1.x `SHELL_WRITE_VENDORED`: `\.claude/temper/(?:(?!\.claude/temper/)[\w./-])*(?![\w.])`.
/// Every pattern reaches it through a run that holds `[\w./-]`, so its last `.claude/temper/`
/// in reach always matches: the run after it ends before a char outside `[\w./-]`. So the
/// target is `.claude/temper/` alone, and `[\w./-]*` after it where a quote must follow.
pub(super) const VENDORED: Target = Target {
    name: "shell-write:vendored",
    ends: r"\.claude/temper/",
    bare: r"\.claude/temper/[\w./-]*",
    copies: true,
};

/// 1.x `fromFirst(key, rest, { sep })`: in each segment, every one of `rest` matches after the
/// segment's first `key`.
#[derive(Debug)]
struct FromFirst {
    key: Regex,
    sep: &'static [char],
    rest: Vec<Regex>,
}

impl FromFirst {
    fn test(&self, text: &str) -> bool {
        text.split(self.sep).any(|seg| {
            self.key.find(seg).is_some_and(|key| {
                let after = seg.get(key.end()..).unwrap_or("");
                self.rest.iter().all(|re| re.is_match(after))
            })
        })
    }
}

/// The compiled patterns of one [`Target`].
#[derive(Debug)]
pub(super) struct ShellWrites {
    anywhere: Vec<Regex>,
    from_first: Vec<FromFirst>,
}

/// 1.x's default `fromFirst` separator, `/[|;&]/`.
const SEGMENT: &[char] = &['|', ';', '&'];

impl ShellWrites {
    /// Compiles `target`'s patterns.
    pub(super) fn new(target: Target) -> Result<Self, PolicySetupError> {
        let Target {
            name, ends, bare, ..
        } = target;
        let re = |source: &str| compile(name, source, JsFlags::Plain);
        let keyed = |key: &str, sep, rest: &[String]| -> Result<FromFirst, PolicySetupError> {
            Ok(FromFirst {
                key: re(key)?,
                sep,
                rest: rest.iter().map(|r| re(r)).collect::<Result<_, _>>()?,
            })
        };
        let in_place = r"\s-[a-zA-Z]*i".to_owned();
        let anywhere = vec![
            re(&format!(
                r#"(?:&>>?|(?:^|[^>&0-9])>>?\|?)\s*["']?[^\s"'|;&<>]*{ends}"#
            ))?,
            re(&format!(r#"\btee\s+(?:-a\s+)?["']?[^\s"'|;&]*{ends}"#))?,
        ];
        let mut from_first = vec![
            // 1.x `(?=[^|;&]*\s-[a-zA-Z]*i)[^|;&]*${t}` after `\bsed\b`.
            keyed(r"\bsed\b", SEGMENT, &[in_place.clone(), ends.to_owned()])?,
            keyed(
                r"\bsed\b",
                SEGMENT,
                &["--in-place".to_owned(), ends.to_owned()],
            )?,
            keyed(r"\bperl\b", SEGMENT, &[in_place, ends.to_owned()])?,
            // 1.x `[^)]*${target}["']\s*,\s*["'][wa+]`, segments split on `)`.
            keyed(
                r"\bopen\s*\(",
                &[')'],
                &[format!(r#"{bare}["']\s*,\s*["'][wa+]"#)],
            )?,
            keyed(
                r"\b(?:write_text|writeFileSync|Set-Content|Out-File|Add-Content)\b",
                SEGMENT,
                &[ends.to_owned()],
            )?,
        ];
        if target.copies {
            // 1.x `[^|;&]*\s["']?[^\s"'|;&<>]*${t}["']?\s*(?:$|[|;&)])` under `m`: `$` also ends
            // a line before a JS line terminator, which is consumed here instead.
            from_first.push(keyed(
                r"\b(?:cp|mv|install|Copy-Item|Move-Item)\b",
                SEGMENT,
                &[format!(
                    r#"\s["']?[^\s"'|;&<>]*{bare}["']?\s*(?:[\n\r  )]|$)"#
                )],
            )?);
        }
        Ok(Self {
            anywhere,
            from_first,
        })
    }

    /// Does `cmd` write the target (1.x `SHELL_WRITE_*.some((re) => re.test(cmd))`)?
    pub(super) fn test(&self, cmd: &str) -> bool {
        self.anywhere.iter().any(|re| re.is_match(cmd))
            || self.from_first.iter().any(|f| f.test(cmd))
    }
}
