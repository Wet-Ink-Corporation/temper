//! Telemetry rows, one JSON line each, as 1.x `project.mjs` `telemetry()` writes them.

use std::fmt::Write as _;

use super::SkillVia;
use crate::js::{Value, js_json, json_string};
use crate::time::Timestamp;

/// What a telemetry row records. Each variant arrives with the handler that emits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelemetryEvent {
    /// `skill-loaded`: the rust-craft skill was loaded in an agent context.
    SkillLoaded {
        /// How.
        via: SkillVia,
    },
}

impl TelemetryEvent {
    const fn name(&self) -> &'static str {
        match self {
            Self::SkillLoaded { .. } => "skill-loaded",
        }
    }

    /// Appends the detail properties, each as `,"key":value`, in 1.x's order.
    fn push_detail(&self, out: &mut String) {
        match self {
            Self::SkillLoaded { via } => {
                out.push_str(r#","via":"#);
                out.push_str(&json_string(via.name()));
            }
        }
    }
}

/// One telemetry row: `{t, sid?, ccr?, agent, event, ...detail}`.
#[derive(Debug, Clone, Copy)]
pub struct TelemetryRow<'a> {
    /// When (`t`).
    pub at: Timestamp,
    /// The input's `session_id` as given, `null` included; `None` leaves `sid` out.
    pub sid: Option<&'a Value>,
    /// The Claude Code cloud session (`CLAUDE_CODE_REMOTE_SESSION_ID`); `None` leaves `ccr` out.
    pub ccr: Option<&'a str>,
    /// The input's `agent_type` unless it is missing or `null`; `None` writes `"main"`.
    pub agent: Option<&'a Value>,
    /// What happened.
    pub event: &'a TelemetryEvent,
}

impl TelemetryRow<'_> {
    /// The row as `JSON.stringify` writes it, without the newline.
    #[must_use]
    pub fn to_json(self) -> String {
        let mut out = String::from(r#"{"t":"#);
        // Writing to a `String` cannot fail.
        if write!(out, r#""{}""#, self.at).is_err() {
            out.push_str("null");
        }
        if let Some(sid) = self.sid {
            out.push_str(r#","sid":"#);
            out.push_str(&js_json(sid));
        }
        if let Some(ccr) = self.ccr {
            out.push_str(r#","ccr":"#);
            out.push_str(&json_string(ccr));
        }
        out.push_str(r#","agent":"#);
        match self.agent {
            Some(agent) => out.push_str(&js_json(agent)),
            None => out.push_str(r#""main""#),
        }
        out.push_str(r#","event":"#);
        out.push_str(&json_string(self.event.name()));
        self.event.push_detail(&mut out);
        out.push('}');
        out
    }
}
