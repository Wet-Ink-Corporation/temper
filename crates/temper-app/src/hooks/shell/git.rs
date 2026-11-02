//! git as the write, delete and patch rules read it (1.x `shell.mjs` `dashedGit`,
//! `GIT_BUILTINS`, `gitCommand`).

use std::borrow::Cow;
use std::collections::HashMap;

use super::walk::Command;
use super::words::{Dir, PathCx, Word, command_name, word_path};

/// 1.x `GIT_BUILTINS`: git's own commands (`git --list-cmds=main`, plus the newer `backfill`,
/// `refs` and `replay`). An alias never overrides one; any other subcommand may be an alias.
const GIT_BUILTINS: [&str; 159] = [
    "add",
    "am",
    "annotate",
    "apply",
    "archive",
    "backfill",
    "bisect",
    "blame",
    "branch",
    "bugreport",
    "bundle",
    "cat-file",
    "check-attr",
    "check-ignore",
    "check-mailmap",
    "check-ref-format",
    "checkout",
    "checkout-index",
    "cherry",
    "cherry-pick",
    "citool",
    "clean",
    "clone",
    "column",
    "commit",
    "commit-graph",
    "commit-tree",
    "config",
    "count-objects",
    "credential",
    "credential-cache",
    "credential-store",
    "daemon",
    "describe",
    "diagnose",
    "diff",
    "diff-files",
    "diff-index",
    "diff-tree",
    "difftool",
    "fast-export",
    "fast-import",
    "fetch",
    "fetch-pack",
    "filter-branch",
    "fmt-merge-msg",
    "for-each-ref",
    "for-each-repo",
    "format-patch",
    "fsck",
    "fsck-objects",
    "gc",
    "get-tar-commit-id",
    "grep",
    "gui",
    "hash-object",
    "help",
    "hook",
    "http-backend",
    "http-fetch",
    "http-push",
    "imap-send",
    "index-pack",
    "init",
    "init-db",
    "instaweb",
    "interpret-trailers",
    "log",
    "ls-files",
    "ls-remote",
    "ls-tree",
    "mailinfo",
    "mailsplit",
    "maintenance",
    "merge",
    "merge-base",
    "merge-file",
    "merge-index",
    "merge-octopus",
    "merge-one-file",
    "merge-ours",
    "merge-recursive",
    "merge-recursive-ours",
    "merge-recursive-theirs",
    "merge-resolve",
    "merge-subtree",
    "merge-tree",
    "mergetool",
    "mktag",
    "mktree",
    "multi-pack-index",
    "mv",
    "name-rev",
    "notes",
    "pack-objects",
    "pack-redundant",
    "pack-refs",
    "patch-id",
    "pickaxe",
    "prune",
    "prune-packed",
    "pull",
    "push",
    "quiltimport",
    "range-diff",
    "read-tree",
    "rebase",
    "receive-pack",
    "reflog",
    "refs",
    "remote",
    "remote-ext",
    "remote-fd",
    "remote-ftp",
    "remote-ftps",
    "remote-http",
    "remote-https",
    "repack",
    "replace",
    "replay",
    "request-pull",
    "rerere",
    "reset",
    "restore",
    "rev-list",
    "rev-parse",
    "revert",
    "rm",
    "send-pack",
    "shell",
    "shortlog",
    "show",
    "show-branch",
    "show-index",
    "show-ref",
    "sparse-checkout",
    "stage",
    "stash",
    "status",
    "stripspace",
    "submodule",
    "switch",
    "symbolic-ref",
    "tag",
    "unpack-file",
    "unpack-objects",
    "update-index",
    "update-ref",
    "update-server-info",
    "upload-archive",
    "upload-pack",
    "var",
    "verify-commit",
    "verify-pack",
    "verify-tag",
    "version",
    "whatchanged",
    "worktree",
    "write-tree",
];

fn is_builtin(sub: &str) -> bool {
    GIT_BUILTINS.contains(&sub)
}

/// A git command as the rules read it.
#[derive(Debug)]
pub(crate) struct Git<'c> {
    /// The subcommand; empty when temper cannot tell it.
    pub(crate) sub: String,
    /// The words after it. An alias expansion builds new ones.
    pub(crate) args: Cow<'c, [Word]>,
    /// Where it runs, after `-C`.
    pub(crate) cwd: Option<Dir>,
    /// A subcommand git does not have, under configuration temper cannot read, which may be
    /// an alias of any command.
    pub(crate) hidden_alias: bool,
}

/// 1.x `dashedGit(c)`: the subcommand of `git-apply`, `/usr/lib/git-core/git-am` and the like.
pub(super) fn dashed_git(c: &Command) -> Option<String> {
    let name = match &c.name {
        Some(name) => Cow::Borrowed(name.as_str()),
        None => Cow::Owned(command_name(&c.argv0.raw)),
    };
    name.strip_prefix("git-")
        .filter(|sub| {
            !sub.is_empty()
                && sub
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
        .map(str::to_owned)
}

/// 1.x `INCLUDE_KEY`, `/^include(?:if\.[\s\S]*)?\.path=/i`: a key that includes another
/// configuration file.
fn includes_config(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.starts_with("include.path=")
        || lower
            .strip_prefix("includeif.")
            .is_some_and(|rest| rest.contains(".path="))
}

/// `^[A-Za-z][\w-]*\.[\w.-]*=`: a plain `section.key=` setting.
fn plain_setting(text: &str) -> bool {
    let mut bytes = text.bytes();
    if !bytes.next().is_some_and(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let rest = text.get(1..).unwrap_or("");
    let section = rest.bytes().take_while(|&b| word(b)).count();
    let Some(after) = rest.get(section..).and_then(|r| r.strip_prefix('.')) else {
        return false;
    };
    let key = after.bytes().take_while(|&b| word(b) || b == b'.').count();
    after.get(key..).is_some_and(|r| r.starts_with('='))
}

/// 1.x `gitCommand(c)`: `git [-C dir] [-c k=v] [--config-env k=ENV] <sub> args`, or `None`
/// when the command is not git. A subcommand that is not one of git's own and has an alias
/// set with `-c alias.x=...` resolves to the command it names (at most 8 deep).
pub(crate) fn git_command<'c>(c: &'c Command, cx: PathCx<'_>) -> Option<Git<'c>> {
    if let Some(sub) = dashed_git(c).filter(|s| s == "apply" || s == "am") {
        return Some(Git {
            sub,
            args: Cow::Borrowed(&c.args),
            cwd: c.cwd.clone(),
            hidden_alias: false,
        });
    }
    if c.name.as_deref() != Some("git") {
        return None;
    }
    let mut dir = c.cwd.clone();
    let mut a: Cow<'c, [Word]> = Cow::Borrowed(&c.args);
    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut hidden = c.git_config;
    let mut expansions = 0;
    let mut i = 0;
    let ends = |sub: String, args: Cow<'c, [Word]>, dir: Option<Dir>, hidden_alias: bool| Git {
        sub,
        args,
        cwd: dir,
        hidden_alias,
    };
    while let Some(word) = a.get(i) {
        let t = word.raw.as_str();
        if t == "-C" {
            i = i.saturating_add(1);
            dir = a
                .get(i)
                .and_then(|w| word_path(cx, w, dir.as_deref()))
                .map(Dir::from);
        } else if t == "-c" {
            i = i.saturating_add(1);
            let w = a.get(i);
            let text = w.map_or("", |w| w.value().unwrap_or(&w.raw));
            let alias = text
                .get(..6)
                .filter(|head| head.eq_ignore_ascii_case("alias."))
                .and_then(|_| text.get(6..))
                .and_then(|rest| rest.split_once('='))
                .filter(|(name, _)| !name.is_empty());
            if let Some((name, def)) = alias {
                let def = match w.and_then(Word::value) {
                    Some(_) => temper_domain::js_trim(def).to_owned(),
                    None => "!".to_owned(),
                };
                aliases.insert(name.to_lowercase(), def);
            } else if includes_config(text) {
                hidden = true;
            }
        } else if t == "--config-env" || t.starts_with("--config-env=") {
            // Its value comes from the environment, so an alias it sets is never readable.
            let v = if t == "--config-env" {
                i = i.saturating_add(1);
                a.get(i).map_or("", |w| w.raw.as_str())
            } else {
                t.get(13..).unwrap_or("")
            };
            let alias = v.get(..6).is_some_and(|h| h.eq_ignore_ascii_case("alias."));
            if alias || !plain_setting(v) || includes_config(v) {
                hidden = true;
            }
        } else if t == "--git-dir" || t == "--work-tree" || t == "--namespace" {
            i = i.saturating_add(1);
        } else if !t.starts_with('-') {
            let rest = |a: &Cow<'c, [Word]>| -> Cow<'c, [Word]> {
                match a {
                    Cow::Borrowed(words) => {
                        Cow::Borrowed(words.get(i.saturating_add(1)..).unwrap_or(&[]))
                    }
                    Cow::Owned(words) => {
                        Cow::Owned(words.get(i.saturating_add(1)..).unwrap_or(&[]).to_vec())
                    }
                }
            };
            let Some(sub) = word.value() else {
                return Some(ends(String::new(), rest(&a), dir, false));
            };
            let lower = sub.to_lowercase();
            let alias = (!is_builtin(sub)).then(|| aliases.get(&lower)).flatten();
            if let Some(def) = alias {
                // A chain longer than temper follows may end on git apply.
                if expansions == 8 {
                    return Some(ends(String::new(), rest(&a), dir, true));
                }
                if def.is_empty() || def.starts_with('!') {
                    return Some(ends(String::new(), rest(&a), dir, false));
                }
                // git reads options at the start of an alias as its own: the expansion goes
                // back through this loop.
                expansions += 1;
                let mut words: Vec<Word> = def
                    .split(crate::project::is_js_space)
                    .filter(|w| !w.is_empty())
                    .map(Word::literal)
                    .collect();
                words.extend(rest(&a).iter().cloned());
                a = Cow::Owned(words);
                i = 0;
                continue;
            }
            let hidden_alias = hidden && !is_builtin(sub);
            let sub = sub.to_owned();
            let args = rest(&a);
            return Some(ends(sub, args, dir, hidden_alias));
        }
        i = i.saturating_add(1);
    }
    Some(ends(String::new(), Cow::Borrowed(&[]), dir, false))
}

#[cfg(test)]
mod tests {
    use super::{GIT_BUILTINS, includes_config, plain_setting};

    #[test]
    fn helpers_read_as_1x_regexes_do() {
        assert!(includes_config("Include.Path=x"));
        assert!(includes_config("includeIf.gitdir:x/.path=y"));
        assert!(!includes_config("includeif.path=x"));
        assert!(plain_setting("color.ui=auto"));
        assert!(plain_setting("a.b.c-d=1"));
        assert!(!plain_setting("1a.b=1"));
        assert!(!plain_setting("a=1"));
        let mut sorted = GIT_BUILTINS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), GIT_BUILTINS.len());
    }
}
