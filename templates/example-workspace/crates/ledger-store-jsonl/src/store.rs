//! One append-only JSON Lines file per account.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use ledger_app::{EventStore, Recorded, Version};
use ledger_domain::AccountId;
use thiserror::Error;

use crate::record::{self, RecordError};

/// Longest record accepted on read, in bytes, newline excluded. Caps the
/// memory one damaged or hostile line can claim. The longest record this
/// store writes, an `opened` event with an 80-character name, is well under
/// 1 KiB. Lowering the limit below a record already written makes that
/// stream unreadable.
const MAX_RECORD_BYTES: u16 = u16::MAX;

/// File-backed [`EventStore`]: `<root>/account-<account-id>.jsonl`.
///
/// Appends take an exclusive advisory lock on the account's file and re-read
/// it to check the expected version, so a writer working from stale history,
/// in this process or another, is refused instead of interleaved. Reads take
/// a shared lock.
///
/// Every record ends with a newline, written in the same batch and synced
/// before the append returns. Bytes after the last newline are an append that
/// a crash interrupted and that was never acknowledged: reads skip them and
/// the next append removes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlStore {
    root: PathBuf,
}

/// Why the store could not load or append a stream.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StoreError {
    /// Opening, locking or reading a stream file failed.
    #[error("failed to read {}", path.display())]
    Read {
        /// The stream file.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: io::Error,
    },
    /// Creating, locking, writing or syncing a stream file failed. Nothing
    /// was appended.
    #[error("failed to write {}", path.display())]
    Write {
        /// The stream file.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: io::Error,
    },
    /// A write failed and removing its partial output failed too: the stream
    /// may end with part of the batch.
    #[error("failed to write {} and to remove the partial append (write error: {write})", path.display())]
    PartialAppend {
        /// The stream file.
        path: PathBuf,
        /// Why the write failed.
        write: io::Error,
        /// Why the rollback failed.
        #[source]
        source: io::Error,
    },
    /// A stored line is not a valid event record.
    #[error("{}:{line}: invalid record", path.display())]
    Corrupt {
        /// The stream file.
        path: PathBuf,
        /// One-based line number.
        line: u64,
        /// What is wrong with the record.
        #[source]
        source: RecordError,
    },
    /// A stored line is longer than any record this store writes.
    #[error("{}:{line}: record exceeds {limit} bytes", path.display())]
    RecordTooLong {
        /// The stream file.
        path: PathBuf,
        /// One-based line number.
        line: u64,
        /// The limit, in bytes.
        limit: usize,
    },
    /// Another writer appended since the caller loaded the stream.
    #[error("account {account} is at version {actual}, not the expected {expected}")]
    Conflict {
        /// The contended account.
        account: AccountId,
        /// The version the caller loaded.
        expected: Version,
        /// The version on disk.
        actual: Version,
    },
}

// Compile-time proof that these types can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<JsonlStore>();
    assert_send_sync::<StoreError>();
};

impl JsonlStore {
    /// A store keeping its files under `root`. The directory is created on
    /// the first append.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn stream_path(&self, account: &AccountId) -> PathBuf {
        // `AccountId` admits only `a-z`, `0-9` and `-`; the prefix keeps ids
        // such as `con` or `nul` clear of names Windows reserves for devices.
        self.root.join(format!("account-{account}.jsonl"))
    }
}

impl EventStore for JsonlStore {
    type Error = StoreError;

    fn load(&self, account: &AccountId) -> Result<Vec<Recorded>, StoreError> {
        let path = self.stream_path(account);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(StoreError::Read { path, source }),
        };
        if let Err(source) = file.lock_shared() {
            return Err(StoreError::Read { path, source });
        }
        Ok(read_stream(&path, &file)?.events)
    }

    fn append(
        &mut self,
        account: &AccountId,
        expected: Version,
        events: &[Recorded],
    ) -> Result<Version, StoreError> {
        let path = self.stream_path(account);
        let file = open_locked_for_append(&self.root, &path)?;
        let stream = read_stream(&path, &file)?;
        let actual = Version::from_len(stream.events.len());
        if actual != expected {
            return Err(StoreError::Conflict {
                // Clone: the error outlives the borrowed id.
                account: account.clone(),
                expected,
                actual,
            });
        }
        if stream.torn_tail
            && let Err(source) = file.set_len(stream.committed_len)
        {
            return Err(StoreError::Write { path, source });
        }
        write_batch(&path, &file, stream.committed_len, events)?;
        Ok(Version::from_len(
            stream.events.len().saturating_add(events.len()),
        ))
    }
}

/// Create the root and the stream file as needed, then take the exclusive lock
/// that serializes check-then-append across writers.
fn open_locked_for_append(root: &Path, path: &Path) -> Result<File, StoreError> {
    let write_error = |source| StoreError::Write {
        path: path.to_owned(),
        source,
    };
    fs::create_dir_all(root).map_err(write_error)?;
    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
        .map_err(write_error)?;
    file.lock().map_err(write_error)?;
    Ok(file)
}

/// Encode the whole batch in memory, append it with one write and sync it.
/// On failure, cut the file back to `committed_len` so no reader ever sees
/// part of a batch.
fn write_batch(
    path: &Path,
    file: &File,
    committed_len: u64,
    events: &[Recorded],
) -> Result<(), StoreError> {
    let mut batch = Vec::new();
    let written = events
        .iter()
        .try_for_each(|recorded| record::encode(recorded, &mut batch))
        .and_then(|()| write_and_sync(file, &batch));
    let Err(write) = written else {
        return Ok(());
    };
    let path = path.to_owned();
    match file.set_len(committed_len) {
        Ok(()) => Err(StoreError::Write {
            path,
            source: write,
        }),
        Err(source) => Err(StoreError::PartialAppend {
            path,
            write,
            source,
        }),
    }
}

fn write_and_sync(mut file: &File, batch: &[u8]) -> io::Result<()> {
    file.write_all(batch)?;
    file.sync_data()
}

/// The records of a stream file.
struct Stream {
    events: Vec<Recorded>,
    /// Length of the file up to and including its last newline.
    committed_len: u64,
    /// Whether bytes follow the last newline: an interrupted append.
    torn_tail: bool,
}

/// Read every record of an open stream file from the start, one bounded line
/// at a time.
fn read_stream(path: &Path, file: &File) -> Result<Stream, StoreError> {
    let read_limit = u64::from(MAX_RECORD_BYTES).saturating_add(1);
    let mut reader = BufReader::new(file);
    let mut stream = Stream {
        events: Vec::new(),
        committed_len: 0,
        torn_tail: false,
    };
    let mut buffer = Vec::new();
    let mut line: u64 = 0;
    loop {
        line = line.saturating_add(1);
        buffer.clear();
        let mut bounded = reader.by_ref().take(read_limit);
        bounded
            .read_until(b'\n', &mut buffer)
            .map_err(|source| StoreError::Read {
                path: path.to_owned(),
                source,
            })?;
        let consumed = read_limit.saturating_sub(bounded.limit());
        // With a newline, the bounded read proves the record fits the limit.
        let Some(text) = buffer.strip_suffix(b"\n") else {
            if buffer.len() > usize::from(MAX_RECORD_BYTES) {
                return Err(StoreError::RecordTooLong {
                    path: path.to_owned(),
                    line,
                    limit: usize::from(MAX_RECORD_BYTES),
                });
            }
            stream.torn_tail = !buffer.is_empty();
            return Ok(stream);
        };
        let recorded = record::decode(text).map_err(|source| StoreError::Corrupt {
            path: path.to_owned(),
            line,
            source,
        })?;
        stream.events.push(recorded);
        stream.committed_len = stream.committed_len.saturating_add(consumed);
    }
}
