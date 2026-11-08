//! The verdict records under `<state dir>/verdicts/` (1.x `verdict.mjs` `recordVerdict`).

use std::path::Path;

use temper_domain::{JsValue, PathStyle, ValidVerdict, kept_record, verdict_record};

use super::verdict_for;
use crate::engine::HookError;
use crate::paths;
use crate::ports::{Clock, Files, Ports};
use crate::project::Workspace;

/// What 1.x's `recordVerdict(proj, v, meta)` adds to the verdict: `meta`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RecordMeta<'a> {
    /// `input.agent_id`, `null` included; `None` is `undefined`, which leaves the key out.
    pub(crate) agent_id: Option<&'a JsValue>,
    /// Whether the verdict's fingerprint is no longer the tree's.
    pub(crate) stale: bool,
}

/// 1.x `recordVerdict(proj, v, meta)`: records `verdict` as the one for its fingerprint, in
/// `verdicts/<fp>.json` as `JSON.stringify(rec, null, 2)`, and returns the record. A changes
/// request with no reason to overturn a fresh approval of the same tree leaves that approval in
/// place, and is appended to `verdicts/<fp>.history.jsonl` with `kept: 'approved'` instead
/// (#136).
///
/// # Errors
///
/// [`HookError::Files`] when the directory cannot be made or the file written, as 1.x's
/// `mkdirSync`, `appendFileSync` and `writeAtomic` throw; [`HookError::InputType`] where 1.x's
/// `overturns(v)` throws, before anything is written.
pub(crate) fn record_verdict(
    ports: &impl Ports,
    style: PathStyle,
    ws: &Workspace,
    verdict: &ValidVerdict,
    meta: RecordMeta<'_>,
) -> Result<JsValue, HookError> {
    let rec = verdict_record(verdict, ports.clock().now(), meta.agent_id, meta.stale);
    let files = ports.files();
    let dir = paths::join(style, &ws.state_dir, "verdicts");
    let fp = verdict.fingerprint();
    let prev = verdict_for(files, style, ws, fp);
    if verdict.keeps_approval_over(prev.as_ref())? {
        let kept = kept_record(rec);
        files.create_dir_all(Path::new(&dir))?;
        let history = paths::join(style, &dir, &format!("{fp}.history.jsonl"));
        files.append_line(Path::new(&history), &kept.to_json())?;
        return Ok(kept);
    }
    let path = paths::join(style, &dir, &format!("{fp}.json"));
    files.write_atomic(Path::new(&path), &rec.to_json_pretty())?;
    Ok(rec)
}

#[cfg(test)]
mod tests;
