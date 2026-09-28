//! Hand-written fakes of the application ports, and test data builders.

use std::collections::BTreeMap;
use std::error::Error;

use ledger_app::{Clock, EventStore, Ledger, Recorded, Timestamp, Version};
use ledger_domain::{
    AccountId, Cents, Command, OwnerName, ParseAccountIdError, ParseOwnerNameError, State,
};
use thiserror::Error;

/// Outcome of a test; `?` works on every error type.
pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// The instant every fake clock reports.
pub(crate) const NOW: Timestamp = Timestamp::from_unix_millis(1_767_225_600_000);

/// A clock frozen at one instant.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FixedClock(pub(crate) Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

/// Failures the fakes can report.
#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum FakeStoreError {
    #[error("expected version {expected}, found {actual}")]
    Conflict { expected: Version, actual: Version },
    #[error("store unavailable")]
    Unavailable,
}

/// In-memory event store with the same contract as the real adapter.
#[derive(Debug, Default)]
pub(crate) struct InMemoryStore {
    streams: BTreeMap<AccountId, Vec<Recorded>>,
}

impl EventStore for InMemoryStore {
    type Error = FakeStoreError;

    fn load(&self, account: &AccountId) -> Result<Vec<Recorded>, FakeStoreError> {
        Ok(self.streams.get(account).cloned().unwrap_or_default())
    }

    fn append(
        &mut self,
        account: &AccountId,
        expected: Version,
        events: &[Recorded],
    ) -> Result<Version, FakeStoreError> {
        let stream = self.streams.entry(account.clone()).or_default();
        let actual = Version::from_len(stream.len());
        if actual != expected {
            return Err(FakeStoreError::Conflict { expected, actual });
        }
        stream.extend_from_slice(events);
        Ok(Version::from_len(stream.len()))
    }
}

/// A store whose backing system is down.
#[derive(Debug)]
pub(crate) struct UnavailableStore;

impl EventStore for UnavailableStore {
    type Error = FakeStoreError;

    fn load(&self, _account: &AccountId) -> Result<Vec<Recorded>, FakeStoreError> {
        Err(FakeStoreError::Unavailable)
    }

    fn append(
        &mut self,
        _account: &AccountId,
        _expected: Version,
        _events: &[Recorded],
    ) -> Result<Version, FakeStoreError> {
        Err(FakeStoreError::Unavailable)
    }
}

/// A store that always loses the race: another writer appends one event
/// between every load and append.
#[derive(Debug)]
pub(crate) struct ContendedStore;

impl EventStore for ContendedStore {
    type Error = FakeStoreError;

    fn load(&self, _account: &AccountId) -> Result<Vec<Recorded>, FakeStoreError> {
        Ok(Vec::new())
    }

    fn append(
        &mut self,
        _account: &AccountId,
        expected: Version,
        _events: &[Recorded],
    ) -> Result<Version, FakeStoreError> {
        Err(FakeStoreError::Conflict {
            expected,
            actual: Version::new(expected.get() + 1),
        })
    }
}

/// A ledger over an empty in-memory store and the fixed clock.
pub(crate) fn ledger() -> Ledger<InMemoryStore, FixedClock> {
    Ledger::new(InMemoryStore::default(), FixedClock(NOW))
}

pub(crate) fn account() -> Result<AccountId, ParseAccountIdError> {
    "acc-1".parse()
}

pub(crate) fn ada() -> Result<OwnerName, ParseOwnerNameError> {
    "Ada Lovelace".parse()
}

pub(crate) fn open() -> Result<Command, ParseOwnerNameError> {
    Ok(Command::Open { owner: ada()? })
}

/// Ada's account, open and holding `cents`.
pub(crate) fn open_with(cents: u64) -> Result<State, ParseOwnerNameError> {
    Ok(State::Open {
        owner: ada()?,
        balance: Cents::new(cents),
    })
}

pub(crate) fn deposit(cents: u64) -> Command {
    Command::Deposit {
        amount: Cents::new(cents),
    }
}

pub(crate) fn withdraw(cents: u64) -> Command {
    Command::Withdraw {
        amount: Cents::new(cents),
    }
}
