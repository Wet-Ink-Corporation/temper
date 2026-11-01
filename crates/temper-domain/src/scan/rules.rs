//! The rule table's types. The data is in `rules/table.rs`, one entry per rule in 1.x order, so
//! the table diffs entry by entry against `scan.mjs`.

mod table;

pub(crate) use table::RULES;

use super::predicates::{Keep, Look};
use super::scanner::CodeKind;
use crate::js::JsFlags;

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

/// A context row in 1.x key order: prod, bin, example, test.
const fn ctx(
    prod: Option<ContextSeverity>,
    bin: Option<ContextSeverity>,
    example: Option<ContextSeverity>,
    test: Option<ContextSeverity>,
) -> Contexts {
    Contexts {
        prod,
        bin,
        example,
        test,
    }
}

const HARD: Option<ContextSeverity> = Some(ContextSeverity::Hard);
const SOFT: Option<ContextSeverity> = Some(ContextSeverity::Soft);
const OFF: Option<ContextSeverity> = None;

/// A 1.x regex rule: the JS source and flags, compiled by `Scanner::new`, and the predicate a
/// match must pass.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pattern {
    pub(crate) source: &'static str,
    pub(crate) flags: JsFlags,
    pub(crate) look: Option<Look>,
    pub(crate) keep: Option<Keep>,
}

/// A pattern with no flag and no predicate.
const fn re(source: &'static str) -> Pattern {
    Pattern {
        source,
        flags: JsFlags::Plain,
        look: None,
        keep: None,
    }
}

impl Pattern {
    /// The pattern with JS flags other than `g`.
    const fn flags(self, flags: JsFlags) -> Self {
        Self { flags, ..self }
    }

    /// The pattern with the rewrite of the look-around its 1.x source had.
    const fn look(self, look: Look) -> Self {
        Self {
            look: Some(look),
            ..self
        }
    }

    /// The pattern with 1.x's `keep` predicate.
    const fn keep(self, keep: Keep) -> Self {
        Self {
            keep: Some(keep),
            ..self
        }
    }
}

/// How a rule finds its matches.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Matcher {
    /// Every match of a 1.x JS regex over the masked code.
    Regex(Pattern),
    /// A hand-written finder.
    Find(FinderKind),
}

/// The hand-written finders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinderKind {
    /// `if let Err(_) = .. {}` with an empty body and no `else`.
    EmptyIfLetErr,
    /// `spin_loop()`, and atomic swaps or compare-exchanges inside a loop.
    SpinLocks,
    /// `HashMap` and `HashSet` fields of a `Serialize` type.
    SerializedHashMaps,
    /// Test `cfg`s inside production function bodies.
    TestBranches,
    /// Trait bounds on a generic `struct` or `enum` definition.
    StructBounds,
    /// Unwrap, expect and panics in compiled doctest fences.
    DocUnwrap,
    /// `# Arguments`/`# Returns` sections in doc prose.
    DocArgSections,
    /// `#[expect(..)]` attributes, with or without a reason.
    ExpectAttrs,
    /// `#[allow(..)]` and `#[expect(..)]` of a lint the crate forbids.
    ForbidOverrides,
    /// `pub fn` signatures that return `String` errors, `Box<dyn Error>`, `anyhow` or `eyre`.
    ErasedErrors,
    /// `TODO`-style comments without a tracking reference.
    TodoComments,
    /// Comments that read as code.
    CommentedCode,
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
