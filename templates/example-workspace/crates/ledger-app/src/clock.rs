//! Time as a port.

/// A point in time: whole milliseconds since the Unix epoch, UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

impl Timestamp {
    /// The instant `millis` milliseconds after the Unix epoch.
    #[must_use]
    pub const fn from_unix_millis(millis: u64) -> Self {
        Self(millis)
    }

    /// Milliseconds since the Unix epoch.
    #[must_use]
    pub const fn as_unix_millis(self) -> u64 {
        self.0
    }
}

/// Port: the current time. Injected so use cases stay deterministic under test.
pub trait Clock {
    /// The current time.
    fn now(&self) -> Timestamp;
}

// Compile-time proof that this type can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Timestamp>();
};
