//! Ledger application layer: the account use cases and the ports they need.
//!
//! [`Ledger`] loads an account's history through the [`EventStore`] port,
//! folds it into state, asks the domain to decide, stamps the resulting
//! events with the [`Clock`] port and appends them under an optimistic
//! concurrency check. Adapters implement the ports; nothing here does I/O.

mod clock;
mod event_store;
mod ledger;

pub use clock::{Clock, Timestamp};
pub use event_store::{EventStore, Recorded, Version};
pub use ledger::{AccountSummary, AppError, Ledger};
