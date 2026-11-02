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
        }) => Some(
            String::from_utf8(stdout)
                .unwrap_or_else(|invalid| String::from_utf8_lossy(invalid.as_bytes()).into_owned()),
        ),
        // 1.x reads every other outcome as a failed git run: another exit code, a signal, a
        // timeout, git missing (its code 127) and `maxBuffer`'s `ENOBUFS` alike.
        Ok(_) | Err(_) => None,
    }
}
