//! Runs the real `temper` binary.

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Outcome of a test; `?` works on every error type.
pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// What one run of the binary produced.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// The `temper` binary, with git kept from looking for a repository above the system's
/// temporary directory, so a test's tree is a repository only when the test makes it one.
fn temper_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_temper"));
    command.env("GIT_CEILING_DIRECTORIES", std::env::temp_dir());
    command
}

/// Run `temper <args..>`.
pub(crate) fn temper(args: &[&str]) -> Result<Run, Box<dyn Error>> {
    run(temper_command().args(args))
}

/// Run `temper <args..>` in `dir`.
pub(crate) fn temper_in(dir: &Path, args: &[&str]) -> Result<Run, Box<dyn Error>> {
    run(temper_command().args(args).current_dir(dir))
}

/// The variables a hook reads, which a test sets itself or not at all.
const HOOK_VARS: [&str; 6] = [
    "TEMPER_DISABLE",
    "CLAUDE_PLUGIN_DATA",
    "CLAUDE_CODE_REMOTE_SESSION_ID",
    "TMPDIR",
    "TMP",
    "TEMP",
];

/// Run `temper hook <event>` in `dir` with `stdin` and `env` and none of the other variables a
/// hook reads; the temporary directory is `dir` unless `env` names one.
pub(crate) fn hook_in(
    dir: &Path,
    event: &str,
    stdin: &str,
    env: &[(&str, &OsStr)],
) -> Result<Run, Box<dyn Error>> {
    let mut command = temper_command();
    command
        .args(["hook", event])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in HOOK_VARS {
        command.env_remove(name);
    }
    for name in ["TMPDIR", "TMP", "TEMP"] {
        command.env(name, dir);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command.spawn()?;
    // Dropping the handle after the write closes the pipe, so the hook reads to its end.
    child
        .stdin
        .take()
        .ok_or("stdin is piped")?
        .write_all(stdin.as_bytes())?;
    let output = child.wait_with_output()?;
    Ok(Run {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout)?,
        stderr: String::from_utf8(output.stderr)?,
    })
}

/// `git <args..>` in `dir`, as a committer that needs no configuration.
pub(crate) fn git(dir: &Path, args: &[&str]) -> TestResult {
    let status = Command::new("git")
        .args([
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("git {args:?} failed in {}: {status}", dir.display()).into())
    }
}

fn run(command: &mut Command) -> Result<Run, Box<dyn Error>> {
    let output = command.output()?;
    Ok(Run {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout)?,
        stderr: String::from_utf8(output.stderr)?,
    })
}

/// Run `temper <args..>` with stdout a pipe whose reading end is already closed, as in
/// `temper scan | head` after `head` exits; return the exit code.
pub(crate) fn temper_into_closed_pipe(args: &[&str]) -> Result<Option<i32>, Box<dyn Error>> {
    temper_in_into_closed_pipe(Path::new("."), args)
}

/// [`temper_into_closed_pipe`] in `dir`.
pub(crate) fn temper_in_into_closed_pipe(
    dir: &Path,
    args: &[&str],
) -> Result<Option<i32>, Box<dyn Error>> {
    let (reader, writer) = std::io::pipe()?;
    drop(reader);
    let status = temper_command()
        .args(args)
        .current_dir(dir)
        .stdout(writer)
        .status()?;
    Ok(status.code())
}

/// Writes each `(relative path, text)` below `root`, creating directories as needed.
pub(crate) fn write_tree(root: &Path, files: &[(&str, &str)]) -> TestResult {
    for (rel, text) in files {
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, text)?;
    }
    Ok(())
}

/// `git init -q` in `dir`.
pub(crate) fn git_init(dir: &Path) -> TestResult {
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("git init failed in {}: {status}", dir.display()).into())
    }
}
