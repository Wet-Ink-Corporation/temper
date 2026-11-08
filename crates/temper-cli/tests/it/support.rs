//! Runs the real `temper` binary.

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use temper_domain::{PathStyle, unverbatim};

/// Outcome of a test; `?` works on every error type.
pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// `path`'s real path as the engine spells it: canonicalized, without the verbatim prefix that
/// `fs::canonicalize` adds on Windows and 1.x's `unverbatim` strips.
pub(crate) fn real_path(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let real = fs::canonicalize(path)?;
    if cfg!(windows) {
        Ok(engine_spelling(&real.to_string_lossy()))
    } else {
        Ok(real)
    }
}

/// A Windows path without its verbatim prefix, as 1.x's `unverbatim` (the domain's) spells it:
/// `\\?\UNC\server\share` becomes `\\server\share`, and `\\?\C:\x` becomes `C:\x`.
fn engine_spelling(path: &str) -> PathBuf {
    PathBuf::from(unverbatim(path, PathStyle::Windows).as_ref())
}

/// #210 review F4: a verbatim UNC path is the share's own `\\server\share` spelling, not
/// `UNC\server\share` with the `\\?\` cut off; a verbatim drive path loses the prefix only.
#[test]
fn a_verbatim_path_is_spelled_as_the_engine_spells_it() {
    let cases = [
        (r"\\?\UNC\server\share\dir", r"\\server\share\dir"),
        (r"\\?\C:\Users\u", r"C:\Users\u"),
        (r"C:\plain", r"C:\plain"),
    ];

    for (verbatim, spelled) in cases {
        assert_eq!(
            engine_spelling(verbatim),
            PathBuf::from(spelled),
            "{verbatim}"
        );
    }
}

/// What one run of the binary produced.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// The `temper` binary, with git kept from looking for a repository above the system's
/// temporary directory, so a test's tree is a repository only when the test makes it one.
pub(crate) fn temper_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_temper"));
    // The tests' own temper session (TEMPER_CLI, TEMPER_BASE, CLAUDE_PLUGIN_ROOT, ...) must
    // not reach the engine: it changes the gate command and the base the engine reports.
    for (name, _) in std::env::vars_os() {
        let text = name.to_string_lossy();
        // Windows reads variable names without regard to case, so `Temper_Base` is
        // `TEMPER_BASE` there.
        let text = if cfg!(windows) {
            text.to_ascii_uppercase()
        } else {
            text.into_owned()
        };
        if text.starts_with("TEMPER_") || text.starts_with("CLAUDE") {
            command.env_remove(&name);
        }
    }
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
    // Dropping the handle after the write closes the pipe, so the hook reads to its end. A hook
    // that reads no input (an unknown event, as in 1.x) may exit before the write lands, and the
    // write then fails with a broken pipe; the run's output still decides the test.
    let written = child
        .stdin
        .take()
        .ok_or("stdin is piped")?
        .write_all(stdin.as_bytes());
    match written {
        Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => return Err(error.into()),
        Ok(()) | Err(_) => {}
    }
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
