use std::assert_matches;
use std::fs;

use ledger_app::{EventStore, Version};
use ledger_store_jsonl::{JsonlStore, StoreError};

use crate::support::{TestResult, account, fixture, history};

#[test]
fn appended_events_load_back_in_order() -> TestResult {
    let mut fixture = fixture()?;
    let history = history()?;

    let version = fixture
        .store
        .append(&account()?, Version::new(0), &history)?;

    assert_eq!(version, Version::new(3));
    assert_eq!(fixture.store.load(&account()?)?, history);
    Ok(())
}

#[test]
fn unknown_account_has_empty_history() -> TestResult {
    let fixture = fixture()?;

    assert_eq!(fixture.store.load(&account()?)?, []);
    Ok(())
}

#[test]
fn append_from_stale_version_is_refused_and_writes_nothing() -> TestResult {
    let mut fixture = fixture()?;
    let history = history()?;
    let (opened, rest) = history.split_at(1);
    fixture.store.append(&account()?, Version::new(0), opened)?;
    // A second handle on the same directory, like a second process.
    let mut rival = JsonlStore::new(fixture.dir.path());
    rival.append(&account()?, Version::new(1), rest)?;

    let result = fixture.store.append(&account()?, Version::new(1), rest);

    assert_matches!(
        result,
        Err(StoreError::Conflict { expected, actual, .. })
            if expected == Version::new(1) && actual == Version::new(3)
    );
    assert_eq!(fixture.store.load(&account()?)?, history);
    Ok(())
}

#[test]
fn skips_and_then_repairs_an_append_interrupted_by_a_crash() -> TestResult {
    let mut fixture = fixture()?;
    let history = history()?;
    let (opened, rest) = history.split_at(1);
    fixture.store.append(&account()?, Version::new(0), opened)?;
    let path = fixture.stream_path(&account()?);
    let mut bytes = fs::read(&path)?;
    bytes.extend_from_slice(br#"{"schema_version":2,"recorded_at"#);
    fs::write(&path, bytes)?;

    let before_repair = fixture.store.load(&account()?)?;
    fixture.store.append(&account()?, Version::new(1), rest)?;

    assert_eq!(before_repair, opened);
    assert_eq!(fixture.store.load(&account()?)?, history);
    Ok(())
}
