//! Reads of the files temper keeps, with 1.x's fallbacks (`readText` and the readers on it).

use std::path::Path;

use temper_domain::{
    BaselineParseError, BaselineSummary, Fingerprint, GateRecord, JsValue, PathStyle,
};

use crate::paths;
use crate::ports::Files;
use crate::project::Workspace;

/// 1.x `readText(p)`: the file's text, or `None` for any failure, which 1.x's `catch` turns
/// into `null`. The `Files` port already refuses a text past V8's longest string (3-24), so a
/// file 1.x cannot read is `None` here too.
pub(crate) fn read_js_text(files: &impl Files, path: &str) -> Option<String> {
    // Unreadable, or longer than a JS string, is 1.x's `catch { return null; }`.
    files.read_text(Path::new(path)).ok().flatten()
}

/// 1.x `cachedGate(proj, fp)`: `<state dir>/gate/<fp>.json` parsed, or `None` when it is
/// missing, empty, unreadable or not JSON.
pub(crate) fn cached_gate(
    files: &impl Files,
    style: PathStyle,
    ws: &Workspace,
    fp: Fingerprint,
) -> Option<GateRecord> {
    let text = record_text(files, style, &paths::join(style, &ws.state_dir, "gate"), fp)?;
    GateRecord::from_json(&text)
}

/// 1.x `verdictFor(proj, fp)`: `<state dir>/verdicts/<fp>.json` parsed, any JSON, or `None`
/// as [`cached_gate`].
pub(crate) fn verdict_for(
    files: &impl Files,
    style: PathStyle,
    ws: &Workspace,
    fp: Fingerprint,
) -> Option<JsValue> {
    let text = record_text(
        files,
        style,
        &paths::join(style, &ws.state_dir, "verdicts"),
        fp,
    )?;
    // Not JSON is 1.x's `catch { return null; }` (D16(n), OC-3 A).
    JsValue::from_json(&text).ok()
}

/// 1.x `readBaseline(root)`: `None` without a readable file, else the summary or why 1.x's
/// reader throws on it (an empty file included: `JSON.parse('')` throws).
pub(crate) fn read_baseline(
    files: &impl Files,
    style: PathStyle,
    ws: &Workspace,
) -> Option<Result<BaselineSummary, BaselineParseError>> {
    let claude = paths::join(style, &ws.root, ".claude");
    let text = read_js_text(files, &paths::join(style, &claude, "temper-baseline.json"))?;
    Some(BaselineSummary::parse(&text))
}

/// The text of `<dir>/<fp>.json`, unless it is missing, unreadable or empty (1.x's
/// `if (!t) return null`).
fn record_text(files: &impl Files, style: PathStyle, dir: &str, fp: Fingerprint) -> Option<String> {
    let path = paths::join(style, dir, &format!("{fp}.json"));
    read_js_text(files, &path).filter(|text| !text.is_empty())
}
