//! Writing a command's result and choosing the exit code.

use std::error::Error;
use std::io::{self, Write};
use std::process::ExitCode;

/// The exit codes, in one place so none is a loose constant (H-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exit {
    /// 0: done.
    Success,
    /// 1: the command failed (`scan` found a hard hit), could not run, or its output could not
    /// be written for a reason other than a closed pipe.
    Failure,
    /// 2: the command line was not understood, or the command cannot run here (`scan` outside
    /// a Rust project).
    Usage,
}

impl From<temper_app::Exit> for Exit {
    fn from(exit: temper_app::Exit) -> Self {
        match exit {
            temper_app::Exit::Ok => Self::Success,
            temper_app::Exit::Fail => Self::Failure,
            temper_app::Exit::Usage => Self::Usage,
        }
    }
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        match exit {
            Exit::Success => Self::SUCCESS,
            Exit::Failure => Self::from(1),
            Exit::Usage => Self::from(2),
        }
    }
}

/// Writes `stderr`, then `stdout`, each through one locked handle and flushed (H-06, ERR-13),
/// and returns the exit: `exit` itself, or [`Exit::Failure`] when a stream cannot be written.
///
/// Stderr goes first because every 1.x command that writes both writes its note while it
/// works (`status`'s `TEMPER_BASE` note) and its result last, so a merged stream (a terminal,
/// `2>&1`) reads as 1.x's does (#207 review F1).
///
/// A reader that went away (`temper .. | head`) is not a failure: temper stops writing to it
/// and keeps the command's own code (port plan §2.4, D16(b)).
pub(crate) fn emit(stdout: &str, stderr: &str, exit: Exit) -> Exit {
    let written = [
        write_to(io::stderr().lock(), stderr),
        write_to(io::stdout().lock(), stdout),
    ];
    let failed = written
        .iter()
        .any(|result| matches!(result, Err(error) if error.kind() != io::ErrorKind::BrokenPipe));
    if failed { Exit::Failure } else { exit }
}

/// Reports `error` on stderr as `temper: ` and the error with each of its causes, joined with
/// `: `, and returns [`Exit::Failure`].
pub(crate) fn fail(error: &dyn Error) -> Exit {
    emit("", &format!("temper: {}\n", chain(error)), Exit::Failure)
}

/// `error` and each of its causes, joined with `: ` (anyhow's `{:#}` shape).
pub(crate) fn chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        text.push_str(": ");
        text.push_str(&source.to_string());
        cause = source.source();
    }
    text
}

fn write_to(mut out: impl Write, text: &str) -> io::Result<()> {
    out.write_all(text.as_bytes())?;
    out.flush()
}
