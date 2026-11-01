//! temper's adapters to the standard library, implementing the ports `temper-app` defines: so
//! far the file system ([`StdFiles`]) and child processes with timeouts ([`StdProcess`]); the
//! clock comes in Phase 3.
//!
//! Adapters hold no policy and no message texts.

mod child;
mod fs;

pub use child::StdProcess;
pub use fs::StdFiles;
use temper_app::Ports;

/// Every port on the standard library, as the binary wires them.
#[derive(Debug, Default)]
pub struct HostPorts {
    files: StdFiles,
    process: StdProcess,
}

impl Ports for HostPorts {
    type Files = StdFiles;
    type Process = StdProcess;

    fn files(&self) -> &StdFiles {
        &self.files
    }

    fn process(&self) -> &StdProcess {
        &self.process
    }
}
