//! Ledger application layer: the account use cases and the ports they need.
//!
//! [`Ledger`] loads an account's history through the [`EventStore`] port,
//! folds it into state, asks the domain to decide, stamps the resulting
//! events with the [`Clock`] port and appends them under an optimistic
//! concurrency check. Adapters implement the ports; nothing here does I/O.
//!
//! # Examples
//!
//! Implement the two ports for your storage and time source (here, an
//! in-memory store and a fixed clock, hidden), then run commands through a
//! [`Ledger`]:
//!
//! ```
//! use ledger_app::{Ledger, Version};
//! use ledger_domain::{AccountId, Command};
//! # use std::collections::BTreeMap;
//! # use std::io;
//! # use ledger_app::{Clock, EventStore, Recorded, Timestamp};
//! # #[derive(Default)]
//! # struct MemoryStore(BTreeMap<AccountId, Vec<Recorded>>);
//! # impl EventStore for MemoryStore {
//! #     type Error = io::Error;
//! #     fn load(&self, account: &AccountId) -> io::Result<Vec<Recorded>> {
//! #         Ok(self.0.get(account).cloned().unwrap_or_default())
//! #     }
//! #     fn append(
//! #         &mut self,
//! #         account: &AccountId,
//! #         expected: Version,
//! #         events: &[Recorded],
//! #     ) -> io::Result<Version> {
//! #         // Clone: the map owns its keys.
//! #         let stream = self.0.entry(account.clone()).or_default();
//! #         if Version::from_len(stream.len()) != expected {
//! #             return Err(io::Error::other("stale version"));
//! #         }
//! #         stream.extend_from_slice(events);
//! #         Ok(Version::from_len(stream.len()))
//! #     }
//! # }
//! # struct FixedClock;
//! # impl Clock for FixedClock {
//! #     fn now(&self) -> Timestamp {
//! #         Timestamp::from_unix_millis(1_767_225_600_000)
//! #     }
//! # }
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//!
//! let mut ledger = Ledger::new(MemoryStore::default(), FixedClock);
//! let account: AccountId = "acc-1".parse()?;
//!
//! ledger.execute(&account, &Command::Open { owner: "Ada Lovelace".parse()? })?;
//! let summary = ledger.execute(&account, &Command::Deposit { amount: "12.50".parse()? })?;
//!
//! assert_eq!(summary.version, Version::new(2));
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! This crate has no Cargo features.

mod clock;
mod event_store;
mod ledger;

pub use clock::{Clock, Timestamp};
pub use event_store::{EventStore, Recorded, Version};
pub use ledger::{AccountSummary, AppError, Ledger};
