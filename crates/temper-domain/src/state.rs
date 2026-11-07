//! The state files temper reads, with 1.x's JS semantics: each reader is a `JsValue` newtype
//! with one accessor per 1.x read, not a DTO, since 1.x reads these files with JS property
//! semantics (port plan 3-14, the OUT-05 exception of 2-10 A).

mod baseline;
mod gate;
mod progress;

pub use baseline::{BaselineParseError, BaselineSummary};
pub use gate::GateRecord;
pub use progress::{ProgressQuery, gate_progress};
