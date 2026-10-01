//! Schema v1, frozen. Never edit: it describes bytes already on disk.
//!
//! v1 wrote a flat record tagged by `kind`, named the owner `name`, stored
//! amounts as `cents` and called withdrawals `withdrawn`:
//!
//! ```text
//! {"schema_version":1,"kind":"withdrawn","at":1700000000000,"cents":250}
//! ```

use serde::Deserialize;

/// The v1 wire shape, minus the `schema_version` tag.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum RecordV1 {
    Opened { at: u64, name: String },
    Deposited { at: u64, cents: u64 },
    Withdrawn { at: u64, cents: u64 },
    Closed { at: u64 },
}
