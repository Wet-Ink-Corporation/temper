//! Runs the real `ledger` binary against a temporary store.

use std::error::Error;
use std::path::Path;
use std::process::Command;

/// Outcome of a test; `?` works on every error type.
pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// What one run of the binary produced.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
}

/// Run `ledger --store <store> <args..>` with logging at its default level.
pub(crate) fn ledger(store: &Path, args: &[&str]) -> Result<Run, Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_ledger"))
        .arg("--store")
        .arg(store)
        .args(args)
        .env_remove("RUST_LOG")
        .output()?;
    Ok(Run {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout)?,
    })
}

/// Run each command in order, requiring every one to succeed.
pub(crate) fn ledger_all(store: &Path, commands: &[&[&str]]) -> TestResult {
    for args in commands {
        let run = ledger(store, args)?;
        if run.code != Some(0) {
            return Err(format!("`ledger {}` failed: {run:?}", args.join(" ")).into());
        }
    }
    Ok(())
}
