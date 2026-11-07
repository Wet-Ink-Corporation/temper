//! The `temper` binary: the composition root of the 2.0 port.
//!
//! Until the 2.0.0 cutover it answers `--version`, the usage text, `temper scan --all`,
//! `temper scan FILES..`, `temper status` (text) and `temper hook post-tool`, `user-prompt`,
//! `pre-edit` and `pre-bash`; the launcher routes only the hooks in its `RUST_READY` to it,
//! and every other 1.x subcommand still runs in the Node engine (`scripts/temper.mjs`).

mod args;
mod hook_io;
mod output;
mod wire;

use std::io::{self, IsTerminal};
use std::process::ExitCode;

use args::{Command, StatusUsage, USAGE, UsageExit};
use output::Exit;
use temper_app::{CommandError, CommandOutput, Engine, HookInput};
use temper_domain::{HookEvent, HookOutcome, STATUS_USAGE, UnknownHookEvent};
use temper_host::HostPorts;

fn main() -> ExitCode {
    let exit = match args::parse(std::env::args_os().skip(1)) {
        Command::Version => output::emit(
            concat!("temper ", env!("CARGO_PKG_VERSION"), "\n"),
            "",
            Exit::Success,
        ),
        Command::Usage(UsageExit::Requested) => output::emit(USAGE, "", Exit::Success),
        Command::Usage(UsageExit::Unknown) => output::emit(USAGE, "", Exit::Usage),
        Command::Scan(args) => command(|engine| engine.scan(&args)),
        Command::Hook(event) => hook(event),
        Command::Status(Ok(args)) => command(|engine| engine.status(&args)),
        Command::Status(Err(StatusUsage)) => output::emit("", STATUS_USAGE, Exit::Usage),
    };
    exit.into()
}

/// `temper scan` or `status`: the engine's output, its refusal (1.x's stderr line and exit),
/// or a failure.
fn command(run: impl FnOnce(&Engine<HostPorts>) -> Result<CommandOutput, CommandError>) -> Exit {
    let engine = match wire::engine() {
        Ok(engine) => engine,
        Err(error) => return output::fail(&error),
    };
    match run(&engine) {
        Ok(out) => output::emit(&out.stdout, &out.stderr, out.exit.into()),
        Err(CommandError::Refused { exit, message }) => {
            output::emit("", &format!("{message}\n"), exit.into())
        }
        Err(error) => output::fail(&error),
    }
}

/// `temper hook <event>`, in 1.x's order (`scripts/temper.mjs` `hook()`): an unknown event is
/// reported on stderr before stdin is read; input that cannot be read or parsed gets a notice
/// unless `TEMPER_DISABLE=1`, which otherwise silences the hook once stdin is read; a handler
/// failure becomes a notice. Every answer exits 0 and never blocks the action.
fn hook(event: Result<HookEvent, UnknownHookEvent>) -> Exit {
    let event = match event {
        Ok(event) => event,
        Err(unknown) => return output::emit("", &format!("{unknown}\n"), Exit::Success),
    };
    let stdin = io::stdin();
    let text = if stdin.is_terminal() {
        Ok(String::new())
    } else {
        hook_io::read_input(stdin.lock(), hook_io::STDIN_CAP)
    };
    let disabled = wire::hooks_disabled();
    let input = match text.map(|text| HookInput::parse(&text)) {
        Ok(Ok(input)) => input,
        // Too large, unreadable or not JSON: nothing is checked, and 1.x says so (D16(n)).
        Ok(Err(_)) | Err(_) if disabled => return Exit::Success,
        Ok(Err(_)) | Err(_) => return answer(&HookOutcome::input_unparseable(event)),
    };
    if disabled {
        return Exit::Success;
    }
    let outcome = match wire::engine() {
        Ok(engine) => match engine.hook(event, &input) {
            Ok(outcome) => outcome,
            Err(error) => HookOutcome::handler_failed(event, &output::chain(&error)),
        },
        Err(error) => HookOutcome::handler_failed(event, &output::chain(&error)),
    };
    answer(&outcome)
}

/// Writes the outcome's JSON, without a newline, and exits 0.
fn answer(outcome: &HookOutcome) -> Exit {
    match outcome.to_json() {
        Some(json) => output::emit(&json, "", Exit::Success),
        None => Exit::Success,
    }
}
