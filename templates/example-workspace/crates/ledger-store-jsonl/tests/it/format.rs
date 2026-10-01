use std::assert_matches;
use std::fs;

use ledger_app::{EventStore, Recorded, Timestamp, Version};
use ledger_domain::{Cents, Event, ParseOwnerNameError};
use ledger_store_jsonl::{RecordError, StoreError};

use crate::support::{TestResult, account, fixture, history};

#[test]
fn writes_one_current_schema_record_per_line() -> TestResult {
    let mut fixture = fixture()?;
    fixture
        .store
        .append(&account()?, Version::new(0), &history()?)?;

    let written = fs::read_to_string(fixture.stream_path(&account()?))?;

    insta::assert_snapshot!(written);
    Ok(())
}

#[test]
fn closed_event_is_written_as_a_bare_type_tag_and_read_back() -> TestResult {
    let mut fixture = fixture()?;
    let closed = [Recorded {
        event: Event::Closed,
        recorded_at: Timestamp::from_unix_millis(0),
    }];

    fixture
        .store
        .append(&account()?, Version::new(0), &closed)?;

    assert_eq!(
        fs::read_to_string(fixture.stream_path(&account()?))?,
        "{\"schema_version\":2,\"recorded_at_ms\":0,\"event\":{\"type\":\"closed\"}}\n"
    );
    assert_eq!(fixture.store.load(&account()?)?, closed);
    Ok(())
}

#[test]
fn upcasts_schema_v1_records_on_read() -> TestResult {
    let fixture = fixture()?;
    fixture.write_stream(
        &account()?,
        concat!(
            r#"{"schema_version":1,"kind":"opened","at":1700000000000,"name":"Ada Lovelace"}"#,
            "\n",
            r#"{"schema_version":1,"kind":"deposited","at":1700000060000,"cents":500}"#,
            "\n",
            r#"{"schema_version":1,"kind":"withdrawn","at":1700000120000,"cents":250}"#,
            "\n",
        ),
    )?;

    let loaded = fixture.store.load(&account()?)?;

    let at = Timestamp::from_unix_millis;
    assert_eq!(
        loaded,
        [
            Recorded {
                event: Event::Opened {
                    owner: "Ada Lovelace".parse()?,
                },
                recorded_at: at(1_700_000_000_000),
            },
            Recorded {
                event: Event::Deposited {
                    amount: Cents::new(500),
                },
                recorded_at: at(1_700_000_060_000),
            },
            Recorded {
                event: Event::Withdrew {
                    amount: Cents::new(250),
                },
                recorded_at: at(1_700_000_120_000),
            },
        ]
    );
    Ok(())
}

#[test]
fn refuses_records_from_a_newer_schema() -> TestResult {
    let fixture = fixture()?;
    fixture.write_stream(
        &account()?,
        "{\"schema_version\":3,\"recorded_at_ms\":0,\"event\":{\"type\":\"closed\"}}\n",
    )?;

    let result = fixture.store.load(&account()?);

    assert_matches!(
        result,
        Err(StoreError::Corrupt {
            line: 1,
            source: RecordError::UnsupportedSchema { found: 3 },
            ..
        })
    );
    Ok(())
}

#[test]
fn refuses_unknown_fields_even_on_field_less_events() -> TestResult {
    let fixture = fixture()?;
    fixture.write_stream(
        &account()?,
        "{\"schema_version\":2,\"recorded_at_ms\":0,\"event\":{\"type\":\"closed\",\"reason\":\"x\"}}\n",
    )?;

    let result = fixture.store.load(&account()?);

    assert_matches!(
        result,
        Err(StoreError::Corrupt {
            line: 1,
            source: RecordError::Malformed(_),
            ..
        })
    );
    Ok(())
}

#[test]
fn validates_stored_owner_names_into_the_domain() -> TestResult {
    let fixture = fixture()?;
    fixture.write_stream(
        &account()?,
        "{\"schema_version\":2,\"recorded_at_ms\":0,\"event\":{\"type\":\"opened\",\"owner\":\" \"}}\n",
    )?;

    let result = fixture.store.load(&account()?);

    assert_matches!(
        result,
        Err(StoreError::Corrupt {
            line: 1,
            source: RecordError::InvalidOwner(ParseOwnerNameError::Blank),
            ..
        })
    );
    Ok(())
}

#[test]
fn refuses_a_line_longer_than_the_record_limit() -> TestResult {
    let fixture = fixture()?;
    let huge = format!("{}\n", "x".repeat(70_000));
    fixture.write_stream(&account()?, &huge)?;

    let result = fixture.store.load(&account()?);

    assert_matches!(
        result,
        Err(StoreError::RecordTooLong {
            line: 1,
            limit: 65_535,
            ..
        })
    );
    Ok(())
}
