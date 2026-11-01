//! The `temper` binary: the composition root of the 2.0 port.
//!
//! Until the 2.0.0 cutover it answers `--version`, the usage text, `temper scan --all` and
//! `temper scan FILES..`; nothing routes to it yet, and every 1.x subcommand still runs in the
//! Node engine (`scripts/temper.mjs`).

mod args;
mod output;
mod wire;

use std::process::ExitCode;

use args::{Command, USAGE, UsageExit};
use output::Exit;
use temper_app::{CommandError, ScanArgs};

fn main() -> ExitCode {
    let exit = match args::parse(std::env::args_os().skip(1)) {
        Command::Version => output::emit(
            concat!("temper ", env!("CARGO_PKG_VERSION"), "\n"),
            "",
            Exit::Success,
        ),
        Command::Usage(UsageExit::Requested) => output::emit(USAGE, "", Exit::Success),
        Command::Usage(UsageExit::Unknown) => output::emit(USAGE, "", Exit::Usage),
        Command::Scan(args) => scan(&args),
    };
    exit.into()
}

/// `temper scan`: the engine's output, its refusal (1.x's stderr line and exit), or a failure.
fn scan(args: &ScanArgs) -> Exit {
    let engine = match wire::engine() {
        Ok(engine) => engine,
        Err(error) => return output::fail(&error),
    };
    match engine.scan(args) {
        Ok(out) => output::emit(&out.stdout, &out.stderr, out.exit.into()),
        Err(CommandError::Refused { exit, message }) => {
            output::emit("", &format!("{message}\n"), exit.into())
        }
        Err(error) => output::fail(&error),
    }
}
