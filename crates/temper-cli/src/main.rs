//! The `temper` binary: the composition root of the 2.0 port.
//!
//! Until the 2.0.0 cutover it answers only `--version` and the usage text; every 1.x subcommand
//! still runs in the Node engine (`scripts/temper.mjs`).

mod args;

use std::io::{self, Write};
use std::process::ExitCode;

use args::{Command, USAGE, UsageExit};

fn main() -> ExitCode {
    let (text, exit) = match args::parse(std::env::args_os().skip(1)) {
        Command::Version => (
            concat!("temper ", env!("CARGO_PKG_VERSION"), "\n"),
            Exit::Success,
        ),
        Command::Usage(UsageExit::Requested) => (USAGE, Exit::Success),
        Command::Usage(UsageExit::Unknown) => (USAGE, Exit::Usage),
    };
    match write_out(text) {
        Ok(()) => exit.into(),
        // Nothing useful can be said about a stdout that cannot be written to.
        Err(_) => Exit::Output.into(),
    }
}

/// Write `text` to stdout through one locked handle and flush it (H-06, ERR-13).
fn write_out(text: &str) -> io::Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(text.as_bytes())?;
    out.flush()
}

/// The exit codes, in one place so none is a loose constant (H-11).
enum Exit {
    /// 0: done.
    Success,
    /// 1: stdout could not be written.
    Output,
    /// 2: the command line was not understood.
    Usage,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        match exit {
            Exit::Success => Self::SUCCESS,
            Exit::Output => Self::from(1),
            Exit::Usage => Self::from(2),
        }
    }
}
