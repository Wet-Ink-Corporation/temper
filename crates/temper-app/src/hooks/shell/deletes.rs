//! What a command deletes (1.x `shell.mjs` `commandDeletes`). Kept apart from the writes: the
//! state-dir rule lets `rm` through.

use std::borrow::Cow;

use super::git::git_command;
use super::walk::Command;
use super::words::{Dir, PathCx, Word};
use super::wrappers::cluster_reaches;
use super::writes::{PsParams, param_word, ps_remove_switch};

/// One path a command deletes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Delete<'c> {
    pub(crate) word: Cow<'c, Word>,
    pub(crate) cwd: Option<Dir>,
    /// The delete takes a whole directory (`rm -r`, `-Recurse`, `git rm -r`).
    pub(crate) recursive: bool,
}

/// `^-.`: an option word (JS `.` stops at line ends).
fn option(raw: &str) -> bool {
    let mut chars = raw.chars();
    chars.next() == Some('-')
        && chars
            .next()
            .is_some_and(|c| !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
}

/// 1.x `commandDeletes(c)`: the operands of `rm` and `unlink`, the paths of `Remove-Item` and
/// its aliases, and the pathspecs of `git rm` (`--cached` too: the file leaves the
/// repository). Dry runs (`git rm -n`, `-WhatIf`) delete nothing.
pub(crate) fn command_deletes<'c>(c: &'c Command, cx: PathCx<'_>) -> Vec<Delete<'c>> {
    let name = c.name.as_deref().unwrap_or("");
    let cwd = || c.cwd.clone();
    match name {
        "rm" | "unlink" => {
            let mut recursive = false;
            let mut opts = true;
            let mut pos = Vec::new();
            for a in &c.args {
                if opts && a.raw == "--" {
                    opts = false;
                } else if opts && option(&a.raw) {
                    recursive |= cluster_reaches(&a.raw, "rR") || a.raw == "--recursive";
                } else {
                    pos.push(a);
                }
            }
            pos.into_iter()
                .map(|a| Delete {
                    word: Cow::Borrowed(a),
                    cwd: cwd(),
                    recursive,
                })
                .collect()
        }
        "remove-item" | "ri" | "del" | "erase" | "rd" => {
            let mut params = PsParams::new(&c.args, ps_remove_switch);
            if params.dry_run() {
                return Vec::new();
            }
            let mut paths: Vec<Cow<'c, Word>> = [
                params.get(&["path"]),
                params.get(&["literalpath"]),
                params.exact("lp"),
            ]
            .into_iter()
            .filter_map(param_word)
            .collect();
            if paths.is_empty() && !params.pos.is_empty() {
                paths.push(params.pos.remove(0));
            }
            let recursive = params
                .keys()
                .any(|k| k.starts_with('r') && "recurse".starts_with(k));
            params
                .split_paths(paths)
                .into_iter()
                .map(|word| Delete {
                    word,
                    cwd: cwd(),
                    recursive,
                })
                .collect()
        }
        "git" => git_rm(c, cx),
        _ => Vec::new(),
    }
}

/// The pathspecs of `git rm`.
fn git_rm<'c>(c: &'c Command, cx: PathCx<'_>) -> Vec<Delete<'c>> {
    let Some(g) = git_command(c, cx).filter(|g| g.sub == "rm") else {
        return Vec::new();
    };
    let mut recursive = false;
    let mut opts = true;
    let mut pos = Vec::new();
    for (k, a) in g.args.iter().enumerate() {
        if opts && a.raw == "--" {
            opts = false;
        } else if opts && option(&a.raw) {
            let r = a.raw.as_str();
            if cluster_reaches(r, "n") || r == "--dry-run" || r.starts_with("--pathspec-from-file")
            {
                return Vec::new();
            }
            recursive |= cluster_reaches(r, "r");
        } else {
            pos.push(k);
        }
    }
    // An alias expansion's words live in `g`, so its deletes own theirs.
    let word = |k: usize| -> Option<Cow<'c, Word>> {
        match &g.args {
            Cow::Borrowed(words) => words.get(k).map(Cow::Borrowed),
            Cow::Owned(words) => words.get(k).cloned().map(Cow::Owned),
        }
    };
    pos.into_iter()
        .filter_map(word)
        .map(|word| Delete {
            word,
            cwd: g.cwd.clone(),
            recursive,
        })
        .collect()
}
