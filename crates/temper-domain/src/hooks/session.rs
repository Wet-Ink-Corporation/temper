//! `SessionStart`'s texts (1.x `hooks.mjs` `sessionStart` and `withRuntimeWarnings`): the
//! lines it appends to `CLAUDE_ENV_FILE`, its context and the runtime warnings.

use std::borrow::Cow;

use super::{HookOutcome, IRON_RULES};
use crate::js::{JsTypeError, Value, js_string_checked};
use crate::{ConfigProblem, Fingerprint, GateRecord, PathStyle};

/// What `SessionStart` exports to the session's shell through `CLAUDE_ENV_FILE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvExports<'a> {
    /// The 1.x CLI, `<plugin root>/scripts/temper.mjs` with `/` separators; `None` without a
    /// plugin root (the binary run directly), which writes no `TEMPER_CLI` line (OC-6 A).
    pub cli: Option<&'a str>,
    /// The session base, when there is one.
    pub base: Option<&'a str>,
    /// This engine's executable, as the OS reports it (D17).
    pub bin: Option<&'a str>,
    /// The directory that holds it (D17).
    pub bin_dir: Option<&'a str>,
    /// The platform's path rules: on Windows the directory goes on `PATH` in MSYS form.
    pub style: PathStyle,
}

/// The text `SessionStart` appends to `CLAUDE_ENV_FILE`, each line ending in `\n`.
///
/// The lines are 1.x's `export TEMPER_CLI="…"` and `export TEMPER_BASE="…"` byte for byte, then D17's
/// `export TEMPER_BIN="…"` and `export PATH="<dir>:$PATH"` (owner, wi-1d4cd6; from port Phase
/// 3d, OC-6 A, wi-168ff8; D16(w)). The D17 values use `/` separators, the directory is in MSYS
/// form on Windows (`/c/…`, as Git Bash splits `PATH` on `:`), and `\`, `"`, `$` and `` ` ``
/// in them are escaped for the shell's double quotes. 1.x's lines are written as 1.x writes
/// them, unescaped.
///
/// # Examples
///
/// ```
/// use temper_domain::{EnvExports, PathStyle, env_file_lines};
///
/// let lines = env_file_lines(&EnvExports {
///     cli: Some("/p/scripts/temper.mjs"),
///     base: Some("abc"),
///     bin: Some(r"C:\t\temper.exe"),
///     bin_dir: Some(r"C:\t"),
///     style: PathStyle::Windows,
/// });
/// assert_eq!(
///     lines,
///     "export TEMPER_CLI=\"/p/scripts/temper.mjs\"\nexport TEMPER_BASE=\"abc\"\n\
///      export TEMPER_BIN=\"C:/t/temper.exe\"\nexport PATH=\"/c/t:$PATH\"\n"
/// );
/// ```
#[must_use]
pub fn env_file_lines(exports: &EnvExports<'_>) -> String {
    let mut out = String::new();
    if let Some(cli) = exports.cli {
        export(&mut out, "TEMPER_CLI", cli, "");
    }
    if let Some(base) = exports.base.filter(|base| !base.is_empty()) {
        export(&mut out, "TEMPER_BASE", base, "");
    }
    if let Some(bin) = exports.bin {
        export(
            &mut out,
            "TEMPER_BIN",
            &shell_quoted(&bin.replace('\\', "/")),
            "",
        );
    }
    if let Some(dir) = exports.bin_dir {
        let dir = dir.replace('\\', "/");
        let dir = match exports.style {
            PathStyle::Windows => msys_form(&dir),
            PathStyle::Posix => dir,
        };
        export(&mut out, "PATH", &shell_quoted(&dir), ":$PATH");
    }
    out
}

/// Appends `export <name>="<value><tail>"` and a newline.
fn export(out: &mut String, name: &str, value: &str, tail: &str) {
    for part in ["export ", name, "=\"", value, tail, "\"\n"] {
        out.push_str(part);
    }
}

/// `C:/x` as Git Bash names it, `/c/x`; anything without a drive is left as it is.
fn msys_form(path: &str) -> String {
    let mut chars = path.chars();
    match (chars.next(), chars.next()) {
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic() => {
            let rest = chars.as_str();
            let rest = if rest.is_empty() { "/" } else { rest };
            format!("/{}{rest}", drive.to_ascii_lowercase())
        }
        _ => path.to_owned(),
    }
}

/// What the tree holds against the session base, for the context's last line.
#[derive(Debug, Clone, Copy)]
pub enum TreeState<'a> {
    /// No Rust file changed since the base.
    NoChanges,
    /// Rust files changed since the base.
    Changed {
        /// How many.
        files: usize,
        /// The cached gate result for the tree, as `cachedGate` read it.
        gate: Option<&'a GateRecord>,
        /// The review recorded for the tree, as `verdictFor` read it.
        review: Option<&'a Value>,
        /// The tree's fingerprint against the base.
        fingerprint: Fingerprint,
    },
}

/// 1.x's `state`, the context's last words, rendered on its own.
///
/// 1.x builds it, where its template literal can throw, before its telemetry row and lost
/// gates, and reads the skill marker only after them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionState(String);

impl SessionState {
    /// `no Rust changes since the session base`, or `${n} Rust file(s) changed; gate …;
    /// review … (fingerprint …)`.
    ///
    /// # Errors
    ///
    /// [`JsTypeError`] where 1.x's template literal throws on the review's `verdict` (an object
    /// with its own `toString` key), outside any `try`, so the hook fails.
    pub fn new(state: &TreeState<'_>) -> Result<Self, JsTypeError> {
        tree_state(state).map(Self)
    }
}

/// What [`session_start_context`] says.
#[derive(Debug, Clone, Copy)]
pub struct SessionStartFacts<'a> {
    /// The rust-craft skill is loaded in this session.
    pub skill_loaded: bool,
    /// The gate command (1.x `gateCmd()`, or `temper gate` without a plugin root, OC-6 A).
    pub gate_command: &'a str,
    /// `TEMPER_CLI` is exported to the session, so the command names it as an alias too.
    pub cli_exported: bool,
    /// The session base, `None` outside git.
    pub base: Option<&'a str>,
    /// The tree against the base, as the context's last words.
    pub state: &'a SessionState,
    /// What was wrong with `.claude/temper.json`, for OC-5's line.
    pub config_problem: Option<&'a ConfigProblem>,
}

/// 1.x `sessionStart`'s context text, byte for byte, without a final newline.
///
/// When the config could not be read, OC-5's `- Config: unreadable (<why>): defaults in use.` (owner,
/// wi-018023). Without `TEMPER_CLI` exported, the gate command has no alias (OC-6 A). The one
/// part that can throw in 1.x, the state, is [`SessionState`]'s.
#[must_use]
pub fn session_start_context(facts: &SessionStartFacts<'_>) -> String {
    let loaded = if facts.skill_loaded {
        "is loaded in this session"
    } else {
        "has not been loaded in this session yet"
    };
    let alias = if facts.cli_exported {
        r#" (also available as: node "$TEMPER_CLI" gate)"#
    } else {
        ""
    };
    let mut lines = vec![
        "This repository is a Rust workspace governed by the temper plugin and the rust-craft engineering standard.".to_owned(),
        format!("- The standard is the skill `temper:rust-craft`. It {loaded}. Edits to .rs and Cargo.toml files are refused until it has been loaded in the current agent context."),
        format!("- Iron rules (production code): {}.", IRON_RULES.join("; ")),
        "- Workflow: orient, frame, plan (.temper/plans/), red, green, refactor, self-review, gate, independent review by the temper:rust-reviewer subagent, then report with the Status block.".to_owned(),
        format!("- Gate command: {}{alias}. It runs cargo fmt, the escape-hatch scan, the member-manifest check, clippy -D warnings, tests and rustdoc -D warnings, cached per tree fingerprint. When .claude/temper-baseline.json exists, clippy and rustdoc findings ratchet against it: legacy debt passes; findings on changed lines and counts above the baseline fail.", facts.gate_command),
        "- A turn that changed Rust ends when the gate is green and temper:rust-reviewer has approved the current tree. To pause instead, the final message carries a line \"Status: WIP\" or \"Status: BLOCKED\" with the reason or question.".to_owned(),
        "- Edits to lint, format, toolchain, CI, hook and agent configuration, new dependencies, lint exceptions and removed tests are routed to the human for approval.".to_owned(),
        format!(
            "- Session base revision: {}. Current state: {}.",
            facts.base.unwrap_or("none (no git)"),
            facts.state.0
        ),
    ];
    if let Some(problem) = facts.config_problem {
        lines.push(format!(
            "- Config: unreadable ({problem}): defaults in use."
        ));
    }
    lines.join("\n")
}

/// 1.x's `state`: `${n} Rust file(s) changed; gate …; review … (fingerprint …)`.
fn tree_state(state: &TreeState<'_>) -> Result<String, JsTypeError> {
    let TreeState::Changed {
        files,
        gate,
        review,
        fingerprint,
    } = *state
    else {
        return Ok("no Rust changes since the session base".to_owned());
    };
    let gate = match gate.filter(|g| g.is_present()) {
        Some(g) if g.ok() => "green",
        Some(_) => "red",
        None => "not run",
    };
    let review = match review.filter(|v| v.is_truthy()) {
        Some(v) => match v.get("verdict") {
            Some(verdict) => js_string_checked(verdict)?,
            None => Cow::Borrowed("undefined"),
        },
        None => Cow::Borrowed("none"),
    };
    Ok(format!(
        "{files} Rust file(s) changed; gate {gate}; review {review} (fingerprint {fingerprint})"
    ))
}

/// 1.x `withRuntimeWarnings(out, warnings)`: the warnings for the human and the model.
///
/// Each warning goes to the human (`systemMessage`, one per line) and to the model (a `- <warning>` line after the context). Unchanged with no
/// warnings, and for an answer that is not a context, which 1.x never gives it.
#[must_use]
pub fn with_runtime_warnings(outcome: HookOutcome, warnings: &[String]) -> HookOutcome {
    match outcome {
        HookOutcome::Context { event, mut text } if !warnings.is_empty() => {
            for warning in warnings {
                text.push_str("\n- ");
                text.push_str(warning);
            }
            HookOutcome::ContextWithNotice {
                event,
                text,
                notice: warnings.join("\n"),
            }
        }
        other => other,
    }
}

/// `text` safe inside a shell's double quotes.
fn shell_quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '"' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests;
