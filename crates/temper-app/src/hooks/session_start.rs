//! `session-start` (1.x `hooks.mjs` `sessionStart`).

use std::path::Path;

use temper_domain::{
    ContextEvent, EnvExports, Fingerprint, GateRecord, HookOutcome, JsValue, PathStyle,
    SessionStartFacts, TelemetryEvent, TreeState, env_file_lines, session_start_context,
    with_runtime_warnings,
};

use super::lost_gates::log_lost_gates;
use super::skill::skill_loaded;
use super::{HookCall, logged_then, session_segment};
use crate::changes::{fingerprint, rust_changes};
use crate::engine::{HookError, RunEnv};
use crate::paths;
use crate::ports::{Files, Ports};
use crate::project::Workspace;
use crate::retention::{prune_fingerprint_state, prune_markers, prune_sessions};
use crate::session::{Force, base_rev, set_base, start_fingerprint};
use crate::state::{cached_gate, verdict_for};
use crate::trees::note_session_tree;

/// The sources after which a session starts afresh, with a new base (1.x's
/// `['startup', 'clear', 'fork'].includes(input.source)`).
const FRESH: [&str; 3] = ["startup", "clear", "fork"];

/// 1.x `sessionStart(input)`, in its order: the base, the prunes on `startup`, the linked
/// worktree's record, the env file, the tree against the base, the telemetry row, the lost
/// gates, then the context with any runtime warnings. Outside a Rust project, nothing.
///
/// # Errors
///
/// [`HookError::NullInput`] for `null` input; [`HookError::Files`] when the base or the tree
/// record cannot be written, as 1.x's `writeAtomic` throws (fail-open F1); and
/// [`HookError::InputType`] when the recorded review's `verdict` is a value 1.x's template
/// literal throws on.
pub(crate) fn session_start(call: &HookCall<'_, impl Ports>) -> Result<HookOutcome, HookError> {
    let HookCall { ports, env, input } = *call;
    input.require_object("cwd")?;
    let Some(ws) = call.workspace() else {
        return Ok(HookOutcome::Pass);
    };
    let style = env.style;
    let source = input.raw("source");
    let is = |name: &str| source.and_then(JsValue::as_str) == Some(name);
    let force = if FRESH.iter().any(|name| is(name)) {
        Force::Always
    } else {
        Force::IfAbsent
    };
    set_base(call, &ws, force)?;
    let sid = session_segment(input);
    if is("startup") {
        prune_sessions(ports, style, &ws);
        prune_markers(ports, env);
        let protect = start_fingerprint(ports.files(), style, &ws, &sid);
        prune_fingerprint_state(ports, style, &ws, protect.as_deref());
    }
    note_session_tree(call, &ws, force)?;
    let base = base_rev(ports, style, &ws, &sid);
    let base = base.as_deref();
    write_env_file(ports, env, base);
    let tree = TreeRecords::read(ports, env, &ws, base);
    let gate_command = gate_command(env);
    // 1.x builds the state line, which can throw, before its telemetry row, and reads the skill
    // marker after its lost gates, which never touch the markers.
    let text = session_start_context(&SessionStartFacts {
        skill_loaded: skill_loaded(call),
        gate_command: &gate_command,
        cli_exported: cli_path(env).is_some(),
        base,
        state: tree.state(),
        config_problem: ws.config_problem.as_ref(),
    })?;
    let event = TelemetryEvent::SessionStart {
        source: source.map(JsValue::to_json),
    };
    let context = HookOutcome::Context {
        event: ContextEvent::SessionStart,
        text,
    };
    let outcome = with_runtime_warnings(context, &env.runtime_warnings);
    Ok(logged_then(call, &ws, &event, || {
        // Best effort, as 1.x's `catch`: what it logged changes nothing in the answer.
        let _logged = log_lost_gates(call, &ws);
        outcome
    }))
}

/// The tree against the session base, as 1.x reads it for the context's last line: the Rust
/// changes, and only when there are some the fingerprint and its gate and review records.
#[derive(Debug)]
struct TreeRecords {
    changed: usize,
    records: Option<(Fingerprint, Option<GateRecord>, Option<JsValue>)>,
}

impl TreeRecords {
    fn read(ports: &impl Ports, env: &RunEnv, ws: &Workspace, base: Option<&str>) -> Self {
        let changed = rust_changes(ports, env, ws, base).len();
        let records = (changed > 0).then(|| {
            let fp = fingerprint(ports, env, ws, base);
            let (files, style) = (ports.files(), env.style);
            (
                fp,
                cached_gate(files, style, ws, fp),
                verdict_for(files, style, ws, fp),
            )
        });
        Self { changed, records }
    }

    fn state(&self) -> TreeState<'_> {
        match &self.records {
            Some((fingerprint, gate, review)) => TreeState::Changed {
                files: self.changed,
                gate: gate.as_ref(),
                review: review.as_ref(),
                fingerprint: *fingerprint,
            },
            None => TreeState::NoChanges,
        }
    }
}

/// 1.x `cli()`: `path.join(pluginRoot(), 'scripts', 'temper.mjs')`.
fn plugin_cli(style: PathStyle, root: &str) -> String {
    let tail = match style {
        PathStyle::Posix => "scripts/temper.mjs",
        PathStyle::Windows => r"scripts\temper.mjs",
    };
    if root.is_empty() {
        // `path.join` skips an empty part.
        return tail.to_owned();
    }
    if paths::is_absolute(style, root) {
        // For an absolute root, `path.join` normalises as `path.resolve` does.
        return paths::resolve(style, root, tail);
    }
    let joined = paths::posix_normalize(&format!("{}/{tail}", paths::norm(root)));
    match style {
        PathStyle::Posix => joined,
        PathStyle::Windows => joined.replace('/', r"\"),
    }
}

/// The 1.x CLI under the plugin root, when there is one.
fn cli_path(env: &RunEnv) -> Option<String> {
    let root = env.plugin_root.as_deref()?;
    Some(plugin_cli(env.style, &root.to_string_lossy()))
}

/// 1.x `gateCmd()`: `node "<cli>" gate`; `temper gate` for the binary run without a plugin
/// root (OC-6 A).
pub(super) fn gate_command(env: &RunEnv) -> String {
    match cli_path(env) {
        Some(cli) => format!("node \"{cli}\" gate"),
        None => "temper gate".to_owned(),
    }
}

/// Appends the session's exports to `CLAUDE_ENV_FILE` in one write (1.x's `appendFileSync`,
/// then D17's lines, OC-6 A); whether it was written. Without the variable, or with nothing to
/// export, nothing is; a write that fails is ignored, as 1.x's `catch { /* ignore */ }`.
fn write_env_file(ports: &impl Ports, env: &RunEnv, base: Option<&str>) -> bool {
    let Some(file) = env.env_file.as_deref() else {
        return false;
    };
    let cli = cli_path(env).map(|cli| paths::norm(&cli).into_owned());
    let bin = env
        .temper_bin
        .as_deref()
        .map(|bin| bin.to_string_lossy().into_owned());
    let bin_dir = bin.as_deref().map(|bin| paths::dirname(env.style, bin));
    let text = env_file_lines(&EnvExports {
        cli: cli.as_deref(),
        base,
        bin: bin.as_deref(),
        bin_dir,
        style: env.style,
    });
    // `append_line` adds the last newline back.
    match text.strip_suffix('\n') {
        Some(lines) => ports.files().append_line(Path::new(file), lines).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests;
