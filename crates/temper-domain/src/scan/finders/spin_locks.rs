//! `spin-lock`: `spin_loop()`, and an atomic `swap(true, ..)` or `compare_exchange*` inside a
//! loop, its condition included; one hit per line (1.x `spinLocks`).

use regex::Regex;

use super::Found;
use crate::scan::env::Env;
use crate::scan::hit::LineNo;
use crate::scan::lexer::{body_brace, match_brace, slice};
use crate::scan::regions::Regions;
use crate::scan::scanner::{ScannerSetupError, compile};

/// The compiled patterns of the finder.
#[derive(Debug)]
pub(crate) struct SpinPatterns {
    loop_kw: Regex,
    in_kw: Regex,
    op: Regex,
}

impl SpinPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            loop_kw: compile("spin:loop", r"\b(?:loop|while|for)\b")?,
            in_kw: compile("spin:in", r"\bin\b")?,
            op: compile(
                "spin:op",
                r"\bspin_loop\s*\(|\.\s*(?:swap\s*\(\s*true\b|compare_exchange(?:_weak)?\s*\()",
            )?,
        })
    }
}

/// Pushes the first spin operation on each line: every `spin_loop(`, and every swap or
/// compare-exchange inside a loop.
pub(crate) fn find(env: &Env<'_>, p: &SpinPatterns, out: &mut Vec<Found>) {
    let code = env.code;
    let loops = loop_regions(code, p);
    // Matches come in offset order, so equal lines are adjacent.
    let mut last_line: Option<LineNo> = None;
    for m in p.op.find_iter(code) {
        if !m.as_str().starts_with("spin") && !loops.contains(m.start()) {
            continue;
        }
        let line = env.idx.line_of(m.start());
        if last_line != Some(line) {
            last_line = Some(line);
            out.push(Found::at(m.start()));
        }
    }
}

/// Each loop, from its keyword to the brace closing its body. `impl Trait for T {` and
/// `for<'a>` are not loops: a `for` loop has `in` before its body.
fn loop_regions(code: &str, p: &SpinPatterns) -> Regions {
    p.loop_kw
        .find_iter(code)
        .filter_map(|m| {
            let open = body_brace(code, m.end())?;
            let not_a_loop = m.as_str() == "for" && !p.in_kw.is_match(slice(code, m.end(), open));
            (!not_a_loop).then(|| m.start()..=match_brace(code, open))
        })
        .collect()
}
