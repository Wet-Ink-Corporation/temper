//! The line-based reading of a manifest that 1.x uses beside (or instead of) the TOML parse:
//! the lint and profile sections, and whether their settings changed.

use crate::js::{Value, js_json, js_trim};

use super::Manifest;

/// JS `text.split(/\r?\n/)`: split at each `\n`, dropping one `\r` before it.
pub(crate) fn lines(text: &str) -> impl Iterator<Item = &str> + '_ {
    let mut pieces = text.split('\n').peekable();
    std::iter::from_fn(move || {
        let piece = pieces.next()?;
        // Only a piece that a `\n` ended had its `\r\n` matched; the last piece keeps its `\r`.
        Some(if pieces.peek().is_some() {
            piece.strip_suffix('\r').unwrap_or(piece)
        } else {
            piece
        })
    })
}

/// The table name in a header line, as 1.x's `line.match(/^\s*\[([^\]]+)\]\s*$/)` captures it
/// (untrimmed). `[[bin]]` is no header to this reading: its second `]` is not whitespace.
pub(crate) fn header(line: &str) -> Option<&str> {
    let inner = line
        .trim_start_matches(crate::js::is_js_space)
        .strip_prefix('[')?;
    let (name, after) = inner.split_once(']')?;
    (!name.is_empty() && after.chars().all(crate::js::is_js_space)).then_some(name)
}

/// The text of the sections that set lints and profiles, as 1.x's `lintSections`: every
/// trimmed, non-empty line from a `[..lints..]` or `[profile..]` header to the next header.
///
/// # Examples
///
/// ```
/// use temper_domain::lint_sections;
///
/// let text = "[package]\nname = \"a\"\n[lints]\nworkspace = true\n";
/// assert_eq!(lint_sections(text), "[lints]\nworkspace = true");
/// ```
#[must_use]
pub fn lint_sections(text: &str) -> String {
    let mut keep = false;
    let mut out: Vec<&str> = Vec::new();
    for line in lines(text) {
        if let Some(name) = header(line) {
            keep = sets_lints_or_profile(js_trim(name));
        }
        let trimmed = js_trim(line);
        if keep && !trimmed.is_empty() {
            out.push(trimmed);
        }
    }
    out.join("\n")
}

/// 1.x `/(^|\.)lints(\.|$)|^profile(\.|$)/`: a `lints` segment anywhere, or `profile` first.
fn sets_lints_or_profile(name: &str) -> bool {
    name.split('.').any(|segment| segment == "lints") || name.split('.').next() == Some("profile")
}

/// Do the lint or profile settings differ between two manifest texts (1.x
/// `lintConfigChanged`)?
///
/// Adding or keeping a bare `[lints] workspace = true` (plain inheritance) is no change of
/// settings. An empty `before` is a new manifest; when either side is not valid TOML, the
/// [`lint_sections`] texts are compared instead.
///
/// # Examples
///
/// ```
/// use temper_domain::lint_config_changed;
///
/// assert!(!lint_config_changed("", "[package]\nname = \"a\"\n[lints]\nworkspace = true\n"));
/// assert!(lint_config_changed("[lints.rust]\nunsafe_code = \"forbid\"\n", "[package]\n"));
/// ```
#[must_use]
pub fn lint_config_changed(before: &str, after: &str) -> bool {
    let parsed_before = if before.is_empty() {
        Ok(Manifest::empty())
    } else {
        before.parse()
    };
    match (parsed_before, after.parse::<Manifest>()) {
        (Ok(b), Ok(a)) => lint_settings(&b) != lint_settings(&a),
        // Invalid TOML on either side: 1.x compares the section texts line by line.
        _ => lint_sections(before) != lint_sections(after),
    }
}

/// 1.x's `pick`: `lints` (unless it is only `workspace = true`), `workspace.lints` and `profile`,
/// each as `JSON.stringify` text, absent as `None` (which `JSON.stringify` omits).
fn lint_settings(m: &Manifest) -> [Option<String>; 3] {
    let lints = m
        .get("lints")
        .filter(|lints| !(super::inherits_lints(m) && only_one_key(lints)));
    let workspace_lints = m.get("workspace").and_then(|w| w.get("lints"));
    [lints, workspace_lints, m.get("profile")].map(|v| v.map(js_json))
}

/// `Object.keys(lints).length === 1`, for a `lints` that `inheritsLints` saw as a table.
fn only_one_key(lints: &Value) -> bool {
    matches!(lints, Value::Object(o) if o.iter().count() == 1)
}
