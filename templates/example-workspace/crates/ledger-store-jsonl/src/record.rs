//! The record format: one JSON object per line, tagged with `schema_version`.
//!
//! Current (v2) shape, with fields in this order:
//!
//! ```text
//! {"schema_version":2,"recorded_at_ms":1767225600000,"event":{"type":"deposited","amount_cents":1250}}
//! ```

mod v1;

use std::borrow::Cow;
use std::io::{self, Write};

use ledger_app::{Recorded, Timestamp};
use ledger_domain::{Cents, Event, ParseOwnerNameError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use self::v1::RecordV1;

/// Schema version this build writes. Raise it only together with a new
/// frozen version module and an upcaster from the previous shape: readers
/// refuse any version they do not know.
const CURRENT_SCHEMA: u32 = 2;

/// Why a stored line is not a valid event record.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RecordError {
    /// Not a JSON object, or not the shape its schema version requires.
    #[error("record does not match its schema")]
    Malformed(#[source] serde_json::Error),
    /// The record has no `schema_version` field.
    #[error("record has no schema_version")]
    MissingSchema,
    /// Written by a newer build, or never a valid version.
    #[error("unsupported schema version {found}")]
    UnsupportedSchema {
        /// The version found in the record.
        found: u32,
    },
    /// The stored owner name fails domain validation.
    #[error("stored owner name is invalid")]
    InvalidOwner(#[source] ParseOwnerNameError),
}

// Compile-time proof that this type can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RecordError>();
};

/// Schema v2 wire shape, minus the `schema_version` tag. Borrows on write,
/// owns on read.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct RecordV2<'a> {
    recorded_at_ms: u64,
    event: EventV2<'a>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum EventV2<'a> {
    Opened { owner: Cow<'a, str> },
    Deposited { amount_cents: u64 },
    Withdrew { amount_cents: u64 },
    // A unit struct in a newtype variant, not a unit variant: serde ignores
    // `deny_unknown_fields` on unit variants of a tagged enum, but a unit
    // struct refuses any field. Both forms write `{"type":"closed"}`.
    Closed(ClosedV2),
}

/// The body of a `closed` event: no fields.
#[derive(Debug, Serialize, Deserialize)]
struct ClosedV2;

/// The tag written before the body, so it leads every line.
#[derive(Serialize)]
struct Tagged<'a> {
    schema_version: u32,
    #[serde(flatten)]
    record: RecordV2<'a>,
}

impl<'a> From<&'a Recorded> for RecordV2<'a> {
    fn from(recorded: &'a Recorded) -> Self {
        let event = match &recorded.event {
            Event::Opened { owner } => EventV2::Opened {
                owner: Cow::Borrowed(owner.as_str()),
            },
            Event::Deposited { amount } => EventV2::Deposited {
                amount_cents: amount.get(),
            },
            Event::Withdrew { amount } => EventV2::Withdrew {
                amount_cents: amount.get(),
            },
            Event::Closed => EventV2::Closed(ClosedV2),
        };
        Self {
            recorded_at_ms: recorded.recorded_at.as_unix_millis(),
            event,
        }
    }
}

impl TryFrom<RecordV2<'_>> for Recorded {
    type Error = RecordError;

    fn try_from(record: RecordV2<'_>) -> Result<Self, RecordError> {
        let event = match record.event {
            EventV2::Opened { owner } => Event::Opened {
                owner: owner.parse().map_err(RecordError::InvalidOwner)?,
            },
            EventV2::Deposited { amount_cents } => Event::Deposited {
                amount: Cents::new(amount_cents),
            },
            EventV2::Withdrew { amount_cents } => Event::Withdrew {
                amount: Cents::new(amount_cents),
            },
            EventV2::Closed(ClosedV2) => Event::Closed,
        };
        Ok(Self {
            event,
            recorded_at: Timestamp::from_unix_millis(record.recorded_at_ms),
        })
    }
}

/// Write `recorded` as one current-schema line, newline included. Pass
/// `&mut out` to keep using the writer.
pub(crate) fn encode(recorded: &Recorded, mut out: impl Write) -> io::Result<()> {
    let line = Tagged {
        schema_version: CURRENT_SCHEMA,
        record: RecordV2::from(recorded),
    };
    serde_json::to_writer(&mut out, &line)?;
    out.write_all(b"\n")
}

/// Read one line of any supported schema into a validated domain record.
pub(crate) fn decode(line: &[u8]) -> Result<Recorded, RecordError> {
    let mut fields: Map<String, Value> =
        serde_json::from_slice(line).map_err(RecordError::Malformed)?;
    let schema = fields
        .remove("schema_version")
        .ok_or(RecordError::MissingSchema)?;
    let schema: u32 = serde_json::from_value(schema).map_err(RecordError::Malformed)?;
    let body = Value::Object(fields);
    let record = match schema {
        1 => upcast_v1(serde_json::from_value(body).map_err(RecordError::Malformed)?),
        CURRENT_SCHEMA => serde_json::from_value(body).map_err(RecordError::Malformed)?,
        found => return Err(RecordError::UnsupportedSchema { found }),
    };
    Recorded::try_from(record)
}

/// Lift a v1 record to the current shape, so only one shape reaches the domain.
fn upcast_v1(record: RecordV1) -> RecordV2<'static> {
    let (recorded_at_ms, event) = match record {
        RecordV1::Opened { at, name } => (
            at,
            EventV2::Opened {
                owner: Cow::Owned(name),
            },
        ),
        RecordV1::Deposited { at, cents } => (
            at,
            EventV2::Deposited {
                amount_cents: cents,
            },
        ),
        RecordV1::Withdrawn { at, cents } => (
            at,
            EventV2::Withdrew {
                amount_cents: cents,
            },
        ),
        RecordV1::Closed { at } => (at, EventV2::Closed(ClosedV2)),
    };
    RecordV2 {
        recorded_at_ms,
        event,
    }
}
