//! The ports: what the use cases need from the outside world, as traits that `temper-host`
//! implements and the tests fake (ARC-03): files and processes so far, and from Phase 3 the
//! clock.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The file system, read-only, with 1.x's (Node's) semantics.
pub trait Files {
    /// The file's text, decoded as lossy UTF-8 with a leading BOM kept, as Node's
    /// `readFileSync(path, 'utf8')`; `None` when there is no such file.
    ///
    /// # Errors
    ///
    /// [`FsError`] when the file exists but cannot be read, or is longer than a JS string.
    fn read_text(&self, path: &Path) -> Result<Option<String>, FsError>;

    /// The entries of a directory, in the order Node's `readdirSync` gives them: sorted by name
    /// bytes on Unix, and in the file system's own order on Windows.
    ///
    /// # Errors
    ///
    /// [`FsError`] when the directory cannot be listed.
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FsError>;

    /// The path with every symlink resolved and, on Windows, its long and verbatim-free form,
    /// as Node's `realpathSync.native`; `None` when nothing is there.
    ///
    /// # Errors
    ///
    /// [`FsError`] when the path cannot be resolved for another reason.
    fn real_path(&self, path: &Path) -> Result<Option<PathBuf>, FsError>;
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The entry's name, without its directory.
    pub name: OsString,
    /// What the entry is, without following a symlink.
    pub kind: EntryKind,
}

/// What a directory entry is, read from the entry itself, never following a symlink (Node's
/// `Dirent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A directory.
    Dir,
    /// A regular file.
    File,
    /// A symlink, or on Windows any reparse point, whatever it points to.
    Symlink,
    /// Anything else: a FIFO, a socket, a device.
    Other,
}

/// A file-system operation failed.
#[derive(Debug, thiserror::Error)]
#[error("cannot {op} {}", path.display())]
pub struct FsError {
    op: FsOp,
    path: PathBuf,
    #[source]
    source: io::Error,
}

impl FsError {
    /// The failure of `op` on `path`, caused by `source`.
    pub fn new(op: FsOp, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self {
            op,
            path: path.into(),
            source,
        }
    }

    /// The operation that failed.
    #[must_use]
    pub fn op(&self) -> FsOp {
        self.op
    }

    /// The path it failed on.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A [`Files`] operation, as an [`FsError`] names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsOp {
    /// [`Files::read_text`].
    Read,
    /// [`Files::read_dir`].
    ReadDir,
    /// [`Files::real_path`].
    RealPath,
}

impl fmt::Display for FsOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Read => "read",
            Self::ReadDir => "read directory",
            Self::RealPath => "resolve",
        })
    }
}

/// Runs child processes to completion, as 1.x's `spawnSync` does.
///
/// Stdin is null, both outputs are captured up to a cap, and the child is killed and reaped
/// when its timeout expires.
pub trait ProcessRunner {
    /// Runs `request` and returns how the child ended and what it wrote. A timeout or a
    /// failing exit code is an outcome ([`ExitKind`]), not an error.
    ///
    /// # Errors
    ///
    /// [`ProcessError::Spawn`] when the program cannot be started, [`ProcessError::Io`] when
    /// the child cannot be waited for or its output read, and [`ProcessError::Overflow`] when
    /// it writes more than a stream's cap.
    fn run(&self, request: &ProcessRequest<'_>) -> Result<ProcessOutput, ProcessError>;
}

/// One child process to run. Phase 4 replaces `stdout_cap` with a sink that can stream to a
/// file, and adds the environment (OC-8 B).
#[derive(Debug)]
pub struct ProcessRequest<'a> {
    /// The program, found on `PATH` as the OS finds it.
    pub program: &'a str,
    /// Its arguments.
    pub args: &'a [OsString],
    /// The directory it runs in.
    pub cwd: &'a Path,
    /// The most stdout may hold.
    pub stdout_cap: ByteCap,
    /// The most stderr may hold.
    pub stderr_cap: ByteCap,
    /// How long the child may run before it is killed.
    pub timeout: Duration,
}

/// The most bytes a captured stream may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteCap(usize);

impl ByteCap {
    /// A cap of `bytes` bytes.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    /// The cap in bytes.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// How a child process ended, and what it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    /// How it ended.
    pub status: ExitKind,
    /// Its stdout, at most the request's cap.
    pub stdout: Vec<u8>,
    /// Its stderr, at most the request's cap.
    pub stderr: Vec<u8>,
}

/// How a child process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    /// It exited with this code.
    Code(i32),
    /// A signal ended it (Unix).
    Signal,
    /// It ran past its timeout and was killed.
    TimedOut,
}

/// A child process's output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

impl fmt::Display for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        })
    }
}

/// A child process could not be run to completion.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// The program could not be started: it is missing, not executable, or the directory is.
    #[error("cannot start `{program}`")]
    Spawn {
        /// The program.
        program: String,
        /// Why.
        #[source]
        source: io::Error,
    },
    /// The child started, but could not be waited for or its output read.
    #[error("lost track of `{program}`")]
    Io {
        /// The program.
        program: String,
        /// Why.
        #[source]
        source: io::Error,
    },
    /// The child wrote more than a stream's cap, as 1.x's `maxBuffer` reports `ENOBUFS`
    /// (OC-3 A).
    #[error("`{program}` wrote more than {cap} bytes to {stream}")]
    Overflow {
        /// The program.
        program: String,
        /// The stream that overflowed.
        stream: Stream,
        /// The cap it passed, in bytes.
        cap: usize,
    },
}

/// Every port a use case can reach, as the composition root wires them.
pub trait Ports {
    /// The file system.
    type Files: Files;
    /// Child processes.
    type Process: ProcessRunner;

    /// The file-system adapter.
    fn files(&self) -> &Self::Files;

    /// The process adapter.
    fn process(&self) -> &Self::Process;
}
