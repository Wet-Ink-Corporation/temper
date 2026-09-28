use ledger_app::{AccountSummary, AppError, Ledger, Version};
use ledger_domain::{Cents, Rejection, State};

use crate::support::{
    ContendedStore, FakeStoreError, FixedClock, NOW, TestResult, UnavailableStore, account,
    deposit, ledger, open, open_with, withdraw,
};

#[test]
fn deposit_after_opening_reports_new_balance_and_version() -> TestResult {
    let mut ledger = ledger();
    ledger.execute(&account()?, &open()?)?;

    let summary = ledger.execute(&account()?, &deposit(1_250))?;

    assert_eq!(
        summary,
        AccountSummary {
            state: open_with(1_250)?,
            version: Version::new(2),
            last_activity: Some(NOW),
        }
    );
    Ok(())
}

#[test]
fn summary_replays_the_stored_history() -> TestResult {
    let mut ledger = ledger();
    for command in [open()?, deposit(900), withdraw(400)] {
        ledger.execute(&account()?, &command)?;
    }

    let summary = ledger.summary(&account()?)?;

    assert_eq!(
        summary,
        AccountSummary {
            state: open_with(500)?,
            version: Version::new(3),
            last_activity: Some(NOW),
        }
    );
    Ok(())
}

#[test]
fn unknown_account_is_not_opened_at_version_zero() -> TestResult {
    let summary = ledger().summary(&account()?);

    assert_eq!(
        summary,
        Ok(AccountSummary {
            state: State::NotOpened,
            version: Version::new(0),
            last_activity: None,
        })
    );
    Ok(())
}

#[test]
fn rejected_command_stores_nothing() -> TestResult {
    let mut ledger = ledger();
    ledger.execute(&account()?, &open()?)?;

    let result = ledger.execute(&account()?, &withdraw(1));

    assert_eq!(
        result,
        Err(AppError::Rejected(Rejection::InsufficientFunds {
            balance: Cents::ZERO,
            requested: Cents::new(1),
        }))
    );
    assert_eq!(ledger.summary(&account()?)?.version, Version::new(1));
    Ok(())
}

#[test]
fn store_failure_is_not_reported_as_a_rejection() -> TestResult {
    let mut ledger = Ledger::new(UnavailableStore, FixedClock(NOW));

    let result = ledger.execute(&account()?, &open()?);

    assert_eq!(result, Err(AppError::Store(FakeStoreError::Unavailable)));
    Ok(())
}

#[test]
fn concurrent_write_surfaces_as_a_store_conflict() -> TestResult {
    let mut ledger = Ledger::new(ContendedStore, FixedClock(NOW));

    let result = ledger.execute(&account()?, &open()?);

    assert_eq!(
        result,
        Err(AppError::Store(FakeStoreError::Conflict {
            expected: Version::new(0),
            actual: Version::new(1),
        }))
    );
    Ok(())
}
