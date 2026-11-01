//! The rule table, in 1.x order. Patterns are 1.x's JS regex sources verbatim, translated when
//! `Scanner::new` compiles them, so each entry diffs line by line against `scan.mjs`.

use super::scanner::CodeKind;

/// The severity a table entry gives a context. A table entry never asks; only an
/// `#[expect(..)]` turns a hard hit into an exception request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextSeverity {
    Hard,
    Soft,
}

/// A rule's severity in each code context; `None` ignores the match there.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Contexts {
    pub(crate) prod: Option<ContextSeverity>,
    pub(crate) bin: Option<ContextSeverity>,
    pub(crate) example: Option<ContextSeverity>,
    pub(crate) test: Option<ContextSeverity>,
}

impl Contexts {
    /// The severity for code of `kind`.
    pub(crate) const fn get(self, kind: CodeKind) -> Option<ContextSeverity> {
        match kind {
            CodeKind::Prod => self.prod,
            CodeKind::Bin => self.bin,
            CodeKind::Example => self.example,
            CodeKind::Test => self.test,
        }
    }
}

/// How a rule finds its matches.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Matcher {
    /// Every match of a 1.x JS regex source over the masked code.
    Regex(&'static str),
    /// A hand-written finder.
    Find(FinderKind),
}

/// The hand-written finders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinderKind {
    /// `#[expect(..)]` attributes, with or without a reason.
    ExpectAttrs,
}

/// One rule of the table.
#[derive(Debug)]
pub(crate) struct RuleSpec {
    pub(crate) key: &'static str,
    pub(crate) id: &'static str,
    pub(crate) contexts: Contexts,
    pub(crate) msg: &'static str,
    pub(crate) fix: &'static str,
    /// The fix text for build scripts, when it differs.
    pub(crate) fix_build: Option<&'static str>,
    /// The lints whose `#[expect(.., reason = ..)]` turns a hard hit into an exception request.
    pub(crate) lints: &'static [&'static str],
    pub(crate) matcher: Matcher,
}

const HARD: Option<ContextSeverity> = Some(ContextSeverity::Hard);
const SOFT: Option<ContextSeverity> = Some(ContextSeverity::Soft);
const OFF: Option<ContextSeverity> = None;

/// The rules, in 1.x table order. Phase 1a ports these six; 1b and 1c add the rest in place.
pub(crate) const RULES: &[RuleSpec] = &[
    RuleSpec {
        key: "unsafe",
        id: "H-01",
        contexts: Contexts {
            prod: HARD,
            bin: HARD,
            example: HARD,
            test: HARD,
        },
        msg: "`unsafe` is forbidden in this workspace",
        fix: "Find a safe API or restructure. A genuine need (FFI) is a human decision: isolate it in a -sys crate and change the lint config with approval.",
        fix_build: None,
        lints: &[],
        matcher: Matcher::Regex(r"\bunsafe\b"),
    },
    RuleSpec {
        key: "unwrap",
        id: "H-02",
        contexts: Contexts {
            prod: HARD,
            bin: HARD,
            example: HARD,
            test: OFF,
        },
        msg: "unwrap/expect in production code",
        fix: "Propagate with `?`, convert with `ok_or_else`/`let ... else`, or make the invariant a type. See references/errors.md \"Eliminating unwrap/expect\".",
        fix_build: None,
        lints: &["clippy::unwrap_used", "clippy::expect_used"],
        matcher: Matcher::Regex(
            r#"\.\s*(?:unwrap\s*\(\s*\)|unwrap_err\s*\(\s*\)|unwrap_unchecked\s*\(|expect\s*\(\s*"|expect_err\s*\(\s*")|\b(?:Option|Result)\s*::\s*(?:unwrap|expect|unwrap_unchecked|unwrap_err|expect_err)\b"#,
        ),
    },
    RuleSpec {
        key: "panic",
        id: "H-03",
        contexts: Contexts {
            prod: HARD,
            bin: HARD,
            example: HARD,
            test: OFF,
        },
        msg: "panicking macro in production code",
        fix: "Return an error, or restructure types so the case cannot occur (exhaustive match, newtype, NonZero, typestate). Unfinished work is `Status: WIP`, never `todo!()`.",
        fix_build: None,
        lints: &[
            "clippy::panic",
            "clippy::todo",
            "clippy::unimplemented",
            "clippy::unreachable",
        ],
        matcher: Matcher::Regex(
            r"\b(?:panic|todo|unimplemented|unreachable)\s*!|\bprocess\s*::\s*abort\s*\(",
        ),
    },
    RuleSpec {
        key: "dbg",
        id: "H-06",
        contexts: Contexts {
            prod: HARD,
            bin: HARD,
            example: HARD,
            test: HARD,
        },
        msg: "`dbg!` left in code",
        fix: "Remove it. Use `tracing::debug!` with structured fields for diagnostics that should stay.",
        fix_build: None,
        lints: &["clippy::dbg_macro"],
        matcher: Matcher::Regex(r"\bdbg\s*!"),
    },
    RuleSpec {
        key: "print",
        id: "H-06",
        contexts: Contexts {
            prod: HARD,
            bin: HARD,
            example: SOFT,
            test: OFF,
        },
        msg: "print macro in production code",
        fix: "Libraries: use `tracing`. Binaries: write through a locked handle, `writeln!(io::stdout().lock(), ...)?`. It returns an error on a closed pipe where `println!` would panic.",
        fix_build: Some(
            "Build scripts emit directives with `writeln!(io::stdout().lock(), \"cargo::rerun-if-changed=..\")?` from a `main` that returns `Result` (ARC-17, H-06).",
        ),
        lints: &["clippy::print_stdout", "clippy::print_stderr"],
        matcher: Matcher::Regex(r"\be?print(?:ln)?\s*!"),
    },
    RuleSpec {
        key: "expect-attr",
        id: "H-05",
        contexts: Contexts {
            prod: HARD,
            bin: HARD,
            example: HARD,
            test: HARD,
        },
        msg: "`#[expect]` lint exception",
        fix: "Exception requests need human approval (H-05).",
        fix_build: None,
        lints: &[],
        matcher: Matcher::Find(FinderKind::ExpectAttrs),
    },
];
