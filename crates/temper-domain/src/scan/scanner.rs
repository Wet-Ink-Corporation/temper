//! The scanner: compiles the rule table once and runs it over a source (1.x `scanSource`).

use std::borrow::Cow;
use std::collections::BTreeSet;

use regex::Regex;

use super::counts::{CountPatterns, count_clones, count_ignored, count_tests};
use super::env::Env;
use super::finders::expect_attrs::exception_for;
use super::finders::{FinderPatterns, Found};
use super::hit::{Exception, Hit, RuleId, RuleKey, Severity};
use super::lexer::{mask_rust, next_char};
use super::predicates::{Candidate, Keep, Look, PredicatePatterns};
use super::rules::{ContextSeverity, FinderKind, Matcher, Pattern, RULES, RuleSpec};
use crate::js::{self, JsFlags, UnsupportedJs};

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
    /// `build.rs` at the crate root (see [`is_build_script`](super::is_build_script)).
    BuildScript,
}

/// Options for one scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions<'a> {
    /// The context of the file's code.
    pub kind: CodeKind,
    /// Whether the file is a build script.
    pub file: FileRole,
    /// The lints the crate forbids (`unsafe_code`, `clippy::unwrap_used`), which the
    /// `forbid-override` rule reads. `None`, or an empty set, finds nothing.
    pub forbid: Option<&'a BTreeSet<String>>,
}

/// A built-in scanner pattern did not compile. This is a bug in temper; a unit test guards it.
#[derive(Debug, thiserror::Error)]
#[error("scanner pattern `{rule}` does not compile")]
pub struct ScannerSetupError {
    rule: &'static str,
    #[source]
    source: SetupCause,
}

/// Why a built-in pattern failed: its JS source has no faithful translation, or `regex` rejects
/// the translation.
#[derive(Debug, thiserror::Error)]
enum SetupCause {
    #[error(transparent)]
    Js(#[from] UnsupportedJs),
    #[error(transparent)]
    Regex(#[from] regex::Error),
}

impl ScannerSetupError {
    /// The rule key, or the lexer pattern name such as `regions:attr`, whose pattern failed.
    #[must_use]
    pub fn rule(&self) -> &'static str {
        self.rule
    }
}

/// Compiles the 1.x JS regex source `js_source` (no flags), named `rule` in the error.
pub(crate) fn compile(rule: &'static str, js_source: &str) -> Result<Regex, ScannerSetupError> {
    compile_with(rule, js_source, JsFlags::Plain)
}

/// Compiles the 1.x JS regex source `js_source` with `flags`, named `rule` in the error.
pub(crate) fn compile_with(
    rule: &'static str,
    js_source: &str,
    flags: JsFlags,
) -> Result<Regex, ScannerSetupError> {
    let compiled = js::translate(js_source, flags)
        .map_err(SetupCause::from)
        .and_then(|translated| Regex::new(&translated).map_err(SetupCause::from));
    compiled.map_err(|source| ScannerSetupError { rule, source })
}

/// The escape-hatch scanner: every rule's pattern, compiled once.
///
/// # Examples
///
/// ```
/// use temper_domain::{CodeKind, FileRole, ScanOptions, Scanner, Severity};
///
/// let scanner = Scanner::new()?;
/// let options = ScanOptions { kind: CodeKind::Prod, file: FileRole::Source, forbid: None };
/// let report = scanner.scan("fn f() { dbg!(1); }", &options);
///
/// let hit = report.hits().first().map(|h| (h.key().as_str(), h.severity()));
/// assert_eq!(hit, Some(("dbg", Severity::Hard)));
/// # Ok::<(), temper_domain::ScannerSetupError>(())
/// ```
#[derive(Debug)]
pub struct Scanner {
    rules: Vec<CompiledRule>,
    predicates: PredicatePatterns,
    counts: CountPatterns,
    finders: FinderPatterns,
}

#[derive(Debug)]
struct CompiledRule {
    spec: &'static RuleSpec,
    matcher: CompiledMatcher,
}

#[derive(Debug)]
enum CompiledMatcher {
    Regex(CompiledPattern),
    Find(FinderKind),
}

/// A table pattern, translated and compiled.
#[derive(Debug)]
struct CompiledPattern {
    re: Regex,
    flags: JsFlags,
    look: Option<Look>,
    keep: Option<Keep>,
}

impl CompiledPattern {
    fn new(rule: &'static str, pattern: Pattern) -> Result<Self, ScannerSetupError> {
        Ok(Self {
            re: compile_with(rule, pattern.source, pattern.flags)?,
            flags: pattern.flags,
            look: pattern.look,
            keep: pattern.keep,
        })
    }
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
                    Matcher::Regex(pattern) => {
                        CompiledMatcher::Regex(CompiledPattern::new(spec.key, pattern)?)
                    }
                    Matcher::Find(kind) => CompiledMatcher::Find(kind),
                };
                Ok(CompiledRule { spec, matcher })
            })
            .collect::<Result<_, ScannerSetupError>>()?;
        Ok(Self {
            rules,
            predicates: PredicatePatterns::new()?,
            counts: CountPatterns::new()?,
            finders: FinderPatterns::new()?,
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
    /// let options = ScanOptions { kind: CodeKind::Prod, file: FileRole::Source, forbid: None };
    /// let report = scanner.scan(src, &options);
    ///
    /// assert_eq!((report.hits().len(), report.tests()), (0, 1));
    /// # Ok::<(), temper_domain::ScannerSetupError>(())
    /// ```
    #[must_use]
    pub fn scan(&self, src: &str, options: &ScanOptions<'_>) -> ScanReport {
        let masked = mask_rust(src);
        let env = Env::new(src, &masked, options, &self.predicates.regions);
        let code = env.code;
        let mut hits = Vec::new();
        // One buffer for the whole scan; each rule's matches are drained into hits (F7).
        let mut found = Vec::new();
        for rule in &self.rules {
            self.matches(rule, &env, &mut found);
            hits.extend(found.drain(..).filter_map(|f| self.hit(rule.spec, &env, f)));
        }
        ScanReport {
            hits,
            tests: count_tests(code, &self.counts),
            ignores: count_ignored(code, &self.counts),
            clones: count_clones(code, &self.counts),
        }
    }

    /// Pushes the matches of one rule onto `out`.
    fn matches(&self, rule: &CompiledRule, env: &Env<'_>, out: &mut Vec<Found>) {
        match &rule.matcher {
            CompiledMatcher::Regex(pattern) => self.regex_matches(pattern, env, out),
            CompiledMatcher::Find(kind) => self.finders.find(*kind, env, out),
        }
    }

    /// The hit for one match, or `None` when the rule ignores the match's context.
    fn hit(&self, spec: &'static RuleSpec, env: &Env<'_>, found: Found) -> Option<Hit> {
        let attribute = found.exception.map(Exception::Attribute);
        let (severity, exception) = match found.severity {
            Some(forced) => (forced, attribute),
            None => match spec.contexts.get(env.context_at(found.offset))? {
                ContextSeverity::Hard => {
                    match exception_for(env, found.offset, spec.lints, &self.finders.expect) {
                        Some(lint) => (Severity::Ask, Some(Exception::Requested(lint.to_owned()))),
                        None => (Severity::Hard, attribute),
                    }
                }
                ContextSeverity::Soft => (Severity::Soft, attribute),
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
            fix: found.fix.or(build_fix).unwrap_or(spec.fix),
            exception,
        })
    }

    /// Pushes every match of `pattern` in the masked code onto `out` (1.x `regexMatches`).
    ///
    /// The search resumes where JS's `lastIndex` would: after a match, also one that `keep`
    /// drops, or one char on after an empty match. `find_at` reads the text before `at` for
    /// `\b` and `^`, as JS does.
    fn regex_matches(&self, pattern: &CompiledPattern, env: &Env<'_>, out: &mut Vec<Found>) {
        let code = env.code;
        let mut at = 0;
        // `find_at` panics past the end; every step moves `at` forward, so the loop ends.
        while at <= code.len() {
            let Some(m) = pattern.re.find_at(code, at) else {
                break;
            };
            if m.is_empty() {
                at = next_char(code, m.end());
                continue;
            }
            // Under `m`, a match may begin with the line terminator its `^` consumed.
            let skip = match pattern.flags {
                JsFlags::Multiline => js::terminator_len(m.as_str()),
                JsFlags::Plain | JsFlags::IgnoreCase => 0,
            };
            let candidate = Candidate {
                start: m.start().saturating_add(skip),
                text: m.as_str().get(skip..).unwrap_or(""),
            };
            if pattern
                .look
                .is_some_and(|look| !look.holds(env, &self.predicates, candidate))
            {
                at = next_char(code, m.start());
                continue;
            }
            at = m.end();
            if pattern
                .keep
                .is_some_and(|keep| !keep.holds(env, &self.predicates, candidate))
            {
                continue;
            }
            out.push(Found::at(candidate.start));
        }
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
