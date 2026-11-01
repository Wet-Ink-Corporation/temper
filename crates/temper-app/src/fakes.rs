//! In-memory fakes of the ports, for this crate's tests (TST-09). A `test-util` feature will
//! expose them when another crate's tests need them.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use temper_domain::PathStyle;

use crate::ports::{DirEntry, EntryKind, Files, FsError, FsOp, Ports};

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

/// The ports of one test.
#[derive(Debug)]
pub(crate) struct FakePorts {
    pub(crate) files: FakeFiles,
}

impl Ports for FakePorts {
    type Files = FakeFiles;

    fn files(&self) -> &FakeFiles {
        &self.files
    }
}
