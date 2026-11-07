//! Copies, moves and links onto `.rs` files, and patches of Rust sources, from the shell (1.x
//! `hooks.mjs` `shellRustWrites`, `rustFilesUnder` and `patchTouches`, audit engine-1-8).
//!
//! A copy whose source temper can read is scanned as an Edit or Write of that content to the
//! destination would be ([`RustWrites::check`]). A move of a repository source compares
//! against the source's own findings, as the gate does for a rename. A path an earlier copy in
//! the line made reads as what it was made from; one an earlier write of another kind
//! rewrote reads as unknown. A write temper cannot read (a variable, a glob, stdin, a missing
//! source) is refused, and so is a patch that touches `.rs` files. The domain decides
//! ([`BashPolicy::rust_writes`]); this reads the command and the files.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use temper_domain::{
    BashPolicy, CodeKind, CopiedFile, FileRole, InterpreterCode, LazyExemptions, LazyGlobs,
    PathStyle, PolicyDecision, RustWrites, ScanOptions, Scanner, Severity, Unread, classify_path,
    is_build_script,
};

use crate::hooks::HookCall;
use crate::hooks::shell::{
    Command, CopyDest, Guard, ParseOpts, Patch, PatchCommand, PathCx, Stdin, Target, Threw, Verb,
    Word, command_writes, landing, node_basename, parse_commands, patch_command, patch_files,
    resolve_path, word_path,
};
use crate::paths::{self, PathEnv};
use crate::ports::{EntryKind, FileMeta, Files, Ports};
use crate::project::{Workspace, crate_of};
use crate::relpath::rel_to;

/// 1.x `MAX_SOURCE_BYTES`: the largest source a copy is scanned from.
const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024;
/// The largest patch file temper reads (1.x `MAX_SOURCE_BYTES * 8`).
const PATCH_CAP: u64 = 8 * MAX_SOURCE_BYTES;
/// 1.x `rustFilesUnder`'s limit on the entries of a copied tree.
const TREE_LIMIT: usize = 10_000;

/// The scan, with what it reads the files with.
pub(super) struct Scan<'a> {
    pub(super) policy: &'a BashPolicy,
    pub(super) scanner: &'a Scanner,
}

/// 1.x `shellRustWrites(cmd, proj, input)` under `readerRule`: the answer to what `cmd`
/// copies, moves, links or patches onto `.rs` files, or `None` when it writes no Rust source,
/// or where 1.x's reader throws.
pub(super) fn shell_rust_writes(
    call: &HookCall<'_, impl Ports>,
    ws: &Workspace,
    cmd: &str,
    scan: &Scan<'_>,
) -> Option<PolicyDecision> {
    let env = call.env;
    let style = env.style;
    let cwd = env.cwd.to_string_lossy();
    let start = super::start_dir(call, ws)?;
    let temp = env.temp_dir.to_string_lossy();
    let home = env.home.to_string_lossy();
    let guard = Guard::default();
    let cx = PathCx {
        at: PathEnv { style, cwd: &cwd },
        os_tmp: &temp,
        guard: Some(&guard),
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
    let commands = parse_commands(cmd, opts).ok()?;
    let config = &ws.config;
    // Compiled where 1.x's `checkFile` first tries each glob; one that does not compile there
    // stops the scan as 1.x's throw does.
    let protected = LazyGlobs::new(config.protected().iter().map(String::as_str));
    let exempt = LazyExemptions::new(config.exempt());
    let mut reader = Reader {
        files: call.ports.files(),
        cx,
        root: &ws.root,
        code: scan.policy.code(),
        scanner: scan.scanner,
        protected: &protected,
        exempt: &exempt,
        found: RustWrites::default(),
        earlier: HashMap::new(),
        last_write: BTreeMap::new(),
        written: HashSet::new(),
        step: 0,
    };
    for c in &commands {
        reader.command(c).ok()?;
        // A path past V8's longest string, or past the budget: 1.x has thrown, or would
        // take longer than the hook may.
        if guard.overflowed() {
            return None;
        }
    }
    let found = reader.found;
    scan.policy.rust_writes(cmd, found)
}

/// What an earlier copy in the line put at a path (1.x `earlierCopies`' values).
#[derive(Debug, Clone)]
struct Made {
    /// What the path holds: the copy's source.
    real: String,
    /// The content was duplicated, so moving it on is still a copy.
    copy: bool,
    /// The step of the command that made it.
    step: usize,
    /// The source was rewritten before that.
    stale: bool,
}

struct Reader<'a, F> {
    files: &'a F,
    cx: PathCx<'a>,
    root: &'a str,
    code: &'a InterpreterCode,
    scanner: &'a Scanner,
    protected: &'a LazyGlobs<'a>,
    exempt: &'a LazyExemptions<'a>,
    found: RustWrites,
    earlier: HashMap<String, Made>,
    /// Every path a command in the line writes, with the last step that wrote it.
    last_write: BTreeMap<String, usize>,
    /// The paths written so far, which a patch read later in the line cannot trust.
    written: HashSet<String>,
    step: usize,
}

impl<F: Files> Reader<'_, F> {
    fn style(&self) -> PathStyle {
        self.cx.at.style
    }

    fn stat(&self, path: &str) -> Option<FileMeta> {
        // 1.x's `statOf` reads any failure as nothing there.
        self.files.stat(Path::new(path)).ok().flatten()
    }

    fn is(&self, path: &str, kind: EntryKind) -> bool {
        self.stat(path).is_some_and(|meta| meta.kind == kind)
    }

    /// 1.x `readText`: any failure is no text.
    fn read(&self, path: &str) -> Option<String> {
        self.files.read_text(Path::new(path)).ok().flatten()
    }

    /// A file's text when it is a file of at most `cap` bytes outside `/dev/` and `/proc/`.
    fn text_of(&self, path: &str, cap: u64) -> Option<String> {
        if special(path) {
            return None;
        }
        let meta = self.stat(path)?;
        (meta.kind == EntryKind::File && meta.len <= cap)
            .then(|| self.read(path))
            .flatten()
    }

    fn rel(&self, abs: &str) -> String {
        rel_to(self.files, self.cx.at, self.root, abs)
    }

    /// 1.x `inProject(abs)`.
    fn in_project(&self, abs: &str) -> bool {
        let r = self.rel(abs);
        let drive = r.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && r.as_bytes().get(1) == Some(&b':');
        !(r.starts_with("..")
            || paths::is_absolute(self.style(), &r)
            || drive
            || r.split('/').any(|seg| seg == ".git"))
    }

    /// 1.x `scratch(abs)`: the gate's scratch directories, `.temper/` and `target/`.
    fn scratch(&self, abs: &str) -> bool {
        let r = self.rel(abs);
        let first = r.split('/').next().unwrap_or("");
        first == ".temper" || first == "target"
    }

    /// 1.x `kindOf(abs)`: how the scanner reads a file there.
    fn kind_of(&self, abs: &str) -> (CodeKind, FileRole) {
        let krate = crate_of(self.files, self.cx.at, abs);
        let crate_rel = match &krate {
            Some(k) => rel_to(self.files, self.cx.at, &k.dir, abs),
            None => self.rel(abs),
        };
        let kind = krate.map_or(CodeKind::Prod, |k| classify_path(&crate_rel, k.lib));
        let role = if is_build_script(&crate_rel) {
            FileRole::BuildScript
        } else {
            FileRole::Source
        };
        (kind, role)
    }

    /// 1.x `rustDir(abs)`: a directory a copy would put Rust sources in.
    fn rust_dir(&self, abs: &str) -> bool {
        if !self.in_project(abs) || self.scratch(abs) {
            return false;
        }
        let holds_rs = self
            .files
            .read_dir(Path::new(abs))
            .is_ok_and(|entries| entries.iter().any(|e| rs(&e.name.to_string_lossy())));
        if holds_rs {
            return true;
        }
        let probe = paths::join(self.style(), abs, "x.rs");
        crate_of(self.files, self.cx.at, &probe).is_some_and(|k| {
            let r = rel_to(self.files, self.cx.at, &k.dir, abs);
            let head = r.split('/').next().unwrap_or("");
            head == "src" || (matches!(r.as_str(), "tests" | "benches" | "examples"))
        })
    }

    /// One command, in 1.x's order: its patch, its copies, then what it writes.
    fn command(&mut self, c: &Command) -> Result<(), Threw> {
        let writes = command_writes(c, self.cx, self.code);
        let targets: Vec<Option<String>> = writes.iter().map(|t| t.path(self.cx)).collect();
        for (t, path) in writes.iter().zip(&targets) {
            if t.verb == Verb::Redirect
                && let Some(path) = path
            {
                self.written.insert(path.to_owned()); // the set and the target each hold one
            }
        }
        if let Some(PatchCommand::Applies(p)) = patch_command(c, self.cx) {
            self.patch(c, &p);
        }
        let symbolic = ln_flag(c, 's', "--symbolic");
        let relative = symbolic && ln_flag(c, 'r', "--relative");
        self.step = self.step.saturating_add(1);
        for (t, dest) in writes.iter().zip(&targets) {
            let verb = verb_name(&t.verb);
            if is_copy(verb) {
                let how = if verb == "ln" && symbolic {
                    How::Link { relative }
                } else if is_move(verb) {
                    How::Move
                } else {
                    How::Copy
                };
                let op = Copying {
                    t,
                    dest: dest.as_deref(),
                    how,
                };
                self.copy(&op)?;
            }
        }
        for path in targets.iter().flatten() {
            self.written.insert(path.to_owned()); // the set and the map each hold one
            let resolved = paths::resolve(self.style(), self.cx.at.cwd, path);
            self.last_write.insert(resolved, self.step);
        }
        // A name spelled `x.r\s` or `x.r''s`, or a writer the shell-write patterns do not know
        // (`curl -o`, `dd of=`, `sponge`), read the way the shell does.
        for (t, path) in writes.iter().zip(&targets) {
            let verb = verb_name(&t.verb);
            let skipped = is_copy(verb)
                || matches!(
                    t.verb,
                    Verb::GitMv | Verb::GitRestore { .. } | Verb::Into(_)
                )
                || t.word.glob();
            if let Some(path) = path.as_deref().filter(|_| !skipped)
                && rs(path)
                && self.in_project(path)
                && !self.scratch(path)
            {
                self.found.direct();
            }
        }
        Ok(())
    }

    /// A patch that applies: the Rust sources it changes, or why temper cannot read it. A
    /// guessed alias counts only with a patch file temper can read.
    fn patch(&mut self, c: &Command, p: &Patch<'_>) {
        match self.patch_touches(p, c) {
            Touches::Read(touched) => self.found.patched(touched),
            Touches::Unread { later } if !p.guessed => {
                let text: Vec<&str> = std::iter::once(&c.argv0)
                    .chain(&c.args)
                    .map(Word::raw)
                    .collect();
                let why = if later { Unread::Later } else { Unread::Patch };
                self.found.unread(text.join(" "), why);
            }
            Touches::Unread { .. } => {}
        }
    }

    /// 1.x `patchTouches(p, c, written)`: the `.rs` files a patch changes, without `a/` or
    /// `b/`, each once.
    fn patch_touches(&self, p: &Patch<'_>, c: &Command) -> Touches {
        if p.stored {
            return Touches::Unread { later: false };
        }
        let mut touched: Vec<String> = p
            .outputs
            .iter()
            .chain(&p.target)
            .map(|w| w.raw())
            .filter(|f| rs(f))
            .map(str::to_owned)
            .collect();
        let mut inputs: Vec<Input> = p
            .inputs
            .iter()
            .map(|w| Input::File(word_path(self.cx, w, p.cwd.as_ref())))
            .collect();
        if inputs.is_empty() {
            match (&c.stdin, c.piped) {
                (
                    Some(Stdin::Heredoc {
                        body: Some(body),
                        literal: true,
                    }),
                    false,
                ) => inputs.push(Input::Body(body.to_string())),
                (Some(Stdin::File(word)), false) => {
                    inputs.push(Input::File(word_path(self.cx, word, c.cwd.as_ref())));
                }
                _ => return Touches::Unread { later: false },
            }
        }
        for input in inputs {
            let text = match input {
                Input::Body(body) => Some(body),
                Input::File(Some(abs)) if self.written.contains(&abs) => {
                    return Touches::Unread { later: true };
                }
                Input::File(abs) => abs.and_then(|abs| self.text_of(&abs, PATCH_CAP)),
            };
            let Some(text) = text else {
                return Touches::Unread { later: false };
            };
            touched.extend(patch_files(&text).into_iter().filter(|f| rs(f)));
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for mut f in touched {
            if f.starts_with("a/") || f.starts_with("b/") {
                f.drain(..2);
            }
            if seen.insert(f.clone()) {
                out.push(f); // the set and the list each hold one
            }
        }
        Touches::Read(out)
    }
}

/// How a copy takes its sources.
#[derive(Clone, Copy, PartialEq, Eq)]
enum How {
    /// The content is duplicated (`cp`, `install`, a hard `ln`).
    Copy,
    /// The source goes away (`mv`, `git mv`).
    Move,
    /// `ln -s`: the link reads its target later; `relative` for `ln -sr`, whose target is
    /// read from the current directory.
    Link { relative: bool },
}

/// One copy target and how it takes its sources.
struct Copying<'t, 'c> {
    t: &'t Target<'c>,
    /// The destination's path, when temper knows it.
    dest: Option<&'t str>,
    how: How,
}

/// One source of a copy whose path and destination temper knows.
#[derive(Clone, Copy)]
struct Known<'k> {
    s: &'k Word,
    src: &'k str,
    dest: &'k str,
}

/// What a known source holds when the command reaches it.
struct Holds {
    /// The file it was made from, or itself.
    real: String,
    /// The content is duplicated.
    copied: bool,
    /// An earlier write rewrote it.
    stale: bool,
}

/// Where a scanned file's content comes from.
#[derive(Clone, Copy)]
struct From<'f> {
    path: &'f str,
    /// A move, whose source's own findings may count as already there.
    moves: bool,
    /// The copied directory it came with, if any.
    tree: Option<&'f str>,
}

/// Where a patch is read from.
enum Input {
    File(Option<String>),
    Body(String),
}

/// What [`Reader::patch_touches`] found.
enum Touches {
    Read(Vec<String>),
    /// `later`: an earlier command in the line writes the patch file.
    Unread {
        later: bool,
    },
}

impl<F: Files> Reader<'_, F> {
    /// 1.x `copy(t, move, symbolic, relative)`: each source of a copy, move or link.
    fn copy(&mut self, op: &Copying<'_, '_>) -> Result<(), Threw> {
        let t = op.t;
        let Some(copy) = &t.copy else {
            return Ok(());
        };
        for s in &copy.sources {
            let src = match op.how {
                How::Link { relative } => self.link_source(op, s, relative),
                How::Copy | How::Move => word_path(self.cx, s, t.cwd.as_ref()),
            };
            if let Some(src) = &src
                && t.word.value().is_none()
                && self.clean_anywhere(src, t)
            {
                continue;
            }
            let no_src = src.is_none();
            let (Some(src), Some(dest)) = (src.as_deref(), op.dest) else {
                self.unknown_copy(op, s, no_src);
                continue;
            };
            self.copy_one(op, Known { s, src, dest })?;
        }
        Ok(())
    }

    /// A source or destination temper cannot name: refused when it is a `.rs` file in the
    /// project, or a source that may be Rust copied into a Rust source directory.
    fn unknown_copy(&mut self, op: &Copying<'_, '_>, s: &Word, no_src: bool) {
        let (t, dest) = (op.t, op.dest);
        let w = &t.word;
        let why = if t.cwd.is_none()
            && s.value().is_some()
            && !s.glob()
            && w.value().is_some()
            && !w.glob()
        {
            Unread::Cwd
        } else {
            Unread::Dynamic
        };
        let named = rs(s.raw()) || rs(w.raw());
        let into_rust = dest.is_some_and(|dest| {
            let op = t.copy.as_ref();
            let into = op.and_then(|op| op.into_dir).unwrap_or(false)
                || self.is(dest, EntryKind::Dir)
                || w.raw().ends_with(['/', '\\'])
                || (op.is_some_and(|op| op.recursive)
                    && !has_extension(paths::basename(self.style(), dest)));
            no_src && !other_extension(s.raw()) && into && self.rust_dir(dest)
        });
        let in_scope = dest.is_none_or(|dest| self.in_project(dest) && !self.scratch(dest));
        if (named && in_scope) || into_rust {
            let text = if no_src { s.raw() } else { w.raw() };
            self.found.unread(text.to_owned(), why);
        }
    }

    /// One source whose path and destination temper knows.
    fn copy_one(&mut self, op: &Copying<'_, '_>, k: Known<'_>) -> Result<(), Threw> {
        let made = self.prior(k.src);
        let holds = Holds {
            stale: self.rewritten(k.src, made.as_ref()),
            copied: op.how != How::Move || made.as_ref().is_some_and(|m| m.copy),
            real: made.map_or_else(|| k.src.to_owned(), |m| m.real),
        };
        let meta = if special(&holds.real) {
            None
        } else {
            self.stat(&holds.real)
        };
        if meta.is_some_and(|m| m.kind == EntryKind::Dir) {
            if op_recursive(op.t) || matches!(op.how, How::Link { .. }) {
                self.copy_tree(op.t, k, &holds)?;
            }
            return Ok(());
        }
        let at = CopyDest {
            path: k.dest,
            is_dir: self.is(k.dest, EntryKind::Dir),
        };
        // A source with a known name always lands somewhere; 1.x throws if not.
        let target = landing(self.cx, op.t, at, k.s).ok_or(Threw)?;
        let is_file = meta.is_some_and(|m| m.kind == EntryKind::File);
        if is_file {
            // The map keeps the target; the scan below reads it too.
            self.earlier.insert(target.clone(), self.made(&holds));
        }
        if !rs(&target) || !self.in_project(&target) || self.scratch(&target) {
            return Ok(());
        }
        if holds.stale {
            self.found.unread(k.s.raw().to_owned(), Unread::Stale);
        } else if !is_file {
            self.found.unread(k.s.raw().to_owned(), Unread::Dynamic);
        } else {
            let from = From {
                path: &holds.real,
                moves: !holds.copied,
                tree: None,
            };
            self.check_file(&target, from)?;
        }
        Ok(())
    }

    /// What this step records of a copy that made a path.
    fn made(&self, holds: &Holds) -> Made {
        Made {
            real: holds.real.clone(), // each record owns its path
            copy: holds.copied,
            step: self.step,
            stale: holds.stale,
        }
    }

    /// A directory copied, moved or linked: each `.rs` file in it lands below the root.
    fn copy_tree(&mut self, t: &Target<'_>, k: Known<'_>, holds: &Holds) -> Result<(), Threw> {
        let root = self.tree_root(t, k);
        // The map keeps the root; the checks below read it too.
        self.earlier.insert(root.clone(), self.made(holds));
        if !self.in_project(&root) || self.scratch(&root) {
            return Ok(());
        }
        if holds.stale {
            self.found.unread(k.s.raw().to_owned(), Unread::Stale);
            return Ok(());
        }
        let real = holds.real.as_str();
        let Some(files) = self.rust_files_under(real) else {
            self.found.unread(k.s.raw().to_owned(), Unread::Dynamic);
            return Ok(());
        };
        for f in files {
            let inner = paths::relative(self.style(), self.cx.at.cwd, real, &f);
            let target = paths::resolve(self.style(), &root, &inner);
            if self.in_project(&target) {
                let from = From {
                    path: &f,
                    moves: !holds.copied,
                    tree: Some(real),
                };
                self.check_file(&target, from)?;
            }
        }
        Ok(())
    }

    /// Where a copied directory lands: rsync's `src/` copies the contents, and a copy into a
    /// directory puts the source's own name inside it.
    fn tree_root(&self, t: &Target<'_>, k: Known<'_>) -> String {
        let Known { s, src, dest } = k;
        let rsync = t.verb == Verb::Named("rsync");
        if rsync && s.raw().ends_with(['/', '\\']) {
            return dest.to_owned();
        }
        let into = t
            .copy
            .as_ref()
            .and_then(|op| op.into_dir)
            .unwrap_or_else(|| self.is(dest, EntryKind::Dir));
        if rsync || into {
            let name = node_basename(self.style(), src);
            paths::resolve(self.style(), dest, name)
        } else {
            dest.to_owned()
        }
    }

    /// 1.x `linkSource(t, s, relative)`: `ln -s` reads a relative source from the link's
    /// directory, `ln -sr` from the current one.
    fn link_source(&self, op: &Copying<'_, '_>, s: &Word, relative: bool) -> Option<String> {
        if relative {
            return word_path(self.cx, s, op.t.cwd.as_ref());
        }
        let value = s.value().filter(|_| !s.glob())?;
        if let Some(abs) = resolve_path(self.cx, Some(value), None) {
            return Some(abs);
        }
        let dest = op.dest?;
        let at = CopyDest {
            path: dest,
            is_dir: self.is(dest, EntryKind::Dir),
        };
        let landed = landing(self.cx, op.t, at, s)?;
        let dir = paths::dirname(self.style(), &landed);
        Some(paths::resolve(self.style(), dir, value))
    }

    /// 1.x `cleanAnywhere(srcAbs, s, t)`: a readable source copied to a destination held in a
    /// variable lands clean wherever it goes when it scans clean as production code and does
    /// not land as a `mod.rs`.
    fn clean_anywhere(&self, src: &str, t: &Target<'_>) -> bool {
        let made = self.prior(src);
        if self.rewritten(src, made.as_ref()) {
            return false;
        }
        let real = made.as_ref().map_or(src, |m| m.real.as_str());
        let Some(text) = self.text_of(real, MAX_SOURCE_BYTES) else {
            return false;
        };
        let style = self.style();
        let real_name = node_basename(style, real);
        let into = t.copy.as_ref().and_then(|op| op.into_dir).unwrap_or(false);
        let name = if t.word.raw().ends_with(['/', '\\']) || into {
            real_name
        } else {
            node_basename(style, t.word.raw())
        };
        if name == "mod.rs" || (real_name == "mod.rs" && !rs(name)) {
            return false;
        }
        let options = ScanOptions {
            kind: CodeKind::Prod,
            file: FileRole::Source,
            forbid: None,
        };
        !self
            .scanner
            .scan(&text, &options)
            .hits()
            .iter()
            .any(|h| matches!(h.severity(), Severity::Hard | Severity::Ask))
    }

    /// 1.x `earlier(abs)`: what an earlier copy put at `abs` or at a directory above it.
    fn earlier_at(&self, abs: &str) -> Option<Made> {
        let style = self.style();
        let mut p = abs;
        let mut rest: Vec<&str> = Vec::new();
        loop {
            if let Some(hit) = self.earlier.get(p) {
                let mut made = hit.clone(); // a lookup's own copy, with `rest` joined on
                if !rest.is_empty() {
                    rest.reverse();
                    made.real = paths::resolve(style, &hit.real, &rest.join("/"));
                }
                return Some(made);
            }
            let up = paths::dirname(style, p);
            if up == p {
                return None;
            }
            rest.push(paths::basename(style, p));
            p = up;
        }
    }

    /// 1.x `prior(abs)`: what `abs` holds when the command reaches it, if an earlier copy made
    /// it (over a file that exists too) and what it was made from still exists.
    fn prior(&self, abs: &str) -> Option<Made> {
        let hit = self.earlier_at(abs)?;
        (self.stat(abs).is_none() || self.stat(&hit.real).is_some()).then_some(hit)
    }

    /// 1.x `rewritten(abs, hit)`: a write other than the copy that made `abs`, at it, above it
    /// or below it, after that copy.
    fn rewritten(&self, abs: &str, hit: Option<&Made>) -> bool {
        if hit.is_some_and(|h| h.stale) {
            return true;
        }
        let since = hit.map_or(0, |h| h.step);
        let style = self.style();
        let abs = paths::resolve(style, self.cx.at.cwd, abs);
        let after = |w: &str| self.last_write.get(w).is_some_and(|&n| n > since);
        // At it, or at a directory above it other than a root, which 1.x's `${w}${sep}`
        // prefix never matches.
        if after(&abs) {
            return true;
        }
        let mut p = paths::dirname(style, &abs);
        while paths::dirname(style, p) != p {
            if after(p) {
                return true;
            }
            p = paths::dirname(style, p);
        }
        // Below it.
        let sep = match style {
            PathStyle::Posix => "/",
            PathStyle::Windows => "\\",
        };
        let below = [abs.as_str(), sep].concat();
        self.last_write
            .range(below.clone()..) // the range's own bound
            .take_while(|(w, _)| w.starts_with(&below))
            .any(|(_, &n)| n > since)
    }

    /// 1.x `rustFilesUnder(dir)`: every `.rs` file or link named `.rs` under `dir`, in its
    /// order, or `None` past [`TREE_LIMIT`] entries.
    fn rust_files_under(&self, dir: &str) -> Option<Vec<String>> {
        let style = self.style();
        let mut out = Vec::new();
        let mut stack = vec![dir.to_owned()];
        let mut seen = 0_usize;
        while let Some(d) = stack.pop() {
            let Ok(entries) = self.files.read_dir(Path::new(&d)) else {
                continue;
            };
            for e in entries {
                seen = seen.saturating_add(1);
                if seen > TREE_LIMIT {
                    return None;
                }
                let name = e.name.to_string_lossy();
                let p = paths::join(style, &d, &name);
                match e.kind {
                    EntryKind::Dir if name != ".git" => stack.push(p),
                    EntryKind::File | EntryKind::Symlink if rs(&name) => out.push(p),
                    EntryKind::Dir | EntryKind::File | EntryKind::Symlink | EntryKind::Other => {}
                }
            }
        }
        Some(out)
    }

    /// 1.x `checkFile(target, from, move, tree)`: the scan of `from`'s content landing at
    /// `target`. `tree` is the copied directory when `from` came with one. A `protected` or
    /// `scanner.exempt` glob that does not compile where 1.x first tries it is a throw.
    fn check_file(&mut self, target: &str, from: From<'_>) -> Result<(), Threw> {
        let From {
            path: from,
            moves,
            tree,
        } = from;
        let style = self.style();
        let cwd = self.cx.at.cwd;
        if paths::resolve(style, cwd, target) == paths::resolve(style, cwd, from)
            || self.scratch(target)
        {
            return Ok(());
        }
        let Some(after) = self.text_of(from, MAX_SOURCE_BYTES) else {
            self.found
                .unread(paths::norm(from).into_owned(), Unread::Dynamic);
            return Ok(());
        };
        let rel = self.rel(target);
        // 1.x `exists ? readText(target) ?? '' : ''`: a target it cannot read counts as empty.
        let before = self
            .is(target, EntryKind::File)
            .then(|| self.read(target).unwrap_or_default());
        // A tree that carries its crate's manifest lands as that crate: the target maps the
        // way its source does.
        let carried = tree.is_some_and(|tree| {
            crate_of(self.files, self.cx.at, from).is_some_and(|k| {
                let r = paths::relative(style, cwd, tree, &k.dir);
                !r.starts_with("..") && !paths::is_absolute(style, &r)
            })
        });
        let (kind, role) = self.kind_of(if carried { from } else { target });
        // A moved repository source brings its own findings along, unless the move changes
        // how the scanner reads it.
        let lenient =
            moves && rs(from) && self.in_project(from) && self.kind_of(from) == (kind, role);
        let file = CopiedFile {
            rel: &rel,
            before: before.as_deref(),
            after: &after,
            options: ScanOptions {
                kind,
                file: role,
                forbid: None,
            },
            moved_from: lenient.then(|| node_basename(style, from)),
            // 1.x `globMatch`'s `new RegExp` throws, and `readerRule` catches it.
            protected: self.protected.any_match(&rel)?,
        };
        self.found.check(&file, self.exempt, self.scanner)?;
        Ok(())
    }
}

/// 1.x `/\.rs$/`, which is case-sensitive, as the compiler's module lookup is.
fn rs(name: &str) -> bool {
    name.strip_suffix(".rs").is_some()
}

/// 1.x `SPECIAL_FILE`: `/dev/` and `/proc/`, which are not read.
fn special(path: &str) -> bool {
    let slashed = paths::norm(path);
    slashed.starts_with("/dev/") || slashed.starts_with("/proc/")
}

/// 1.x's verb names, as `RS_COPIES` lists them.
fn verb_name<'v>(verb: &Verb<'v>) -> &'v str {
    match verb {
        Verb::Named(name) | Verb::Into(name) => name,
        Verb::GitMv => "git mv",
        Verb::Redirect | Verb::InPlace(_) | Verb::GitRestore { .. } | Verb::Code => "",
    }
}

/// 1.x `RS_COPIES`.
fn is_copy(verb: &str) -> bool {
    matches!(
        verb,
        "cp" | "mv"
            | "install"
            | "ln"
            | "rsync"
            | "scp"
            | "git mv"
            | "copy-item"
            | "move-item"
            | "cpi"
            | "mi"
            | "copy"
            | "move"
    )
}

/// 1.x `RS_MOVES`.
fn is_move(verb: &str) -> bool {
    matches!(verb, "mv" | "git mv" | "move-item" | "mi" | "move")
}

fn op_recursive(t: &Target<'_>) -> bool {
    t.copy.as_ref().is_some_and(|op| op.recursive)
}

/// 1.x `lnFlag(letter, long)`: `ln` with `-<letters>` holding `letter`, or `long`.
fn ln_flag(c: &Command, letter: char, long: &str) -> bool {
    c.name.as_deref() == Some("ln")
        && c.args.iter().any(|a| {
            a.raw() == long
                || a.raw().strip_prefix('-').is_some_and(|rest| {
                    rest.chars()
                        .take_while(char::is_ascii_alphabetic)
                        .any(|ch| ch == letter)
                })
        })
}

/// 1.x `/\.\w+$/`: the name ends in an extension.
fn has_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        !ext.is_empty() && ext.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

/// 1.x `/\.(?!rs$)\w+$/`: the word ends in an extension other than `.rs`.
fn other_extension(raw: &str) -> bool {
    // Any `.` followed by word characters to the end, other than exactly `.rs`.
    raw.char_indices()
        .filter(|&(_, c)| c == '.')
        .any(|(at, _)| {
            let ext = raw.get(at.saturating_add(1)..).unwrap_or("");
            !ext.is_empty()
                && ext.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && ext != "rs"
        })
}
