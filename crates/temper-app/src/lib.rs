//! temper's application layer: the use cases behind each subcommand and hook handler, and the
//! ports (files, processes, clock) they reach the outside world through.
//!
//! It depends only on `temper-domain`; `temper-host` implements the ports. So far it has
//! `temper scan --all` and `temper scan FILES..` ([`Engine::scan`], port Phase 1d.2), and the
//! `post-tool` and `user-prompt` hooks ([`Engine::hook`], port Phase 2b) with their skill
//! markers and telemetry, and the `pre-edit` hook (port Phase 2c) with the manifests' workspace
//! roots, over the [`Files`], [`ProcessRunner`] and [`Clock`] ports, with 1.x's project
//! discovery (the git top level first, the state dir, the config) and path rules.

mod changes;
mod engine;
#[cfg(test)]
mod fakes;
mod git;
mod hooks;
mod manifests;
mod paths;
mod ports;
mod procs;
mod project;
mod relpath;
mod retention;
mod scan;
mod session;
mod state;
mod status;
mod telemetry;
mod trees;

pub use engine::{
    CommandError, CommandOutput, Engine, Exit, HookError, OutputFormat, RunEnv, ScanArgs,
    ScanTargets, StatusArgs,
};
pub use hooks::{HookInput, HookInputError};
pub use ports::{
    ByteCap, Clock, DirEntry, EntryKind, ExitKind, FileMeta, Files, FsError, FsOp, Ports,
    ProcessError, ProcessOutput, ProcessRequest, ProcessRunner, ReadInto, StderrSink, StdoutSink,
    Stream,
};
