//! Shared fixtures: a store in a fresh temporary directory, and sample events.

use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;

use ledger_app::{Recorded, Timestamp};
use ledger_domain::{AccountId, Cents, Event, ParseAccountIdError, ParseOwnerNameError};
use ledger_store_jsonl::JsonlStore;
use tempfile::TempDir;

/// Outcome of a test; `?` works on every error type.
pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// A store rooted in its own temporary directory, removed on drop.
pub(crate) struct Fixture {
    pub(crate) dir: TempDir,
    pub(crate) store: JsonlStore,
}

pub(crate) fn fixture() -> io::Result<Fixture> {
    let dir = tempfile::tempdir()?;
    let store = JsonlStore::new(dir.path());
    Ok(Fixture { dir, store })
}

impl Fixture {
    /// Where the store keeps `account`'s stream.
    pub(crate) fn stream_path(&self, account: &AccountId) -> PathBuf {
        self.dir.path().join(format!("account-{account}.jsonl"))
    }

    /// Write `lines` as `account`'s stream file, bypassing the store.
    pub(crate) fn write_stream(&self, account: &AccountId, lines: &str) -> io::Result<()> {
        fs::write(self.stream_path(account), lines)
    }
}

pub(crate) fn account() -> Result<AccountId, ParseAccountIdError> {
    "acc-1".parse()
}

/// Ada opens an account, pays in 12.50 and takes out 2.50.
pub(crate) fn history() -> Result<Vec<Recorded>, ParseOwnerNameError> {
    let at = |millis| Timestamp::from_unix_millis(millis);
    Ok(vec![
        Recorded {
            event: Event::Opened {
                owner: "Ada Lovelace".parse()?,
            },
            recorded_at: at(1_767_225_600_000),
        },
        Recorded {
            event: Event::Deposited {
                amount: Cents::new(1_250),
            },
            recorded_at: at(1_767_225_660_000),
        },
        Recorded {
            event: Event::Withdrew {
                amount: Cents::new(250),
            },
            recorded_at: at(1_767_225_720_000),
        },
    ])
}
