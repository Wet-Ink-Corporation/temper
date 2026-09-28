//! Append-only JSON Lines event store for the ledger.
//!
//! [`JsonlStore`] implements the application's
//! [`EventStore`](ledger_app::EventStore) port with one file per account under
//! a root directory, one versioned JSON record per line. Records are read into
//! wire types first and validated into domain events; older schema versions
//! are upcast on read and only the current version is written.
//!
//! # Examples
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use ledger_app::{EventStore, Recorded, Timestamp, Version};
//! use ledger_domain::{AccountId, Event};
//! use ledger_store_jsonl::JsonlStore;
//!
//! let dir = tempfile::tempdir()?;
//! let mut store = JsonlStore::new(dir.path());
//! let account: AccountId = "acc-1".parse()?;
//! let history = [Recorded {
//!     event: Event::Opened { owner: "Ada Lovelace".parse()? },
//!     recorded_at: Timestamp::from_unix_millis(1_767_225_600_000),
//! }];
//!
//! let version = store.append(&account, Version::new(0), &history)?;
//!
//! assert_eq!(version, Version::new(1));
//! assert_eq!(store.load(&account)?, history);
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! This crate has no Cargo features.

mod record;
mod store;

pub use record::RecordError;
pub use store::{JsonlStore, StoreError};
