//! Structural problems of a package manifest inside a workspace (1.x `memberProblems`): missing
//! lint inheritance (MOD-09, ARC-01), member profiles (PERF-09) and mocking crates in the
//! normal dependencies (TST-09), with the ratchet that tells new problems from old ones.

use std::borrow::Cow;

use crate::js::{Value, entries, js_string, truthy};

use super::{Manifest, inherits_lints};

/// How the governing workspace root relates to the manifest being checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootRelation {
    /// The manifest is its own workspace root (it has a `[workspace]` table).
    Itself,
    /// The manifest is a member of a root elsewhere.
    Member,
}

/// The workspace root that governs a package manifest, as the application found it.
#[derive(Debug, Clone, Copy)]
pub struct GoverningRoot<'m> {
    /// The root's parsed manifest.
    pub manifest: &'m Manifest,
    /// Whether the root is the checked manifest itself.
    pub relation: RootRelation,
}

/// Whether a human listed the member in `manifests.ownLints` (`.claude/temper.json`), which lets
/// it carry its own full `[lints]` table instead of inheriting the workspace's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnLints {
    /// Listed: its own non-empty `[lints]` table is accepted.
    Listed,
    /// Not listed: it must inherit with `[lints] workspace = true`.
    NotListed,
}

/// One structural problem, with 1.x's texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestProblem {
    /// MOD-09: the package does not inherit the workspace lints.
    MissingLintInheritance {
        /// Whether the member is listed in `manifests.ownLints`, which changes the texts.
        own_lints: OwnLints,
    },
    /// PERF-09: a member manifest has a `[profile]` table, which Cargo ignores.
    MemberProfile,
    /// TST-09: a mocking crate is a normal dependency.
    MockingDependency {
        /// The crate name (`package =` wins over the key).
        name: String,
    },
}

impl ManifestProblem {
    /// The rule id: `MOD-09`, `PERF-09` or `TST-09`.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::MissingLintInheritance { .. } => "MOD-09",
            Self::MemberProfile => "PERF-09",
            Self::MockingDependency { .. } => "TST-09",
        }
    }

    /// What tells problems of one rule apart for the ratchet: the id, or `TST-09:<name>`.
    #[must_use]
    pub fn key(&self) -> Cow<'_, str> {
        match self {
            Self::MockingDependency { name } => Cow::Owned(format!("TST-09:{name}")),
            _ => Cow::Borrowed(self.id()),
        }
    }

    /// The problem, as 1.x words it.
    #[must_use]
    pub fn msg(&self) -> Cow<'_, str> {
        Cow::Borrowed(match self {
            Self::MissingLintInheritance {
                own_lints: OwnLints::Listed,
            } => {
                "the package is listed in manifests.ownLints but has no `[lints]` table, so clippy would accept unwrap, panic!, as casts and #[allow] in it"
            }
            Self::MissingLintInheritance {
                own_lints: OwnLints::NotListed,
            } => {
                "the package does not inherit the workspace lints, so clippy would accept unwrap, panic!, as casts and #[allow] in it"
            }
            Self::MemberProfile => "a `[profile]` table in a member manifest is ignored by Cargo",
            Self::MockingDependency { name } => {
                return Cow::Owned(format!(
                    "the mocking crate `{name}` is a normal dependency, so it is compiled into production code"
                ));
            }
        })
    }

    /// The fix, as 1.x words it.
    #[must_use]
    pub const fn fix(&self) -> &'static str {
        match self {
            Self::MissingLintInheritance {
                own_lints: OwnLints::Listed,
            } => "give it its full, human-approved `[lints]` table (MOD-09, H-05).",
            Self::MissingLintInheritance {
                own_lints: OwnLints::NotListed,
            } => {
                "add a `[lints]` table containing only `workspace = true` (MOD-09, ARC-01). A crate that needs its own lints (an approved FFI crate) is listed by a human in `.claude/temper.json` under `manifests.ownLints`."
            }
            Self::MemberProfile => {
                "move the profile setting to the workspace root manifest, as an H-16 decision recorded in the root AGENTS.md (PERF-09)."
            }
            Self::MockingDependency { .. } => {
                "declare it under `[dev-dependencies]` and use it only at external seams (HTTP, cloud SDKs); fake your own ports by hand (TST-09). An optional dependency behind a `test-util` feature that `default` does not enable is also fine."
            }
        }
    }

    /// Whether the check is new in 1.x, so that without a base revision the gate only advises.
    #[must_use]
    pub const fn new_only(&self) -> bool {
        matches!(self, Self::MockingDependency { .. })
    }
}

/// Mocking and HTTP-stub crates (TST-09), as 1.x's `MOCKING_CRATES`: the crates the standard
/// names, plus the common ones.
pub(crate) const MOCKING_CRATES: [&str; 8] = [
    "faux",
    "httpmock",
    "mockall",
    "mockall_double",
    "mockito",
    "mry",
    "unimock",
    "wiremock",
];

/// The structural problems of `member` under its governing `root`, in 1.x's order. A standalone
/// package (no root) has none.
///
/// # Examples
///
/// ```
/// use temper_domain::{GoverningRoot, Manifest, OwnLints, RootRelation, member_problems};
///
/// let root: Manifest = "[workspace]\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n".parse()?;
/// let member: Manifest = "[package]\nname = \"a\"\n".parse()?;
/// let governing = GoverningRoot { manifest: &root, relation: RootRelation::Member };
/// let problems = member_problems(&member, Some(&governing), OwnLints::NotListed);
/// assert_eq!(problems.iter().map(|p| p.id()).collect::<Vec<_>>(), ["MOD-09"]);
/// # Ok::<(), temper_domain::ManifestParseError>(())
/// ```
#[must_use]
pub fn member_problems(
    member: &Manifest,
    root: Option<&GoverningRoot<'_>>,
    own_lints: OwnLints,
) -> Vec<ManifestProblem> {
    let mut out = Vec::new();
    let Some(root) = root else {
        return out;
    };
    let is_package = member.get("package").is_some_and(truthy);
    let root_lints = root.manifest.get("workspace").and_then(|w| w.get("lints"));
    if is_package && root_lints.is_some() && !inherits_lints(member) {
        let own_table = match member.get("lints") {
            Some(Value::Object(table)) => !table.is_empty(),
            Some(Value::Array(items)) => !items.is_empty(),
            _ => false,
        };
        if !(own_lints == OwnLints::Listed && own_table) {
            out.push(ManifestProblem::MissingLintInheritance { own_lints });
        }
    }
    if root.relation == RootRelation::Member && member.get("profile").is_some() {
        out.push(ManifestProblem::MemberProfile);
    }
    if is_package {
        out.extend(
            mocking_dependencies(member)
                .into_iter()
                .map(|name| ManifestProblem::MockingDependency { name }),
        );
    }
    out
}

/// Mocking crates among the `[dependencies]` (also per target), by crate name, sorted. An
/// optional one counts only when the default features turn it on.
fn mocking_dependencies(m: &Manifest) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut defaults: Option<Vec<String>> = None;
    let mut visit = |deps: Option<&Value>| {
        for (key, spec) in deps.into_iter().flat_map(entries) {
            let optional = spec.get("optional") == Some(&Value::Bool(true));
            if optional
                && !defaults
                    .get_or_insert_with(|| enabled_by_default(m))
                    .iter()
                    .any(|f| *f == *key)
            {
                continue;
            }
            let name = spec.get("package").and_then(Value::as_str).unwrap_or(&key);
            if MOCKING_CRATES.contains(&name) && !out.iter().any(|n| n == name) {
                out.push(name.to_owned());
            }
        }
    };
    visit(m.get("dependencies"));
    for (_, target) in m.get("target").into_iter().flat_map(entries) {
        visit(target.get("dependencies"));
    }
    out.sort();
    out
}

/// What the default features enable, as 1.x's `enabledByDefault`: feature names and optional
/// dependency keys, following `default` through the features it names (`dep:x`, `x` and `x/f`
/// enable `x`; a weak `x?/f` does not).
fn enabled_by_default(m: &Manifest) -> Vec<String> {
    let features = m.get("features");
    let mut on: Vec<String> = Vec::new();
    let mut todo = vec!["default".to_owned()];
    while let Some(feature) = todo.pop() {
        if on.contains(&feature) {
            continue;
        }
        let items = match features.and_then(|f| f.get(&feature)) {
            Some(Value::Array(items)) => items.as_slice(),
            _ => &[],
        };
        on.push(feature);
        for item in items {
            let s = js_string(item);
            if !is_weak(&s) {
                todo.push(enabled_name(&s).to_owned());
            }
        }
    }
    on
}

/// 1.x `/^[^/]+\?\//`: a weak dependency feature (`x?/f`).
fn is_weak(s: &str) -> bool {
    s.split_once('/')
        .is_some_and(|(head, _)| head.len() > 1 && head.ends_with('?'))
}

/// 1.x `s.replace(/^dep:/, '').replace(/\/.*$/, '')`. The second cut starts at the first `/`
/// that no JS line terminator follows, since `.` stops at one and `$` is the end of the text.
fn enabled_name(s: &str) -> &str {
    let s = s.strip_prefix("dep:").unwrap_or(s);
    let last_line = s
        .char_indices()
        .rev()
        .find(|&(_, c)| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        .map_or(0, |(at, c)| at.saturating_add(c.len_utf8()));
    let cut = s
        .get(last_line..)
        .and_then(|tail| tail.find('/'))
        .map(|at| at.saturating_add(last_line));
    cut.and_then(|at| s.get(..at)).unwrap_or(s)
}

/// Problems split by the ratchet: those `before` did not have, and those it already had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitProblems {
    /// New with this change.
    pub fresh: Vec<ManifestProblem>,
    /// Already there before it.
    pub legacy: Vec<ManifestProblem>,
}

/// Splits `after` by whether `before` has a problem with the same [`ManifestProblem::key`].
#[must_use]
pub fn split_problems(before: &[ManifestProblem], after: Vec<ManifestProblem>) -> SplitProblems {
    let (legacy, fresh) = after
        .into_iter()
        .partition(|p| before.iter().any(|b| b.key() == p.key()));
    SplitProblems { fresh, legacy }
}
