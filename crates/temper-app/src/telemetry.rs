//! The telemetry log, `<state dir>/telemetry.jsonl` (1.x `project.mjs` `telemetry`).

use std::path::Path;

use temper_domain::{JsTypeError, JsValue, PathStyle, TelemetryEvent, TelemetryRow};

use crate::hooks::HookCall;
use crate::paths;
use crate::ports::{Clock, Files, FsError, Ports};
use crate::project::Workspace;
use crate::state::read_js_text;

const LOG: &str = "telemetry.jsonl";
const ROTATED: &str = "telemetry.1.jsonl";

/// The size at which the log rotates to [`ROTATED`], replacing the one there: 1.x's
/// `RETENTION.telemetryBytes`, so the pair stays under about twice it.
const ROTATE_AT: u64 = 1024 * 1024;

/// Appends one row for `event` to the workspace's log, rotating it first when it has reached
/// [`ROTATE_AT`]. Two hooks rotating at once can lose the older file's history, as in 1.x.
///
/// Telemetry is best effort in 1.x (`catch { /* best effort */ }`), so the caller drops the
/// error: a row that cannot be written never fails a hook.
pub(crate) fn log(
    call: &HookCall<'_, impl Ports>,
    workspace: &Workspace,
    event: &TelemetryEvent,
) -> Result<(), FsError> {
    log_as(call, workspace, call.input.given("agent_type"), event)
}

/// [`log`] with `agent` for the input's `agent_type` (`None` writes `"main"`): 1.x's
/// `telemetry(proj, who, …)` for a `who` that is the input with another agent.
///
/// # Errors
///
/// As [`log`].
pub(crate) fn log_as(
    call: &HookCall<'_, impl Ports>,
    workspace: &Workspace,
    agent: Option<&JsValue>,
    event: &TelemetryEvent,
) -> Result<(), FsError> {
    let HookCall { ports, env, input } = *call;
    let files = ports.files();
    let dir = workspace.state_dir.as_str();
    files.create_dir_all(Path::new(dir))?;
    let log = paths::join(env.style, dir, LOG);
    // An absent or unreadable log is not rotated, as 1.x's `statSync` throws inside its `try`.
    let full = matches!(files.stat(Path::new(&log)), Ok(Some(meta)) if meta.len >= ROTATE_AT);
    let rotation = if full {
        files.rename(
            Path::new(&log),
            Path::new(&paths::join(env.style, dir, ROTATED)),
        )
    } else {
        Ok(())
    };
    let row = TelemetryRow {
        at: ports.clock().now(),
        sid: input.raw("session_id"),
        ccr: env.remote_session.as_deref(),
        agent,
        event,
    };
    let appended = files.append_line(Path::new(&log), &row.to_json());
    match rotation {
        // A failed rotation (another hook rotated first, or the rename is refused) still
        // appends, as 1.x does.
        Ok(()) | Err(_) => appended,
    }
}

/// 1.x `sessionTelemetry(proj, sid)`: the rows of session `sid`, the rotated file first. Only
/// lines that hold `JSON.stringify(String(sid))` are parsed; a line that does not parse, a
/// falsy row and a row whose `sid` is not `=== sid` are skipped. A falsy `sid` has none.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x's `String(sid)` throws.
pub(crate) fn session_rows(
    files: &impl Files,
    style: PathStyle,
    workspace: &Workspace,
    sid: Option<&JsValue>,
) -> Result<Vec<JsValue>, JsTypeError> {
    let Some(sid) = sid.filter(|sid| sid.is_truthy()) else {
        return Ok(Vec::new());
    };
    let id = JsValue::String(sid.to_js_string_checked()?.into_owned()).to_json();
    let mut rows = Vec::new();
    for name in [ROTATED, LOG] {
        // A missing or unreadable file has no rows (`String(readText(…) ?? '')`).
        let Some(text) = read_js_text(files, &paths::join(style, &workspace.state_dir, name))
        else {
            continue;
        };
        let parsed = text
            .split('\n')
            .filter(|line| line.contains(&id))
            // A line that does not parse is skipped (1.x's `catch { return null; }`).
            .filter_map(|line| JsValue::from_json(line).ok())
            .filter(|row| row.is_truthy() && row.get("sid").is_some_and(|r| r.strict_equals(sid)));
        rows.extend(parsed);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests;
