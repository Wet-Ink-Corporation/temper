//! temper's pure policy layer: the scanner, the manifest, config and verdict rules, the findings,
//! baseline and report models, and every message text the hooks print.
//!
//! Nothing here touches the file system, processes, the environment or the clock; the
//! application layer (`temper-app`) supplies those through ports. The crate is empty until the
//! port's Phase 1 brings the scanner (`docs/plans/temper-2-rust-port.md`).
