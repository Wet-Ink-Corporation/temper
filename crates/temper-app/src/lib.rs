//! temper's application layer: the use cases behind each subcommand and hook handler, and the
//! ports (files, processes, clock) they reach the outside world through.
//!
//! It depends only on `temper-domain`; `temper-host` implements the ports. So far it has
//! `temper scan --all` and `temper scan FILES..` ([`Engine::scan`], port Phase 1d.2) over the
//! [`Files`] and [`ProcessRunner`] ports, with 1.x's project discovery (the git top level
//! first) and path rules.

mod engine;
#[cfg(test)]
mod fakes;
mod git;
mod paths;
mod ports;
mod project;
mod relpath;
mod scan;

pub use engine::{
    CommandError, CommandOutput, Engine, Exit, OutputFormat, RunEnv, ScanArgs, ScanTargets,
};
pub use ports::{
    ByteCap, DirEntry, EntryKind, ExitKind, Files, FsError, FsOp, Ports, ProcessError,
    ProcessOutput, ProcessRequest, ProcessRunner, Stream,
};
