//! The predicates a regex match must pass: 1.x's `keep` functions, and the rewrites of its JS
//! look-arounds, which `regex` does not support (port plan §1.6, D9).

use regex::Regex;

use super::counts::top_level_cfg_attr_arg;
use super::env::Env;
use super::hit::LineNo;
use super::lexer::{match_brace, match_pair, slice, statement_end};
use super::regions::RegionPatterns;
use super::scanner::{ScannerSetupError, compile, compile_with};
use crate::js::{JsFlags, is_js_space, js_trim, utf16_window_start};

/// A regex match as the predicates see it: its start and its text.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Candidate<'c> {
    pub(crate) start: usize,
    pub(crate) text: &'c str,
}

impl Candidate<'_> {
    /// The offset after the match.
    fn end(self) -> usize {
        self.start.saturating_add(self.text.len())
    }
}

/// 1.x `keep`: when it does not hold, the match is dropped and the search resumes after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keep {
    /// `assert-in-prod`: not in a `const` item or block.
    OutsideConst,
    /// `ignore-no-reason`: `ignore` is a top-level `cfg_attr` argument. The pattern consumes the
    /// char 1.x's look-ahead `(?=[\],)])` tests, so it is dropped first.
    TopLevelIgnore,
    /// `compiler-fence`: std's fence, by an `atomic::` path in the 40 UTF-16 units before the
    /// match or an import from `std` or `core::sync::atomic` anywhere in the file.
    StdFence,
    /// `global-allocator`: the file has no top-level `fn main` (a binary root).
    NoMainFn,
    /// `map-index`: what is indexed is not a keyword (`in[&k]` is a `for` loop's array).
    NotKeyword,
    /// `seqcst`: no comment on the same or the previous line names the order, and the match is
    /// not in a `use` list (one starting within the 300 UTF-16 units before it).
    UnjustifiedSeqCst,
    /// `pub-glob-reexport`: no `#[cfg(..)]` among the attribute lines directly above (a glob of
    /// `cfg`-selected platform modules).
    NoCfgAbove,
    /// `ops-impl`: the body, from the first `{` after the match (which may be a later item's,
    /// as in 1.x), has no `checked_`, `saturating_`, `wrapping_` or `overflowing_` call.
    UncheckedOps,
    /// `ambient-now`, `get-prefix`: not in a trait impl body, whose names the trait fixes and
    /// whose clock adapter may read the clock.
    OutsideTraitImpl,
    /// `detached-spawn`: the spawn is a statement ending in `;` (not a returned tail expression)
    /// whose handle is not joined or awaited in it.
    DetachedStatement,
}

/// The Rust keywords 1.x's `map-index` keep skips, in its order.
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "dyn", "else", "for", "if", "impl", "in",
    "let", "loop", "match", "move", "mut", "ref", "return", "static", "unsafe", "where", "while",
    "yield",
];

/// A rewritten JS look-around: when it does not hold, nothing matches at this start, and the
/// search resumes one char later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Look {
    /// `extern-crate`'s `(?!(?:alloc|std)\s*;|self\s+as\b)` after `crate\s+`, with JS's
    /// backtracking: when `\s+` took two or more spaces, JS gives one back and the look-ahead
    /// then sees a space, so it holds.
    NeededExternCrate,
    /// `compiler-fence`'s `(?<![.\w]\s*|\bfn\s+)`: before the match and its JS spaces is neither
    /// `.` nor a word char. The `fn` alternative adds nothing, since `n` is a word char.
    NotMember,
    /// `unwrap-or-default`'s `(?<!\bparse\s*(?:::\s*<[^>]*>)?\s*\(\s*\)\s*)`: the code before the
    /// match does not end with a `parse()` call. The end-anchored test runs backwards from the
    /// match (`regex`'s reverse-anchored strategy), not over the whole prefix.
    NotAfterParseCall,
    /// `get-prefix`'s `get_(?!(?:mut|ref|pin_mut|unchecked(?:_mut)?)\b)`: the identifier after
    /// `get_` is none of those names. The pattern's `\w+` always takes the whole identifier.
    NotReservedGetter,
}

/// The getter names after `get_` that `get-prefix` allows.
const RESERVED_GETTERS: &[&str] = &["mut", "ref", "pin_mut", "unchecked", "unchecked_mut"];

/// The compiled patterns the predicates use.
#[derive(Debug)]
pub(crate) struct PredicatePatterns {
    fence_path: Regex,
    fence_use: Regex,
    fn_main: Regex,
    cfg_line: Regex,
    seqcst_note: Regex,
    use_tail: Regex,
    join_await: Regex,
    checked_ops: Regex,
    parse_tail: Regex,
    extern_exempt: Regex,
    /// Also the scanner's test-region patterns: one owner per compiled pattern.
    pub(crate) regions: RegionPatterns,
}

impl PredicatePatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            fence_path: compile("keep:fence-path", r"\batomic\s*::\s*$")?,
            fence_use: compile(
                "keep:fence-use",
                r"\buse\s+(?:std|core)\s*::\s*sync\s*::\s*atomic\s*::\s*(?:compiler_fence\b|\{[^}]*\bcompiler_fence\b)",
            )?,
            fn_main: compile_with(
                "keep:fn-main",
                r"^(?:pub\s+)?fn\s+main\s*\(",
                JsFlags::Multiline,
            )?,
            cfg_line: compile("keep:cfg-line", r"^#\s*\[\s*cfg\s*\(")?,
            seqcst_note: compile_with(
                "keep:seqcst-note",
                r"\bSeqCst\b|\b(?:total|single|global)\s+order",
                JsFlags::IgnoreCase,
            )?,
            use_tail: compile("keep:use-tail", r"\buse\s+[\w\s:{},*]*$")?,
            join_await: compile("keep:join-await", r"^\s*\??\s*\.\s*(?:join|await)\b")?,
            checked_ops: compile(
                "keep:checked-ops",
                r"\b(?:checked|saturating|wrapping|overflowing)_\w+\s*\(",
            )?,
            parse_tail: compile(
                "look:parse-tail",
                r"\bparse\s*(?:::\s*<[^>]*>)?\s*\(\s*\)\s*$",
            )?,
            extern_exempt: compile("look:extern-exempt", r"^(?:(?:alloc|std)\s*;|self\s+as\b)")?,
            regions: RegionPatterns::new()?,
        })
    }
}

/// Does a comment that starts on or before the line of `offset`, and ends on that line or the
/// one before, name the order `SeqCst` gives? The lines are tested before the regex.
/// Walking up from the line above `offset`'s, over lines that start with `#` once trimmed, is
/// one of them a `#[cfg(..)]` (1.x `cfgAttrAbove`)? The first other line, or the first line of
/// the file, ends the walk.
fn cfg_attr_above(env: &Env<'_>, cfg_line: &Regex, offset: usize) -> bool {
    // 0-based indices of the lines above, nearest first.
    let above = env.idx.line_of(offset).get().saturating_sub(1);
    for index in (0..above).rev() {
        let text = js_trim(env.idx.line_text(LineNo::from_index(index), env.code));
        if !text.starts_with('#') {
            return false;
        }
        if cfg_line.is_match(text) {
            return true;
        }
    }
    false
}

fn seqcst_justified(env: &Env<'_>, p: &PredicatePatterns, offset: usize) -> bool {
    let line = env.idx.line_of(offset);
    let previous = line.saturating_sub(1);
    env.comments.iter().any(|c| {
        let last = c.start.max(c.end.saturating_sub(1));
        env.idx.line_of(c.start) <= line
            && env.idx.line_of(last) >= previous
            && p.seqcst_note.is_match(c.text)
    })
}

/// The JS `code.slice(max(0, start - units), start)` before a match: `units` UTF-16 code units,
/// counted over the source, whose units match JS's masked code one for one (decision 1b-4).
fn window_before<'e>(env: &Env<'e>, start: usize, units: usize) -> &'e str {
    slice(env.code, utf16_window_start(env.src, start, units), start)
}

impl Look {
    /// Does the look-around hold at the match?
    pub(crate) fn holds(self, env: &Env<'_>, p: &PredicatePatterns, m: Candidate<'_>) -> bool {
        match self {
            Self::NeededExternCrate => {
                let backtracks = m.text.chars().rev().take(2).all(is_js_space);
                let rest = env.code.get(m.end()..).unwrap_or("");
                backtracks || !p.extern_exempt.is_match(rest)
            }
            Self::NotMember => {
                let before = env.code.get(..m.start).unwrap_or("");
                let prev = before.trim_end_matches(is_js_space).chars().next_back();
                !prev.is_some_and(|c| c == '.' || c.is_ascii_alphanumeric() || c == '_')
            }
            Self::NotAfterParseCall => {
                !p.parse_tail.is_match(env.code.get(..m.start).unwrap_or(""))
            }
            Self::NotReservedGetter => {
                let name = m.text.split_once("get_").map_or("", |(_, rest)| rest);
                let end = name
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(name.len());
                !RESERVED_GETTERS.contains(&name.get(..end).unwrap_or(name))
            }
        }
    }
}

/// `detached-spawn`: is the spawn a `;`-terminated statement that drops its handle? The match
/// ends with the call's `(`.
fn detached_statement(env: &Env<'_>, p: &PredicatePatterns, m: Candidate<'_>) -> bool {
    let code = env.code;
    let end = statement_end(code, m.start);
    if code.as_bytes().get(end) != Some(&b';') {
        return false;
    }
    let close = match_pair(code, m.end().saturating_sub(1), b'(', b')');
    !p.join_await
        .is_match(slice(code, close.saturating_add(1), end))
}

/// `ops-impl`: does the body after the match use checked, saturating, wrapping or overflowing
/// arithmetic? `false` when no `{` follows.
fn unchecked_ops(env: &Env<'_>, p: &PredicatePatterns, start: usize) -> bool {
    let rest = env.code.get(start..).unwrap_or("");
    let Some(open) = rest.find('{').map(|at| start.saturating_add(at)) else {
        return false;
    };
    let body = slice(env.code, open, match_brace(env.code, open));
    !p.checked_ops.is_match(body)
}

impl Keep {
    /// Does the match pass?
    pub(crate) fn holds(self, env: &Env<'_>, p: &PredicatePatterns, m: Candidate<'_>) -> bool {
        match self {
            Self::OutsideConst => !env.consts(&p.regions).contains(m.start),
            Self::TopLevelIgnore => {
                let mut attr = m.text.chars();
                attr.next_back();
                top_level_cfg_attr_arg(attr.as_str())
            }
            Self::StdFence => {
                p.fence_path.is_match(window_before(env, m.start, 40))
                    || p.fence_use.is_match(env.code)
            }
            Self::NoMainFn => !p.fn_main.is_match(env.code),
            Self::UnjustifiedSeqCst => {
                !seqcst_justified(env, p, m.start)
                    && !p.use_tail.is_match(window_before(env, m.start, 300))
            }
            Self::NoCfgAbove => !cfg_attr_above(env, &p.cfg_line, m.start),
            Self::UncheckedOps => unchecked_ops(env, p, m.start),
            Self::OutsideTraitImpl => !env.trait_impls(&p.regions).contains(m.start),
            Self::DetachedStatement => detached_statement(env, p, m),
            Self::NotKeyword => {
                // The text before `[` is the identifier, or the `)` or `]` 1.x captures nothing for.
                let indexed = m.text.split_once('[').map_or(m.text, |(head, _)| head);
                !KEYWORDS.contains(&indexed)
            }
        }
    }
}
