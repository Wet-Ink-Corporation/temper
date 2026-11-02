//! The telemetry log, `<state dir>/telemetry.jsonl` (1.x `project.mjs` `telemetry`).

use std::path::Path;

use temper_domain::{TelemetryEvent, TelemetryRow};

use crate::hooks::HookCall;
use crate::paths;
use crate::ports::{Clock, Files, FsError, Ports};
use crate::project::Workspace;

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
        agent: input.given("agent_type"),
        event,
    };
    let appended = files.append_line(Path::new(&log), &row.to_json());
    match rotation {
        // A failed rotation means another hook rotated first; 1.x appends to the new file.
        Ok(()) | Err(_) => appended,
    }
}

#[cfg(test)]
mod tests;
