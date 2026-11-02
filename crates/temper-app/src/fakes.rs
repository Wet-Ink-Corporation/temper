//! In-memory fakes of the ports, for this crate's tests (TST-09). A `test-util` feature will
//! expose them when another crate's tests need them.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use temper_domain::{PathStyle, Timestamp};

use crate::engine::RunEnv;
use crate::ports::{
    Clock, DirEntry, EntryKind, ExitKind, FileMeta, Files, FsError, FsOp, Ports, ProcessError,
    ProcessOutput, ProcessRequest, ProcessRunner, Stream,
};

/// A file tree held as text paths in one [`PathStyle`]. A directory exists when a file is below
/// it or it was created; listings are derived in name-byte order unless one is set.
///
/// The tree sits in `RefCell`s because the `Files` port writes through `&self`, as the real
/// file system does; the fake is single-threaded and no borrow outlives one call (OWN-11).
#[derive(Debug)]
pub(crate) struct FakeFiles {
    style: PathStyle,
    /// Each file's text, or `None` for one that exists but cannot be read.
    files: RefCell<BTreeMap<String, Option<String>>>,
    /// Directories created through the port.
    dirs: RefCell<BTreeSet<String>>,
    /// Listings that replace the derived ones; `None` for a directory that cannot be listed.
    listings: BTreeMap<String, Option<Vec<DirEntry>>>,
    /// Prefixes that resolve to another path, as a symlinked ancestor or an 8.3 short name does.
    links: Vec<(String, String)>,
    /// Paths whose real path fails with an error other than "not found".
    unresolvable: Vec<String>,
    /// Paths at and below which every write fails, as in a read-only directory.
    read_only: Vec<String>,
}

impl FakeFiles {
    pub(crate) fn new(style: PathStyle) -> Self {
        Self {
            style,
            files: RefCell::new(BTreeMap::new()),
            dirs: RefCell::new(BTreeSet::new()),
            listings: BTreeMap::new(),
            links: Vec::new(),
            unresolvable: Vec::new(),
            read_only: Vec::new(),
        }
    }

    pub(crate) fn with_file(self, path: &str, text: &str) -> Self {
        self.files
            .borrow_mut()
            .insert(path.to_owned(), Some(text.to_owned()));
        self
    }

    pub(crate) fn with_unreadable(self, path: &str) -> Self {
        self.files.borrow_mut().insert(path.to_owned(), None);
        self
    }

    pub(crate) fn with_listing(mut self, dir: &str, entries: &[(&str, EntryKind)]) -> Self {
        let entries = entries
            .iter()
            .map(|&(name, kind)| DirEntry {
                name: OsString::from(name),
                kind,
            })
            .collect();
        self.listings.insert(dir.to_owned(), Some(entries));
        self
    }

    pub(crate) fn with_unlistable(mut self, dir: &str) -> Self {
        self.listings.insert(dir.to_owned(), None);
        self
    }

    /// `from` and everything below it resolves to `to` and below.
    pub(crate) fn with_link(mut self, from: &str, to: &str) -> Self {
        self.links.push((from.to_owned(), to.to_owned()));
        self
    }

    /// `real_path` of `path` fails, as a permission error makes `realpathSync.native` throw.
    pub(crate) fn with_unresolvable(mut self, path: &str) -> Self {
        self.unresolvable.push(path.to_owned());
        self
    }

    /// Every write at or below `dir` fails with a permission error.
    pub(crate) fn with_read_only(mut self, dir: &str) -> Self {
        self.read_only.push(dir.to_owned());
        self
    }

    /// The text of the file at `path`, if there is a readable one.
    pub(crate) fn text(&self, path: &str) -> Option<String> {
        self.files.borrow().get(path).cloned().flatten()
    }

    /// Every file path, in byte order.
    pub(crate) fn paths(&self) -> Vec<String> {
        self.files.borrow().keys().cloned().collect()
    }

    fn sep(&self) -> char {
        match self.style {
            PathStyle::Windows => '\\',
            PathStyle::Posix => '/',
        }
    }

    /// The part of `path` below `dir`, without the separator; `None` when it is not below.
    fn below<'p>(&self, path: &'p str, dir: &str) -> Option<&'p str> {
        let rest = path.strip_prefix(dir)?;
        if dir.ends_with(self.sep()) {
            Some(rest).filter(|rest| !rest.is_empty())
        } else {
            rest.strip_prefix(self.sep())
        }
    }

    /// `path` with the first matching link applied: a new path, so the fake keeps its links.
    fn unlinked(&self, path: &str) -> String {
        self.links
            .iter()
            .find_map(|(from, to)| {
                if path == from {
                    Some(to.clone())
                } else {
                    let rest = self.below(path, from)?;
                    Some(format!("{to}{}{rest}", self.sep()))
                }
            })
            .unwrap_or_else(|| path.to_owned())
    }

    fn is_dir(&self, path: &str) -> bool {
        self.listings.contains_key(path)
            || self
                .dirs
                .borrow()
                .iter()
                .any(|dir| dir == path || self.below(dir, path).is_some())
            || self
                .files
                .borrow()
                .keys()
                .any(|file| self.below(file, path).is_some())
    }

    fn exists(&self, path: &str) -> bool {
        self.files.borrow().contains_key(path) || self.is_dir(path)
    }

    /// The write of `op` on `path` fails with `kind` when `path` is read-only.
    fn check_writable(&self, op: FsOp, path: &str) -> Result<(), FsError> {
        let denied = self
            .read_only
            .iter()
            .any(|dir| path == dir || self.below(path, dir).is_some());
        if denied {
            Err(failure(op, path, io::ErrorKind::PermissionDenied))
        } else {
            Ok(())
        }
    }
}

fn text_of(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn failure(op: FsOp, path: &str, kind: io::ErrorKind) -> FsError {
    FsError::new(op, path, io::Error::from(kind))
}

/// Each read hands out a copy, as reading a real file makes one; the fake keeps its tree.
impl Files for FakeFiles {
    fn read_text(&self, path: &Path) -> Result<Option<String>, FsError> {
        match self.files.borrow().get(&self.unlinked(&text_of(path))) {
            Some(Some(text)) => Ok(Some(text.clone())),
            Some(None) => Err(FsError::new(
                FsOp::Read,
                path,
                io::Error::from(io::ErrorKind::PermissionDenied),
            )),
            None => Ok(None),
        }
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FsError> {
        let dir = self.unlinked(&text_of(path));
        let not_listed = |kind| Err(FsError::new(FsOp::ReadDir, path, io::Error::from(kind)));
        match self.listings.get(&dir) {
            Some(Some(entries)) => return Ok(entries.clone()),
            Some(None) => return not_listed(io::ErrorKind::PermissionDenied),
            None => {}
        }
        let mut children = BTreeMap::new();
        for file in self.files.borrow().keys() {
            if let Some(rest) = self.below(file, &dir) {
                let (name, kind) = rest
                    .split_once(self.sep())
                    .map_or((rest, EntryKind::File), |(name, _)| (name, EntryKind::Dir));
                children.insert(name.to_owned(), kind);
            }
        }
        if children.is_empty() {
            return not_listed(io::ErrorKind::NotFound);
        }
        Ok(children
            .into_iter()
            .map(|(name, kind)| DirEntry {
                name: OsString::from(name),
                kind,
            })
            .collect())
    }

    fn real_path(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        let real = self.unlinked(&text_of(path));
        if self.unresolvable.contains(&real) {
            return Err(FsError::new(
                FsOp::RealPath,
                path,
                io::Error::from(io::ErrorKind::PermissionDenied),
            ));
        }
        Ok(self.exists(&real).then(|| PathBuf::from(real)))
    }

    fn stat(&self, path: &Path) -> Result<Option<FileMeta>, FsError> {
        let real = self.unlinked(&text_of(path));
        let len = match self.files.borrow().get(&real) {
            Some(text) => text.as_ref().map_or(0, String::len),
            None if self.is_dir(&real) => 0,
            None => return Ok(None),
        };
        Ok(Some(FileMeta {
            len: u64::try_from(len).unwrap_or(u64::MAX),
        }))
    }

    fn write_atomic(&self, path: &Path, text: &str) -> Result<(), FsError> {
        let path = self.unlinked(&text_of(path));
        self.check_writable(FsOp::Write, &path)?;
        self.files.borrow_mut().insert(path, Some(text.to_owned()));
        Ok(())
    }

    fn append_line(&self, path: &Path, line: &str) -> Result<(), FsError> {
        let path = self.unlinked(&text_of(path));
        self.check_writable(FsOp::Append, &path)?;
        let mut files = self.files.borrow_mut();
        let Some(text) = files
            .entry(path.clone())
            .or_insert_with(|| Some(String::new()))
        else {
            return Err(failure(
                FsOp::Append,
                &path,
                io::ErrorKind::PermissionDenied,
            ));
        };
        text.push_str(line);
        text.push('\n');
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        let path = self.unlinked(&text_of(path));
        self.check_writable(FsOp::CreateDir, &path)?;
        if self.files.borrow().contains_key(&path) {
            return Err(failure(
                FsOp::CreateDir,
                &path,
                io::ErrorKind::AlreadyExists,
            ));
        }
        self.dirs.borrow_mut().insert(path);
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let (from, to) = (self.unlinked(&text_of(from)), self.unlinked(&text_of(to)));
        self.check_writable(FsOp::Rename, &from)?;
        self.check_writable(FsOp::Rename, &to)?;
        let mut files = self.files.borrow_mut();
        let Some(text) = files.remove(&from) else {
            return Err(failure(FsOp::Rename, &from, io::ErrorKind::NotFound));
        };
        files.insert(to, text);
        Ok(())
    }
}

/// What a [`FakeProcess`] answers to one request.
#[derive(Debug)]
pub(crate) enum Reply {
    /// The child ends with `status`, having written `stdout` and nothing on stderr.
    Exit { status: ExitKind, stdout: String },
    /// The program cannot be started.
    SpawnFails,
    /// The child writes past the cap of a stream.
    Overflows(Stream),
}

/// Child processes that answer from a table: each entry matches a program, its arguments and
/// its directory. Anything else fails to start, as on a machine without the program.
#[derive(Debug, Default)]
pub(crate) struct FakeProcess {
    replies: Vec<(String, Vec<OsString>, PathBuf, Reply)>,
}

impl FakeProcess {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `command` (the program, then its arguments) run in `cwd` answers `reply`.
    pub(crate) fn answering(mut self, command: &[&str], cwd: &str, reply: Reply) -> Self {
        let (program, args) = command.split_first().unwrap_or((&"", &[]));
        let args = args.iter().map(OsString::from).collect();
        self.replies
            .push(((*program).to_owned(), args, PathBuf::from(cwd), reply));
        self
    }
}

impl ProcessRunner for FakeProcess {
    fn run(&self, request: &ProcessRequest<'_>) -> Result<ProcessOutput, ProcessError> {
        let program = request.program.to_owned();
        let reply = self.replies.iter().find_map(|(name, args, cwd, reply)| {
            (name == request.program && args == request.args && cwd == request.cwd).then_some(reply)
        });
        match reply {
            Some(Reply::Exit { status, stdout }) => Ok(ProcessOutput {
                status: *status,
                stdout: stdout.clone().into_bytes(),
                stderr: Vec::new(),
            }),
            Some(Reply::Overflows(stream)) => Err(ProcessError::Overflow {
                program,
                stream: *stream,
                cap: match stream {
                    Stream::Stdout => request.stdout_cap.get(),
                    Stream::Stderr => request.stderr_cap.get(),
                },
            }),
            Some(Reply::SpawnFails) | None => Err(ProcessError::Spawn {
                program,
                source: io::Error::from(io::ErrorKind::NotFound),
            }),
        }
    }
}

/// A clock stopped at one instant.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FakeClock(pub(crate) Timestamp);

/// 2024-10-27T03:33:20.123Z.
pub(crate) const NOW: Timestamp = Timestamp::from_unix_millis(1_730_000_000_123);

impl Default for FakeClock {
    fn default() -> Self {
        Self(NOW)
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

/// The ports of one test.
#[derive(Debug)]
pub(crate) struct FakePorts {
    pub(crate) files: FakeFiles,
    pub(crate) process: FakeProcess,
    pub(crate) clock: FakeClock,
}

impl FakePorts {
    /// `files`, on a machine where no program can be started (git is missing), at [`NOW`].
    pub(crate) fn new(files: FakeFiles) -> Self {
        Self {
            files,
            process: FakeProcess::new(),
            clock: FakeClock::default(),
        }
    }
}

impl Ports for FakePorts {
    type Files = FakeFiles;
    type Process = FakeProcess;
    type Clock = FakeClock;

    fn files(&self) -> &FakeFiles {
        &self.files
    }

    fn process(&self) -> &FakeProcess {
        &self.process
    }

    fn clock(&self) -> &FakeClock {
        &self.clock
    }
}

impl RunEnv {
    /// A run in `cwd` with `style`'s rules, no plugin data dir, no cloud session, and the
    /// system temporary directory `/tmp` (or `C:\Temp`).
    pub(crate) fn at(cwd: &str, style: PathStyle) -> Self {
        Self {
            cwd: PathBuf::from(cwd),
            style,
            plugin_data: None,
            temp_dir: PathBuf::from(match style {
                PathStyle::Posix => "/tmp",
                PathStyle::Windows => r"C:\Temp",
            }),
            remote_session: None,
        }
    }
}
