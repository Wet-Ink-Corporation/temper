//! The plans `subagent-start` names (1.x `hooks.mjs` `listPlans`).

use std::path::Path;

use temper_domain::PathStyle;

use crate::paths;
use crate::ports::Files;

/// How many plans the context names.
const SHOWN: usize = 5;

/// 1.x `listPlans(proj)`: the five newest `.md` entries of `<root>/.temper/plans/` by
/// modification time, ties in directory order, as `.temper/plans/<name>`. Any failure, a
/// missing directory or an entry that cannot be stat'ed (a dangling `x.md`), gives none, as
/// 1.x's `catch { return []; }`; so does an entry without a modification time, which Node
/// always has.
pub(crate) fn list_plans(files: &impl Files, style: PathStyle, root: &str) -> Vec<String> {
    let dir = paths::join(style, &paths::join(style, root, ".temper"), "plans");
    let Ok(entries) = files.read_dir(Path::new(&dir)) else {
        return Vec::new();
    };
    let mut timed = Vec::new();
    // A name that is not UTF-8 cannot end in `.md` as Node reads it lossily: it ends in U+FFFD
    // or in what follows it, which `ends_with` sees either way.
    let names = entries
        .iter()
        .map(|entry| entry.name.to_string_lossy())
        .filter(|name| name.ends_with(".md"));
    for name in names {
        let path = paths::join(style, &dir, &name);
        let Ok(Some(meta)) = files.stat(Path::new(&path)) else {
            return Vec::new();
        };
        let Some(modified) = meta.modified else {
            return Vec::new();
        };
        timed.push((name, modified.millis()));
    }
    // 1.x's stable `sort((a, b) => b.t - a.t)`: newest first, ties kept in order. An `mtimeMs`
    // is never NaN or -0, where `total_cmp` and the subtraction would differ.
    timed.sort_by(|(_, a), (_, b)| b.total_cmp(a));
    timed
        .into_iter()
        .take(SHOWN)
        .map(|(name, _)| format!(".temper/plans/{name}"))
        .collect()
}

#[cfg(test)]
mod tests;
