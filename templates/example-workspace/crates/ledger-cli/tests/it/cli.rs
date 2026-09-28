use std::assert_matches;
use std::fs;

use serde_json::{Value, json};

use crate::support::{Run, TestResult, ledger, ledger_all};

#[test]
fn account_lifecycle_shows_the_running_balance() -> TestResult {
    let store = tempfile::tempdir()?;
    ledger_all(
        store.path(),
        &[
            &["open", "acc-1", "Ada Lovelace"],
            &["deposit", "acc-1", "20"],
            &["withdraw", "acc-1", "7.5"],
        ],
    )?;

    let shown = ledger(store.path(), &["show", "acc-1"])?;

    assert_eq!(
        shown,
        Run {
            code: Some(0),
            stdout: "acc-1: open, Ada Lovelace, balance 12.50 (version 3)\n".to_owned(),
        }
    );
    Ok(())
}

#[test]
fn emptied_account_can_be_closed() -> TestResult {
    let store = tempfile::tempdir()?;
    ledger_all(
        store.path(),
        &[
            &["open", "acc-1", "Ada Lovelace"],
            &["deposit", "acc-1", "5"],
            &["withdraw", "acc-1", "5"],
        ],
    )?;

    let closed = ledger(store.path(), &["close", "acc-1"])?;

    assert_eq!(
        closed,
        Run {
            code: Some(0),
            stdout: "acc-1: closed, Ada Lovelace (version 4)\n".to_owned(),
        }
    );
    Ok(())
}

#[test]
fn overdraft_exits_3_and_records_nothing() -> TestResult {
    let store = tempfile::tempdir()?;
    ledger_all(store.path(), &[&["open", "acc-1", "Ada Lovelace"]])?;

    let overdraft = ledger(store.path(), &["withdraw", "acc-1", "0.01"])?;

    assert_eq!(
        overdraft,
        Run {
            code: Some(3),
            stdout: String::new(),
        }
    );
    assert_eq!(
        ledger(store.path(), &["show", "acc-1"])?.stdout,
        "acc-1: open, Ada Lovelace, balance 0.00 (version 1)\n"
    );
    Ok(())
}

#[test]
fn json_format_reports_balance_in_cents() -> TestResult {
    let store = tempfile::tempdir()?;
    ledger_all(store.path(), &[&["open", "acc-1", "Ada Lovelace"]])?;

    let run = ledger(
        store.path(),
        &["--format", "json", "deposit", "acc-1", "12.50"],
    )?;

    let mut summary: Value = serde_json::from_str(&run.stdout)?;
    // The timestamp comes from the real clock: check its type, not its value.
    let recorded_at = summary
        .as_object_mut()
        .and_then(|fields| fields.remove("last_activity_unix_ms"));
    assert_matches!(recorded_at, Some(Value::Number(_)));
    assert_eq!(
        summary,
        json!({
            "account": "acc-1",
            "status": "open",
            "owner": "Ada Lovelace",
            "balance_cents": 1250,
            "version": 2,
        })
    );
    Ok(())
}

#[test]
fn unknown_command_is_a_usage_error() -> TestResult {
    let store = tempfile::tempdir()?;

    let run = ledger(store.path(), &["transfer", "acc-1", "acc-2"])?;

    assert_eq!(run.code, Some(2));
    Ok(())
}

#[test]
fn corrupt_stream_is_a_store_error() -> TestResult {
    let store = tempfile::tempdir()?;
    fs::write(store.path().join("account-acc-1.jsonl"), "not json\n")?;

    let run = ledger(store.path(), &["show", "acc-1"])?;

    assert_eq!(run.code, Some(4));
    Ok(())
}
