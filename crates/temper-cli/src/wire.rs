//! The composition root: the standard-library adapters and the run environment, wired into
//! the engine once per process.

use std::error::Error;
use std::fmt;
use std::io;

use temper_app::{Engine, RunEnv};
use temper_domain::{PathStyle, Scanner, ScannerSetupError};
use temper_host::HostPorts;

/// The engine on the host's adapters, in this process's working directory.
pub(crate) fn engine() -> Result<Engine<HostPorts>, WireError> {
    let cwd = std::env::current_dir().map_err(WireError::Cwd)?;
    let style = if cfg!(windows) {
        PathStyle::Windows
    } else {
        PathStyle::Posix
    };
    let scanner = Scanner::new().map_err(WireError::Scanner)?;
    Ok(Engine::new(
        HostPorts::default(),
        RunEnv { cwd, style },
        scanner,
    ))
}

/// The engine could not be built.
#[derive(Debug)]
pub(crate) enum WireError {
    /// The working directory cannot be read, for example because it was deleted.
    Cwd(io::Error),
    /// A built-in scanner pattern does not compile: a bug in temper.
    Scanner(ScannerSetupError),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cwd(_) => "cannot read the working directory",
            Self::Scanner(_) => "cannot build the scanner",
        })
    }
}

impl Error for WireError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cwd(source) => Some(source),
            Self::Scanner(source) => Some(source),
        }
    }
}
