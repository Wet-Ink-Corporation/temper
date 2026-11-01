//! `test-branch`: `cfg!(..test..)` and `#[cfg(..test..)]` inside the body of a production
//! function (1.x `testBranches`). A function in a test region is test code.

use regex::Regex;

use super::Found;
use crate::scan::env::Env;
use crate::scan::hit::Severity;
use crate::scan::lexer::{body_brace, match_brace, match_pair, slice};
use crate::scan::scanner::{CodeKind, ScannerSetupError, compile};

/// The compiled patterns of the finder.
#[derive(Debug)]
pub(crate) struct TestBranchPatterns {
    cfg_word: Regex,
    fn_head: Regex,
    cfg: Regex,
    test_word: Regex,
}

impl TestBranchPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            cfg_word: compile("test-branch:cfg-word", r"\bcfg\b")?,
            fn_head: compile("test-branch:fn", r"\bfn\s+(?:r#)?[A-Za-z_]\w*")?,
            cfg: compile("test-branch:cfg", r"\bcfg\s*!\s*\(|#\s*\[\s*cfg\s*\(")?,
            test_word: compile("test-branch:test-word", r"\btest\b")?,
        })
    }
}

/// A function body: the offsets of its braces.
#[derive(Debug, Clone, Copy)]
struct FnBody {
    open: usize,
    close: usize,
}

/// Pushes every test `cfg` strictly inside a production function body. The severity is the
/// file's, not the match's context, since the `#[cfg(test)]` found is itself a test region: hard
/// outside test files, nothing in them (1.x `TEST_BRANCH_CTX`).
pub(crate) fn find(env: &Env<'_>, p: &TestBranchPatterns, out: &mut Vec<Found>) {
    let code = env.code;
    if env.kind == CodeKind::Test || !p.cfg_word.is_match(code) {
        return;
    }
    let bodies = fn_bodies(env, p);
    if bodies.is_empty() {
        return;
    }
    let found = p.cfg.find_iter(code).filter_map(|m| {
        // The match ends with the `(`.
        let open = m.end().saturating_sub(1);
        let args = slice(
            code,
            open.saturating_add(1),
            match_pair(code, open, b'(', b')'),
        );
        let inside = |b: &FnBody| b.open < m.start() && m.start() < b.close;
        (p.test_word.is_match(args) && bodies.iter().any(inside)).then(|| Found {
            severity: Some(Severity::Hard),
            ..Found::at(m.start())
        })
    });
    out.extend(found);
}

/// The bodies of the functions whose `fn` is outside the test regions (1.x `fnBodies`, filtered).
fn fn_bodies(env: &Env<'_>, p: &TestBranchPatterns) -> Vec<FnBody> {
    let code = env.code;
    p.fn_head
        .find_iter(code)
        .filter(|m| !env.regions.contains(m.start()))
        .filter_map(|m| {
            let open = body_brace(code, m.end())?;
            Some(FnBody {
                open,
                close: match_brace(code, open),
            })
        })
        .collect()
}
