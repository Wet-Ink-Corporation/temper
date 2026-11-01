//! In-memory fakes of the ports, for this crate's tests (TST-09). A `test-util` feature will
//! expose them when another crate's tests need them.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use temper_domain::PathStyle;

use crate::ports::{
    DirEntry, EntryKind, ExitKind, Files, FsError, FsOp, Ports, ProcessError, ProcessOutput,
    ProcessRequest, ProcessRunner, Stream,
};

/// A file tree held as text paths in one [`PathStyle`]. A directory exists when a file is below
/// it; listings are derived in name-byte order unless one is set.
#[derive(Debug)]
pub(crate) struct FakeFiles {
    style: PathStyle,
    /// Each file's text, or `None` for one that exists but cannot be read.
    files: BTreeMap<String, Option<String>>,
    /// Listings that replace the derived ones; `None` for a directory that cannot be listed.
    listings: BTreeMap<String, Option<Vec<DirEntry>>>,
    /// Prefixes that resolve to another path, as a symlinked ancestor or an 8.3 short name does.
    links: Vec<(String, String)>,
    /// Paths whose real path fails with an error other than "not found".
    unresolvable: Vec<String>,
}

impl FakeFiles {
    pub(crate) fn new(style: PathStyle) -> Self {
        Self {
            style,
            files: BTreeMap::new(),
            listings: BTreeMap::new(),
            links: Vec::new(),
            unresolvable: Vec::new(),
        }
    }

    pub(crate) fn with_file(mut self, path: &str, text: &str) -> Self {
        self.files.insert(path.to_owned(), Some(text.to_owned()));
        self
    }

    pub(crate) fn with_unreadable(mut self, path: &str) -> Self {
        self.files.insert(path.to_owned(), None);
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

    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
            || self.listings.contains_key(path)
            || self
                .files
                .keys()
                .any(|file| self.below(file, path).is_some())
    }
}

fn text_of(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Each read hands out a copy, as reading a real file makes one; the fake keeps its tree.
impl Files for FakeFiles {
    fn read_text(&self, path: &Path) -> Result<Option<String>, FsError> {
        match self.files.get(&self.unlinked(&text_of(path))) {
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
        for file in self.files.keys() {
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

/// The ports of one test.
#[derive(Debug)]
pub(crate) struct FakePorts {
    pub(crate) files: FakeFiles,
    pub(crate) process: FakeProcess,
}

impl FakePorts {
    /// `files`, on a machine where no program can be started: git is missing.
    pub(crate) fn new(files: FakeFiles) -> Self {
        Self {
            files,
            process: FakeProcess::new(),
        }
    }
}

impl Ports for FakePorts {
    type Files = FakeFiles;
    type Process = FakeProcess;

    fn files(&self) -> &FakeFiles {
        &self.files
    }

    fn process(&self) -> &FakeProcess {
        &self.process
    }
}
