//! Gate runs of the session whose hook was killed (1.x `hooks.mjs` `logLostGates`, #137).
//!
//! A `gate-start` row with no outcome after it, whose pid is neither this process nor alive,
//! is a gate whose hook was killed. The next hook of the session logs a `gate-lost` row for
//! it, with the step it was on from the progress file it left behind and its heartbeat. Two
//! hooks of a session can find the same lost gate at once (a Stop and a `SubagentStop`), so
//! each claims it first with a marker file only one can create; the claimant then removes the
//! gate's progress file, which has served.
//!
//! All of it is best effort, as 1.x's `catch { /* telemetry is best effort */ }`: what 1.x's
//! outer `try` catches ends the work silently, and what its inner ones catch skips a step.

use std::cmp::Ordering;
use std::path::Path;

use temper_domain::{
    JsValue, LostGateFacts, PathStyle, ProgressSeen, TelemetryEvent, lost_gate_claim,
    lost_gate_row, open_gate_starts,
};

use super::HookCall;
use super::input::{agent_key_of, given, session_segment};
use crate::engine::RunEnv;
use crate::paths;
use crate::ports::{Clock, Files, Ports};
use crate::procs::{LivePids, live_pids};
use crate::project::Workspace;
use crate::session::session_dir;
use crate::state::{ProgressFile, progress_files, read_js_text};
use crate::telemetry::{log_as, session_rows};

/// What [`log_lost_gates`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LostGatesLogged {
    /// The `gate-lost` rows written.
    pub(crate) rows: usize,
    /// The progress files removed once their row was logged.
    pub(crate) served: usize,
}

/// 1.x `logLostGates(proj, input)`: a `gate-lost` row for each gate run of the call's session
/// whose hook was killed, claimed first so that it is logged once.
pub(crate) fn log_lost_gates(call: &HookCall<'_, impl Ports>, ws: &Workspace) -> LostGatesLogged {
    let HookCall { ports, env, input } = *call;
    let (files, style) = (ports.files(), env.style);
    let mut done = LostGatesLogged::default();
    // `String(sid)` or the template literal throwing ends it, inside 1.x's outer `try`.
    let Ok(rows) = session_rows(files, style, ws, input.raw("session_id")) else {
        return done;
    };
    let Ok(open) = open_gate_starts(&rows) else {
        return done;
    };
    let lost = dead_starts(ports, env, open);
    if lost.is_empty() {
        return done;
    }
    let progress = progress_files(files, style, ws);
    let dir = session_dir(style, ws, &session_segment(input));
    if files.create_dir_all(Path::new(&dir)).is_err() {
        return done;
    }
    for start in lost {
        // A claim that cannot be made is another hook's (1.x's `catch { continue; }`), and so
        // is one whose name throws, inside the same `try`.
        let Ok(claim) = lost_gate_claim(start) else {
            continue;
        };
        if files
            .create_new(Path::new(&paths::join(style, &dir, &claim)))
            .is_err()
        {
            continue;
        }
        let beat = heartbeat(files, style, &dir, start);
        let seen = start.get("pid").and_then(|pid| progress.get(pid));
        let facts = LostGateFacts {
            start,
            progress: seen.map(|p| ProgressSeen {
                record: &p.record,
                modified: p.modified,
            }),
            heartbeat: Some(&beat),
            now: ports.clock().now(),
        };
        // 1.x's `lostGateRow` throwing ends the loop, inside the outer `try`.
        let Ok(row) = lost_gate_row(&facts) else {
            break;
        };
        // `who` is the input with the row's agent: `agent_type` is `r.agent` unless it is
        // `'main'`, which telemetry writes as `main` anyway.
        let event = TelemetryEvent::GateLost(row);
        let agent = given(Some(start), "agent");
        done.rows += usize::from(log_as(call, ws, agent, &event).is_ok());
        if let Some(p) = seen {
            done.served += usize::from(serve(files, p, start));
        }
    }
    done
}

/// 1.x's `.filter((r) => Number.isInteger(r.pid) && r.pid !== process.pid && !pidAlive(r.pid))`.
/// The live pids are read once, and only when some start might be lost (P4).
fn dead_starts<'a>(ports: &impl Ports, env: &RunEnv, open: Vec<&'a JsValue>) -> Vec<&'a JsValue> {
    let mut live: Option<LivePids> = None;
    let own = f64::from(env.pid);
    let mut lost = Vec::new();
    for row in open {
        let foreign = |pid: &&JsValue| {
            pid.as_integer()
                .is_some_and(|n| n.partial_cmp(&own) != Some(Ordering::Equal))
        };
        let Some(pid) = row.get("pid").filter(foreign) else {
            continue;
        };
        if !live.get_or_insert_with(|| live_pids(ports, env)).alive(pid) {
            lost.push(row);
        }
    }
    lost
}

/// Removes the progress file `p` of the lost gate `start`, unless another gate on the same
/// tree has written it since (its `pid` is no longer `=== r.pid`); whether it was removed. A
/// file that is gone or cannot be read or removed stays, as 1.x's `catch { /* gone */ }`.
fn serve(files: &impl Files, p: &ProgressFile, start: &JsValue) -> bool {
    let current = read_js_text(files, &p.path).and_then(|text| JsValue::from_json(&text).ok());
    let same = match (
        current.as_ref().and_then(|c| c.get("pid")),
        start.get("pid"),
    ) {
        (Some(now), Some(then)) => now.strict_equals(then),
        // A missing pid on either side is `undefined`, which a lost gate's pid never is.
        _ => false,
    };
    same && files.remove_file(Path::new(&p.path)).is_ok()
}

/// The heartbeat file of the agent that ran `start`'s gate (1.x `heartbeatPath(proj, who)`),
/// parsed: `null` when it is missing, unreadable or not JSON, or its name throws, as 1.x's
/// `catch { /* none */ }` keeps `beat = null`.
fn heartbeat(files: &impl Files, style: PathStyle, dir: &str, start: &JsValue) -> JsValue {
    let Ok(key) = agent_key_of(start.get("agent_id")) else {
        return JsValue::Null;
    };
    let path = paths::join(style, dir, &format!("gate-alive-{key}.json"));
    read_js_text(files, &path)
        .and_then(|text| JsValue::from_json(&text).ok())
        .unwrap_or(JsValue::Null)
}

#[cfg(test)]
mod tests;
