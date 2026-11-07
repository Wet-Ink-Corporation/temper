//! How long temper keeps its state (1.x `project.mjs` `RETENTION`, `pruneSessions`,
//! `pruneMarkers`, `pruneFingerprintState` and `pruneGateProgress`). `SessionStart` on
//! `startup` runs every prune.
//!
//! Each prune returns how many entries it removed. A removal that fails (a file in use on
//! Windows, a race with another session) is not counted and is otherwise ignored, as 1.x's
//! `catch` ignores it: the next startup tries again.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;

use temper_domain::{PathStyle, Timestamp};

use crate::engine::RunEnv;
use crate::hooks::markers_root;
use crate::paths;
use crate::ports::{Clock, EntryKind, Files, Ports};
use crate::project::Workspace;

const DAY_MS: i64 = 86_400_000;

/// How long one kind of per-fingerprint record is kept (1.x `RETENTION.verdicts` and
/// `RETENTION.gate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    /// Older groups go.
    pub(crate) max_age_days: i64,
    /// Past this many groups, newest first, the rest go.
    pub(crate) keep: u32,
    /// Past this many bytes in all, the oldest groups go (the newest always stays).
    pub(crate) max_bytes: Option<u64>,
}

/// Review verdicts: the costliest record to lose (a fresh reviewer run).
pub(crate) const VERDICTS: Limits = Limits {
    max_age_days: 60,
    keep: 300,
    max_bytes: None,
};

/// Gate runs, each `gate/<fp>.json` with its `logs/<fp>-*` files.
pub(crate) const GATE: Limits = Limits {
    max_age_days: 30,
    keep: 300,
    max_bytes: Some(64 * 1024 * 1024),
};

/// A group touched this recently is never removed: the state dir is shared by every worktree,
/// and another session's gate may be writing a fingerprint's logs.
pub(crate) const GRACE_MINUTES: i64 = 60;

/// 1.x `pruneSessions(proj)`: each entry of `<state dir>/sessions` last modified over 14 days
/// ago, removed whole.
pub(crate) fn prune_sessions(ports: &impl Ports, style: PathStyle, ws: &Workspace) -> usize {
    let dir = paths::join(style, &ws.state_dir, "sessions");
    prune_entries(ports, style, &dir, 14)
}

/// 1.x `pruneMarkers()`: each session's skill markers last modified over 7 days ago, under the
/// plugin data dir (or the temporary directory's), whatever the project.
pub(crate) fn prune_markers(ports: &impl Ports, env: &RunEnv) -> usize {
    let dir = paths::join(env.style, &markers_root(env), "sessions");
    prune_entries(ports, env.style, &dir, 7)
}

/// 1.x `pruneFingerprintState(proj, currentFp)`: verdicts and gate runs to their limits, the
/// records of `protect` (the tree the session starts on) kept whatever their age, then the
/// gate progress files over a day old.
pub(crate) fn prune_fingerprint_state(
    ports: &impl Ports,
    style: PathStyle,
    ws: &Workspace,
    protect: Option<&str>,
) -> usize {
    let dir = |name: &str| paths::join(style, &ws.state_dir, name);
    let verdicts = prune_groups(ports, style, &[dir("verdicts")], VERDICTS, protect);
    let gate = prune_groups(ports, style, &[dir("gate"), dir("logs")], GATE, protect);
    let progress = prune_gate_progress(ports, style, ws);
    verdicts.saturating_add(gate).saturating_add(progress)
}

/// Removes each entry of `dir` whose modification time is over `max_age_days` old.
fn prune_entries(ports: &impl Ports, style: PathStyle, dir: &str, max_age_days: i64) -> usize {
    let files = ports.files();
    let Ok(entries) = files.read_dir(Path::new(dir)) else {
        // 1.x: `try { readdirSync } catch { return; }`.
        return 0;
    };
    let cutoff = before_now(ports, max_age_days.saturating_mul(DAY_MS));
    entries
        .iter()
        .filter_map(|entry| entry.name.to_str())
        .map(|name| paths::join(style, dir, name))
        .filter(|path| modified(files, path).is_some_and(|t| t < cutoff))
        .filter(|path| files.remove_all(Path::new(path)).is_ok())
        .count()
}

/// One fingerprint's files across the directories pruned together (1.x `fingerprintGroups`).
#[derive(Debug)]
struct Group {
    /// The name up to its first `-` or `.`.
    key: String,
    files: Vec<String>,
    /// The newest modification time, at least 0 (1.x's `Math.max(g.t, s.mtimeMs)` from 0).
    newest: f64,
    size: u64,
}

/// The regular files of `dirs` grouped by fingerprint, in first-seen order: one `stat` each.
fn groups(files: &impl Files, style: PathStyle, dirs: &[String]) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for dir in dirs {
        let Ok(entries) = files.read_dir(Path::new(dir)) else {
            // 1.x: an unlistable directory is skipped.
            continue;
        };
        // A name that is not UTF-8 reaches 1.x decoded lossily, and the stat of that name
        // fails, so it is skipped there too.
        for name in entries.iter().filter_map(|entry| entry.name.to_str()) {
            let path = paths::join(style, dir, name);
            let meta = match files.stat(Path::new(&path)) {
                Ok(Some(meta)) if meta.kind == EntryKind::File => meta,
                // Gone, unreadable or not a file: 1.x's `continue`.
                Ok(_) | Err(_) => continue,
            };
            let key = name.split(['-', '.']).next().unwrap_or(name);
            let index = *at.entry(key.to_owned()).or_insert_with(|| {
                out.push(Group {
                    key: key.to_owned(),
                    files: Vec::new(),
                    newest: 0.0,
                    size: 0,
                });
                out.len().saturating_sub(1)
            });
            if let Some(group) = out.get_mut(index) {
                group.files.push(path);
                if let Some(t) = meta.modified {
                    group.newest = group.newest.max(t.millis());
                }
                group.size = group.size.saturating_add(meta.len);
            }
        }
    }
    out
}

/// 1.x `pruneGroups(dirs, limits, protect)`: newest first, a group goes when it is older than
/// the grace period and older than the age limit, past the count, or (after the first) past
/// the byte budget, every file of it at once. The protected group counts toward nothing.
fn prune_groups(
    ports: &impl Ports,
    style: PathStyle,
    dirs: &[String],
    limits: Limits,
    protect: Option<&str>,
) -> usize {
    let files = ports.files();
    let cutoff = before_now(ports, limits.max_age_days.saturating_mul(DAY_MS));
    let recent = before_now(ports, GRACE_MINUTES.saturating_mul(60_000));
    // 1.x's `!protect || g.key !== protect`: an empty or missing start fingerprint protects
    // nothing.
    let protect = protect.filter(|key| !key.is_empty());
    let mut groups: Vec<Group> = groups(files, style, dirs)
        .into_iter()
        .filter(|group| protect.is_none_or(|key| group.key != key))
        .collect();
    // A stable sort, newest first, as 1.x's `sort((a, b) => b.t - a.t)`.
    groups.sort_by(|a, b| b.newest.partial_cmp(&a.newest).unwrap_or(Ordering::Equal));
    let keep = usize::try_from(limits.keep).unwrap_or(usize::MAX);
    let mut bytes: u64 = 0;
    let mut removed = 0_usize;
    for (i, group) in groups.iter().enumerate() {
        bytes = bytes.saturating_add(group.size);
        let over_budget = i > 0 && limits.max_bytes.is_some_and(|max| bytes > max);
        let expired = group.newest < cutoff || i >= keep || over_budget;
        if group.newest < recent && expired {
            let gone = group
                .files
                .iter()
                .filter(|path| files.remove_file(Path::new(path)).is_ok())
                .count();
            removed = removed.saturating_add(gone);
        }
    }
    removed
}

/// 1.x `pruneGateProgress(proj)`: the progress files over a day old. A younger one whose gate
/// is gone stays for its session's next hook to read.
fn prune_gate_progress(ports: &impl Ports, style: PathStyle, ws: &Workspace) -> usize {
    let files = ports.files();
    let Ok(entries) = files.read_dir(Path::new(&ws.state_dir)) else {
        // 1.x: `try { readdirSync } catch { return; }`.
        return 0;
    };
    let cutoff = before_now(ports, DAY_MS);
    entries
        .iter()
        .filter_map(|entry| entry.name.to_str())
        .filter(|name| is_progress_file(name))
        .map(|name| paths::join(style, &ws.state_dir, name))
        .filter(|path| modified(files, path).is_some_and(|t| t < cutoff))
        // A directory of that name cannot be removed this way, as 1.x's `rmSync` throws.
        .filter(|path| files.remove_file(Path::new(path)).is_ok())
        .count()
}

/// 1.x's `/^gate-progress-[0-9a-f]+\.json$/`.
pub(crate) fn is_progress_file(name: &str) -> bool {
    name.strip_prefix("gate-progress-")
        .and_then(|rest| rest.strip_suffix(".json"))
        .is_some_and(|hex| {
            !hex.is_empty() && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

/// `Date.now() - ms`, as the JS number 1.x compares an `mtimeMs` with.
fn before_now(ports: &impl Ports, ms: i64) -> f64 {
    let now = ports.clock().now().unix_millis();
    Timestamp::from_unix_millis(now.saturating_sub(ms)).js_millis()
}

/// `statSync(path).mtimeMs`, or `None` when the stat fails (1.x's `catch` skips the entry) or
/// the platform has no modification time.
fn modified(files: &impl Files, path: &str) -> Option<f64> {
    match files.stat(Path::new(path)) {
        Ok(Some(meta)) => meta.modified.map(|t| t.millis()),
        // Gone or unreadable: skipped, as 1.x's `catch { /* ignore */ }`.
        Ok(None) | Err(_) => None,
    }
}

#[cfg(test)]
mod tests;
