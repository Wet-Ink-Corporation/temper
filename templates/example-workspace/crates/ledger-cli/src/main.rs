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

mod args;
mod clock;
mod output;

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

const EXIT_OUTPUT: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_REJECTED: u8 = 3;
const EXIT_STORE: u8 = 4;
const EXIT_SETUP: u8 = 5;

/// How a run failed. Each kind has its own exit code.
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
            let error = anyhow::Error::new(error);
            tracing::error!("{error:#}; run `ledger --help` for usage");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    // Each failure is logged once, here, where it is handled (ERR-08).
    match run(&invocation) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Rejected(rejection)) => {
            tracing::warn!(%rejection, "command rejected");
            ExitCode::from(EXIT_REJECTED)
        }
        Err(Failure::Store(error)) => {
            tracing::error!("{error:#}");
            ExitCode::from(EXIT_STORE)
        }
        Err(Failure::Output(error)) => {
            tracing::error!("{error:#}");
            ExitCode::from(EXIT_OUTPUT)
        }
    }
}

fn run(invocation: &Invocation) -> Result<(), Failure> {
    let mut out = BufWriter::new(io::stdout().lock());
    let mut ledger = Ledger::new(JsonlStore::new(&invocation.store), SystemClock);
    let (account, result) = match &invocation.request {
        Request::Help => return write_and_flush(&mut out, output::write_usage),
        Request::Show(account) => (account, ledger.summary(account).map_err(AppError::Store)),
        Request::Execute(account, command) => (account, ledger.execute(account, command)),
    };
    let summary = result.map_err(|error| match error {
        AppError::Rejected(rejection) => Failure::Rejected(rejection),
        AppError::Store(source) => {
            Failure::Store(anyhow::Error::new(source).context(format!("account {account}")))
        }
    })?;
    tracing::info!(%account, version = %summary.version, "request served");
    write_and_flush(&mut out, |out| {
        output::write_summary(out, invocation.format, account, &summary)
    })
}

fn write_and_flush<W: Write>(
    out: &mut W,
    write: impl FnOnce(&mut W) -> io::Result<()>,
) -> Result<(), Failure> {
    write(out)
        .and_then(|()| out.flush())
        .context("writing to stdout")
        .map_err(Failure::Output)
}

/// Logging is what failed, so this reports straight to stderr.
fn report_setup_failure(error: &dyn std::error::Error) -> ExitCode {
    match writeln!(
        io::stderr().lock(),
        "error: installing the log subscriber: {error}"
    ) {
        // If stderr is closed too, the exit code is the only channel left.
        Ok(()) | Err(_) => ExitCode::from(EXIT_SETUP),
    }
}

/// Installs the stderr subscriber. `try_init`, not `init`: `init` panics when a
/// subscriber is already set (H-03).
fn init_tracing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::WARN.into())
                .from_env_lossy(),
        )
        .try_init()
}
