//! The session's state under `<state dir>/sessions/<sid>/` and the base it compares the tree
//! with (1.x `project.mjs` `sessionDir`, `baseRev`; `temper.mjs` `sessionBase`).

use std::cell::Cell;
use std::path::Path;

use temper_domain::{PathStyle, js_trim, temper_base_note};

use crate::changes::fingerprint;
use crate::engine::RunEnv;
use crate::git;
use crate::paths;
use crate::ports::{Clock, Files, FsError, Ports, ProcessRunner};
use crate::project::{Workspace, exists};
use crate::state::read_js_text;

/// 1.x `sessionDir(proj, sid)` for a session id that is a non-empty string: its directory,
/// with each UTF-16 unit outside `[\w.-]` written `_`, as 1.x's `/[^\w.-]/g` replaces them.
pub(crate) fn session_dir(style: PathStyle, ws: &Workspace, sid: &str) -> String {
    let segment: String = sid
        .chars()
        .flat_map(|c| {
            let kept = c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
            let (c, units) = if kept { (c, 1) } else { ('_', c.len_utf16()) };
            std::iter::repeat_n(c, units)
        })
        .collect();
    let sessions = paths::join(style, &ws.state_dir, "sessions");
    paths::join(style, &sessions, &segment)
}

/// 1.x `baseRev(proj, sid)`: the session's recorded base when it still resolves here, else
/// HEAD; `None` outside git.
pub(crate) fn base_rev(
    ports: &impl Ports,
    style: PathStyle,
    ws: &Workspace,
    sid: &str,
) -> Option<String> {
    let base = paths::join(style, &session_dir(style, ws, sid), "base");
    let recorded = read_js_text(ports.files(), &base);
    let recorded = recorded.as_deref().map(js_trim).filter(|t| !t.is_empty());
    recorded
        .and_then(|rev| resolve(ports.process(), ws, rev))
        .or_else(|| head(ports.process(), ws))
}

/// Whether [`set_base`] replaces a base the session already has (API-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Force {
    /// Always: a session that starts afresh (`startup`, `clear`, `fork`).
    Always,
    /// Only when the session has none yet: a resume or a compaction keeps its base.
    IfAbsent,
}

/// 1.x `setBase(proj, sid, force)`: records HEAD as the session's base (empty without one),
/// the time now as `started`, and the tree's fingerprint against HEAD as `start-fp`, in that
/// order; with [`Force::IfAbsent`], nothing when a base is recorded.
///
/// Each write is 1.x's `writeAtomic`, which throws, so the first that fails ends it.
pub(crate) fn set_base(
    ports: &impl Ports,
    env: &RunEnv,
    ws: &Workspace,
    sid: &str,
    force: Force,
) -> Result<(), FsError> {
    let style = env.style;
    let dir = session_dir(style, ws, sid);
    let base = paths::join(style, &dir, "base");
    if force == Force::IfAbsent && exists(ports.files(), &base) {
        return Ok(());
    }
    let head = head(ports.process(), ws);
    let files = ports.files();
    files.write_atomic(Path::new(&base), head.as_deref().unwrap_or(""))?;
    let now = ports.clock().now().unix_millis().to_string();
    files.write_atomic(Path::new(&paths::join(style, &dir, "started")), &now)?;
    let start = fingerprint(ports, env, ws, head.as_deref()).to_string();
    files.write_atomic(Path::new(&paths::join(style, &dir, "start-fp")), &start)
}

/// 1.x `startFingerprint(proj, sid)`: the session's `start-fp`, trimmed, unless it is missing,
/// unreadable or empty.
pub(crate) fn start_fingerprint(
    files: &impl Files,
    style: PathStyle,
    ws: &Workspace,
    sid: &str,
) -> Option<String> {
    let path = paths::join(style, &session_dir(style, ws, sid), "start-fp");
    read_js_text(files, &path)
        .filter(|text| !text.is_empty())
        .map(|text| js_trim(&text).to_owned())
}

/// What a command run outside a hook compares with, and the note it may print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliBase {
    /// The base: `TEMPER_BASE` as given when it is a commit here, else HEAD.
    pub(crate) base: Option<String>,
    /// For stderr: why `TEMPER_BASE` was not used, once per process.
    pub(crate) note: Option<String>,
}

/// 1.x `sessionBase(proj)`: `TEMPER_BASE` (`temper_base`, set and not empty) when it resolves
/// to a commit here, kept as given; otherwise HEAD, with a note the first time this process
/// (`noted`) passes over it. Outside git nothing resolves, so a `TEMPER_BASE` is noted there too.
pub(crate) fn cli_base(
    process: &impl ProcessRunner,
    ws: &Workspace,
    temper_base: Option<&str>,
    noted: &Cell<bool>,
) -> CliBase {
    let mut note = None;
    if let Some(given) = temper_base {
        if resolve(process, ws, given).is_some() {
            return CliBase {
                base: Some(given.to_owned()),
                note,
            };
        }
        if !noted.replace(true) {
            note = Some(temper_base_note(given));
        }
    }
    CliBase {
        base: head(process, ws),
        note,
    }
}

/// 1.x `resolveRev(proj, rev)`, which needs a git root.
fn resolve(process: &impl ProcessRunner, ws: &Workspace, rev: &str) -> Option<String> {
    git::resolve_rev(process, Path::new(ws.git_root()?), rev)
}

/// 1.x `headRev(proj)`, which needs a git root.
fn head(process: &impl ProcessRunner, ws: &Workspace) -> Option<String> {
    git::head_rev(process, Path::new(ws.git_root()?))
}

#[cfg(test)]
mod tests;
