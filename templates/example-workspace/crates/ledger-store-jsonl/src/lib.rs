//! Append-only JSON Lines event store for the ledger.
//!
//! [`JsonlStore`] implements the application's
//! [`EventStore`](ledger_app::EventStore) port with one file per account under
//! a root directory, one versioned JSON record per line. Records are read into
//! wire types first and validated into domain events (OUT-05); older schema
//! versions are upcast on read and only the current version is written.

mod record;
mod store;

pub use record::RecordError;
pub use store::{JsonlStore, StoreError};
