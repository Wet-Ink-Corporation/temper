//! Event persistence as a port.

use std::error::Error;
use std::fmt;

use ledger_domain::{AccountId, Event};

use crate::clock::Timestamp;

/// How many events an account's stream holds. Appends name the version they
/// expect, so a writer working from stale history is refused instead of
/// silently interleaving its events (optimistic concurrency).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(u64);

impl Version {
    /// The version of a stream holding `events` events.
    #[must_use]
    pub const fn new(events: u64) -> Self {
        Self(events)
    }

    /// The version of a stream whose history has `len` events.
    #[must_use]
    pub fn from_len(len: usize) -> Self {
        // `usize` is at most 64 bits on every supported target, so this never
        // saturates; saturating keeps the conversion total without a panic.
        Self(u64::try_from(len).unwrap_or(u64::MAX))
    }

    /// The number of events in the stream.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An event as stored: the domain fact plus when it was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    /// What happened.
    pub event: Event,
    /// When the ledger recorded it.
    pub recorded_at: Timestamp,
}

/// Port: an append-only store of per-account event streams.
///
/// Owned by this crate and implemented by adapters. Implementations must
/// return events in the order they were appended.
pub trait EventStore {
    /// The adapter's failure type. The application never inspects it; it only
    /// passes it to the caller as [`AppError::Store`](crate::AppError::Store).
    type Error: Error + Send + Sync + 'static;

    /// Load an account's history, oldest first. An unknown account has an
    /// empty history.
    ///
    /// # Errors
    ///
    /// Returns the adapter's error when the stream cannot be read or holds a
    /// record that is not a valid event.
    fn load(&self, account: &AccountId) -> Result<Vec<Recorded>, Self::Error>;

    /// Append `events` to an account's stream, provided the stream is still at
    /// `expected`. Returns the stream's new version.
    ///
    /// # Errors
    ///
    /// Returns the adapter's error when another writer has appended since
    /// `expected` (a conflict), or when the events cannot be stored.
    fn append(
        &mut self,
        account: &AccountId,
        expected: Version,
        events: &[Recorded],
    ) -> Result<Version, Self::Error>;
}

// Compile-time proof that these types can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Version>();
    assert_send_sync::<Recorded>();
};
