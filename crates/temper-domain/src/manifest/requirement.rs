//! Version requirement strings: their semver-compatibility class and the caret-floor rule
//! (ARC-12).

use crate::js::js_trim;

/// The semver-compatibility class of a requirement, as 1.x's `majorOf`.
///
/// That is the major, or `0.minor` below 1.0, or `0.0.patch` below 0.1. It reads the first run
/// of digits with up to two `.n` parts, and is `None` when the text has no digit.
///
/// # Examples
///
/// ```
/// use temper_domain::major_of;
///
/// assert_eq!(major_of("^1.4").as_deref(), Some("1"));
/// assert_eq!(major_of("0.9.1").as_deref(), Some("0.9"));
/// assert_eq!(major_of("*"), None);
/// ```
#[must_use]
pub fn major_of(req: &str) -> Option<String> {
    let start = req.find(|c: char| c.is_ascii_digit())?;
    let rest = req.get(start..)?;
    let (a, rest) = digits(rest)?;
    let (b, rest) = dotted(rest).unzip();
    let c = rest.and_then(dotted).map(|(c, _)| c);
    Some(match (a, b, c) {
        (a, _, _) if a != "0" => a.to_owned(),
        (_, None, _) => "0".to_owned(),
        (_, Some(b), _) if b != "0" => format!("0.{b}"),
        (_, Some(_), None) => "0.0".to_owned(),
        (_, Some(_), Some(c)) => format!("0.0.{c}"),
    })
}

/// A leading run of ASCII digits (JS `\d+`, greedy) and the rest.
fn digits(text: &str) -> Option<(&str, &str)> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (run, rest) = text.split_at_checked(end)?;
    (!run.is_empty()).then_some((run, rest))
}

/// JS `(?:\.(\d+))?`: a `.` and a digit run, if both are there.
fn dotted(text: &str) -> Option<(&str, &str)> {
    digits(text.strip_prefix('.')?)
}

/// Why a version requirement is not a caret floor (ARC-12), or `None` if it is one.
///
/// As 1.x's `nonCaretRequirement`, the comma-separated parts, trimmed, are checked for `=`,
/// `~`, `*`, `<` and an open-ended `>` in that order.
///
/// # Examples
///
/// ```
/// use temper_domain::non_caret_requirement;
///
/// assert_eq!(non_caret_requirement("1.4"), None);
/// assert_eq!(non_caret_requirement("=1.4.2"), Some("an exact `=` pin"));
/// ```
#[must_use]
pub fn non_caret_requirement(req: &str) -> Option<&'static str> {
    let parts: Vec<&str> = req
        .split(',')
        .map(js_trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let any = |test: fn(&str) -> bool| parts.iter().any(|p| test(p));
    if any(|p| p.starts_with('=')) {
        return Some("an exact `=` pin");
    }
    if any(|p| p.starts_with('~')) {
        return Some("a tilde `~` requirement");
    }
    if any(|p| p.contains('*')) {
        return Some("a wildcard `*`");
    }
    if any(|p| p.starts_with('<')) {
        return Some("an upper bound `<`");
    }
    if parts.iter().all(|p| p.starts_with('>')) {
        return Some("an open-ended `>=`");
    }
    None
}
