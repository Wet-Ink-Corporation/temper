//! The engine: the ports and the run environment, wired once, and one method per command.

use std::path::PathBuf;

use temper_domain::{PathStyle, Scanner};

use crate::ports::{FsError, Ports};
use crate::scan;

/// What a command reads from the process it runs in, captured once by the composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEnv {
    /// The working directory, absolute, as the OS reports it.
    pub cwd: PathBuf,
    /// The platform's path rules.
    pub style: PathStyle,
}

/// The use cases, over the ports `P`.
#[derive(Debug)]
pub struct Engine<P> {
    ports: P,
    env: RunEnv,
    scanner: Scanner,
}

impl<P: Ports> Engine<P> {
    /// An engine over `ports`, in `env`, with the compiled `scanner`. It keeps all three.
    pub fn new(ports: P, env: RunEnv, scanner: Scanner) -> Self {
        Self {
            ports,
            env,
            scanner,
        }
    }

    /// `temper scan --all` or `temper scan FILES..`: scans the targets and reports their hits
    /// as 1.x does, exiting [`Exit::Fail`] when any hit is hard.
    ///
    /// # Errors
    ///
    /// [`CommandError::Refused`] (exit [`Exit::Usage`]) outside a Rust project;
    /// [`CommandError::Files`] when a directory of the walk cannot be listed.
    pub fn scan(&self, args: &ScanArgs) -> Result<CommandOutput, CommandError> {
        scan::run(&self.ports, &self.env, &self.scanner, args)
    }
}

/// The arguments of `temper scan`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanArgs {
    /// What to scan.
    pub targets: ScanTargets,
    /// How to write the result.
    pub format: OutputFormat,
}

/// What `temper scan` scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanTargets {
    /// `--all`: every `.rs` file below the project root.
    All,
    /// The files named, relative to the working directory, decoded lossily as Node decodes its
    /// arguments.
    Files(Vec<String>),
}

/// How a command writes its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// For people.
    Text,
    /// `--json`.
    Json,
}

/// What a command writes, and how it exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Everything for stdout.
    pub stdout: String,
    /// Everything for stderr.
    pub stderr: String,
    /// How the process exits.
    pub exit: Exit,
}

/// How a command ends; the binary maps each to its exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Exit 0.
    Ok,
    /// Exit 1: the command found something that fails it, such as a hard hit.
    Fail,
    /// Exit 2: the command cannot run here or was called wrongly.
    Usage,
}

/// Why a command could not produce its output.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The command refused to run, with 1.x's message for stderr and its exit.
    #[error("{message}")]
    Refused {
        /// How the process exits.
        exit: Exit,
        /// The text for stderr, without a newline.
        message: String,
    },
    /// The file system failed where 1.x would fail too.
    #[error(transparent)]
    Files(#[from] FsError),
    /// The result could not be written as JSON.
    #[error("cannot write the results as JSON")]
    Json(#[source] serde_json::Error),
}
