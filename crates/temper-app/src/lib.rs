//! temper's application layer: the use cases behind each subcommand and hook handler, and the
//! ports (files, processes, clock) they reach the outside world through.
//!
//! It depends only on `temper-domain`; `temper-host` implements the ports. The crate is empty
//! until the port's Phase 1 (`docs/plans/temper-2-rust-port.md`).
