//! The ports: what the use cases need from the outside world, as traits that `temper-host`
//! implements and the tests fake (ARC-03). Phase 1d.2b adds processes, and Phase 3 the clock.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

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

/// Every port a use case can reach, as the composition root wires them.
pub trait Ports {
    /// The file system.
    type Files: Files;

    /// The file system.
    fn files(&self) -> &Self::Files;
}
