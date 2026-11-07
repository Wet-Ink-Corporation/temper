//! The checkouts a session changes (1.x `hooks.mjs`, #135). Every checkout of a repository
//! shares its state dir, so the session dir records each checkout as
//! `tree-<checkout id>-<agent>.json`, with the base and fingerprint its scope starts from.

use std::path::Path;

use temper_domain::{JsValue, tree_id};

use crate::git;
use crate::hooks::{HookCall, session_segment};
use crate::paths;
use crate::ports::{Files, FsError, Ports};
use crate::project::Workspace;
use crate::session::{Force, base_rev, session_dir, start_fingerprint};

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
    let has_session = input.raw("session_id").is_some_and(JsValue::is_truthy);
    if !has_session || !git::is_linked(ports.process(), style, git_root) {
        return Ok(());
    }
    let sid = session_segment(input);
    let dir = session_dir(style, ws, &sid);
    let id = tree_id(&paths::resolve(style, &env.cwd.to_string_lossy(), &ws.root));
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
