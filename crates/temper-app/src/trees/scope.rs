//! The scope of each checkout a session changed: its base and starting fingerprint (1.x
//! `hooks.mjs` `ownTree`, `otherTrees`, `coversBase`, `mergedInto` and `dueTrees`).

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use temper_domain::{Fingerprint, JsTypeError, JsValue, TreeRecord};

use super::{checkout_id, has_session};
use crate::changes::{fingerprint, rust_changes};
use crate::git;
use crate::hooks::{HookCall, session_segment};
use crate::paths;
use crate::ports::{Files, Ports, ProcessRunner};
use crate::project::{Workspace, discover_at, exists};
use crate::session::{base_rev, session_dir, start_fingerprint};
use crate::state::read_js_text;

/// Where a checkout's scope starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scope {
    /// The base revision, `None` outside git or without one.
    pub(crate) base: Option<String>,
    /// The tree's fingerprint when the scope started, when it is known.
    pub(crate) start_fp: Option<String>,
}

/// Whether [`own_tree`] lets the main agent's scope reach back to the session base (#157).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Widen {
    /// The main agent: it answers for the session's commits too.
    ToSessionBase,
    /// A subagent: its scope is the checkout's own.
    No,
}

/// Which checkout a [`Tree`] is, as 1.x's `main` flag tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Checkout {
    /// The checkout the hook runs in.
    Here,
    /// Another checkout of the repository that this session changed.
    Other,
}

/// A checkout with its scope.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tree<'a> {
    /// The checkout's project.
    pub(crate) ws: Cow<'a, Workspace>,
    /// Its scope.
    pub(crate) scope: Scope,
    /// Which checkout it is.
    pub(crate) checkout: Checkout,
}

/// A tree that still needs the gate, with its current fingerprint.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DueTree<'a> {
    /// The tree.
    pub(crate) tree: Tree<'a>,
    /// Its fingerprint against its base.
    pub(crate) fp: Fingerprint,
}

/// 1.x `ownTree(proj, input, { widen })`: the scope of the checkout the agent runs in. In a
/// linked worktree, the earliest record of it with a head (by JS `<` on `at`), when that head
/// still resolves; otherwise the session's base and start fingerprint. With
/// [`Widen::ToSessionBase`], a session base that is an ancestor of that head is the base, with
/// no start fingerprint.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x's `resolveRev(proj, first.head)` throws on the head's template
/// literal; a throw while choosing the earliest record is inside 1.x's `try`, which answers the
/// session's scope.
pub(crate) fn own_tree(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    widen: Widen,
) -> Result<Scope, JsTypeError> {
    let HookCall { ports, env, input } = *call;
    let style = env.style;
    let sid = session_segment(input)?;
    let session = Scope {
        base: base_rev(ports, style, ws, &sid),
        start_fp: start_fingerprint(ports.files(), style, ws, &sid),
    };
    let Some(git_root) = ws.git_root() else {
        return Ok(session);
    };
    let process = ports.process();
    if !has_session(input) || !git::is_linked(process, style, git_root) {
        return Ok(session);
    }
    let dir = session_dir(style, ws, &sid);
    let prefix = format!("tree-{}-", checkout_id(env, ws));
    let Some(first) = earliest_with_head(ports.files(), style, &dir, &prefix) else {
        return Ok(session);
    };
    let at = Path::new(git_root);
    let Some(base) = first
        .head_rev()?
        .and_then(|head| git::resolve_rev(process, at, &head))
    else {
        return Ok(session);
    };
    let widened = match session.base.as_deref().filter(|b| !b.is_empty()) {
        Some(from) => {
            widen == Widen::ToSessionBase
                && from != base
                && git::is_ancestor(process, at, from, &base)
        }
        None => false,
    };
    if widened {
        return Ok(Scope {
            base: session.base,
            start_fp: None,
        });
    }
    Ok(Scope {
        base: Some(base),
        start_fp: first.start_fp().map(str::to_owned),
    })
}

/// The earliest record named `<prefix>….json` in `dir` that has a head; `None` when there is
/// none, or when listing or comparing fails (1.x's `catch { return session; }`).
fn earliest_with_head(
    files: &impl Files,
    style: temper_domain::PathStyle,
    dir: &str,
    prefix: &str,
) -> Option<TreeRecord> {
    let Ok(entries) = files.read_dir(Path::new(dir)) else {
        // No session dir: 1.x's `catch { return session; }`.
        return None;
    };
    let mut first: Option<TreeRecord> = None;
    let names = entries
        .iter()
        .filter_map(|entry| entry.name.to_str())
        // 1.x's case-sensitive `x.endsWith('.json')`.
        .filter(|name| name.starts_with(prefix) && name.strip_suffix(".json").is_some());
    for name in names {
        let Some(rec) = read_js_text(files, &paths::join(style, dir, name))
            .and_then(|text| TreeRecord::from_json(&text))
        else {
            continue;
        };
        if !rec.has_head() {
            continue;
        }
        let earlier = match &first {
            None => true,
            Some(seen) => match rec.earlier_than(seen) {
                Ok(earlier) => earlier,
                // A throw here is inside 1.x's outer `try`, which answers the session's scope.
                Err(_) => return None,
            },
        };
        if earlier {
            first = Some(rec);
        }
    }
    first
}

/// 1.x `otherTrees(proj, input)` without `agent` or `gone`: the other checkouts of `ws`'s
/// repository this session changed, by root in UTF-8 byte order (OC-4 A, D16(v)), each with
/// the base and start fingerprint of its earliest record. A root that no longer exists, or
/// whose project is not that root in the same state dir, is left out.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x throws outside any `try`: a record whose root is truthy but not a
/// string (`path.resolve`), `at` values that JS cannot compare, or a head whose template
/// literal throws (F5).
pub(crate) fn other_trees<'a>(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
) -> Result<Vec<Tree<'a>>, JsTypeError> {
    let HookCall { ports, env, input } = *call;
    let style = env.style;
    if ws.git_root().is_none() || !has_session(input) {
        return Ok(Vec::new());
    }
    let sid = session_segment(input)?;
    let dir = session_dir(style, ws, &sid);
    let files = ports.files();
    let Ok(entries) = files.read_dir(Path::new(&dir)) else {
        // 1.x: `catch { return []; }`.
        return Ok(Vec::new());
    };
    let cwd = env.cwd.to_string_lossy();
    let here = paths::resolve(style, &cwd, &ws.root);
    let mut by_root: BTreeMap<String, TreeRecord> = BTreeMap::new();
    let names = entries
        .iter()
        .filter_map(|entry| entry.name.to_str())
        .filter(|name| is_record_name(name));
    for name in names {
        let Some(rec) = read_js_text(files, &paths::join(style, &dir, name))
            .and_then(|text| TreeRecord::from_json(&text))
        else {
            continue;
        };
        keep_earliest(&mut by_root, rec, |root| {
            paths::resolve(style, &cwd, root) == here
        })?;
    }
    let mut out = Vec::with_capacity(by_root.len());
    for (root, rec) in &by_root {
        out.extend(tree_at(call, ws, root, rec)?);
    }
    Ok(out)
}

/// 1.x's `byRoot.set(rec.root, rec)` when `rec` is the first or earliest record of its root,
/// keyed by the root as written; a `BTreeMap` iterates in UTF-8 byte order (OC-4 A). A record
/// without a truthy root, or of this checkout (`is_here`), is skipped.
fn keep_earliest(
    by_root: &mut BTreeMap<String, TreeRecord>,
    rec: TreeRecord,
    is_here: impl Fn(&str) -> bool,
) -> Result<(), JsTypeError> {
    let Some(root) = rec.root()? else {
        return Ok(());
    };
    if is_here(root) {
        return Ok(());
    }
    let replace = match by_root.get(root) {
        None => true,
        Some(seen) => rec.earlier_than(seen)?,
    };
    if replace {
        // The map stores its key: the root is copied out of the record it keeps.
        by_root.insert(root.to_owned(), rec);
    }
    Ok(())
}

/// The checkout a record names, when its root still exists and is a checkout of `ws`'s
/// repository: its base is the record's head when that resolves there, else the session's.
fn tree_at<'a>(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    root: &str,
    rec: &TreeRecord,
) -> Result<Option<Tree<'a>>, JsTypeError> {
    let HookCall { ports, env, input } = *call;
    let style = env.style;
    let cwd = env.cwd.to_string_lossy();
    let resolved = paths::resolve(style, &cwd, root);
    if !exists(ports.files(), &resolved) {
        return Ok(None);
    }
    let Some(tree) = discover_at(ports, env, &resolved) else {
        return Ok(None);
    };
    if paths::resolve(style, &cwd, &tree.root) != resolved || tree.state_dir != ws.state_dir {
        return Ok(None);
    }
    let resolved_head = match (tree.git_root(), rec.head_rev()?) {
        (Some(git_root), Some(head)) => {
            git::resolve_rev(ports.process(), Path::new(git_root), &head)
        }
        (Some(_) | None, None) | (None, Some(_)) => None,
    };
    let sid = session_segment(input)?;
    let base = resolved_head.or_else(|| base_rev(ports, style, &tree, &sid));
    Ok(Some(Tree {
        scope: Scope {
            base,
            start_fp: rec.start_fp().map(str::to_owned),
        },
        ws: Cow::Owned(tree),
        checkout: Checkout::Other,
    }))
}

/// 1.x's `/^tree-[0-9a-f]{12}-.+\.json$/`, where `.` is anything but a line terminator.
fn is_record_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("tree-") else {
        return false;
    };
    let Some((id, rest)) = rest.split_at_checked(12) else {
        return false;
    };
    let hex = id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let agent = rest.strip_prefix('-').and_then(|r| r.strip_suffix(".json"));
    hex && agent.is_some_and(|agent| {
        !agent.is_empty() && !agent.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
    })
}

/// 1.x `coversBase(proj, base, t.base, t.proj)`: does a scope from `base` hold all of `tree`'s
/// own commits (those after its base)? Counted with `git rev-list --count` at `ws`'s root; a
/// failed count is not covered (#157).
pub(crate) fn covers_base(
    process: &impl ProcessRunner,
    ws: &Workspace,
    base: Option<&str>,
    tree: &Tree<'_>,
) -> bool {
    let Some(tree_base) = tree.scope.base.as_deref().filter(|b| !b.is_empty()) else {
        return true;
    };
    let Some(base) = base.filter(|b| !b.is_empty()) else {
        return false;
    };
    let Some(head) = tree
        .ws
        .git_root()
        .and_then(|root| git::resolve_rev(process, Path::new(root), "HEAD"))
    else {
        return false;
    };
    let Some(at) = ws.git_root().map(Path::new) else {
        return false;
    };
    let not_tree_base = format!("^{tree_base}");
    let all = git::rev_list_count(process, at, &[&head, &not_tree_base]);
    let not_base = format!("^{base}");
    all.is_some() && all == git::rev_list_count(process, at, &[&head, &not_tree_base, &not_base])
}

/// 1.x `mergedInto(tree, main)`: everything `tree` holds is in `main`'s HEAD: its HEAD is an
/// ancestor, and it has no uncommitted Rust change.
pub(crate) fn merged_into(
    call: &HookCall<'_, impl Ports>,
    tree: &Workspace,
    main: &Workspace,
) -> bool {
    let HookCall { ports, env, .. } = *call;
    let process = ports.process();
    let head_of = |ws: &Workspace| {
        ws.git_root()
            .and_then(|root| git::resolve_rev(process, Path::new(root), "HEAD"))
    };
    let (Some(head), Some(main_head), Some(at)) = (head_of(tree), head_of(main), main.git_root())
    else {
        return false;
    };
    git::is_ancestor(process, Path::new(at), &head, &main_head)
        && rust_changes(ports, env, tree, Some(&head)).is_empty()
}

/// 1.x `dueTrees(proj, input)` without `agent`: the trees whose Rust changed and that still
/// need the gate. The checkout here first (its scope widened to the session base), then each
/// other checkout this session changed, unless its fingerprint is the one it started from, or
/// it is merged into a gated checkout here whose scope covers its commits.
///
/// # Errors
///
/// As [`own_tree`] and [`other_trees`].
pub(crate) fn due_trees<'a>(
    call: &HookCall<'_, impl Ports>,
    ws: &'a Workspace,
) -> Result<Vec<DueTree<'a>>, JsTypeError> {
    let HookCall { ports, env, .. } = *call;
    let mut candidates = vec![Tree {
        ws: Cow::Borrowed(ws),
        scope: own_tree(call, ws, Widen::ToSessionBase)?,
        checkout: Checkout::Here,
    }];
    candidates.extend(other_trees(call, ws)?);
    let main_base = candidates.first().and_then(|t| t.scope.base.as_deref());
    let mut keep: Vec<Option<Fingerprint>> = Vec::with_capacity(candidates.len());
    for t in &candidates {
        let base = t.scope.base.as_deref();
        if rust_changes(ports, env, &t.ws, base).is_empty() {
            keep.push(None);
            continue;
        }
        let fp = fingerprint(ports, env, &t.ws, base);
        let unchanged = t.scope.start_fp.as_deref() == Some(fp.to_string().as_str());
        let main_due = keep.first().is_some_and(Option::is_some);
        let held_here = t.checkout == Checkout::Other
            && main_due
            && merged_into(call, &t.ws, ws)
            && covers_base(ports.process(), ws, main_base, t);
        keep.push((!unchanged && !held_here).then_some(fp));
    }
    Ok(candidates
        .into_iter()
        .zip(keep)
        .filter_map(|(tree, fp)| Some(DueTree { tree, fp: fp? }))
        .collect())
}

/// 1.x `reviewedTree(proj, input, fp)`: the checkout whose current tree has fingerprint `fp`
/// (a verdict's `fingerprint`, any JSON value). This checkout against its own base, then
/// against the main agent's widened base (#157), then the first other checkout of the session
/// whose tree matches; otherwise, and when `fp` is falsy, this checkout against its own base.
/// Only a string can equal a fingerprint (`===`), but 1.x asks every candidate either way.
///
/// # Errors
///
/// As [`own_tree`] and [`other_trees`] (F5).
pub(crate) fn reviewed_tree<'a>(
    call: &HookCall<'_, impl Ports>,
    ws: &'a Workspace,
    fp: Option<&JsValue>,
) -> Result<Tree<'a>, JsTypeError> {
    let HookCall { ports, env, .. } = *call;
    let base = own_tree(call, ws, Widen::No)?.base;
    let own = |base: Option<String>| Tree {
        ws: Cow::Borrowed(ws),
        scope: Scope {
            base,
            start_fp: None,
        },
        checkout: Checkout::Here,
    };
    let Some(fp) = fp.filter(|fp| fp.is_truthy()) else {
        return Ok(own(base));
    };
    let matches = |tree: &Workspace, base: Option<&str>| {
        fp.as_str() == Some(fingerprint(ports, env, tree, base).to_string().as_str())
    };
    let widened = own_tree(call, ws, Widen::ToSessionBase)?.base;
    if matches(ws, base.as_deref()) {
        return Ok(own(base));
    }
    // `new Set([base, widened])`: a widened base equal to the own one is not asked again.
    if widened != base && matches(ws, widened.as_deref()) {
        return Ok(own(widened));
    }
    let others = other_trees(call, ws)?;
    match others
        .into_iter()
        .find(|t| matches(&t.ws, t.scope.base.as_deref()))
    {
        Some(tree) => Ok(tree),
        None => Ok(own(base)),
    }
}

#[cfg(test)]
mod tests;
