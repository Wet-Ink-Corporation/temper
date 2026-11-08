//! The file system on the standard library, with Node's semantics where 1.x relies on them.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use temper_app::{DirEntry, EntryKind, FileMeta, Files, FsError, FsOp, ReadInto};
use temper_domain::ModTime;

/// The longest JS string, in UTF-16 code units: V8's `String::kMaxLength` on 64-bit targets,
/// Node 22's `buffer.constants.MAX_STRING_LENGTH`. `readFileSync(path, 'utf8')` throws past it.
const V8_MAX_STRING_UNITS: usize = (1 << 29) - 24;

/// [`Files`] on `std::fs`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdFiles;

impl Files for StdFiles {
    fn read_text(&self, path: &Path) -> Result<Option<String>, FsError> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(FsError::new(FsOp::Read, path, error)),
        };
        read_capped_text(file, V8_MAX_STRING_UNITS)
            .map(Some)
            .map_err(|error| FsError::new(FsOp::Read, path, error))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>, FsError> {
        let failed = |error| FsError::new(FsOp::ReadDir, path, error);
        let mut entries = fs::read_dir(path)
            .map_err(failed)?
            .map(|entry| {
                let entry = entry?;
                Ok(DirEntry {
                    kind: entry_kind(&entry)?,
                    name: entry.file_name(),
                })
            })
            .collect::<io::Result<Vec<_>>>()
            .map_err(failed)?;
        in_listing_order(&mut entries);
        Ok(entries)
    }

    fn real_path(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        match fs::canonicalize(path) {
            Ok(real) => Ok(Some(without_verbatim(real))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(FsError::new(FsOp::RealPath, path, error)),
        }
    }

    fn stat(&self, path: &Path) -> Result<Option<FileMeta>, FsError> {
        match fs::metadata(path) {
            Ok(meta) => Ok(Some(FileMeta {
                // `metadata` follows symlinks, as `statSync` does.
                kind: if meta.is_dir() {
                    EntryKind::Dir
                } else if meta.is_file() {
                    EntryKind::File
                } else {
                    EntryKind::Other
                },
                len: meta.len(),
                modified: mod_time(&meta),
            })),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(FsError::new(FsOp::Stat, path, error)),
        }
    }

    /// 1.x `writeAtomic`. A write or rename that fails leaves the temporary file, as 1.x does.
    fn write_atomic(&self, path: &Path, text: &str) -> Result<(), FsError> {
        if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            self.create_dir_all(parent)?;
        }
        let tmp = temp_beside(path);
        fs::write(&tmp, text).map_err(|error| FsError::new(FsOp::Write, &tmp, error))?;
        fs::rename(&tmp, path).map_err(|error| FsError::new(FsOp::Write, path, error))
    }

    /// One `write_all` on a handle opened for appending, so concurrent hooks interleave whole
    /// lines as `appendFileSync` does.
    fn append_line(&self, path: &Path, line: &str) -> Result<(), FsError> {
        let failed = |error| FsError::new(FsOp::Append, path, error);
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .map_err(failed)?;
        let mut bytes = Vec::with_capacity(line.len().saturating_add(1));
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        file.write_all(&bytes).map_err(failed)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        fs::create_dir_all(path).map_err(|error| FsError::new(FsOp::CreateDir, path, error))
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        fs::rename(from, to).map_err(|error| FsError::new(FsOp::Rename, from, error))
    }

    /// The metadata is read before the open, so a FIFO is never opened (its `open` would wait
    /// for a writer), and again from the open handle, in case the path changed in between.
    /// A FIFO swapped in during that window can still block the open; nothing else can.
    fn read_into(
        &self,
        path: &Path,
        limit: u64,
        sink: &mut dyn Write,
    ) -> Result<ReadInto, FsError> {
        let failed = |error| FsError::new(FsOp::ReadInto, path, error);
        let regular = |meta: &fs::Metadata| {
            if !meta.is_file() {
                Some(ReadInto::NotAFile)
            } else if meta.len() > limit {
                Some(ReadInto::TooLarge)
            } else {
                None
            }
        };
        match fs::metadata(path) {
            Ok(meta) => {
                if let Some(answer) = regular(&meta) {
                    return Ok(answer);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ReadInto::NotFound);
            }
            Err(error) => return Err(failed(error)),
        }
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ReadInto::NotFound);
            }
            Err(error) => return Err(failed(error)),
        };
        if let Some(answer) = regular(&file.metadata().map_err(failed)?) {
            return Ok(answer);
        }
        // One byte past the limit tells a file that grew since its metadata was read.
        let copied = io::copy(&mut file.take(limit.saturating_add(1)), sink).map_err(failed)?;
        Ok(if copied > limit {
            ReadInto::TooLarge
        } else {
            ReadInto::Read
        })
    }

    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(FsError::new(FsOp::Remove, path, error)),
        }
    }

    /// The entry itself decides, never what a link points to: `remove_dir_all` for a real
    /// directory, else the entry alone (on Windows a directory link is removed as a directory).
    fn remove_all(&self, path: &Path) -> Result<(), FsError> {
        let failed = |error| FsError::new(FsOp::Remove, path, error);
        let kind = match fs::symlink_metadata(path) {
            Ok(meta) => meta.file_type(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(failed(error)),
        };
        let removed = if kind.is_dir() {
            fs::remove_dir_all(path)
        } else {
            match fs::remove_file(path) {
                Err(error) if cfg!(windows) && kind.is_symlink() => {
                    fs::remove_dir(path).map_err(|_dir| error)
                }
                other => other,
            }
        };
        match removed {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(failed(error)),
        }
    }

    /// `create_new` fails when anything is at the path, a directory or a dangling link
    /// included, as `O_CREAT | O_EXCL` does; the file is closed when the handle drops.
    fn create_new(&self, path: &Path) -> Result<(), FsError> {
        fs::File::options()
            .write(true)
            .create_new(true)
            .open(path)
            .map(drop)
            .map_err(|error| FsError::new(FsOp::Create, path, error))
    }
}

/// The entry's modification time as Node's `mtimeMs`, or `None` where the platform keeps none.
fn mod_time(meta: &fs::Metadata) -> Option<ModTime> {
    let time = match meta.modified() {
        Ok(time) => time,
        // No modification time on this platform: nothing to compare.
        Err(_unsupported) => return None,
    };
    let nanos = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i128::try_from(after.as_nanos()).ok()?,
        Err(before) => i128::try_from(before.duration().as_nanos())
            .ok()?
            .checked_neg()?,
    };
    Some(ModTime::from_unix_nanos(nanos))
}

/// `<path>.<pid>.<nanos>.tmp`: unique per process and write, as 1.x's `<p>.<pid>.<ms>.tmp`.
fn temp_beside(path: &Path) -> PathBuf {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_nanos(),
        // A clock before 1970 still gives a name; the pid keeps it apart from other hooks.
        Err(before) => before.duration().as_nanos(),
    };
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}.{nanos}.tmp", std::process::id()));
    PathBuf::from(name)
}

/// Reads `reader` as lossy UTF-8, as Node's `readFileSync(path, 'utf8')`, failing with
/// [`io::ErrorKind::FileTooLarge`] where Node would: past `max_units` UTF-16 code units. A
/// unit takes at most 3 bytes, a replacement character included, so at most `3 * max_units + 1`
/// bytes are read.
fn read_capped_text(reader: impl Read, max_units: usize) -> io::Result<String> {
    let max_bytes = max_units.saturating_mul(3);
    let mut bytes = Vec::new();
    reader
        .take(u64::try_from(max_bytes.saturating_add(1)).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(invalid) => String::from_utf8_lossy(invalid.as_bytes()).into_owned(),
    };
    // A text has at least as many bytes as units, so only a long one needs counting.
    if text.len() > max_units && text.encode_utf16().count() > max_units {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    Ok(text)
}

/// What an entry is, without following a link (Node's `Dirent`).
fn entry_kind(entry: &fs::DirEntry) -> io::Result<EntryKind> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        /// `FILE_ATTRIBUTE_REPARSE_POINT`.
        const REPARSE_POINT: u32 = 0x400;
        // libuv reports every reparse point as a link (`UV_DIRENT_LINK`), not only symlinks
        // and junctions. `DirEntry::metadata` comes from the listing here, without a call.
        if entry.metadata()?.file_attributes() & REPARSE_POINT != 0 {
            return Ok(EntryKind::Symlink);
        }
    }
    let kind = entry.file_type()?;
    Ok(if kind.is_dir() {
        EntryKind::Dir
    } else if kind.is_file() {
        EntryKind::File
    } else if kind.is_symlink() {
        EntryKind::Symlink
    } else {
        EntryKind::Other
    })
}

/// Node's order on Unix: libuv's `scandir` sorts names with `strcmp`.
#[cfg(unix)]
fn in_listing_order(entries: &mut [DirEntry]) {
    entries.sort_by(|a, b| a.name.as_encoded_bytes().cmp(b.name.as_encoded_bytes()));
}

/// Node's order elsewhere: libuv keeps the file system's order.
#[cfg(not(unix))]
fn in_listing_order(_entries: &mut [DirEntry]) {}

/// Node's `realpathSync.native` strips the `\\?\` prefix that `canonicalize` adds on Windows.
fn without_verbatim(real: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        use temper_domain::{PathStyle, unverbatim};
        if let Some(text) = real.to_str() {
            return PathBuf::from(unverbatim(text, PathStyle::Windows).as_ref());
        }
    }
    real
}

#[cfg(test)]
mod tests;
