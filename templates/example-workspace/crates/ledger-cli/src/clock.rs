//! The operating-system clock, adapting `std::time` to the `Clock` port.

use std::time::{SystemTime, UNIX_EPOCH};

use ledger_app::{Clock, Timestamp};

/// Reads the system's wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
            // Saturates only past the year 584 million.
            Ok(since_epoch) => u64::try_from(since_epoch.as_millis()).unwrap_or(u64::MAX),
            Err(error) => {
                tracing::warn!(
                    name: "clock.read.before_epoch",
                    %error,
                    "system clock is set before 1970; recording the epoch"
                );
                0
            }
        };
        Timestamp::from_unix_millis(millis)
    }
}
