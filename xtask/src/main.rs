//! `cargo xtask <task>`: temper's repository tooling. Never published.
//!
//! Tasks: `check-licenses`.

mod licenses;

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use licenses::{COPIES, Problem};

const USAGE: &str = "usage: cargo xtask check-licenses";

fn main() -> ExitCode {
    let task: Vec<OsString> = std::env::args_os().skip(1).collect();
    let outcome = match task.as_slice() {
        [name] if name == "check-licenses" => check_licenses(&workspace_root()),
        _ => return report(&[USAGE], Exit::Usage),
    };
    match outcome {
        Ok(problems) if problems.is_empty() => report(
            &["check-licenses: every crate's licence copies match the root"],
            Exit::Success,
        ),
        Ok(problems) => {
            let lines: Vec<String> = problems
                .iter()
                .map(|p| format!("check-licenses: {p}"))
                .collect();
            report(&lines, Exit::Failure)
        }
        Err(error) => report(&[format!("check-licenses: {error}")], Exit::Failure),
    }
}

/// The workspace root: the parent of this crate's directory.
fn workspace_root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .map_or_else(|| here.to_path_buf(), Path::to_path_buf)
}

/// Every licence copy under `crates/*` that differs from the root, or is missing.
fn check_licenses(root: &Path) -> Result<Vec<Problem>, XtaskError> {
    let crates_dir = root.join("crates");
    let unreadable = |source| XtaskError::Read {
        path: crates_dir.clone(),
        source,
    };
    let mut crates: Vec<PathBuf> = fs::read_dir(&crates_dir)
        .map_err(unreadable)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(unreadable)?;
    crates.retain(|dir| dir.join("Cargo.toml").is_file());
    crates.sort();
    let mut problems = Vec::new();
    for copy in COPIES {
        let root_text = read(&root.join(copy.at_root))?;
        for dir in &crates {
            let name = dir
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let path = dir.join(copy.in_crate);
            let found = if path.is_file() {
                Some(read(&path)?)
            } else {
                None
            };
            problems.extend(licenses::compare(&name, copy, &root_text, found.as_deref()));
        }
    }
    Ok(problems)
}

fn read(path: &Path) -> Result<String, XtaskError> {
    fs::read_to_string(path).map_err(|source| XtaskError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// Write each line to stdout (success) or stderr (anything else), then exit.
fn report(lines: &[impl fmt::Display], exit: Exit) -> ExitCode {
    let written = match exit {
        Exit::Success => write_lines(io::stdout().lock(), lines),
        Exit::Failure | Exit::Usage => write_lines(io::stderr().lock(), lines),
    };
    match written {
        Ok(()) => exit.into(),
        Err(_) => Exit::Failure.into(),
    }
}

fn write_lines(mut out: impl Write, lines: &[impl fmt::Display]) -> io::Result<()> {
    for line in lines {
        writeln!(out, "{line}")?;
    }
    out.flush()
}

/// Why a task could not run.
#[derive(Debug)]
enum XtaskError {
    /// A file or directory could not be read.
    Read { path: PathBuf, source: io::Error },
}

impl fmt::Display for XtaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(f, "cannot read {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for XtaskError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
        }
    }
}

/// The exit codes: 0 done, 1 a check failed or a file could not be read, 2 usage.
enum Exit {
    Success,
    Failure,
    Usage,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        match exit {
            Exit::Success => Self::SUCCESS,
            Exit::Failure => Self::from(1),
            Exit::Usage => Self::from(2),
        }
    }
}
