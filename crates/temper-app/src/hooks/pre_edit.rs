//! `pre-edit`: `Edit`, `Write` and `MultiEdit` calls are checked before they run (1.x `hooks.mjs`
//! `preEdit` and `discoverFor`). Edits under `.git/` or the state dir and of `Cargo.lock` are
//! refused, Rust edits wait for the skill, and the edit's result goes through the domain's
//! policy: the protected-config ask, the manifest checks and the new-hit scan.
//!
//! 1.x's `noteTree` (the `tree-*.json` record of a Rust or manifest edit in a git repository
//! with a session id) arrives in port Phase 3 with the fingerprint, and `pre-edit` graduates
//! then (2-2). Without git or a session id it writes nothing, as here.

mod edited;

use std::path::Path;

use temper_domain::{
    Before, CodeKind, DependencyCheck, EditFacts, EditTarget, EditTool, FileRole, GoverningRoot,
    HookOutcome, JsValue, LazyExemptions, LazyGlobs, Manifest, ManifestEdit, ManifestEditFacts,
    ManifestFindings, RootRelation, RustEdit, STATE_DIR_DENIAL, ScanOptions, Scanner,
    TelemetryEvent, classify_path, edit_policy, is_build_script, lockfile_denial, manifest_checks,
    skill_gate_denial, skill_preloaded,
};

use self::edited::{After, edited_text};
use super::input::{given, js_string_or_empty};
use super::skill::skill_loaded;
use super::{HookCall, answered, logged};
use crate::engine::HookError;
use crate::manifests::{RootFor, own_lints_member, workspace_root_for};
use crate::paths::{self, PathEnv};
use crate::ports::{Files, Ports};
use crate::project::{Workspace, crate_of};
use crate::relpath::rel_to;

/// V8's text when a non-string reaches `path.resolve`.
const PATH_NOT_A_STRING: &str = "The \"paths[0]\" argument must be of type string";
/// V8's text when the scan splits written content that is not a string.
const CONTENT_NOT_A_STRING: &str = "src.split is not a function";
/// V8's text when `lintSections` or `depNamesByLine` splits written manifest content that is
/// not a string.
const MANIFEST_NOT_A_STRING: &str =
    "toml.split is not a function or its return value is not iterable";

/// The edited file, as the checks name it.
#[derive(Clone, Copy)]
struct Edited<'a> {
    /// `file_path` as given.
    file: &'a str,
    /// Its path relative to the project root.
    rel: &'a str,
    /// The project.
    workspace: &'a Workspace,
    /// What paths are resolved against.
    at: PathEnv<'a>,
    /// `tool_name`, when it is a string.
    tool: Option<&'a str>,
    /// The file's text before the edit; empty for a new file.
    before: &'a str,
}

/// `pre-edit`: the checks on the path, then the edit applied to the file's text, then
/// [`checked`].
pub(crate) fn pre_edit(
    call: &HookCall<'_, impl Ports>,
    scanner: &Scanner,
) -> Result<HookOutcome, HookError> {
    let input = call.input;
    input.require_object("tool_name")?;
    let tool_input = input.given("tool_input");
    let file = match given(tool_input, "file_path").filter(|f| f.is_truthy()) {
        None => return Ok(HookOutcome::Pass),
        Some(JsValue::String(file)) => file.as_str(),
        Some(_) => return Err(HookError::InputType(PATH_NOT_A_STRING)),
    };
    let Some(workspace) = call.workspace_for_file(file) else {
        return Ok(HookOutcome::Pass);
    };
    let files = call.ports.files();
    let cwd = call.env.cwd.to_string_lossy();
    let at = PathEnv {
        style: call.env.style,
        cwd: &cwd,
    };
    let rel = rel_to(files, at, &workspace.root, file);
    if rel.starts_with("..") {
        return Ok(HookOutcome::Pass);
    }
    let abs = paths::resolve(at.style, at.cwd, file);
    let state = paths::resolve(at.style, at.cwd, &workspace.state_dir);
    if abs.starts_with(&state) || rel.starts_with(".git/") || rel.contains("/.git/") {
        return Ok(deny(STATE_DIR_DENIAL.to_owned()));
    }
    if is_named(&rel, "Cargo.lock") {
        let outcome = deny(lockfile_denial(&rel));
        let event = TelemetryEvent::DenyLockfile { file: rel };
        return Ok(logged(call, &workspace, &event, outcome));
    }
    let before = match files.read_text(Path::new(&abs)) {
        Ok(Some(text)) => text,
        // 1.x's `readText(file) ?? ''`: a missing or unreadable file reads as a new one.
        Ok(None) | Err(_) => String::new(),
    };
    let edited = Edited {
        file,
        rel: &rel,
        workspace: &workspace,
        at,
        tool: input.raw("tool_name").and_then(JsValue::as_str),
        before: &before,
    };
    match edited_text(&before, edited.tool, tool_input)? {
        Some(after) => checked(call, scanner, edited, after),
        // The edit fails on its own.
        None => Ok(HookOutcome::Pass),
    }
}

/// The rest of 1.x `preEdit`: the skill gate for Rust and manifest edits, then the domain's
/// policy on the edit's result, with its telemetry row.
fn checked(
    call: &HookCall<'_, impl Ports>,
    scanner: &Scanner,
    edited: Edited<'_>,
    after: After<'_>,
) -> Result<HookOutcome, HookError> {
    let Edited {
        rel,
        workspace,
        tool,
        before,
        ..
    } = edited;
    let files = call.ports.files();
    // 1.x's case-sensitive `rel.endsWith('.rs')`.
    let rust = rel.strip_suffix(".rs").is_some();
    let manifest = is_named(rel, "Cargo.toml");
    // 1.x notes the tree here (`noteTree`); that waits for port Phase 3 (2-2).
    let config = &workspace.config;
    if (rust || manifest) && config.skill_gate() && !preloaded(call) && !skill_loaded(call) {
        // The row stores the path.
        let event = TelemetryEvent::DenySkillGate {
            file: rel.to_owned(),
        };
        return Ok(logged(call, workspace, &event, deny(skill_gate_denial())));
    }
    let after = match after {
        After::Text(text) => text,
        After::NotText(_) if rust => return Err(HookError::InputType(CONTENT_NOT_A_STRING)),
        After::NotText(value) if manifest => manifest_text(value, before)?,
        // Only the protected-config check runs, and it reads no text.
        After::NotText(_) => String::new(),
    };
    // Each glob compiles where a match first tries it, as 1.x's `globMatch` builds it.
    let protected = LazyGlobs::new(config.protected().iter().map(String::as_str));
    let exempt = LazyExemptions::new(config.exempt());
    let target = if manifest {
        EditTarget::Manifest(manifest_edit(files, edited, &after)?)
    } else if rust {
        EditTarget::Rust(RustEdit {
            options: scan_options(files, edited),
            exempt: &exempt,
        })
    } else {
        EditTarget::Other
    };
    let facts = EditFacts {
        rel,
        tool: if tool == Some("Write") {
            EditTool::Write
        } else {
            EditTool::Replace
        },
        before,
        after: &after,
        protected: &protected,
        target,
    };
    Ok(answered(call, workspace, edit_policy(facts, scanner)?))
}

/// Written manifest content that is not a string, as 1.x's checks read it: `parseToml` reads
/// `String(value)`, so the checks go on with that text when it parses and the file before the
/// edit is empty or parses. Otherwise `lintConfigChanged` falls back to `lintSections`, which
/// splits the value itself and throws.
///
/// # Errors
///
/// [`HookError::InputType`] with V8's text where 1.x throws.
fn manifest_text(value: &JsValue, before: &str) -> Result<String, HookError> {
    let text = value.to_js_string();
    let parses = |text: &str| text.parse::<Manifest>().is_ok();
    if parses(&text) && (before.is_empty() || parses(before)) {
        Ok(text.into_owned())
    } else {
        Err(HookError::InputType(MANIFEST_NOT_A_STRING))
    }
}

/// 1.x `deny(reason)`.
fn deny(reason: String) -> HookOutcome {
    HookOutcome::Deny {
        reason,
        context: None,
    }
}

/// 1.x `/(^|\/)<name>$/` on the relative path.
fn is_named(rel: &str, name: &str) -> bool {
    rel.strip_suffix(name)
        .is_some_and(|head| head.is_empty() || head.ends_with('/'))
}

/// `PRELOADED.test(input.agent_type ?? '')`.
fn preloaded(call: &HookCall<'_, impl Ports>) -> bool {
    skill_preloaded(&js_string_or_empty(call.input.given("agent_type")))
}

/// How 1.x scans the edited Rust file: classified by its crate, a build script by its path.
fn scan_options(files: &impl Files, edited: Edited<'_>) -> ScanOptions<'static> {
    let at = edited.at;
    let krate = crate_of(files, at, edited.file);
    let crate_rel = match &krate {
        Some(krate) => rel_to(files, at, &krate.dir, edited.file),
        None => edited.rel.to_owned(),
    };
    ScanOptions {
        kind: krate.map_or(CodeKind::Prod, |krate| classify_path(&crate_rel, krate.lib)),
        file: if is_build_script(&crate_rel) {
            FileRole::BuildScript
        } else {
            FileRole::Source
        },
        forbid: None,
    }
}

/// 1.x `manifestChecks(file, rel, before, after, proj)`, with `askOnNewDependency` and
/// `preapprovedDependencies`.
///
/// # Errors
///
/// [`HookError::Glob`] when a `manifests.ownLints` glob does not compile.
fn manifest_edit<'w>(
    files: &impl Files,
    edited: Edited<'w>,
    after: &str,
) -> Result<ManifestEdit<'w>, HookError> {
    let Edited {
        workspace,
        at,
        before,
        ..
    } = edited;
    let config = &workspace.config;
    let findings = match after.parse::<Manifest>() {
        // Invalid TOML: cargo rejects it on its own.
        Err(_) => ManifestFindings::default(),
        Ok(after) => {
            let root = if after.has_package() {
                workspace_root_for(files, at, edited.file, &after)
            } else {
                None
            };
            let own_lints = own_lints_member(at, config, &workspace.root, edited.file)?;
            let parsed = before.parse::<Manifest>();
            let before = match (&parsed, before.is_empty()) {
                (_, true) => Before::Empty,
                (Ok(m), false) => Before::Parsed(m),
                (Err(_), false) => Before::Invalid,
            };
            let root = root.as_ref().map(|root| match root {
                RootFor::Itself => GoverningRoot {
                    manifest: &after,
                    relation: RootRelation::Itself,
                },
                RootFor::Member(m) => GoverningRoot {
                    manifest: m,
                    relation: RootRelation::Member,
                },
            });
            manifest_checks(&ManifestEditFacts {
                rel: edited.rel,
                before,
                after: &after,
                root,
                own_lints,
            })
        }
    };
    let dependencies = if config.ask_on_new_dependency() {
        DependencyCheck::Ask {
            preapproved: config.preapproved_dependencies(),
        }
    } else {
        DependencyCheck::Off
    };
    Ok(ManifestEdit {
        findings,
        dependencies,
    })
}

#[cfg(test)]
mod tests;
