//! Child processes on the standard library, with timeouts and capped output (H-13).

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, Scope, ScopedJoinHandle};
use std::time::Duration;

use temper_app::{
    ByteCap, ExitKind, ProcessError, ProcessOutput, ProcessRequest, ProcessRunner, Stream,
};
use wait_timeout::ChildExt;

/// [`ProcessRunner`] on `std::process`, as 1.x's `spawnSync` with `stdio: ['ignore', 'pipe',
/// 'pipe']`, `maxBuffer` and `timeout`.
///
/// Two named reader threads drain stdout and stderr inside a `thread::scope`, so both are
/// joined before `run` returns. A reader past its cap stops reading and closes its pipe; the
/// child then fails its next write or meets its timeout, so neither memory nor time is
/// unbounded. `kill` reaches only the child, not its own children: a grandchild that inherits
/// a pipe keeps its reader waiting (Phase 4 brings process groups for cargo).
#[derive(Debug, Default, Clone, Copy)]
pub struct StdProcess;

impl ProcessRunner for StdProcess {
    fn run(&self, request: &ProcessRequest<'_>) -> Result<ProcessOutput, ProcessError> {
        let program = || request.program.to_owned();
        let mut command = Command::new(request.program);
        command
            .args(request.args)
            .current_dir(request.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_window(&mut command);
        let mut child = command.spawn().map_err(|source| ProcessError::Spawn {
            program: program(),
            source,
        })?;
        let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
        let (status, out, err) = thread::scope(|scope| {
            let out = spawn_reader(scope, "temper-stdout", stdout, request.stdout_cap);
            let err = spawn_reader(scope, "temper-stderr", stderr, request.stderr_cap);
            let status = if out.is_ok() && err.is_ok() {
                wait(&mut child, request.timeout)
            } else {
                // A reader that never started dropped its pipe; stop the child, so the other
                // reader meets the end of its pipe, and report the reader's failure below.
                kill_and_reap(&mut child).map(exit_kind)
            };
            // Join both readers before looking at either result (ASY-10).
            (status, joined(out), joined(err))
        });
        let lost = |source| ProcessError::Io {
            program: program(),
            source,
        };
        let (out, err) = (out.map_err(lost)?, err.map_err(lost)?);
        let status = status.map_err(lost)?;
        for (captured, stream, cap) in [
            (&out, Stream::Stdout, request.stdout_cap),
            (&err, Stream::Stderr, request.stderr_cap),
        ] {
            if captured.overflowed {
                return Err(ProcessError::Overflow {
                    program: program(),
                    stream,
                    cap: cap.get(),
                });
            }
        }
        Ok(ProcessOutput {
            status,
            stdout: out.bytes,
            stderr: err.bytes,
        })
    }
}

/// At most a cap's worth of one output stream.
#[derive(Debug, Default)]
struct Captured {
    /// The bytes read, at most the cap.
    bytes: Vec<u8>,
    /// Whether the stream held more than the cap.
    overflowed: bool,
}

/// Starts a named reader of `pipe` in `scope`.
fn spawn_reader<'scope, R: Read + Send + 'scope>(
    scope: &'scope Scope<'scope, '_>,
    name: &str,
    pipe: Option<R>,
    cap: ByteCap,
) -> io::Result<ScopedJoinHandle<'scope, io::Result<Captured>>> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn_scoped(scope, move || {
            pipe.map_or_else(|| Ok(Captured::default()), |pipe| read_capped(pipe, cap))
        })
}

/// A reader's result, once it has finished.
fn joined(reader: io::Result<ScopedJoinHandle<'_, io::Result<Captured>>>) -> io::Result<Captured> {
    reader?
        .join()
        // `read_capped` does not panic; the payload holds nothing to report.
        .unwrap_or_else(|_payload| Err(io::Error::other("output reader panicked")))
}

/// Reads `pipe` to its end or one byte past `cap`, whichever comes first, then closes it.
fn read_capped(pipe: impl Read, cap: ByteCap) -> io::Result<Captured> {
    let limit = u64::try_from(cap.get())
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::new();
    pipe.take(limit).read_to_end(&mut bytes)?;
    let overflowed = bytes.len() > cap.get();
    bytes.truncate(cap.get());
    Ok(Captured { bytes, overflowed })
}

/// Waits for `child` up to `timeout`; past it, kills and reaps it.
fn wait(child: &mut Child, timeout: Duration) -> io::Result<ExitKind> {
    match child.wait_timeout(timeout) {
        Ok(Some(status)) => Ok(exit_kind(status)),
        Ok(None) => kill_and_reap(child).map(|_status| ExitKind::TimedOut),
        Err(error) => {
            match kill_and_reap(child) {
                // The failed wait is what to report; the child is stopped or cannot be.
                Ok(_) | Err(_) => Err(error),
            }
        }
    }
}

/// Kills `child` and waits for it, so it leaves no zombie.
///
/// `kill` answers `Ok` for a child that has already exited, so an error means the signal
/// could not be sent. A child that exited all the same is reaped; one that did not is
/// reported rather than waited for without a bound.
fn kill_and_reap(child: &mut Child) -> io::Result<ExitStatus> {
    match child.kill() {
        Ok(()) => child.wait(),
        Err(error) => child.try_wait()?.ok_or(error),
    }
}

fn exit_kind(status: ExitStatus) -> ExitKind {
    status.code().map_or(ExitKind::Signal, ExitKind::Code)
}

/// 1.x's `windowsHide`: no console window for the child.
#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    /// `CREATE_NO_WINDOW` from the Win32 process-creation flags.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// Only Windows opens a window for a child.
#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}

#[cfg(test)]
mod tests;
