//! The file system on the standard library, with Node's semantics where 1.x relies on them.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use temper_app::{DirEntry, EntryKind, Files, FsError, FsOp};

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
