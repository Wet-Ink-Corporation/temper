//! `temper status`: the state of the current tree (1.x `scripts/temper.mjs` `status` and
//! `statusJson`).

use std::cell::Cell;

use temper_domain::{
    BaselineLine, Fingerprint, JsTypeError, JsValue, NOT_A_RUST_PROJECT, ProgressQuery, StatusDoc,
    StatusFacts, gate_progress, render_no_project_json, render_status_json, render_status_text,
};

use crate::changes::{fingerprint, rust_changes};
use crate::engine::{CommandError, CommandOutput, Exit, OutputFormat, RunEnv, StatusArgs};
use crate::paths;
use crate::ports::{Clock, Ports};
use crate::procs::live_pids;
use crate::project::{self, Workspace};
use crate::session::{self, CliBase};
use crate::state::{cached_gate, read_baseline, read_js_text, verdict_for};

/// `temper status` in text or JSON, in 1.x's order: the project, the base (the session's with
/// `--session`, else `TEMPER_BASE` or HEAD, with its note on stderr), the Rust changes, the
/// fingerprint, then the gate, review and baseline records for it, and for JSON a gate running
/// now. Past discovery, only a recorded value 1.x's `String()` throws on fails: the review
/// verdict in text (wi-32d7c9), a running gate's start time in JSON; every unreadable record
/// reads as none (F6).
pub(crate) fn run(
    ports: &impl Ports,
    env: &RunEnv,
    base_noted: &Cell<bool>,
    args: &StatusArgs,
) -> Result<CommandOutput, CommandError> {
    let version = env
        .plugin_version
        .as_deref()
        .unwrap_or(env!("CARGO_PKG_VERSION"));
    let cwd = env.cwd.to_string_lossy();
    let Some(ws) = project::discover_at(ports, env, &cwd) else {
        return outside_a_project(args.format, version);
    };
    let style = env.style;
    let CliBase { base, note } = match &args.session {
        Some(sid) => CliBase {
            base: session::base_rev(ports, style, &ws, sid),
            note: None,
        },
        None => session::cli_base(ports.process(), &ws, env.temper_base.as_deref(), base_noted),
    };
    let base = base.as_deref();
    let changed = rust_changes(ports, env, &ws, base);
    let fp = fingerprint(ports, env, &ws, base);
    let files = ports.files();
    let gate = cached_gate(files, style, &ws, fp);
    let review = verdict_for(files, style, &ws, fp);
    let baseline = read_baseline(files, style, &ws);
    let facts = StatusFacts {
        root: &ws.root,
        state_dir: &ws.state_dir,
        base,
        fingerprint: fp,
        changed: &changed,
        gate: gate.as_ref(),
        review: review.as_ref(),
        baseline: match &baseline {
            None => BaselineLine::Absent,
            Some(Ok(summary)) => BaselineLine::Read(summary),
            Some(Err(error)) => BaselineLine::Unreadable(error),
        },
        config: &ws.config,
        config_problem: ws.config_problem.as_ref(),
    };
    let stdout = match args.format {
        OutputFormat::Text => render_status_text(&facts),
        OutputFormat::Json => running_gate(ports, env, &ws, fp).map(|running| {
            let doc = StatusDoc {
                version,
                running: running.as_ref(),
                disabled: env.disabled,
            };
            render_status_json(&facts, &doc)
        }),
    };
    Ok(CommandOutput {
        stdout: stdout.map_err(CommandError::Unconvertible)?,
        // No note is nothing on stderr.
        stderr: note.unwrap_or_default(),
        exit: Exit::Ok,
    })
}

/// Outside a Rust project: the JSON says `project: null` (exit 0); the text refuses (exit 2).
fn outside_a_project(format: OutputFormat, version: &str) -> Result<CommandOutput, CommandError> {
    match format {
        OutputFormat::Json => Ok(CommandOutput {
            stdout: render_no_project_json(version),
            stderr: String::new(),
            exit: Exit::Ok,
        }),
        OutputFormat::Text => Err(CommandError::Refused {
            exit: Exit::Usage,
            message: NOT_A_RUST_PROJECT.to_owned(),
        }),
    }
}

/// 1.x `gateProgress(proj, fp)`: the progress file of a gate running now for `fp`, if any.
/// The pid probe runs only when the file is for this tree, and at most once (OC-2 A).
fn running_gate(
    ports: &impl Ports,
    env: &RunEnv,
    ws: &Workspace,
    fp: Fingerprint,
) -> Result<Option<JsValue>, JsTypeError> {
    let path = paths::join(
        env.style,
        &ws.state_dir,
        &format!("gate-progress-{fp}.json"),
    );
    // A missing or unreadable file is 1.x's `readText` null: no gate running.
    let Some(text) = read_js_text(ports.files(), &path) else {
        return Ok(None);
    };
    let query = ProgressQuery {
        fp,
        now: ports.clock().now(),
        step_timeout_sec: ws.config.gate().step_timeout_sec(),
    };
    gate_progress(&text, &query, |pid| live_pids(ports, env).alive(pid))
}

#[cfg(test)]
mod tests;
