//! `ledger`: the command-line front end and composition root.
//!
//! Parses arguments, wires the JSON Lines store and the system clock into the
//! use cases, runs one request, and maps the outcome to an exit code. Data goes
//! to stdout; diagnostics go to stderr through `tracing` (`RUST_LOG` filters).
//!
//! | Exit code | Meaning |
//! |---|---|
//! | 0 | success |
//! | 1 | stdout could not be written |
//! | 2 | usage error |
//! | 3 | the ledger rejected the command; nothing was recorded |
//! | 4 | the event store failed: I/O, a corrupt record, or a concurrent write |
//! | 5 | the log subscriber could not be installed |

mod args;
mod clock;
mod output;

use std::error::Error;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use anyhow::Context;
use ledger_app::{AppError, Ledger};
use ledger_domain::Rejection;
use ledger_store_jsonl::JsonlStore;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use crate::args::{Invocation, Request};
use crate::clock::SystemClock;

/// Why the process exits unsuccessfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exit {
    Output,
    Usage,
    Rejected,
    Store,
    Setup,
}

impl From<Exit> for ExitCode {
    /// The documented exit codes. Scripts branch on them, so a code never
    /// changes meaning; a new kind of failure gets a new code.
    fn from(exit: Exit) -> Self {
        let code: u8 = match exit {
            Exit::Output => 1,
            Exit::Usage => 2,
            Exit::Rejected => 3,
            Exit::Store => 4,
            Exit::Setup => 5,
        };
        Self::from(code)
    }
}

/// How a run failed. Each kind has its own [`Exit`].
#[derive(Debug)]
enum Failure {
    Rejected(Rejection),
    Store(anyhow::Error),
    Output(anyhow::Error),
}

fn main() -> ExitCode {
    if let Err(error) = init_tracing() {
        return report_setup_failure(&*error);
    }
    let invocation = match args::parse(std::env::args_os().skip(1)) {
        Ok(invocation) => invocation,
        Err(error) => {
            let error: &(dyn Error + 'static) = &error;
            tracing::error!(
                name: "args.parse.failed",
                error,
                "invalid command line; run `ledger --help` for usage"
            );
            return Exit::Usage.into();
        }
    };
    // Each failure is logged once, here, where it is handled (ERR-08).
    match run(&invocation) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Rejected(rejection)) => {
            tracing::warn!(name: "command.execute.rejected", %rejection, "command rejected");
            Exit::Rejected.into()
        }
        Err(Failure::Store(error)) => {
            tracing::error!(name: "store.request.failed", error = &*error, "event store failed");
            Exit::Store.into()
        }
        Err(Failure::Output(error)) => {
            tracing::error!(name: "stdout.write.failed", error = &*error, "output failed");
            Exit::Output.into()
        }
    }
}

fn run(invocation: &Invocation) -> Result<(), Failure> {
    let out = BufWriter::new(io::stdout().lock());
    let mut ledger = Ledger::new(JsonlStore::new(&invocation.store), SystemClock);
    let (account, result) = match &invocation.request {
        Request::Help => return write_and_flush(out, |out| output::write_usage(out)),
        Request::Show(account) => (account, ledger.summary(account).map_err(AppError::Store)),
        Request::Execute(account, command) => (account, ledger.execute(account, command)),
    };
    let summary = result.map_err(|error| match error {
        AppError::Rejected(rejection) => Failure::Rejected(rejection),
        AppError::Store(source) => {
            Failure::Store(anyhow::Error::new(source).context(format!("account {account}")))
        }
    })?;
    tracing::info!(
        name: "request.serve.succeeded",
        %account,
        version = %summary.version,
        "request served"
    );
    write_and_flush(out, |out| {
        output::write_summary(out, invocation.format, account, &summary)
    })
}

/// Write with `write`, then flush `out`, so a failed write surfaces here as
/// an error instead of being lost when the buffer drops.
fn write_and_flush<W: Write>(
    mut out: W,
    write: impl FnOnce(&mut W) -> io::Result<()>,
) -> Result<(), Failure> {
    write(&mut out)
        .and_then(|()| out.flush())
        .context("writing to stdout")
        .map_err(Failure::Output)
}

/// Logging is what failed, so this reports straight to stderr.
fn report_setup_failure(error: &dyn Error) -> ExitCode {
    match writeln!(
        io::stderr().lock(),
        "error: installing the log subscriber: {error}"
    ) {
        // If stderr is closed too, the exit code is the only channel left.
        Ok(()) | Err(_) => Exit::Setup.into(),
    }
}

/// Installs the stderr subscriber, filtered by `RUST_LOG` (default `warn`).
fn init_tracing() -> Result<(), Box<dyn Error + Send + Sync>> {
    // `try_init`, not `init`: `init` panics when a subscriber is already set (H-03).
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::WARN.into())
                .from_env_lossy(),
        )
        .try_init()
}
