//! `temper status`'s text and JSON (1.x `scripts/temper.mjs` `status`, `statusJson`).

mod json;

pub use json::{StatusDoc, render_no_project_json, render_status_json};

use std::borrow::Cow;

use crate::js::{JsTypeError, Value, js_string_checked, truncate_utf16};
use crate::state::{BaselineParseError, BaselineSummary, GateRecord};
use crate::{BASELINE_FILE, Config, ConfigProblem, Fingerprint, ReviewMode};

/// `temper status`'s usage line, for stderr: 1.x's text.
pub const STATUS_USAGE: &str = "usage: temper status [--json] [--session ID]\n";

/// What `status` found about the baseline.
#[derive(Debug, Clone, Copy)]
pub enum BaselineLine<'a> {
    /// There is no baseline file.
    Absent,
    /// The baseline, read.
    Read(&'a BaselineSummary),
    /// The file is there but 1.x's reader throws on it.
    Unreadable(&'a BaselineParseError),
}

/// Everything `status` prints, gathered by the use case.
#[derive(Debug, Clone, Copy)]
pub struct StatusFacts<'a> {
    /// The project root.
    pub root: &'a str,
    /// The state dir.
    pub state_dir: &'a str,
    /// The base the tree is compared with, if any.
    pub base: Option<&'a str>,
    /// The tree's fingerprint.
    pub fingerprint: Fingerprint,
    /// The Rust changes since the base.
    pub changed: &'a [String],
    /// The cached gate result for the fingerprint, if any.
    pub gate: Option<&'a GateRecord>,
    /// The recorded review for the fingerprint (any JSON), if any.
    pub review: Option<&'a Value>,
    /// The baseline.
    pub baseline: BaselineLine<'a>,
    /// The config in use.
    pub config: &'a Config,
    /// What was wrong with the config file, if anything (D16(d)).
    pub config_problem: Option<&'a ConfigProblem>,
}

/// `temper status`'s nine lines, each ending in `\n`, as 1.x writes them; a config problem is
/// appended to the `config:` line after `; ` (OC-5 A, wi-018023).
///
/// # Examples
///
/// ```
/// use temper_domain::{BaselineLine, Config, Fingerprint, StatusFacts, render_status_text};
///
/// let config = Config::defaults();
/// let facts = StatusFacts {
///     root: "/w", state_dir: "/w/.git/temper", base: None,
///     fingerprint: "0123456789abcdef".parse()?, changed: &[], gate: None, review: None,
///     baseline: BaselineLine::Absent, config: &config, config_problem: None,
/// };
/// assert!(render_status_text(&facts)?.starts_with("root:        /w\n"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
///
/// [`JsTypeError`] when the review's `verdict` is a value 1.x's template literal cannot
/// convert (an object with its own `toString` key, also inside an array): 1.x throws there,
/// outside any `try`, and `status` fails.
pub fn render_status_text(facts: &StatusFacts<'_>) -> Result<String, JsTypeError> {
    let changed = if facts.changed.is_empty() {
        "none".to_owned()
    } else {
        facts.changed.join(", ")
    };
    let gate = match facts.gate {
        Some(g) if g.is_present() && g.ok() => "green",
        Some(g) if g.is_present() => "red",
        Some(_) | None => "not run for this fingerprint",
    };
    let review = match facts.review.filter(|v| v.is_truthy()) {
        Some(v) => {
            let verdict = match v.get("verdict") {
                Some(verdict) => js_string_checked(verdict)?,
                None => Cow::Borrowed("undefined"),
            };
            let stale = v.get("stale").is_some_and(Value::is_truthy);
            format!("{verdict}{}", if stale { " (stale)" } else { "" })
        }
        None => "none for this fingerprint".to_owned(),
    };
    let config = facts.config;
    let problem = facts.config_problem.map_or(String::new(), |problem| {
        format!("; config unreadable ({problem}): defaults in use")
    });
    Ok([
        format!("root:        {}", facts.root),
        format!("state:       {}", facts.state_dir),
        format!("base:        {}", facts.base.unwrap_or("none")),
        format!("fingerprint: {}", facts.fingerprint),
        format!("changed:     {changed}"),
        format!("gate:        {gate}"),
        format!("review:      {review}"),
        format!("baseline:    {}", baseline_line(&facts.baseline)),
        format!(
            "config:      review={} skillGate={}{problem}",
            review_name(config.review()),
            config.skill_gate()
        ),
        String::new(),
    ]
    .join("\n"))
}

/// The text after `baseline:`.
fn baseline_line(line: &BaselineLine<'_>) -> String {
    match line {
        BaselineLine::Absent => "none: the gate requires zero warnings".to_owned(),
        BaselineLine::Read(b) => {
            // `${b.toolchain ?? '?'}` runs inside 1.x's `try`, so a throw there is unreadable.
            let toolchain = match b.toolchain().filter(|t| **t != Value::Null) {
                Some(t) => match js_string_checked(t) {
                    Ok(text) => text,
                    Err(error) => {
                        return format!("unreadable: {}", BaselineParseError::NoPrimitive(error));
                    }
                },
                None => Cow::Borrowed("?"),
            };
            format!(
                "{BASELINE_FILE} ({} finding(s), toolchain {toolchain}): clippy and rustdoc ratchet",
                b.total().to_js_string()
            )
        }
        BaselineLine::Unreadable(error) => format!("unreadable: {error}"),
    }
}

/// The config's `review` as 1.x's JSON writes it.
fn review_name(mode: ReviewMode) -> &'static str {
    match mode {
        ReviewMode::Required => "required",
        ReviewMode::Advisory => "advisory",
        ReviewMode::Off => "off",
    }
}

/// 1.x `sessionBase`'s note for a `TEMPER_BASE` that is not a commit here, with the base cut
/// to 12 UTF-16 units, never splitting a surrogate pair (D16(o)); for stderr.
#[must_use]
pub fn temper_base_note(base: &str) -> String {
    format!(
        "temper: TEMPER_BASE {} is not a commit in this repository (another session's base?); using HEAD.\n",
        truncate_utf16(base, 12)
    )
}

#[cfg(test)]
mod tests;
