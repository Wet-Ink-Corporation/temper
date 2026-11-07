//! The summaries the hooks print of a verdict (1.x `summarizeFindings`, `earlierReview`,
//! `oneLine`).

use std::borrow::Cow;

use crate::js::{JsTypeError, Value, is_js_space, js_string, js_trim};

use super::VerdictContext;
use super::problems::{GateState, ProblemKind, VerdictField, is_serious, low_scores, problems_of};

/// 1.x `(v.findings ?? [])` where the next call is an array method: an array, or `undefined` or
/// `null` as none.
fn findings_of(v: &Value) -> Result<&[Value], JsTypeError> {
    if matches!(v, Value::Null) {
        return Err(JsTypeError::new(
            "the verdict is null, which 1.x reads findings of",
        ));
    }
    match v.get("findings") {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(JsTypeError::new(
            "the verdict's findings are not an array, which 1.x filters",
        )),
    }
}

/// 1.x `oneLine(x)`: `String(x ?? '')` on one line.
fn one_line_of(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(value) => one_line(&js_string(value)).into_owned(),
    }
}

/// 1.x `summarizeFindings(v, max)`: a line per blocker or major finding, at most `max`, then a
/// count of the others.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x throws: `v` is `null`, or its `findings` is present and neither
/// an array nor `null`.
pub fn summarize_findings(verdict: &Value, max: usize) -> Result<String, JsTypeError> {
    let findings = findings_of(verdict)?;
    let mut serious = 0_usize;
    let mut lines = Vec::new();
    for f in findings.iter().filter(|f| is_serious(f)) {
        serious += 1;
        if lines.len() < max {
            lines.push(finding_line(f));
        }
    }
    let mut text = lines.join("\n");
    let rest = findings.len() - serious;
    if rest > 0 {
        text.push_str("\n(");
        text.push_str(&rest.to_string());
        text.push_str(" minor/nit finding(s) in the full verdict)");
    }
    Ok(text)
}

/// `- [severity] rule file:line: fix`, each field on one line.
fn finding_line(f: &Value) -> String {
    let field = |key: &str| one_line_of(f.get(key));
    // Only serious findings are listed, so the severity is `blocker` or `major`.
    let severity = f
        .get("severity")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let line = match f.get("line") {
        Some(line) if line.is_truthy() => format!(":{}", one_line_of(Some(line))),
        _ => String::new(),
    };
    format!(
        "- [{severity}] {} {}{line}: {}",
        field("rule"),
        field("file"),
        field("fix")
    )
}

/// 1.x `overturns(v)`: whether `v`, read as an approval, has a reason to overturn one.
fn overturns(v: &Value) -> Result<bool, JsTypeError> {
    let ctx = VerdictContext {
        changed_rust: &[],
        gate: GateState::NotRed,
    };
    let problems = problems_of(v, VerdictField::Approved, &ctx)?;
    Ok(problems.iter().any(|p| {
        matches!(
            p.kind(),
            ProblemKind::ApprovedWithFindings
                | ProblemKind::ApprovedLowScore
                | ProblemKind::ApprovedGateEntry
        )
    }))
}

/// 1.x `failedGate(v)`: the keys of `Object.entries(v.gate ?? {})` whose value is neither
/// `pass` nor `n/a`. A string's entries are its UTF-16 code units, none of which passes.
fn failed_gate(v: &Value) -> Vec<String> {
    let fails = |x: &Value| !matches!(x.as_str(), Some("pass" | "n/a"));
    match v.get("gate") {
        Some(Value::Object(gate)) => gate
            .iter()
            .filter(|(_, x)| fails(x))
            .map(|(k, _)| one_line(k).into_owned())
            .collect(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .filter(|(_, x)| fails(x))
            .map(|(i, _)| i.to_string())
            .collect(),
        Some(Value::String(s)) => (0..s.encode_utf16().count())
            .map(|i| i.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// 1.x `earlierReview(v, kept)`: the reviewer's lines for a tree that already has verdict `v`,
/// recorded with `kept` later verdicts as history beside it.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x throws: as [`summarize_findings`], and a `null` finding when no
/// finding is serious, which 1.x's `overturns` reads the severity of.
pub fn earlier_review(verdict: &Value, kept: usize) -> Result<String, JsTypeError> {
    let summary = summarize_findings(verdict, 10)?;
    let list = js_trim(&summary);
    let outcome = verdict.get("verdict");
    let recorded_at = match verdict.get("recorded_at") {
        None | Some(Value::Null) => "an unknown time".to_owned(),
        Some(at) => one_line(&js_string(at)).into_owned(),
    };
    let named = outcome.map_or(Cow::Borrowed("undefined"), js_string);
    let head =
        format!("- An earlier review of this same tree is recorded: {named} at {recorded_at}");
    let first = if findings_of(verdict)?.iter().any(is_serious) {
        format!("{head}.\n{list}")
    } else {
        let tail = if overturns(verdict)? {
            "."
        } else {
            ", with no blocker or major finding."
        };
        let list = if list.is_empty() {
            String::new()
        } else {
            format!("\n{list}")
        };
        format!("{head}{tail}{list}")
    };
    let mut lines = vec![first];
    let low = low_scores(verdict.get("scores"));
    if !low.is_empty() {
        lines.push(format!("  Scores below 3: {}.", low.join(", ")));
    }
    let gate = failed_gate(verdict);
    if !gate.is_empty() {
        lines.push(format!("  Gate entries not \"pass\": {}.", gate.join(", ")));
    }
    let approved = outcome.and_then(Value::as_str) == Some("approved");
    if approved && kept > 0 {
        lines.push(format!("  {kept} later changes-requested verdict(s) gave no reason to overturn it and are kept as history beside it."));
    }
    lines.push("  Re-check each earlier finding, and say in your summary which still hold and which no longer do, with evidence.".to_owned());
    if approved {
        lines.push("  Overturn that approval only with a blocker or major finding, a score below 3 or a gate entry that is not \"pass\", quoting the evidence: a changes-requested verdict without one is kept as history and does not replace the approval.".to_owned());
    }
    Ok(lines.join("\n"))
}

/// A line terminator of 1.x's `[\r\n  \u0085]`. All but U+0085 are also JS `\s`.
fn is_break(c: char) -> bool {
    matches!(c, '\r' | '\n' | '\u{2028}' | '\u{2029}' | '\u{85}')
}

/// 1.x `s.replace(/\s*[\r\n  \u0085]+\s*/g, ' ')` with JS's `\s`, matched by hand in
/// one pass: each search resumes where the last match ended, so the whole is linear (P15),
/// where V8 retries `\s*` at every start of a long run of spaces.
///
/// Every offset below comes from `char_indices` or `find` on `text`, so each `get` is on a char
/// boundary and the empty fallbacks are never taken.
pub(crate) fn one_line(text: &str) -> Cow<'_, str> {
    let mut out: Option<String> = None;
    let mut copied = 0;
    while let Some((start, end)) = next_break(text, copied) {
        let buf = out.get_or_insert_with(String::new);
        buf.push_str(text.get(copied..start).unwrap_or_default());
        buf.push(' ');
        copied = end;
    }
    match out {
        None => Cow::Borrowed(text),
        Some(mut buf) => {
            buf.push_str(text.get(copied..).unwrap_or_default());
            Cow::Owned(buf)
        }
    }
}

/// The leftmost-first match of `\s*[breaks]+\s*` at or after `from`, as a backtracking engine
/// finds it: it starts where the run of `\s` holding a break begins, or at a U+0085 that
/// follows no `\s`. With the greedy `\s*` the run is taken whole; a U+0085 right after it
/// extends the match through more breaks and `\s`, else the match ends with the run.
fn next_break(text: &str, from: usize) -> Option<(usize, usize)> {
    let mut run: Option<usize> = None;
    let mut has_break = false;
    for (offset, c) in text.get(from..)?.char_indices() {
        let at = from + offset;
        if is_js_space(c) {
            run.get_or_insert(at);
            has_break |= is_break(c);
            continue;
        }
        if c == '\u{85}' {
            return Some((run.unwrap_or(at), tail_end(text, at)));
        }
        if let Some(start) = run.filter(|_| has_break) {
            return Some((start, at));
        }
        run = None;
        has_break = false;
    }
    run.filter(|_| has_break).map(|start| (start, text.len()))
}

/// Where `[breaks]+\s*` starting at `at` ends.
fn tail_end(text: &str, at: usize) -> usize {
    let rest = text.get(at..).unwrap_or_default();
    let breaks = rest.find(|c| !is_break(c)).unwrap_or(rest.len());
    let after = rest.get(breaks..).unwrap_or_default();
    let spaces = after.find(|c| !is_js_space(c)).unwrap_or(after.len());
    at + breaks + spaces
}
