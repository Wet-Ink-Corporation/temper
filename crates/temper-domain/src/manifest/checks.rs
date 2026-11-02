//! The pre-edit checks of a `Cargo.toml` beyond lints and new dependencies (1.x
//! `manifestChecks`), with 1.x's message texts.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::js::{JsMap, js_string, truncate_utf16, truthy};

use super::{
    GoverningRoot, Manifest, OwnLints, RootRelation, WatchedKey, benign_when_new, dep_requirements,
    major_of, member_problems, non_caret_requirement, split_problems, watched_keys,
};

/// The manifest before the edit, as 1.x reads `before === '' ? {} : parseToml(before)`.
#[derive(Debug, Clone, Copy)]
pub enum Before<'a> {
    /// The file is new (its old text is empty): compared as `{}`.
    Empty,
    /// The old text parsed.
    Parsed(&'a Manifest),
    /// The old text is not valid TOML: no ratchet and no watched-key comparison.
    Invalid,
}

/// What [`manifest_checks`] needs about one edit of a `Cargo.toml`.
///
/// The application gathers it: the edited file's project-relative path, both sides, the
/// governing root of the edited manifest (when it is a package in a workspace) and its
/// `manifests.ownLints` listing.
#[derive(Debug, Clone, Copy)]
pub struct ManifestEditFacts<'a> {
    /// The manifest's path relative to the project root, as the messages print it.
    pub rel: &'a str,
    /// The manifest before the edit.
    pub before: Before<'a>,
    /// The manifest after the edit. An invalid `after` is the caller's early return: cargo
    /// rejects it on its own.
    pub after: &'a Manifest,
    /// The workspace root that governs `after`, found only when `after` has a `[package]`.
    pub root: Option<GoverningRoot<'a>>,
    /// Whether the member is listed in `manifests.ownLints`.
    pub own_lints: OwnLints,
}

/// The findings of [`manifest_checks`], each list in 1.x's order and text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestFindings {
    /// New structural problems: the edit is refused.
    pub denies: Vec<String>,
    /// Human decisions: a new crate, a watched switch, a major-version bump.
    pub asks: Vec<String>,
    /// Problems the manifest already had, and non-caret requirements.
    pub advice: Vec<String>,
}

const NEW_CRATE: &str = "in the workspace. Crate boundaries are a human decision (H-16, ARC-07), and the gate fails until the crate has its charter (`AGENTS.md`, ARC-08).";
const CARET_FIX: &str = "fix: declare a caret floor at the oldest version whose API you use (`\"1.4\"`), never `=`, `~`, `*` or an upper bound.";

/// Checks one manifest edit, as 1.x's `manifestChecks`.
///
/// It denies a member that newly lacks lint inheritance, newly has a profile or newly depends
/// on a mocking crate (a ratchet: problems the manifest already had only advise). It asks on a
/// new crate, on test, target and toolchain switches and on major-version bumps, and advises on
/// non-caret requirements.
#[must_use]
pub fn manifest_checks(facts: &ManifestEditFacts<'_>) -> ManifestFindings {
    let mut out = ManifestFindings::default();
    let rel = facts.rel;
    let after = facts.after;
    let empty = Manifest::empty();
    let before = match facts.before {
        Before::Empty => Some(&empty),
        Before::Parsed(m) => Some(m),
        Before::Invalid => None,
    };
    let is_new = matches!(facts.before, Before::Empty);
    let root = facts.root.as_ref();
    let had = before.map_or_else(Vec::new, |b| member_problems(b, root, facts.own_lints));
    let split = split_problems(&had, member_problems(after, root, facts.own_lints));
    for p in split.fresh {
        out.denies.push(format!(
            "{} {rel}: {}.\n    fix: {}",
            p.id(),
            p.msg(),
            p.fix()
        ));
    }
    for p in split.legacy {
        out.advice.push(format!(
            "{} {rel} (already the case before this edit): {}.\n    fix: {}",
            p.id(),
            p.msg(),
            p.fix()
        ));
    }
    let package = after.get("package").filter(|p| truthy(p));
    if is_new && package.is_some() && root.is_some_and(|r| r.relation == RootRelation::Member) {
        let name = package
            .and_then(|p| p.get("name"))
            .map_or(Cow::Borrowed(rel), js_string);
        out.asks
            .push(format!("{rel}: new crate `{name}` {NEW_CRATE}"));
    }
    if let Some(before) = before {
        out.asks.extend(watched_changes(rel, before, after, is_new));
    }
    requirement_changes(&mut out, rel, before, after);
    out
}

/// The asks for watched switches whose `JSON.stringify` text differs, over the labels of
/// `before` and then the new ones of `after`.
fn watched_changes(rel: &str, before: &Manifest, after: &Manifest, is_new: bool) -> Vec<String> {
    let mut asks = Vec::new();
    let wa = watched_keys(after);
    let wb = watched_keys(before);
    let (now, was) = (index(&wa), index(&wb));
    let mut labels = JsMap::new();
    for (label, _) in wb.iter().chain(&wa) {
        labels.add(label.as_str());
    }
    for label in labels.into_keys() {
        let x = now.get(label).copied();
        let y = was.get(label).copied();
        if x.map(|w| &w.value) == y.map(|w| &w.value) {
            continue;
        }
        if is_new && x.is_some_and(|w| benign_when_new(label, w)) {
            continue;
        }
        let Some(why) = x.or(y).map(|w| w.why) else {
            continue;
        };
        asks.push(format!(
            "{rel}: `{label}` changes ({} -> {}); {why}.",
            short(y),
            short(x)
        ));
    }
    asks
}

/// A lookup by key over an ordered list with unique keys (JS `map.get(key)`).
fn index<V>(list: &[(String, V)]) -> HashMap<&str, &V> {
    list.iter().map(|(k, v)| (k.as_str(), v)).collect()
}

/// 1.x's `short`: `unset`, or the value, cut to 77 UTF-16 units and `...` past 80. The cut
/// never splits a surrogate pair (D16(o)).
fn short(entry: Option<&WatchedKey>) -> Cow<'_, str> {
    let Some(entry) = entry else {
        return Cow::Borrowed("unset");
    };
    let units = entry.value.encode_utf16().count();
    if units > 80 {
        Cow::Owned(format!("{}...", truncate_utf16(&entry.value, 77)))
    } else {
        Cow::Borrowed(&entry.value)
    }
}

/// The major-version asks and the caret-floor advice, over `after`'s requirements.
fn requirement_changes(
    out: &mut ManifestFindings,
    rel: &str,
    before: Option<&Manifest>,
    after: &Manifest,
) {
    let old_reqs = before.map_or_else(Vec::new, dep_requirements);
    let old_index = index(&old_reqs);
    for (key, req) in dep_requirements(after) {
        let old = old_index.get(key.as_str()).map(|r| r.as_str());
        if old == Some(req.as_str()) {
            continue;
        }
        let name = key.rsplit_once(':').map_or(key.as_str(), |(_, name)| name);
        if let Some(old) = old
            && let (Some(was), Some(now)) = (major_of(old), major_of(&req))
            && was != now
        {
            out.asks.push(format!(
                "{rel}: `{name}` moves to a new major version (\"{old}\" -> \"{req}\"). Major-version bumps are a human decision (H-16, ARC-12)."
            ));
        }
        if let Some(why) = non_caret_requirement(&req) {
            out.advice.push(format!(
                "ARC-12 {rel}: `{name} = \"{req}\"` is {why}.\n    {CARET_FIX}"
            ));
        }
    }
}
