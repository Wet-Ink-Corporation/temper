//! The checkouts a session changes (1.x `hooks.mjs`, #135). Every checkout of a repository
//! shares its state dir, so the session dir records each checkout as
//! `tree-<checkout id>-<agent>.json`, with the base and fingerprint its scope starts from.

mod scope;

use std::path::Path;

use temper_domain::{JsValue, TreeId, tree_id};

use crate::changes::fingerprint;
use crate::engine::RunEnv;
use crate::git;
use crate::hooks::{HookCall, HookInput, agent_key, session_segment};
use crate::paths;
use crate::ports::{Clock, Files, FsError, Ports};
use crate::project::{Workspace, exists};
use crate::session::{Force, base_rev, session_dir, start_fingerprint};

pub(crate) use scope::{Checkout, DueTree, Scope, Tree, Widen, due_trees, own_tree};

/// 1.x `noteTree(proj, input)`: records that this agent context changes Rust in `ws`'s
/// checkout, as `tree-<checkout>-<agent>.json`, once: an existing record is kept.
///
/// The record holds the root, the scope's base (HEAD now), the branch (`null` when detached),
/// the tree's fingerprint against that base as `startFp`, and the time. The main agent entering
/// a linked worktree it did not start in, whose HEAD holds the session base, takes the session
/// base instead, with `startFp` `null` (#157). Without a git root or a truthy session id,
/// nothing is written.
///
/// # Errors
///
/// [`FsError`] when the record cannot be written, as 1.x's `writeAtomic` throws (F2, F5).
pub(crate) fn note_tree(call: &HookCall<'_, impl Ports>, ws: &Workspace) -> Result<(), FsError> {
    let HookCall { ports, env, input } = *call;
    let style = env.style;
    let Some(git_root) = ws.git_root() else {
        return Ok(());
    };
    if !has_session(input) {
        return Ok(());
    }
    let sid = session_segment(input);
    let dir = session_dir(style, ws, &sid);
    let id = checkout_id(env, ws);
    let agent = agent_key(input);
    let path = paths::join(style, &dir, &format!("tree-{id}-{agent}.json"));
    let files = ports.files();
    if exists(files, &path) {
        return Ok(());
    }
    let (process, at) = (ports.process(), Path::new(git_root));
    let head = git::resolve_rev(process, at, "HEAD");
    let session_base = base_rev(ports, style, ws, &sid).filter(|base| !base.is_empty());
    let started_here = paths::join(style, &dir, &format!("tree-{id}-session.json"));
    // 1.x's `&&` chain, in its order, so git runs only as far as 1.x's does.
    let widened = match (&head, &session_base) {
        (Some(head), Some(base)) => {
            agent == "main"
                && head != base
                && git::is_linked(process, style, git_root)
                && !exists(files, &started_here)
                && git::is_ancestor(process, at, base, head)
        }
        (Some(_) | None, None) | (None, Some(_)) => false,
    };
    let branch = git::symbolic_ref_short(process, at);
    let (head, start) = if widened {
        // A widened scope already holds the session's commits: no starting fingerprint.
        (session_base, JsValue::Null)
    } else {
        // 1.x's `head ?? baseRev(proj, sid)`; the session base was read just above.
        let from = head.as_deref().or(session_base.as_deref());
        let fp = fingerprint(ports, env, ws, from).to_string();
        (head, JsValue::String(fp))
    };
    let or_null = |text: Option<String>| text.map_or(JsValue::Null, JsValue::String);
    // The record owns a copy of the root: the workspace keeps its own for the handler.
    let record = JsValue::Object(
        [
            ("root", JsValue::String(ws.root.clone())),
            ("head", or_null(head)),
            ("branch", or_null(branch)),
            ("startFp", start),
            ("at", JsValue::Number(ports.clock().now().js_millis())),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect(),
    );
    files.write_atomic(Path::new(&path), &record.to_json())
}

/// The input has a truthy `session_id`.
fn has_session(input: &HookInput) -> bool {
    input.raw("session_id").is_some_and(JsValue::is_truthy)
}

/// 1.x `treeId(proj.root)`: the id of `ws`'s checkout in its records' names.
fn checkout_id(env: &RunEnv, ws: &Workspace) -> TreeId {
    tree_id(&paths::resolve(
        env.style,
        &env.cwd.to_string_lossy(),
        &ws.root,
    ))
}

/// 1.x `noteSessionTree(proj, input, fresh)`: a session that starts in a linked worktree
/// records that checkout with the session's own base and start fingerprint, as its earliest
/// record (`at: 0`), so a later agent's record never moves its base.
///
/// It needs a git root, a truthy session id, a linked worktree and a base. With
/// [`Force::IfAbsent`] (a resume or compaction), a checkout that already has any record of
/// this session keeps it.
///
/// # Errors
///
/// [`FsError`] when the record cannot be written, as 1.x's `writeAtomic` throws (F1).
pub(crate) fn note_session_tree(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    force: Force,
) -> Result<(), FsError> {
    let HookCall { ports, env, input } = *call;
    let style = env.style;
    let Some(git_root) = ws.git_root() else {
        return Ok(());
    };
    if !has_session(input) || !git::is_linked(ports.process(), style, git_root) {
        return Ok(());
    }
    let sid = session_segment(input);
    let dir = session_dir(style, ws, &sid);
    let id = checkout_id(env, ws);
    let files = ports.files();
    if force == Force::IfAbsent {
        let prefix = format!("tree-{id}-");
        let noted = match files.read_dir(Path::new(&dir)) {
            Ok(entries) => entries
                .iter()
                .any(|entry| entry.name.to_string_lossy().starts_with(&prefix)),
            // No session dir yet: no record either (1.x's `catch { /* none yet */ }`).
            Err(_) => false,
        };
        if noted {
            return Ok(());
        }
    }
    let Some(head) = base_rev(ports, style, ws, &sid).filter(|head| !head.is_empty()) else {
        return Ok(());
    };
    let start = start_fingerprint(files, style, ws, &sid).map_or(JsValue::Null, JsValue::String);
    // The record value owns a copy of the root: the workspace keeps its own for the handler.
    let record = JsValue::Object(
        [
            ("root", JsValue::String(ws.root.clone())),
            ("head", JsValue::String(head)),
            ("branch", JsValue::Null),
            ("startFp", start),
            ("at", JsValue::Number(0.0)),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect(),
    );
    let path = paths::join(style, &dir, &format!("tree-{id}-session.json"));
    files.write_atomic(Path::new(&path), &record.to_json())
}

#[cfg(test)]
mod tests;
