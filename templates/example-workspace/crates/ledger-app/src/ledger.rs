//! The account use cases.

use ledger_domain::{AccountId, Command, Rejection, State, decide, evolve, fold};
use thiserror::Error;

use crate::clock::{Clock, Timestamp};
use crate::event_store::{EventStore, Recorded, Version};

/// Why [`Ledger::execute`] did not complete.
///
/// Deliberately exhaustive: the split between "the business said no" and
/// "the store could not answer" is the contract callers map to outcomes, so a
/// third kind of failure must break their `match` and get reviewed.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum AppError<E> {
    /// The domain refused the command. Nothing was stored; retrying the same
    /// command against the same history changes nothing.
    #[error(transparent)]
    Rejected(#[from] Rejection),
    /// The event store failed or reported a concurrent write. The command may
    /// succeed if retried.
    #[error("event store failed")]
    Store(#[source] E),
}

/// An account as of its latest event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    /// The account's state.
    pub state: State,
    /// How many events the account's stream holds.
    pub version: Version,
    /// When the latest event was recorded, if there is one.
    pub last_activity: Option<Timestamp>,
}

/// Account use cases over an event store and a clock.
///
/// Generic over both ports, so the composition root picks the adapters and
/// every call is statically dispatched.
#[derive(Debug)]
pub struct Ledger<S, C> {
    store: S,
    clock: C,
}

impl<S: EventStore, C: Clock> Ledger<S, C> {
    /// Build the use cases around a store and a clock.
    #[must_use]
    pub fn new(store: S, clock: C) -> Self {
        Self { store, clock }
    }

    /// Run `command` against `account`: load its history, decide, and append
    /// the resulting events stamped with the current time.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Rejected`] when the domain refuses the command, and
    /// [`AppError::Store`] when the history cannot be loaded, the events cannot
    /// be appended, or another writer appended first.
    pub fn execute(
        &mut self,
        account: &AccountId,
        command: &Command,
    ) -> Result<AccountSummary, AppError<S::Error>> {
        let history = self.store.load(account).map_err(AppError::Store)?;
        let state = fold(history.iter().map(|recorded| &recorded.event));
        let events = decide(command, &state)?;
        let now = self.clock.now();
        let recorded: Vec<Recorded> = events
            .into_iter()
            .map(|event| Recorded {
                event,
                recorded_at: now,
            })
            .collect();
        let version = self
            .store
            .append(account, Version::from_len(history.len()), &recorded)
            .map_err(AppError::Store)?;
        Ok(AccountSummary {
            state: recorded
                .iter()
                .fold(state, |state, recorded| evolve(state, &recorded.event)),
            version,
            last_activity: Some(now),
        })
    }

    /// Summarize `account` from its history. An unknown account is
    /// [`State::NotOpened`] at version 0.
    ///
    /// # Errors
    ///
    /// Returns the store's error when the history cannot be loaded.
    pub fn summary(&self, account: &AccountId) -> Result<AccountSummary, S::Error> {
        let history = self.store.load(account)?;
        Ok(AccountSummary {
            state: fold(history.iter().map(|recorded| &recorded.event)),
            version: Version::from_len(history.len()),
            last_activity: history.last().map(|recorded| recorded.recorded_at),
        })
    }
}

// Compile-time proof that these types can cross threads (ASY-11). The generic
// types are checked with unit parameters: they add no field of their own that
// is not `Send + Sync`.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AccountSummary>();
    assert_send_sync::<AppError<()>>();
    assert_send_sync::<Ledger<(), ()>>();
};
