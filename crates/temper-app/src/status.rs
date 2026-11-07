//! `temper status`: the state of the current tree (1.x `scripts/temper.mjs` `status`).

use std::cell::Cell;

use temper_domain::{BaselineLine, NOT_A_RUST_PROJECT, StatusFacts, render_status_text};

use crate::changes::{fingerprint, rust_changes};
use crate::engine::{CommandError, CommandOutput, Exit, RunEnv, StatusArgs};
use crate::ports::Ports;
use crate::project;
use crate::session::{self, CliBase};
use crate::state::{cached_gate, read_baseline, verdict_for};

/// `temper status` in text, in 1.x's order: the project, the base (the session's with
/// `--session`, else `TEMPER_BASE` or HEAD, with its note on stderr), the Rust changes, the
/// fingerprint, then the gate, review and baseline records for it. Nothing past discovery can
/// fail, as in 1.x: every unreadable record reads as none (F6).
pub(crate) fn run(
    ports: &impl Ports,
    env: &RunEnv,
    base_noted: &Cell<bool>,
    args: &StatusArgs,
) -> Result<CommandOutput, CommandError> {
    let cwd = env.cwd.to_string_lossy();
    let Some(ws) = project::discover_at(ports, env, &cwd) else {
        return Err(CommandError::Refused {
            exit: Exit::Usage,
            message: NOT_A_RUST_PROJECT.to_owned(),
        });
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
    Ok(CommandOutput {
        stdout: render_status_text(&facts),
        // No note is nothing on stderr.
        stderr: note.unwrap_or_default(),
        exit: Exit::Ok,
    })
}

#[cfg(test)]
mod tests;
