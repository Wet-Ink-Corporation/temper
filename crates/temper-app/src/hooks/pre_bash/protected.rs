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

use temper_domain::{ConfigChanges, Glob, GlobError, InterpreterCode, LazyGlobs};

use crate::git::{self, Short};
use crate::hooks::HookCall;
use crate::hooks::shell::{
    Command, CopyDest, Guard, ParseOpts, Patch, PatchCommand, PatchTool, PathCx, Stdin, Target,
    Verb, Word, command_deletes, command_writes, js_len, landing, names_absolute, parse_commands,
    patch_command, patch_files, pathspec_pattern, resolve_path,
};
use crate::paths::{self, PathEnv};
use crate::ports::{EntryKind, FileMeta, Files, Ports};
use crate::project::{Workspace, discover_at, exists};
use crate::relpath::{LongPaths, Relater};

/// 1.x `PATH_MAX`: an ancestor longer than this, in UTF-16 units, cannot exist.
const PATH_MAX: usize = 4096;
/// 1.x `configFilesUnder`'s limit on the entries of a copied or deleted tree.
const TREE_LIMIT: usize = 10_000;
/// The largest patch file temper reads (1.x's 32 MiB).
const PATCH_CAP: u64 = 32 * 1024 * 1024;

/// The list a protected hit goes into.
#[derive(Debug, Clone, Copy)]
enum List {
    Written,
    Deleted,
}

/// A `git checkout` or `git restore` from another revision: the revision and the pathspec.
#[derive(Debug, Clone, Copy)]
struct Restore<'a> {
    source: &'a str,
    pathspec: &'a str,
}

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
    code: &InterpreterCode,
) -> Changes {
    protected_shell_writes_in(call, ws, (cmd, code), &Guard::default())
}

/// [`protected_shell_writes`] of `line` (the command and the interpreter-code matcher)
/// within `guard`'s bounds, which a test may set lower.
pub(super) fn protected_shell_writes_in(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    line: (&str, &InterpreterCode),
    guard: &Guard,
) -> Changes {
    let (cmd, code) = line;
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
    let cx = PathCx {
        at,
        os_tmp: &temp,
        guard: Some(guard),
    };
    let opts = ParseOpts {
        paths: cx,
        cwd: Some(&start),
        home: &home,
        top: Some(&ws.root),
        git_dir: ".git",
        tmp: Some(&temp),
        untracked: false,
    };
    let Ok(commands) = parse_commands(cmd, opts) else {
        return Changes::default();
    };
    // Each glob compiles where a match first tries it, as 1.x's `globs.some(...)` builds it.
    let globs = LazyGlobs::new(ws.config.protected().iter().map(String::as_str));
    let mut reader = Reader {
        call,
        ws,
        cx,
        start: &start,
        globs,
        code,
        owners: HashMap::new(),
        owner_in: HashMap::new(),
        long: LongPaths::default(),
        listed: HashMap::new(),
        changes: Changes::default(),
    };
    // Once a path has gone past V8's longest string, 1.x has thrown: the rest is not read.
    let read = commands
        .iter()
        .take_while(|_| !guard.overflowed())
        .try_for_each(|c| reader.command(c));
    match read {
        // A path joined past V8's longest string throws in 1.x, and `readerRule` falls back.
        Ok(()) if guard.overflowed() => Changes::default(),
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
    /// `ws`'s protected list, owned beside it.
    globs: LazyGlobs<'static>,
    prefix: String,
}

struct Reader<'a, 'c, P> {
    call: &'a HookCall<'c, P>,
    ws: &'a Workspace,
    cx: PathCx<'a>,
    start: &'a str,
    globs: LazyGlobs<'a>,
    code: &'a InterpreterCode,
    /// 1.x `ownerOf`'s cache: each ancestor checked, by its path relative to the root, and
    /// the checkout there, if any. A hit joins no path.
    owners: HashMap<String, Option<Owner>>,
    /// The owner of every name in a directory, by the directory relative to the root: the
    /// walk up its ancestors is made once per directory, not once per name, and
    /// stops at the first ancestor in here.
    owner_in: HashMap<String, Owner>,
    /// The long paths `rel_to` found, for the names outside the root.
    long: LongPaths,
    /// What git said of each restore, by directory, source and pathspec: 1.x asks each time
    /// the line repeats one, though the answer cannot change within a line.
    listed: HashMap<(String, String, String), Listed>,
    changes: Changes,
}

/// What git says a restore would change.
#[derive(Clone)]
enum Listed {
    /// These files, resolved; a shared handle, so a repeat replays them without a copy.
    Files(Rc<[String]>),
    /// git could not say.
    Unknown,
    /// Not a repository, or no such revision: the restore fails too.
    Failed,
}

/// Where a relative name may resolve: the command's directory, else the start and the root.
fn bases<'b>(cwd: Option<&'b str>, start: &'b str, root: &'b str) -> [Option<&'b str>; 2] {
    match cwd {
        Some(dir) => [Some(dir), None],
        None => [Some(start), Some(root)],
    }
}

impl<'a, P: Ports> Reader<'a, '_, P> {
    fn files(&self) -> &'a P::Files {
        self.call.ports.files()
    }

    /// 1.x `relTo(root, abs)`, through the long paths this line found before.
    fn rel_to(&mut self, root: &str, abs: &str) -> String {
        Relater::new(self.files(), &mut self.long).rel_to(self.cx.at, root, abs)
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
        if word.glob() || !word.value_is_known() {
            return None;
        }
        // Read for this resolve alone and charged, keeping no copy.
        let value = word.text_read()?;
        resolve_path(self.cx, Some(&value), base)
    }

    /// One command's patch, writes and deletes, in 1.x's order.
    fn command(&mut self, c: &Command) -> Result<(), GlobError> {
        if let Some(PatchCommand::Applies(p)) = patch_command(c, self.cx) {
            self.patch(c, &p)?;
        }
        for t in command_writes(c, self.cx, self.code) {
            self.write(&t)?;
        }
        let (start, ws) = (self.start, self.ws);
        let style = self.cx.at.style;
        // A delete takes the file, or with -r a whole directory and the protected files in it.
        for d in command_deletes(c, self.cx) {
            // An absolute name reads no directory, as 1.x's resolve stops at it, so its
            // directory is not built, and one base stands for all.
            let absolute = names_absolute(self.cx, &d.word);
            let cwd = if absolute {
                None
            } else {
                self.cx.dir_text(d.cwd.as_ref())
            };
            if self.cx.stopped() {
                return Ok(()); // 1.x has thrown: the rule falls back
            }
            let dir = if absolute {
                Some(start)
            } else {
                cwd.as_deref()
            };
            for base in bases(dir, start, &ws.root).into_iter().flatten() {
                let Some(abs) = self.word_abs(&d.word, Some(base)) else {
                    continue;
                };
                self.check(&abs, d.recursive, List::Deleted)?;
                if d.recursive && self.is(&abs, EntryKind::Dir) {
                    // Past the walk limit the list is `None`, read as no files: the directory's
                    // own globs, checked above as a tree, stand for them.
                    for f in self.config_files_under(&abs).unwrap_or_default() {
                        self.check(&paths::join(style, &abs, &f), false, List::Deleted)?;
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
        // An unread patch (`None`) names no files; it asks as unread above.
        let listed: Vec<String> = touched
            .unwrap_or_default()
            .into_iter()
            .filter(|f| !named.contains(&Some(f.as_str())))
            .collect();
        let (start, ws) = (self.start, self.ws);
        let style = self.cx.at.style;
        let cwd = self.cx.dir_text(p.cwd.as_ref());
        if self.cx.stopped() {
            return Ok(()); // 1.x has thrown: the rule falls back
        }
        for base in bases(cwd.as_deref(), start, &ws.root).into_iter().flatten() {
            for v in &named {
                if let Some(abs) = resolve_path(self.cx, *v, Some(base)) {
                    self.check(&abs, false, List::Written)?;
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
                    self.check(&paths::resolve(style, b, f), false, List::Written)?;
                }
            }
        }
        Ok(())
    }

    /// A plain write to an absolute name. It reads no directory, as 1.x's resolve stops at
    /// the name, so the command's directory is not built.
    fn write_absolute(&mut self, word: &Word) -> Result<(), GlobError> {
        match self.word_abs(word, None) {
            Some(dest) => self.check(&dest, false, List::Written),
            None => Ok(()),
        }
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
        let restore = source.map(|source| Restore {
            source,
            pathspec: t.word.raw(),
        });
        if t.copy.is_none() && restore.is_none() && names_absolute(self.cx, &t.word) {
            return self.write_absolute(&t.word);
        }
        let (start, ws) = (self.start, self.ws);
        let style = self.cx.at.style;
        let cwd = self.cx.dir_text(t.cwd.as_ref());
        if self.cx.stopped() {
            return Ok(()); // 1.x has thrown: the rule falls back
        }
        for base in bases(cwd.as_deref(), start, &ws.root).into_iter().flatten() {
            if let Some(restore) = restore
                && pathspec_pattern(t.word.raw())
            {
                self.pathspec(restore, base, true)?;
                continue;
            }
            let Some(dest) = self.word_abs(&t.word, Some(base)) else {
                continue;
            };
            let Some(copy) = &t.copy else {
                self.check(&dest, restore.is_some(), List::Written)?;
                // A directory from another revision brings back the protected files git lists
                // in it (`docs/clippy.toml`).
                if let Some(restore) = restore
                    && !self.is(&dest, EntryKind::File)
                {
                    self.pathspec(restore, base, false)?;
                }
                continue;
            };
            let dest_at = CopyDest {
                path: &dest,
                is_dir: self.is(&dest, EntryKind::Dir),
            };
            for s in &copy.sources {
                let Some(land) = landing(self.cx, t, dest_at, s) else {
                    continue;
                };
                // A recursive copy or a move of anything but a known file may bring a whole
                // tree, and with it the protected files inside it (`cfg/sub/clippy.toml`).
                let from = resolve_path(self.cx, s.value(), Some(base));
                let kind = from.as_deref().and_then(|p| self.stat(p)).map(|m| m.kind);
                let tree = copy.recursive && kind != Some(EntryKind::File);
                self.check(&land, tree, List::Written)?;
                if let Some(from) = from.filter(|_| copy.recursive && kind == Some(EntryKind::Dir))
                {
                    // Past the walk limit the list is `None`, read as no files: the landing's own
                    // globs, checked above as a tree, stand for them.
                    for f in self.config_files_under(&from).unwrap_or_default() {
                        self.check(&paths::join(style, &land, &f), false, List::Written)?;
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
        restore: Restore<'_>,
        base: &str,
        fallback: bool,
    ) -> Result<(), GlobError> {
        let key = (
            base.to_owned(),
            restore.source.to_owned(),
            restore.pathspec.to_owned(),
        );
        let listed = if let Some(listed) = self.listed.get(&key) {
            listed.clone() // an `Rc` handle, or a unit variant
        } else {
            let listed = self.list(restore, base);
            self.listed.insert(key, listed.clone()); // an `Rc` handle, or a unit variant
            listed
        };
        match listed {
            Listed::Files(files) => {
                for f in files.iter() {
                    self.check(f, false, List::Written)?;
                }
                Ok(())
            }
            Listed::Unknown => {
                let ws = self.ws;
                if fallback {
                    self.check(&ws.root, true, List::Written)?;
                }
                Ok(())
            }
            Listed::Failed => Ok(()),
        }
    }

    /// Asks git, in `base`, which files `restore` would change.
    fn list(&self, restore: Restore<'_>, base: &str) -> Listed {
        let process = self.call.ports.process();
        let dir = Path::new(base);
        let top = git::short(process, dir, &["rev-parse", "--show-toplevel"]);
        let names = [
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            restore.source,
            "--",
            restore.pathspec,
        ];
        let diff = git::short(process, dir, &names);
        match (top, diff) {
            (Short::Ran(top), Short::Ran(names)) => {
                let top = temper_domain::js_trim(&top);
                let style = self.cx.at.style;
                let files = names
                    .split('\0')
                    .filter(|f| !f.is_empty())
                    .map(|f| paths::resolve(style, top, f))
                    .collect();
                Listed::Files(files)
            }
            (Short::Unknown, _) | (_, Short::Unknown) => Listed::Unknown,
            (Short::Failed, _) | (_, Short::Failed) => Listed::Failed,
        }
    }

    /// 1.x `check(abs, tree, into)`: `abs` when it is protected, and with `tree` (a whole
    /// directory written or deleted) the protected globs under it, into the deleted list or
    /// the written one.
    fn check(&mut self, abs: &str, tree: bool, into: List) -> Result<(), GlobError> {
        let (at, ws) = (self.cx.at, self.ws);
        let rel = self.rel_to(&ws.root, abs);
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
                if !rel.is_empty() && self.globs.any_match(&rel)? {
                    hits.push(rel.clone()); // `rel` is read again below
                }
                if tree {
                    let under = protected_under(ws.config.protected(), &rel)?;
                    hits.extend(under.into_iter().map(str::to_owned));
                }
            }
            Owner::Nested(nested) => {
                let own = self.rel_to(&nested.ws.root, abs);
                if !own.is_empty() && nested.globs.any_match(&own)? {
                    hits.push(rel.clone()); // as above
                }
                if tree {
                    let under = protected_under(nested.ws.config.protected(), &own)?;
                    hits.extend(under.into_iter().map(|g| format!("{}/{g}", nested.prefix)));
                }
            }
        }
        let found = match into {
            List::Written => &mut self.changes.written,
            List::Deleted => &mut self.changes.deleted,
        };
        for hit in hits {
            found.add(hit);
        }
        Ok(())
    }

    /// 1.x `ownerOf(proj, rel, cache)`: the checkout of the deepest ancestor of `rel` with a
    /// `.git` in it, else the project. Each ancestor is looked at once per command line, and
    /// the walk is made once per directory: it reads only the ancestors, which the names in
    /// one directory share.
    fn owner_of(&mut self, rel: &str) -> Result<Owner, GlobError> {
        let Some(slash) = rel.rfind('/') else {
            return Ok(Owner::Project); // no ancestor to look at
        };
        let dir = rel.get(..slash).unwrap_or("");
        if let Some(owner) = self.owner_in.get(dir) {
            return Ok(owner.clone()); // an `Rc` handle
        }
        let owner = self.owner_up(rel)?;
        self.owner_in.insert(dir.to_owned(), owner.clone()); // an `Rc` handle for the cache
        Ok(owner)
    }

    /// [`Self::owner_of`]'s walk up `rel`'s ancestors. It stops at the first ancestor whose
    /// names' owner is known: that answer already weighs the ancestor and everything above
    /// it, so a directory one level below a known one looks at itself alone, and a line of
    /// names in ever deeper directories does not walk every ancestor for each name.
    fn owner_up(&mut self, rel: &str) -> Result<Owner, GlobError> {
        // The UTF-16 length of `rel` up to each `/`, counted down from the end, so the
        // PATH_MAX test does not count the prefix again for each part (#155).
        let mut units = js_len(rel);
        let mut after = rel.len();
        for (end, _) in rel.rmatch_indices('/') {
            units = units.saturating_sub(js_len(rel.get(end..after).unwrap_or("")));
            after = end;
            let dir = rel.get(..end).unwrap_or("");
            if let Some(owner) = self.owner_in.get(dir) {
                return Ok(owner.clone()); // an `Rc` handle
            }
            if end == 0 || units > PATH_MAX {
                continue;
            }
            if let Some(owner) = self.checkout_in(dir)? {
                return Ok(owner);
            }
        }
        Ok(Owner::Project)
    }

    /// The checkout at `dir`, relative to the root, if any; each is looked at once per line.
    fn checkout_in(&mut self, dir: &str) -> Result<Option<Owner>, GlobError> {
        if let Some(owner) = self.owners.get(dir) {
            return Ok(owner.clone()); // an `Rc` handle
        }
        let abs = paths::join(self.cx.at.style, &self.ws.root, dir);
        let owner = self.checkout_at(&abs)?;
        self.owners.insert(dir.to_owned(), owner.clone()); // an `Rc` handle for the cache
        Ok(owner)
    }

    /// The owner a `.git` in `dir` makes: its project, or this one when there is none
    /// (1.x `discover(dir) ?? proj`).
    fn checkout_at(&mut self, dir: &str) -> Result<Option<Owner>, GlobError> {
        let files = self.files();
        if !exists(files, &paths::join(self.cx.at.style, dir, ".git")) {
            return Ok(None);
        }
        let Some(ws) = discover_at(self.call.ports, self.call.env, dir) else {
            return Ok(Some(Owner::Project));
        };
        // A copy of the list, once per nested checkout (the owner is cached), so the lazy
        // matcher can live beside the workspace it reads.
        let globs = LazyGlobs::owned(ws.config.protected().to_vec());
        let prefix = self.rel_to(&self.ws.root, &ws.root);
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
                    let cwd = self.cx.dir_text(c.cwd.as_ref());
                    let abs = self.word_abs(word, cwd.as_deref());
                    texts.push(self.patch_text(abs.as_deref()));
                }
                _ => return None,
            }
        }
        for w in &p.inputs {
            let cwd = self.cx.dir_text(p.cwd.as_ref());
            let abs = self.word_abs(w, cwd.as_deref());
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
