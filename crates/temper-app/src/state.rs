//! Reads of the files temper keeps, with 1.x's fallbacks (`readText` and the readers on it).

use std::collections::HashMap;
use std::path::Path;

use temper_domain::{
    BaselineParseError, BaselineSummary, Fingerprint, GateRecord, JsTypeError, JsValue, ModTime,
    PathStyle, recorded_after,
};

use crate::paths;
use crate::ports::Files;
use crate::project::Workspace;
use crate::retention::is_progress_file;

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

/// Whose verdict history [`history_for`] reads: fingerprint `fp`'s, in `ws`'s state dir.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HistoryOf<'a> {
    pub(crate) style: PathStyle,
    pub(crate) ws: &'a Workspace,
    pub(crate) fp: Fingerprint,
}

/// 1.x `historyFor(proj, fp, since).length`: how many rows of
/// `<state dir>/verdicts/<fp>.history.jsonl` parse and were recorded after `since` (`None` is
/// 1.x's default `''`). A missing or unreadable file has none; a line that does not parse is
/// skipped.
///
/// # Errors
///
/// [`JsTypeError`] where 1.x's filter throws on a row ([`recorded_after`]): a `null` row, or a
/// time V8 cannot convert (F5).
pub(crate) fn history_for(
    files: &impl Files,
    of: HistoryOf<'_>,
    since: Option<&JsValue>,
) -> Result<usize, JsTypeError> {
    let HistoryOf { style, ws, fp } = of;
    let dir = paths::join(style, &ws.state_dir, "verdicts");
    let Some(text) = read_js_text(
        files,
        &paths::join(style, &dir, &format!("{fp}.history.jsonl")),
    ) else {
        // 1.x's `readText(…) ?? ''`: no rows.
        return Ok(0);
    };
    let mut count = 0;
    for line in text.split('\n').filter(|line| !line.is_empty()) {
        // A line that does not parse is 1.x's `catch { return []; }` (D16(n), OC-3 A).
        let Ok(row) = JsValue::from_json(line) else {
            continue;
        };
        if recorded_after(&row, since)? {
            count += 1;
        }
    }
    Ok(count)
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

/// A gate progress file as 1.x's `gateProgressFiles` keeps it: its JSON, path and `mtimeMs`.
#[derive(Debug)]
pub(crate) struct ProgressFile {
    pub(crate) record: JsValue,
    pub(crate) path: String,
    pub(crate) modified: ModTime,
}

/// 1.x `gateProgressFiles(proj)`'s `Map`: the progress files by their record's integer `pid`.
#[derive(Debug, Default)]
pub(crate) struct ProgressFiles(HashMap<u64, ProgressFile>);

impl ProgressFiles {
    /// `progress.get(pid)`: a JS `Map` key, where `-0` and `0` are one (`SameValueZero`).
    pub(crate) fn get(&self, pid: &JsValue) -> Option<&ProgressFile> {
        self.0.get(&map_key(pid.as_integer()?))
    }
}

fn map_key(integer: f64) -> u64 {
    if integer == 0.0 { 0.0_f64 } else { integer }.to_bits()
}

/// 1.x `gateProgressFiles(proj)`: the state dir's progress files by their record's integer
/// `pid`, in directory order, so a later file of the same pid replaces an earlier one. A file
/// that cannot be read, parsed or stat'ed is skipped (1.x's `catch { /* unreadable */ }`).
pub(crate) fn progress_files(
    files: &impl Files,
    style: PathStyle,
    ws: &Workspace,
) -> ProgressFiles {
    let mut out = ProgressFiles::default();
    let Ok(entries) = files.read_dir(Path::new(&ws.state_dir)) else {
        // 1.x: `catch { return out; }`.
        return out;
    };
    let names = entries
        .iter()
        .filter_map(|entry| entry.name.to_str())
        .filter(|name| is_progress_file(name));
    for name in names {
        let path = paths::join(style, &ws.state_dir, name);
        let Some(record) = read_js_text(files, &path).and_then(|t| JsValue::from_json(&t).ok())
        else {
            continue;
        };
        let Some(pid) = record.get("pid").and_then(JsValue::as_integer) else {
            continue;
        };
        let Ok(Some(meta)) = files.stat(Path::new(&path)) else {
            continue;
        };
        // A platform without modification times has no `mtimeMs` to judge by.
        let Some(modified) = meta.modified else {
            continue;
        };
        let file = ProgressFile {
            record,
            path,
            modified,
        };
        out.0.insert(map_key(pid), file);
    }
    out
}
