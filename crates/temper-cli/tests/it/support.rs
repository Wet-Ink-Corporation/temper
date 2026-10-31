//! Runs the real `temper` binary.

use std::error::Error;
use std::process::Command;

/// Outcome of a test; `?` works on every error type.
pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// What one run of the binary produced.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Run `temper <args..>`.
pub(crate) fn temper(args: &[&str]) -> Result<Run, Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_temper"))
        .args(args)
        .output()?;
    Ok(Run {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout)?,
        stderr: String::from_utf8(output.stderr)?,
    })
}
