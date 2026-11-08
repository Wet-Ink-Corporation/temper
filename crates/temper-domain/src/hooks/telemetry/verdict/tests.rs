//! The verdict rows `preHandback` and `subagentStop` write, with their detail in 1.x's order.

use super::super::{TelemetryEvent, TelemetryRow};
use super::{InvalidVerdict, VerdictRow, VerdictVia};
use crate::time::Timestamp;
use crate::verdict::{ProblemKind, VerdictOutcome};

fn row(event: &TelemetryEvent) -> String {
    TelemetryRow {
        at: Timestamp::from_unix_millis(1_730_000_000_123),
        sid: None,
        ccr: None,
        agent: None,
        event,
    }
    .to_json()
}

const HEAD: &str = r#"{"t":"2024-10-27T03:33:20.123Z","agent":"main","event":"#;

#[test]
fn the_handback_and_verdict_rows_are_1x_s() {
    let invalid = |agent_id: Option<&str>| InvalidVerdict {
        problems: 3,
        kinds: vec![ProblemKind::SchemaVersion, ProblemKind::Fingerprint],
        agent_id: agent_id.map(str::to_owned),
    };
    let verdict = |kept: Option<&str>, agent_id: Option<&str>| {
        TelemetryEvent::Verdict(VerdictRow {
            via: VerdictVia::Handback,
            agent_id: agent_id.map(str::to_owned),
            outcome: VerdictOutcome::ChangesRequested,
            fingerprint: "0123456789abcdef".parse().expect("16 hex"),
            stale: true,
            findings: 2,
            kept: kept.map(str::to_owned),
        })
    };
    let events = [
        TelemetryEvent::Handback {
            agent_type: r#""temper:rust-reviewer""#.to_owned(),
            reviewer: true,
        },
        TelemetryEvent::Handback {
            agent_type: "5".to_owned(),
            reviewer: false,
        },
        TelemetryEvent::VerdictInvalid {
            via: VerdictVia::Handback,
            invalid: invalid(Some(r#""rv1""#)),
        },
        TelemetryEvent::VerdictInvalid {
            via: VerdictVia::SubagentStop,
            invalid: invalid(None),
        },
        TelemetryEvent::VerdictInvalidFinal(invalid(Some("null"))),
        verdict(None, Some(r#""rv1""#)),
        verdict(Some(r#""approved""#), None),
        TelemetryEvent::SubagentStop {
            agent_type: r#""""#.to_owned(),
            msg: true,
            transcript: false,
        },
    ];
    let rows: Vec<String> = events.iter().map(row).collect();
    assert_eq!(
        rows,
        [
            format!(r#"{HEAD}"handback","type":"temper:rust-reviewer","reviewer":true}}"#),
            format!(r#"{HEAD}"handback","type":5,"reviewer":false}}"#),
            format!(
                r#"{HEAD}"verdict-invalid","via":"handback","problems":3,"kinds":["schema-version","fingerprint"],"agent_id":"rv1"}}"#
            ),
            format!(
                r#"{HEAD}"verdict-invalid","via":"subagent-stop","problems":3,"kinds":["schema-version","fingerprint"]}}"#
            ),
            format!(
                r#"{HEAD}"verdict-invalid-final","problems":3,"kinds":["schema-version","fingerprint"],"agent_id":null}}"#
            ),
            format!(
                r#"{HEAD}"verdict","via":"handback","agent_id":"rv1","verdict":"changes-requested","fp":"0123456789abcdef","stale":true,"findings":2}}"#
            ),
            format!(
                r#"{HEAD}"verdict","via":"handback","verdict":"changes-requested","fp":"0123456789abcdef","stale":true,"findings":2,"kept":"approved"}}"#
            ),
            format!(r#"{HEAD}"subagent-stop","type":"","msg":true,"transcript":false}}"#),
        ],
        "hooks.mjs preHandback's, storeVerdict's and subagentStop's telemetry() calls"
    );
}
