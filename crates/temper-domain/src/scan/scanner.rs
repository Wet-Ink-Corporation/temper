//! The scanner: compiles the rule table once and runs it over a source (1.x `scanSource`).

use std::borrow::Cow;

use regex::Regex;

use super::counts::{CountPatterns, count_clones, count_ignored, count_tests};
use super::finders::Found;
use super::finders::expect_attrs::{self, ExpectPatterns, exception_for};
use super::hit::{Hit, RuleId, RuleKey, Severity};
use super::lexer::{LineIndex, mask_rust};
use super::regions::{RegionPatterns, Regions, test_regions};
use super::rules::{ContextSeverity, FinderKind, Matcher, RULES, RuleSpec};
use crate::js;

/// The most UTF-16 code units of source line a hit's text keeps, as 1.x.
const HIT_TEXT_UNITS: usize = 160;

/// Where a file's code runs, which picks each rule's severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeKind {
    /// Production code in a library crate.
    Prod,
    /// Production code in a binary or build script.
    Bin,
    /// `examples/` and `benches/`.
    Example,
    /// `tests/` and test files; test regions in other files count as this too.
    Test,
}

/// Whether the file is a build script, which only changes the fix texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileRole {
    /// Any other source file.
    Source,
    /// `build.rs` at the crate root (see [`is_build_script`]).
    BuildScript,
}

/// Options for one scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    /// The context of the file's code.
    pub kind: CodeKind,
    /// Whether the file is a build script.
    pub file: FileRole,
}

/// Is the crate-relative path `rel_to_crate` the crate's build script (`build.rs` at its root)?
///
/// 1.x compares after turning `\` into `/`; `build.rs` has no separator, so that is plain
/// equality.
#[must_use]
pub fn is_build_script(rel_to_crate: &str) -> bool {
    rel_to_crate == "build.rs"
}

/// A built-in scanner pattern did not compile. This is a bug in temper; a unit test guards it.
#[derive(Debug, thiserror::Error)]
#[error("scanner pattern `{rule}` does not compile")]
pub struct ScannerSetupError {
    rule: &'static str,
    #[source]
    source: regex::Error,
}

impl ScannerSetupError {
    /// The rule key, or the lexer pattern name such as `regions:attr`, whose pattern failed.
    #[must_use]
    pub fn rule(&self) -> &'static str {
        self.rule
    }
}

/// Compiles the 1.x JS regex source `js_source`, named `rule` in the error.
pub(crate) fn compile(rule: &'static str, js_source: &str) -> Result<Regex, ScannerSetupError> {
    Regex::new(&js::translate(js_source)).map_err(|source| ScannerSetupError { rule, source })
}

/// The escape-hatch scanner: every rule's pattern, compiled once.
///
/// # Examples
///
/// ```
/// use temper_domain::{CodeKind, FileRole, ScanOptions, Scanner, Severity};
///
/// let scanner = Scanner::new()?;
/// let options = ScanOptions { kind: CodeKind::Prod, file: FileRole::Source };
/// let report = scanner.scan("fn f() { dbg!(1); }", &options);
///
/// let hit = report.hits().first().map(|h| (h.key().as_str(), h.severity()));
/// assert_eq!(hit, Some(("dbg", Severity::Hard)));
/// # Ok::<(), temper_domain::ScannerSetupError>(())
/// ```
#[derive(Debug)]
pub struct Scanner {
    rules: Vec<CompiledRule>,
    regions: RegionPatterns,
    counts: CountPatterns,
    expect: ExpectPatterns,
}

#[derive(Debug)]
struct CompiledRule {
    spec: &'static RuleSpec,
    matcher: CompiledMatcher,
}

#[derive(Debug)]
enum CompiledMatcher {
    Regex(Regex),
    Find(FinderKind),
}

impl Scanner {
    /// Compiles every rule and lexer pattern.
    ///
    /// # Errors
    ///
    /// [`ScannerSetupError`] when a built-in pattern does not compile, which is a bug in temper.
    pub fn new() -> Result<Self, ScannerSetupError> {
        let rules = RULES
            .iter()
            .map(|spec| {
                let matcher = match spec.matcher {
                    Matcher::Regex(source) => CompiledMatcher::Regex(compile(spec.key, source)?),
                    Matcher::Find(kind) => CompiledMatcher::Find(kind),
                };
                Ok(CompiledRule { spec, matcher })
            })
            .collect::<Result<_, ScannerSetupError>>()?;
        Ok(Self {
            rules,
            regions: RegionPatterns::new()?,
            counts: CountPatterns::new()?,
            expect: ExpectPatterns::new()?,
        })
    }

    /// Scans one Rust source. Hits come in rule-table order, then in match order.
    ///
    /// # Examples
    ///
    /// ```
    /// use temper_domain::{CodeKind, FileRole, ScanOptions, Scanner};
    ///
    /// let scanner = Scanner::new()?;
    /// let src = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n";
    /// let report = scanner.scan(src, &ScanOptions { kind: CodeKind::Prod, file: FileRole::Source });
    ///
    /// assert_eq!((report.hits().len(), report.tests()), (0, 1));
    /// # Ok::<(), temper_domain::ScannerSetupError>(())
    /// ```
    #[must_use]
    pub fn scan(&self, src: &str, options: &ScanOptions) -> ScanReport {
        let code = mask_rust(src);
        let idx = LineIndex::new(&code);
        let regions = test_regions(&code, &self.regions);
        let env = Env {
            src,
            code: &code,
            idx: &idx,
            kind: options.kind,
            file: options.file,
            regions: &regions,
        };
        let mut hits = Vec::new();
        for rule in &self.rules {
            let found = self.matches(rule, &env);
            hits.extend(
                found
                    .into_iter()
                    .filter_map(|f| self.hit(rule.spec, &env, f)),
            );
        }
        ScanReport {
            hits,
            tests: count_tests(&code, &self.counts),
            ignores: count_ignored(&code, &self.counts),
            clones: count_clones(&code, &self.counts),
        }
    }

    /// The matches of one rule.
    fn matches(&self, rule: &CompiledRule, env: &Env<'_>) -> Vec<Found> {
        match &rule.matcher {
            CompiledMatcher::Regex(re) => re
                .find_iter(env.code)
                .filter(|m| !m.is_empty())
                .map(|m| Found::at(m.start()))
                .collect(),
            CompiledMatcher::Find(FinderKind::ExpectAttrs) => expect_attrs::find(env, &self.expect),
        }
    }

    /// The hit for one match, or `None` when the rule ignores the match's context.
    fn hit(&self, spec: &'static RuleSpec, env: &Env<'_>, found: Found) -> Option<Hit> {
        let (severity, exception) = match found.severity {
            Some(forced) => (forced, found.exception),
            None => match spec.contexts.get(env.context_at(found.offset))? {
                ContextSeverity::Hard => {
                    match exception_for(env, found.offset, spec.lints, &self.expect) {
                        Some(lint) => (Severity::Ask, Some(lint.to_owned())),
                        None => (Severity::Hard, found.exception),
                    }
                }
                ContextSeverity::Soft => (Severity::Soft, found.exception),
            },
        };
        let line = env.idx.line_of(found.offset);
        let text = js::truncate_utf16(
            js::js_trim(env.idx.line_text(line, env.src)),
            HIT_TEXT_UNITS,
        );
        let build_fix = match env.file {
            FileRole::BuildScript => spec.fix_build,
            FileRole::Source => None,
        };
        Some(Hit {
            key: RuleKey::new(spec.key),
            id: RuleId::new(spec.id),
            severity,
            line,
            text: text.to_owned(),
            msg: found.msg.unwrap_or(Cow::Borrowed(spec.msg)),
            fix: Cow::Borrowed(found.fix.or(build_fix).unwrap_or(spec.fix)),
            exception,
        })
    }
}

/// What one scan reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    hits: Vec<Hit>,
    tests: usize,
    ignores: usize,
    clones: usize,
}

impl ScanReport {
    /// The hits, in rule-table order, then match order.
    #[must_use]
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    /// The hits, owned.
    #[must_use]
    pub fn into_hits(self) -> Vec<Hit> {
        self.hits
    }

    /// The number of tests (`#[test]`-style attributes and `proptest!` functions).
    #[must_use]
    pub fn tests(&self) -> usize {
        self.tests
    }

    /// The number of `#[ignore]` attributes, not counting `cfg_attr(miri, ignore)`.
    #[must_use]
    pub fn ignores(&self) -> usize {
        self.ignores
    }

    /// The number of `.clone()` calls.
    #[must_use]
    pub fn clones(&self) -> usize {
        self.clones
    }
}

/// A per-scan view of one source that the rules read.
#[derive(Debug)]
pub(crate) struct Env<'s> {
    /// The source as written.
    pub(crate) src: &'s str,
    /// The masked source.
    pub(crate) code: &'s str,
    /// Lines of the masked source (the same as the source's).
    pub(crate) idx: &'s LineIndex,
    pub(crate) kind: CodeKind,
    pub(crate) file: FileRole,
    pub(crate) regions: &'s Regions,
}

impl Env<'_> {
    /// The context of the code at `offset`: test inside a test region, else the file's.
    fn context_at(&self, offset: usize) -> CodeKind {
        if self.kind == CodeKind::Test || self.regions.contains(offset) {
            CodeKind::Test
        } else {
            self.kind
        }
    }
}
