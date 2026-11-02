//! Protected configuration a shell command writes, deletes or patches (1.x `hooks.mjs`
//! `protectedShellWrites`, `ownerOf`, `configFilesUnder`, `protectedUnder` and
//! `configPatchTouches`).
//!
//! Relative names resolve against the directory each command runs in; after a `cd` temper
//! cannot follow, against both the starting directory and the project root. A whole directory
//! written in one go (`cp -r d .cargo`, `git checkout <rev> -- .github`) reports the protected
//! globs it replaces. `git checkout` and `git restore` from `HEAD` or the index pass; for a
//! wildcard pathspec or a directory from another revision, git lists the files it would
//! change. A patch writes the files it names, its target and its `-o` output. A write into a
//! nested checkout (`.claude/worktrees/<name>`) is judged by that checkout's own list.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use temper_domain::{ConfigChanges, Glob, GlobError, Globs};

use crate::git::{self, Short};
use crate::hooks::HookCall;
use crate::hooks::shell::{
    Command, ParseOpts, Patch, PatchCommand, PatchTool, PathCx, Stdin, Target, Verb, Word,
    command_deletes, command_writes, js_len, landing, parse_commands, patch_command, patch_files,
    pathspec_pattern, resolve_path,
};
use crate::paths::{self, PathEnv};
use crate::ports::{EntryKind, FileMeta, Files, Ports};
use crate::project::{Workspace, discover_at, exists};
use crate::relpath::rel_to;

/// 1.x `PATH_MAX`: an ancestor longer than this, in UTF-16 units, cannot exist.
const PATH_MAX: usize = 4096;
/// 1.x `configFilesUnder`'s limit on the entries of a copied or deleted tree.
const TREE_LIMIT: usize = 10_000;
/// The largest patch file temper reads (1.x's 32 MiB).
const PATCH_CAP: u64 = 32 * 1024 * 1024;

/// What a command changes of the protected configuration (1.x's `{ files, deleted,
/// unreadPatch }`).
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Changes {
    written: Found,
    deleted: Found,
    /// The first patch command whose patch temper cannot read, as written.
    unread_patch: Option<String>,
}

impl Changes {
    /// The changes as the domain's ask reads them.
    pub(super) fn as_domain(&self) -> ConfigChanges<'_> {
        ConfigChanges {
            written: &self.written.list,
            deleted: &self.deleted.list,
            unread_patch: self.unread_patch.as_deref(),
        }
    }
}

/// Paths and globs in the order found, each once.
#[derive(Debug, Default, PartialEq, Eq)]
struct Found {
    list: Vec<String>,
    seen: HashSet<String>,
}

impl Found {
    fn add(&mut self, hit: String) {
        if !self.seen.contains(&hit) {
            // One copy is the list's, one the set's.
            self.seen.insert(hit.clone());
            self.list.push(hit);
        }
    }
}

/// 1.x `protectedShellWrites(proj, cmd, input.cwd)` under `readerRule`: where 1.x's reader
/// throws (a word past V8's longest string, a glob that does not compile), nothing.
pub(super) fn protected_shell_writes(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    cmd: &str,
) -> Changes {
    let env_cwd = call.env.cwd.to_string_lossy();
    let temp = call.env.temp_dir.to_string_lossy();
    let home = call.env.home.to_string_lossy();
    let at = PathEnv {
        style: call.env.style,
        cwd: &env_cwd,
    };
    let files = call.ports.files();
    // 1.x `cwd && fs.existsSync(cwd) ? path.resolve(cwd) : proj.root`.
    let start = call
        .input
        .given("cwd")
        .and_then(|dir| dir.as_str())
        .filter(|dir| !dir.is_empty())
        .map(|dir| paths::resolve(at.style, at.cwd, dir))
        .filter(|dir| exists(files, dir))
        .unwrap_or_else(|| ws.root.clone());
    let cx = PathCx { at, os_tmp: &temp };
    let opts = ParseOpts {
        paths: cx,
        cwd: Some(&start),
        home: &home,
        top: Some(&ws.root),
        git_dir: ".git",
        tmp: Some(&temp),
    };
    let Ok(commands) = parse_commands(cmd, opts) else {
        return Changes::default();
    };
    let Ok(globs) = Globs::new(ws.config.protected().iter().map(String::as_str)) else {
        return Changes::default();
    };
    let mut reader = Reader {
        call,
        ws,
        cx,
        start: &start,
        globs,
        owners: HashMap::new(),
        changes: Changes::default(),
    };
    match commands.iter().try_for_each(|c| reader.command(c)) {
        Ok(()) => reader.changes,
        Err(_) => Changes::default(),
    }
}

/// Who judges a path: the project, or a checkout nested in it.
#[derive(Clone)]
enum Owner {
    Project,
    Nested(Rc<Nested>),
}

/// A nested checkout, its protected globs, and its root relative to the project's.
struct Nested {
    ws: Workspace,
    globs: Globs,
    prefix: String,
}

struct Reader<'a, 'c, P> {
    call: &'a HookCall<'c, P>,
    ws: &'a Workspace,
    cx: PathCx<'a>,
    start: &'a str,
    globs: Globs,
    /// 1.x `ownerOf`'s cache: each ancestor checked, and the checkout there, if any.
    owners: HashMap<String, Option<Owner>>,
    changes: Changes,
}

/// Where a relative name may resolve: the command's directory, else the start and the root.
fn bases<'b>(cwd: Option<&'b str>, start: &'b str, root: &'b str) -> Vec<&'b str> {
    match cwd {
        Some(dir) => vec![dir],
        None => vec![start, root],
    }
}

impl<'a, P: Ports> Reader<'a, '_, P> {
    fn files(&self) -> &'a P::Files {
        self.call.ports.files()
    }

    fn stat(&self, path: &str) -> Option<FileMeta> {
        // 1.x's `stat` reads any failure as nothing there.
        self.files().stat(Path::new(path)).ok().flatten()
    }

    fn is(&self, path: &str, kind: EntryKind) -> bool {
        self.stat(path).is_some_and(|meta| meta.kind == kind)
    }

    /// 1.x `w && !w.glob ? resolvePath(w.value, base) : null`.
    fn word_abs(&self, word: &Word, base: Option<&str>) -> Option<String> {
        if word.glob() {
            return None;
        }
        resolve_path(self.cx, word.value(), base)
    }

    /// One command's patch, writes and deletes, in 1.x's order.
    fn command(&mut self, c: &Command) -> Result<(), GlobError> {
        if let Some(PatchCommand::Applies(p)) = patch_command(c, self.cx) {
            self.patch(c, &p)?;
        }
        for t in command_writes(c, self.cx) {
            self.write(&t)?;
        }
        let (start, ws) = (self.start, self.ws);
        let style = self.cx.at.style;
        // A delete takes the file, or with -r a whole directory and the protected files in it.
        for d in command_deletes(c, self.cx) {
            for base in bases(d.cwd.as_deref(), start, &ws.root) {
                let Some(abs) = self.word_abs(&d.word, Some(base)) else {
                    continue;
                };
                self.check(&abs, d.recursive, true)?;
                if d.recursive && self.is(&abs, EntryKind::Dir) {
                    // Past the walk limit, the directory's own globs stand for its files.
                    for f in self.config_files_under(&abs).unwrap_or_default() {
                        self.check(&paths::join(style, &abs, &f), false, true)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// A patch writes the files it names, its target and its `-o` output; one temper cannot
    /// read asks, unless it is a guessed alias.
    fn patch(&mut self, c: &Command, p: &Patch<'_>) -> Result<(), GlobError> {
        let touched = self.patch_touches(p, c);
        if touched.is_none() && !p.guessed && self.changes.unread_patch.is_none() {
            let words = std::iter::once(&c.argv0).chain(&c.args);
            let raw: Vec<&str> = words.map(Word::raw).collect();
            self.changes.unread_patch = Some(raw.join(" "));
        }
        let named: Vec<Option<&str>> = p
            .outputs
            .iter()
            .chain(&p.target)
            .map(|w| w.value())
            .collect();
        // An unread patch names no files.
        let listed: Vec<String> = touched
            .unwrap_or_default()
            .into_iter()
            .filter(|f| !named.contains(&Some(f.as_str())))
            .collect();
        let (start, ws) = (self.start, self.ws);
        let style = self.cx.at.style;
        for base in bases(p.cwd.as_deref(), start, &ws.root) {
            for v in &named {
                if let Some(abs) = resolve_path(self.cx, *v, Some(base)) {
                    self.check(&abs, false, false)?;
                }
            }
            // `patch` reads names from its directory; git also from the repository's top.
            let roots: &[&str] = if p.tool == PatchTool::Patch {
                &[base]
            } else {
                &[base, &ws.root]
            };
            for f in &listed {
                for b in roots {
                    self.check(&paths::resolve(style, b, f), false, false)?;
                }
            }
        }
        Ok(())
    }

    /// One write: a file, a copy's landing places, or a restore from another revision.
    fn write(&mut self, t: &Target<'_>) -> Result<(), GlobError> {
        let source = match &t.verb {
            Verb::GitRestore { source, .. } => match source.as_deref() {
                // Undoing one's own edit.
                None | Some("HEAD" | "@") => return Ok(()),
                Some(source) => Some(source),
            },
            _ => None,
        };
        let (start, ws) = (self.start, self.ws);
        let style = self.cx.at.style;
        for base in bases(t.cwd.as_deref(), start, &ws.root) {
            if let Some(source) = source
                && pathspec_pattern(t.word.raw())
            {
                self.pathspec(source, t.word.raw(), base, true)?;
                continue;
            }
            let Some(dest) = self.word_abs(&t.word, Some(base)) else {
                continue;
            };
            let Some(copy) = &t.copy else {
                self.check(&dest, source.is_some(), false)?;
                // A directory from another revision brings back the protected files git lists
                // in it (`docs/clippy.toml`).
                if let Some(source) = source
                    && !self.is(&dest, EntryKind::File)
                {
                    self.pathspec(source, t.word.raw(), base, false)?;
                }
                continue;
            };
            let dest_is_dir = self.is(&dest, EntryKind::Dir);
            for s in &copy.sources {
                let Some(land) = landing(self.cx, t, &dest, s, dest_is_dir) else {
                    continue;
                };
                // A recursive copy or a move of anything but a known file may bring a whole
                // tree, and with it the protected files inside it (`cfg/sub/clippy.toml`).
                let from = resolve_path(self.cx, s.value(), Some(base));
                let kind = from.as_deref().and_then(|p| self.stat(p)).map(|m| m.kind);
                self.check(
                    &land,
                    copy.recursive && kind != Some(EntryKind::File),
                    false,
                )?;
                if let Some(from) = from.filter(|_| copy.recursive && kind == Some(EntryKind::Dir))
                {
                    // Past the walk limit, the landing's own globs stand for its files.
                    for f in self.config_files_under(&from).unwrap_or_default() {
                        self.check(&paths::join(style, &land, &f), false, false)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// A wildcard or magic pathspec, or a directory, restored from `source`: the files git
    /// says it would change. When git cannot say, every protected glob (with `fallback`).
    fn pathspec(
        &mut self,
        source: &str,
        raw: &str,
        base: &str,
        fallback: bool,
    ) -> Result<(), GlobError> {
        let process = self.call.ports.process();
        let dir = Path::new(base);
        let top = git::short(process, dir, &["rev-parse", "--show-toplevel"]);
        let names = [
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            source,
            "--",
            raw,
        ];
        let diff = git::short(process, dir, &names);
        match (top, diff) {
            (Short::Ran(top), Short::Ran(names)) => {
                let top = temper_domain::js_trim(&top);
                for f in names.split('\0').filter(|f| !f.is_empty()) {
                    self.check(&paths::resolve(self.cx.at.style, top, f), false, false)?;
                }
                Ok(())
            }
            (Short::Unknown, _) | (_, Short::Unknown) => {
                let ws = self.ws;
                if fallback {
                    self.check(&ws.root, true, false)?;
                }
                Ok(())
            }
            // Not a repository, or no such revision: the restore fails too.
            (Short::Failed, _) | (_, Short::Failed) => Ok(()),
        }
    }

    /// 1.x `check(abs, tree, into)`: `abs` when it is protected, and with `tree` (a whole
    /// directory written or deleted) the protected globs under it, into the deleted list or
    /// the written one.
    fn check(&mut self, abs: &str, tree: bool, deleted: bool) -> Result<(), GlobError> {
        let (files, at, ws) = (self.files(), self.cx.at, self.ws);
        let rel = rel_to(files, at, &ws.root, abs);
        if rel.starts_with("..") || paths::is_absolute(at.style, &rel) || (rel.is_empty() && !tree)
        {
            return Ok(());
        }
        let owner = if rel.is_empty() {
            Owner::Project
        } else {
            self.owner_of(&rel)?
        };
        let mut hits = Vec::new();
        match &owner {
            Owner::Project => {
                if !rel.is_empty() && self.globs.any_match(&rel) {
                    hits.push(rel.clone()); // `rel` is read again below
                }
                if tree {
                    let under = protected_under(ws.config.protected(), &rel)?;
                    hits.extend(under.into_iter().map(str::to_owned));
                }
            }
            Owner::Nested(nested) => {
                let own = rel_to(files, at, &nested.ws.root, abs);
                if !own.is_empty() && nested.globs.any_match(&own) {
                    hits.push(rel.clone()); // as above
                }
                if tree {
                    let under = protected_under(nested.ws.config.protected(), &own)?;
                    hits.extend(under.into_iter().map(|g| format!("{}/{g}", nested.prefix)));
                }
            }
        }
        let found = if deleted {
            &mut self.changes.deleted
        } else {
            &mut self.changes.written
        };
        for hit in hits {
            found.add(hit);
        }
        Ok(())
    }

    /// 1.x `ownerOf(proj, rel, cache)`: the checkout of the deepest ancestor of `rel` with a
    /// `.git` in it, else the project. Each ancestor is looked at once per command line.
    fn owner_of(&mut self, rel: &str) -> Result<Owner, GlobError> {
        let style = self.cx.at.style;
        let ws = self.ws;
        // The UTF-16 length of `rel` up to each `/`, counted down from the end, so the
        // PATH_MAX test does not count the prefix again for each part (#155).
        let mut units = js_len(rel);
        let mut after = rel.len();
        for (end, _) in rel.rmatch_indices('/') {
            units = units.saturating_sub(js_len(rel.get(end..after).unwrap_or("")));
            after = end;
            if end == 0 || units > PATH_MAX {
                continue;
            }
            let dir = paths::join(style, &ws.root, rel.get(..end).unwrap_or(""));
            let owner = match self.owners.get(&dir) {
                Some(owner) => owner.clone(), // an `Rc` handle
                None => {
                    let owner = self.checkout_at(&dir)?;
                    self.owners.insert(dir, owner.clone()); // an `Rc` handle for the cache
                    owner
                }
            };
            if let Some(owner) = owner {
                return Ok(owner);
            }
        }
        Ok(Owner::Project)
    }

    /// The owner a `.git` in `dir` makes: its project, or this one when there is none
    /// (1.x `discover(dir) ?? proj`).
    fn checkout_at(&self, dir: &str) -> Result<Option<Owner>, GlobError> {
        let files = self.files();
        if !exists(files, &paths::join(self.cx.at.style, dir, ".git")) {
            return Ok(None);
        }
        let Some(ws) = discover_at(self.call.ports, self.call.env, dir) else {
            return Ok(Some(Owner::Project));
        };
        let globs = Globs::new(ws.config.protected().iter().map(String::as_str))?;
        let prefix = rel_to(files, self.cx.at, &self.ws.root, &ws.root);
        Ok(Some(Owner::Nested(Rc::new(Nested { ws, globs, prefix }))))
    }

    /// 1.x `configFilesUnder(dir)`: the files under `dir`, relative to it with `/`, outside
    /// `.git`; `None` past [`TREE_LIMIT`] entries.
    fn config_files_under(&self, dir: &str) -> Option<Vec<String>> {
        let style = self.cx.at.style;
        let mut out = Vec::new();
        let mut stack = vec![String::new()];
        let mut seen = 0_usize;
        while let Some(rel) = stack.pop() {
            let listed = if rel.is_empty() {
                self.files().read_dir(Path::new(dir))
            } else {
                self.files()
                    .read_dir(Path::new(&paths::join(style, dir, &rel)))
            };
            // A directory that cannot be listed adds nothing.
            let Ok(entries) = listed else {
                continue;
            };
            for entry in entries {
                seen = seen.saturating_add(1);
                if seen > TREE_LIMIT {
                    return None;
                }
                let name = entry.name.to_string_lossy();
                let git = name == ".git";
                let r = if rel.is_empty() {
                    name.into_owned()
                } else {
                    format!("{rel}/{name}")
                };
                match entry.kind {
                    EntryKind::Dir if git => {}
                    EntryKind::Dir => stack.push(r),
                    EntryKind::File | EntryKind::Symlink | EntryKind::Other => out.push(r),
                }
            }
        }
        Some(out)
    }

    /// 1.x `configPatchTouches(p, c)`: the files a patch changes, without `a/` or `b/`, each
    /// once; `None` when temper cannot read the patch.
    fn patch_touches(&self, p: &Patch<'_>, c: &Command) -> Option<Vec<String>> {
        if p.stored {
            return None;
        }
        let real = |f: &str| f != "/dev/null";
        let mut touched: Vec<String> = p
            .outputs
            .iter()
            .chain(&p.target)
            .map(|w| w.raw())
            .filter(|f| real(f))
            .map(str::to_owned)
            .collect();
        let mut texts = Vec::new();
        if p.inputs.is_empty() {
            match (&c.stdin, c.piped) {
                (
                    Some(Stdin::Heredoc {
                        body: Some(body),
                        literal: true,
                    }),
                    false,
                ) => {
                    texts.push(Some(body.to_string()));
                }
                (Some(Stdin::File(word)), false) => {
                    let abs = self.word_abs(word, c.cwd.as_deref());
                    texts.push(self.patch_text(abs.as_deref()));
                }
                _ => return None,
            }
        }
        for w in &p.inputs {
            let abs = self.word_abs(w, p.cwd.as_deref());
            texts.push(self.patch_text(abs.as_deref()));
        }
        for text in texts {
            touched.extend(patch_files(&text?).into_iter().filter(|f| real(f)));
        }
        let mut out = Found::default();
        for f in touched {
            let stripped = f
                .strip_prefix("a/")
                .or_else(|| f.strip_prefix("b/"))
                .map(str::to_owned);
            out.add(stripped.unwrap_or(f));
        }
        Some(out.list)
    }

    /// A patch file's text, when it is a file of at most [`PATCH_CAP`] bytes outside `/dev/`
    /// and `/proc/`.
    fn patch_text(&self, abs: Option<&str>) -> Option<String> {
        let abs = abs?;
        let slashed = paths::norm(abs);
        if slashed.starts_with("/dev/") || slashed.starts_with("/proc/") {
            return None;
        }
        let meta = self.stat(abs)?;
        if meta.kind != EntryKind::File || meta.len > PATCH_CAP {
            return None;
        }
        // 1.x's `readText` reads any failure as no text.
        self.files().read_text(Path::new(abs)).ok().flatten()
    }
}

/// 1.x `protectedUnder(globs, rel)`: the protected globs a whole directory `rel` replaces:
/// `<dir>/**` at or below it, and for the project root every glob.
fn protected_under<'g>(globs: &'g [String], rel: &str) -> Result<Vec<&'g str>, GlobError> {
    if rel.is_empty() {
        return Ok(globs.iter().map(String::as_str).collect());
    }
    let below = [rel, "/"].concat();
    let mut out = Vec::new();
    for g in globs {
        let Some(dir) = g.strip_suffix("/**") else {
            continue;
        };
        if Glob::new(dir)?.is_match(rel) || g.starts_with(&below) {
            out.push(g.as_str());
        }
    }
    Ok(out)
}
