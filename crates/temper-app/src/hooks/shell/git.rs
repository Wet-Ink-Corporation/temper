//! git as the write, delete and patch rules read it (1.x `shell.mjs` `dashedGit`,
//! `GIT_BUILTINS`, `gitCommand`).

use std::borrow::Cow;
use std::collections::HashMap;

use super::walk::Command;
use super::words::{Dir, PathCx, Word, command_name, word_dir};

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

pub(super) fn is_builtin(sub: &str) -> bool {
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
    /// 1.x `dynamic`: temper cannot tell the subcommand (an unknown word, a shell alias, a
    /// `-c` value that could define an alias, or a hidden alias).
    pub(crate) dynamic: bool,
}

impl Git<'_> {
    /// The same command, read as one whose subcommand temper cannot tell.
    fn unread(self) -> Self {
        Self {
            dynamic: true,
            ..self
        }
    }
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
            dynamic: false,
        });
    }
    if c.name.as_deref() != Some("git") {
        return None;
    }
    let mut opts = GitOptions {
        dir: c.cwd.clone(),
        aliases: HashMap::new(),
        hidden: c.with.git_config,
        opaque: false,
    };
    let mut args: Cow<'c, [Word]> = Cow::Borrowed(&c.args);
    let mut expansions = 0;
    let mut at = 0;
    while let Some(word) = args.get(at) {
        let opt = word.raw.as_str();
        if opt == "-C" {
            at = at.saturating_add(1);
            opts.dir = args
                .get(at)
                .and_then(|value| word_dir(cx, value, opts.dir.as_ref()));
        } else if opt == "-c" {
            at = at.saturating_add(1);
            opts.config(args.get(at));
        } else if opt == "--config-env" {
            at = at.saturating_add(1);
            opts.config_env(args.get(at).map_or("", |value| value.raw.as_str()));
        } else if let Some(setting) = opt.strip_prefix("--config-env=") {
            opts.config_env(setting);
        } else if opt == "--git-dir" || opt == "--work-tree" || opt == "--namespace" {
            at = at.saturating_add(1);
        } else if !opt.starts_with('-') {
            match opts.subcommand(word, after(&args, at), expansions) {
                Next::Git(git) => return Some(git),
                Next::Expand(words) => {
                    // git reads options at the start of an alias as its own: the expansion
                    // goes back through this loop.
                    expansions += 1;
                    args = Cow::Owned(words);
                    at = 0;
                    continue;
                }
            }
        }
        at = at.saturating_add(1);
    }
    Some(opts.ends(String::new(), Cow::Borrowed(&[]), false))
}

/// What a subcommand word leads to: the git command, or an alias's expansion to read again.
enum Next<'c> {
    Git(Git<'c>),
    Expand(Vec<Word>),
}

/// The words after the one at `at`, borrowed while the command's own words are read.
fn after<'c>(args: &Cow<'c, [Word]>, at: usize) -> Cow<'c, [Word]> {
    let from = at.saturating_add(1);
    match args {
        Cow::Borrowed(words) => Cow::Borrowed(words.get(from..).unwrap_or(&[])),
        Cow::Owned(words) => Cow::Owned(words.get(from..).unwrap_or(&[]).to_vec()),
    }
}

/// What git's options before the subcommand set.
struct GitOptions {
    dir: Option<Dir>,
    /// `-c alias.<name>=<definition>`, by lowercased name; `!` for one temper cannot read.
    aliases: HashMap<String, String>,
    /// Configuration temper cannot read is set, which may define any alias.
    hidden: bool,
    /// A `-c` value temper cannot read that is not a plain `section.key=` setting, which
    /// could define an alias.
    opaque: bool,
}

impl GitOptions {
    /// `-c <setting>`: an alias it defines, or an include temper cannot read.
    fn config(&mut self, word: Option<&Word>) {
        let text = word.map_or("", |word| word.value().unwrap_or(&word.raw));
        let alias = text
            .get(..6)
            .filter(|head| head.eq_ignore_ascii_case("alias."))
            .and_then(|_| text.get(6..))
            .and_then(|rest| rest.split_once('='))
            .filter(|(name, _)| !name.is_empty());
        if let Some((name, def)) = alias {
            let def = if word.and_then(Word::value).is_some() {
                temper_domain::js_trim(def).to_owned()
            } else {
                "!".to_owned()
            };
            self.aliases.insert(name.to_lowercase(), def);
        } else if includes_config(text) {
            self.hidden = true;
        } else if word.and_then(Word::value).is_none()
            && !plain_setting(word.map_or("", |word| word.raw.as_str()))
        {
            self.opaque = true;
        }
    }

    /// `--config-env <name>=<ENV>`: its value comes from the environment, so an alias it sets
    /// is never readable.
    fn config_env(&mut self, setting: &str) {
        let alias = setting
            .get(..6)
            .is_some_and(|head| head.eq_ignore_ascii_case("alias."));
        if alias || !plain_setting(setting) || includes_config(setting) {
            self.hidden = true;
        }
    }

    /// The subcommand `word`, with the words after it: the git command it names, or the
    /// expansion of the alias it is, after `expansions` earlier ones.
    fn subcommand<'c>(&self, word: &Word, rest: Cow<'c, [Word]>, expansions: usize) -> Next<'c> {
        let Some(sub) = word.value() else {
            return Next::Git(self.ends(String::new(), rest, false).unread());
        };
        let alias = (!is_builtin(sub))
            .then(|| self.aliases.get(&sub.to_lowercase()))
            .flatten();
        let Some(def) = alias else {
            let hidden_alias = self.hidden && !is_builtin(sub);
            return Next::Git(self.ends(sub.to_owned(), rest, hidden_alias));
        };
        // A chain longer than temper follows may end on git apply.
        if expansions == 8 {
            return Next::Git(self.ends(String::new(), rest, true).unread());
        }
        if def.is_empty() || def.starts_with('!') {
            return Next::Git(self.ends(String::new(), rest, false).unread());
        }
        let mut words: Vec<Word> = def
            .split(temper_domain::is_js_space)
            .filter(|part| !part.is_empty())
            .map(Word::literal)
            .collect();
        words.extend(rest.iter().cloned());
        Next::Expand(words)
    }

    /// The git command these options run `sub` with.
    fn ends<'c>(&self, sub: String, args: Cow<'c, [Word]>, hidden_alias: bool) -> Git<'c> {
        Git {
            sub,
            args,
            cwd: self.dir.clone(), // a shared `Rc` handle
            hidden_alias,
            dynamic: self.opaque || hidden_alias,
        }
    }
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
