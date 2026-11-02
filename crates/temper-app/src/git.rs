//! The git commands the use cases run, over [`ProcessRunner`] (1.x `project.mjs` `git`).

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use crate::ports::{ByteCap, ExitKind, ProcessOutput, ProcessRequest, ProcessRunner};

/// Every git call's timeout. 1.x set none; 2.0 bounds every child (D16(f), H-13).
const TIMEOUT: Duration = Duration::from_secs(120);

/// The cap on each captured stream: 1.x `run`'s `maxBuffer`.
const CAP: ByteCap = ByteCap::new(256 * 1024 * 1024);

/// 1.x `git(cwd, ['rev-parse', '--show-toplevel'])`: git's stdout as lossy UTF-8, untrimmed,
/// when it exits 0 in `cwd`; `None` otherwise.
pub(crate) fn toplevel(process: &impl ProcessRunner, cwd: &Path) -> Option<String> {
    rev_parse(process, cwd, "--show-toplevel")
}

/// 1.x `git(gitRoot, ['rev-parse', '--git-common-dir'])`: the shared `.git` directory of a
/// linked worktree (relative to `cwd`, or absolute), untrimmed; `None` when git fails.
pub(crate) fn common_dir(process: &impl ProcessRunner, cwd: &Path) -> Option<String> {
    rev_parse(process, cwd, "--git-common-dir")
}

/// The timeout of the protected-configuration checks' git calls (1.x's `timeoutMs: 5000`).
const SHORT: Duration = Duration::from_secs(5);

/// How a short git call ended, as 1.x's `run` tells the outcomes apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Short {
    /// Exit 0, with stdout as lossy UTF-8.
    Ran(String),
    /// Another exit code, or a signal (1.x's `code` is 1 then, with no `error`).
    Failed,
    /// git could not say: it did not start, ran past the timeout, or overflowed a stream
    /// (1.x's `error` is set).
    Unknown,
}

/// `git <args>` in `cwd` with the 5 s timeout.
pub(crate) fn short(process: &impl ProcessRunner, cwd: &Path, args: &[&str]) -> Short {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let request = ProcessRequest {
        program: "git",
        args: &args,
        cwd,
        stdout_cap: CAP,
        stderr_cap: CAP,
        timeout: SHORT,
    };
    match process.run(&request) {
        Ok(ProcessOutput {
            status: ExitKind::Code(0),
            stdout,
            ..
        }) => Short::Ran(lossy(stdout)),
        Ok(ProcessOutput {
            status: ExitKind::Code(_) | ExitKind::Signal,
            ..
        }) => Short::Failed,
        Ok(ProcessOutput {
            status: ExitKind::TimedOut,
            ..
        })
        | Err(_) => Short::Unknown,
    }
}

/// Bytes as UTF-8, with each invalid sequence replaced, as Node's `'utf8'` decoding does.
fn lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|invalid| String::from_utf8_lossy(invalid.as_bytes()).into_owned())
}

fn rev_parse(process: &impl ProcessRunner, cwd: &Path, flag: &str) -> Option<String> {
    let args = [OsString::from("rev-parse"), OsString::from(flag)];
    let request = ProcessRequest {
        program: "git",
        args: &args,
        cwd,
        stdout_cap: CAP,
        stderr_cap: CAP,
        timeout: TIMEOUT,
    };
    match process.run(&request) {
        Ok(ProcessOutput {
            status: ExitKind::Code(0),
            stdout,
            ..
        }) => Some(lossy(stdout)),
        // 1.x reads every other outcome as a failed git run: another exit code, a signal, a
        // timeout, git missing (its code 127) and `maxBuffer`'s `ENOBUFS` alike.
        Ok(_) | Err(_) => None,
    }
}
