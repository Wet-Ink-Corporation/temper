//! The skill marker and its two hooks: `post-tool` records that the rust-craft skill was
//! loaded, and `user-prompt` points out that it was not (1.x `hooks.mjs` `postTool` and
//! `userPromptSubmit`; `project.mjs` `markerDir`, `markSkillLoaded` and `skillLoaded`).

use std::path::Path;

use temper_domain::{
    ContextEvent, HookOutcome, JsTypeError, PathStyle, SKILL_NOT_LOADED, TelemetryEvent,
    skill_load_via,
};

use super::HookCall;
use super::input::{agent_key, given, js_string_or_empty, session_segment};
use crate::engine::{HookError, RunEnv};
use crate::paths;
use crate::ports::{Clock, Files, Ports};
use crate::project::exists;
use crate::telemetry;

/// The markers' directory under the temporary directory when `CLAUDE_PLUGIN_DATA` is not set.
const FALLBACK_DATA_DIR: &str = "temper-plugin-data";

/// `post-tool`: a `Skill` call of rust-craft, or a `Read` of its `SKILL.md`, marks the skill as
/// loaded in this session and agent context, with a `skill-loaded` row inside a project.
pub(crate) fn post_tool(call: &HookCall<'_, impl Ports>) -> Result<HookOutcome, HookError> {
    let input = call.input;
    input.require_object("tool_input")?;
    let tool = input.given("tool_input");
    let skill = js_string_or_empty(given(tool, "skill").or_else(|| given(tool, "name")));
    let file = js_string_or_empty(given(tool, "file_path"));
    let tool_name = input.raw("tool_name").and_then(|name| name.as_str());
    let Some(via) = skill_load_via(tool_name, &skill, &file) else {
        return Ok(HookOutcome::Pass);
    };
    mark_skill_loaded(call)?;
    let Some(workspace) = call.workspace() else {
        return Ok(HookOutcome::Pass);
    };
    match telemetry::log(call, &workspace, &TelemetryEvent::SkillLoaded { via }) {
        // Telemetry is best effort (1.x): a row that cannot be written does not fail the hook.
        Ok(()) | Err(_) => Ok(HookOutcome::Pass),
    }
}

/// `user-prompt`: inside a project with the skill gate on, says that the skill is not loaded
/// in this session yet; once it is, says nothing.
pub(crate) fn user_prompt(call: &HookCall<'_, impl Ports>) -> Result<HookOutcome, HookError> {
    call.input.require_object("session_id")?;
    // 1.x looks for the marker again after `discover`; the marker path does not depend on the
    // project, so one look answers both.
    if skill_loaded(call)? {
        return Ok(HookOutcome::Pass);
    }
    match call.workspace() {
        Some(workspace) if workspace.config.skill_gate() => Ok(HookOutcome::Context {
            event: ContextEvent::UserPromptSubmit,
            text: SKILL_NOT_LOADED.to_owned(),
        }),
        Some(_) | None => Ok(HookOutcome::Pass),
    }
}

/// 1.x `skillLoaded(proj, input)`: has the skill been loaded in this session and agent context?
///
/// # Errors
///
/// [`JsTypeError`] where 1.x's `markerDir` throws on the session id, or `agentKey` on the
/// agent id.
pub(crate) fn skill_loaded<P: Ports>(call: &HookCall<'_, P>) -> Result<bool, JsTypeError> {
    Ok(exists(call.ports.files(), &marker_path(call)?))
}

/// 1.x `markSkillLoaded`: writes the marker, holding the time.
fn mark_skill_loaded(call: &HookCall<'_, impl Ports>) -> Result<(), HookError> {
    let path = marker_path(call)?;
    let now = call.ports.clock().now().to_string();
    call.ports.files().write_atomic(Path::new(&path), &now)?;
    Ok(())
}

/// Where the skill markers' `sessions` directory lives: 1.x's
/// `process.env.CLAUDE_PLUGIN_DATA || path.join(os.tmpdir(), 'temper-plugin-data')`.
pub(crate) fn markers_root(env: &RunEnv) -> String {
    match &env.plugin_data {
        Some(dir) => dir.to_string_lossy().into_owned(),
        None => paths::join(
            env.style,
            &env.temp_dir.to_string_lossy(),
            FALLBACK_DATA_DIR,
        ),
    }
}

/// `path.join(markerDir(input), 'skill-<agent key>')`, resolved against the working directory
/// as the file system reads it: per session and agent context, independent of the project.
/// It fails where `markerDir`'s `String(session_id)` or `agentKey`'s `String(agent_id)`
/// throws, in that order.
fn marker_path<P>(call: &HookCall<'_, P>) -> Result<String, JsTypeError> {
    let env = call.env;
    let base = markers_root(env);
    let sep = match env.style {
        PathStyle::Windows => '\\',
        PathStyle::Posix => '/',
    };
    let marker = format!(
        "{base}{sep}sessions{sep}{}{sep}skill-{}",
        session_segment(call.input)?,
        agent_key(call.input)?
    );
    Ok(paths::resolve(
        env.style,
        &env.cwd.to_string_lossy(),
        &marker,
    ))
}
