//! temper's adapters to the standard library, implementing the ports `temper-app` defines: so
//! far the file system ([`StdFiles`]); processes with timeouts come in port Phase 1d.2b and the
//! clock in Phase 3.
//!
//! Adapters hold no policy and no message texts.

mod fs;

pub use fs::StdFiles;
use temper_app::Ports;

/// Every port on the standard library, as the binary wires them.
#[derive(Debug, Default)]
pub struct HostPorts {
    files: StdFiles,
}

impl Ports for HostPorts {
    type Files = StdFiles;

    fn files(&self) -> &StdFiles {
        &self.files
    }
}
