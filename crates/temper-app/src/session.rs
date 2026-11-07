//! The session's state under `<state dir>/sessions/<sid>/` and the base it compares the tree
//! with (1.x `project.mjs` `sessionDir`, `baseRev`; `temper.mjs` `sessionBase`).

use std::cell::Cell;
use std::path::Path;

use temper_domain::{PathStyle, js_trim, temper_base_note};

use crate::git;
use crate::paths;
use crate::ports::{Ports, ProcessRunner};
use crate::project::Workspace;
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
