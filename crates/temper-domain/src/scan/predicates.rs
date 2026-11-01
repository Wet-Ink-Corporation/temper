//! The predicates a regex match must pass: 1.x's `keep` functions, and the rewrites of its JS
//! look-arounds, which `regex` does not support (port plan §1.6, D9).

use regex::Regex;

use super::counts::top_level_cfg_attr_arg;
use super::env::Env;
use super::regions::RegionPatterns;
use super::scanner::{ScannerSetupError, compile};
use crate::js::is_js_space;

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
}

/// A rewritten JS look-around: when it does not hold, nothing matches at this start, and the
/// search resumes one char later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Look {
    /// `extern-crate`'s `(?!(?:alloc|std)\s*;|self\s+as\b)` after `crate\s+`, with JS's
    /// backtracking: when `\s+` took two or more spaces, JS gives one back and the look-ahead
    /// then sees a space, so it holds.
    NeededExternCrate,
}

/// The compiled patterns the predicates use.
#[derive(Debug)]
pub(crate) struct PredicatePatterns {
    extern_exempt: Regex,
    /// Also the scanner's test-region patterns: one owner per compiled pattern.
    pub(crate) regions: RegionPatterns,
}

impl PredicatePatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            extern_exempt: compile("look:extern-exempt", r"^(?:(?:alloc|std)\s*;|self\s+as\b)")?,
            regions: RegionPatterns::new()?,
        })
    }
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
        }
    }
}
