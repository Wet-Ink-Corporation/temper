//! The predicates a regex match must pass: 1.x's `keep` functions, and the rewrites of its JS
//! look-arounds, which `regex` does not support (port plan §1.6, D9).

use regex::Regex;

use super::counts::top_level_cfg_attr_arg;
use super::env::Env;
use super::lexer::slice;
use super::regions::RegionPatterns;
use super::scanner::{ScannerSetupError, compile, compile_with};
use crate::js::{JsFlags, is_js_space, utf16_window_start};

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
}

/// The compiled patterns the predicates use.
#[derive(Debug)]
pub(crate) struct PredicatePatterns {
    fence_path: Regex,
    fence_use: Regex,
    fn_main: Regex,
    seqcst_note: Regex,
    use_tail: Regex,
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
            seqcst_note: compile_with(
                "keep:seqcst-note",
                r"\bSeqCst\b|\b(?:total|single|global)\s+order",
                JsFlags::IgnoreCase,
            )?,
            use_tail: compile("keep:use-tail", r"\buse\s+[\w\s:{},*]*$")?,
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
        }
    }
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
            Self::NotKeyword => {
                // The text before `[` is the identifier, or the `)` or `]` 1.x captures nothing for.
                let indexed = m.text.split_once('[').map_or(m.text, |(head, _)| head);
                !KEYWORDS.contains(&indexed)
            }
        }
    }
}
