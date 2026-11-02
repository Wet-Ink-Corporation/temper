//! temper's adapters to the standard library, implementing the ports `temper-app` defines: the
//! file system ([`StdFiles`]), child processes with timeouts ([`StdProcess`]) and the clock
//! ([`SystemClock`]).
//!
//! Adapters hold no policy and no message texts.

mod child;
mod clock;
mod fs;

pub use child::StdProcess;
pub use clock::SystemClock;
pub use fs::StdFiles;
use temper_app::Ports;

/// Every port on the standard library, as the binary wires them.
#[derive(Debug, Default)]
pub struct HostPorts {
    files: StdFiles,
    process: StdProcess,
    clock: SystemClock,
}

impl Ports for HostPorts {
    type Files = StdFiles;
    type Process = StdProcess;
    type Clock = SystemClock;

    fn files(&self) -> &StdFiles {
        &self.files
    }

    fn process(&self) -> &StdProcess {
        &self.process
    }

    fn clock(&self) -> &SystemClock {
        &self.clock
    }
}
