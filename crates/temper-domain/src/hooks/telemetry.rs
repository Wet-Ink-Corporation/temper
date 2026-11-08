//! Telemetry rows, one JSON line each, as 1.x `project.mjs` `telemetry()` writes them.

use std::fmt::Write as _;

mod redact;
mod verdict;

pub(crate) use redact::Redactor;
pub use verdict::{InvalidVerdict, VerdictRow, VerdictVia};

use super::{LostGate, RaiseFlag, SkillVia};
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
    /// `deny-lockfile`: an Edit or Write of a `Cargo.lock` was refused.
    DenyLockfile {
        /// The lockfile, relative to the project root.
        file: String,
    },
    /// `deny-skill-gate`: a Rust edit was refused because the skill is not loaded.
    DenySkillGate {
        /// The edited file, relative to the project root.
        file: String,
    },
    /// `deny-manifest`: a `Cargo.toml` edit broke workspace rules.
    DenyManifest {
        /// The manifest, relative to the project root.
        file: String,
        /// The first word of each denial: its rule id.
        rules: Vec<String>,
    },
    /// `deny-scan`: a Rust edit introduced escape hatches.
    DenyScan {
        /// The edited file, relative to the project root.
        file: String,
        /// The first word of each denial: its rule id.
        rules: Vec<String>,
    },
    /// `ask`: an edit needs the human's approval.
    Ask {
        /// The edited file, relative to the project root.
        file: String,
        /// How many things need approval.
        count: usize,
    },
    /// `deny-shell-write`: a shell command writing a file the shell must not write.
    DenyShellWrite {
        /// Which file.
        file: ShellWriteFile,
        /// The command, redacted and clipped.
        cmd: String,
    },
    /// `deny-shell-write` with a `via`: a copy or patch onto `.rs` files the scan refuses, or
    /// the reviewer's patch denials.
    DenyShellWriteVia {
        /// Why.
        via: ShellDenyVia,
        /// The command, redacted and clipped; the reviewer's rows have none.
        cmd: Option<String>,
    },
    /// `deny-scan` with `via: "shell-copy"`: a copy onto `.rs` files adds escape hatches.
    DenyShellCopyScan {
        /// The first word of each denial: its rule id.
        rules: Vec<String>,
        /// The command, redacted and clipped.
        cmd: String,
    },
    /// `ask` with `via: "shell-copy"`: a copy onto `.rs` files needs approval.
    ShellCopyAsk {
        /// How many things need approval.
        count: usize,
    },
    /// `deny-lockfile` with `via: "shell"`: a shell write to `Cargo.lock`.
    DenyShellLockfile {
        /// The command, redacted and clipped.
        cmd: String,
    },
    /// `deny-baseline-raise`: `temper baseline --init` or `--update` was refused.
    DenyBaselineRaise {
        /// The flag (`via` is `baseline <flag>`).
        flag: RaiseFlag,
        /// The command, redacted and clipped.
        cmd: String,
    },
    /// `deny-ignore-rust-version`: a cargo command with `--ignore-rust-version`.
    DenyIgnoreRustVersion {
        /// The command, redacted and clipped.
        cmd: String,
    },
    /// `ask` from `pre-bash`: a shell command needs the human's approval.
    ShellAsk {
        /// Why.
        via: ShellAskVia,
    },
    /// `ask` from `pre-bash` for protected configuration a shell command changes.
    ProtectedAsk {
        /// The first change.
        change: ProtectedChange,
    },
    /// `gate-lost`: a gate run of this session whose hook was killed (from `SessionStart`).
    GateLost(LostGate),
    /// `session-start`: a session started, resumed, was cleared, forked or compacted.
    SessionStart {
        /// The input's `source` as `JSON.stringify` writes it, any JSON value; `None` when
        /// the input has none, which leaves the key out as `undefined` does.
        source: Option<String>,
    },
    /// `subagent-start`: a subagent started, of any type, before the hook filters by type.
    SubagentStart {
        /// The input's `agent_type ?? ''` as `JSON.stringify` writes it.
        agent_type: String,
    },
    /// `handback`: a subagent called `SubagentHandback`, of any type, before the hook filters by
    /// type.
    Handback {
        /// The input's `agent_type ?? ''` as `JSON.stringify` writes it.
        agent_type: String,
        /// Whether that is temper's reviewer.
        reviewer: bool,
    },
    /// `verdict`: a valid review verdict was recorded.
    Verdict(VerdictRow),
    /// `verdict-invalid`: a review verdict was refused.
    VerdictInvalid {
        /// Which hook refused it.
        via: VerdictVia,
        /// What was wrong.
        invalid: InvalidVerdict,
    },
    /// `verdict-invalid-final`: an invalid verdict let through on the `stop_hook_active` repeat.
    VerdictInvalidFinal(InvalidVerdict),
    /// `subagent-stop`: a subagent stopped, of any type, before the hook filters by type.
    SubagentStop {
        /// The input's `agent_type ?? ''` as `JSON.stringify` writes it.
        agent_type: String,
        /// Whether `last_assistant_message` is a string.
        msg: bool,
        /// Whether `agent_transcript_path` is truthy.
        transcript: bool,
    },
}

/// The change a protected-configuration `ask` row names (its `via` and `file`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectedChange {
    /// `shell-write-protected`, with the first file written.
    Written(String),
    /// `shell-delete-protected`, with the first file deleted.
    Deleted(String),
    /// `shell-patch-unread`: a patch temper cannot read.
    UnreadPatch,
}

/// What a `deny-shell-write` row says was written (its `file`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellWriteFile {
    /// A `.rs` file; the row has no `file`.
    RustSource,
    /// `Cargo.toml`.
    Manifest,
    /// `temper-baseline.json`.
    Baseline,
    /// `.claude/temper/`, the vendored CLI.
    Vendored,
    /// temper's state dir.
    StateDir,
}

impl ShellWriteFile {
    /// The row's `file`, if it has one.
    const fn name(self) -> Option<&'static str> {
        match self {
            Self::RustSource => None,
            Self::Manifest => Some("Cargo.toml"),
            Self::Baseline => Some("temper-baseline.json"),
            Self::Vendored => Some(".claude/temper/"),
            Self::StateDir => Some("state-dir"),
        }
    }
}

/// Why a `deny-shell-write` row with a `via` was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellDenyVia {
    /// `shell-reader`: a write into a `.rs` file the shell reader finds (`x.r\s`, `curl -o`).
    ShellReader,
    /// `patch-unread`: a patch temper cannot read.
    PatchUnread,
    /// `copy-unread`: a copy onto `.rs` files whose content temper cannot read.
    CopyUnread,
    /// `patch`: a patch that changes Rust sources, with how many (`files`).
    Patch {
        /// How many Rust sources it changes.
        files: usize,
    },
    /// `reviewer-patch`: the reviewer applies a patch.
    ReviewerPatch,
    /// `reviewer-patch-opaque`: the reviewer runs a command temper cannot rule out applies one.
    ReviewerOpaque,
}

impl ShellDenyVia {
    const fn name(self) -> &'static str {
        match self {
            Self::ShellReader => "shell-reader",
            Self::PatchUnread => "patch-unread",
            Self::CopyUnread => "copy-unread",
            Self::Patch { .. } => "patch",
            Self::ReviewerPatch => "reviewer-patch",
            Self::ReviewerOpaque => "reviewer-patch-opaque",
        }
    }
}

/// Why `pre-bash` asked (an `ask` row's `via`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellAskVia {
    /// `TEMPER_ALLOW_RAISE`.
    AllowRaise,
    /// `git-restore-claude`.
    ClaudeRestore,
    /// `cargo-deps`.
    CargoDeps,
}

impl ShellAskVia {
    const fn name(self) -> &'static str {
        match self {
            Self::AllowRaise => "TEMPER_ALLOW_RAISE",
            Self::ClaudeRestore => "git-restore-claude",
            Self::CargoDeps => "cargo-deps",
        }
    }
}

impl TelemetryEvent {
    const fn name(&self) -> &'static str {
        match self {
            Self::SkillLoaded { .. } => "skill-loaded",
            Self::DenyLockfile { .. } | Self::DenyShellLockfile { .. } => "deny-lockfile",
            Self::DenySkillGate { .. } => "deny-skill-gate",
            Self::DenyManifest { .. } => "deny-manifest",
            Self::DenyScan { .. } | Self::DenyShellCopyScan { .. } => "deny-scan",
            Self::Ask { .. }
            | Self::ShellAsk { .. }
            | Self::ProtectedAsk { .. }
            | Self::ShellCopyAsk { .. } => "ask",
            Self::DenyShellWrite { .. } | Self::DenyShellWriteVia { .. } => "deny-shell-write",
            Self::DenyBaselineRaise { .. } => "deny-baseline-raise",
            Self::DenyIgnoreRustVersion { .. } => "deny-ignore-rust-version",
            Self::GateLost(_) => "gate-lost",
            Self::SessionStart { .. } => "session-start",
            Self::SubagentStart { .. } => "subagent-start",
            Self::Handback { .. } => "handback",
            Self::Verdict(_) => "verdict",
            Self::VerdictInvalid { .. } => "verdict-invalid",
            Self::VerdictInvalidFinal(_) => "verdict-invalid-final",
            Self::SubagentStop { .. } => "subagent-stop",
        }
    }

    /// Appends the detail properties, each as `,"key":value`, in 1.x's order.
    fn push_detail(&self, out: &mut String) {
        match self {
            Self::SkillLoaded { via } => push_field(out, "via", &json_string(via.name())),
            Self::DenyLockfile { file } | Self::DenySkillGate { file } => {
                push_field(out, "file", &json_string(file));
            }
            Self::DenyManifest { file, rules } | Self::DenyScan { file, rules } => {
                push_field(out, "file", &json_string(file));
                push_rules(out, rules);
            }
            Self::Ask { file, count } => {
                push_field(out, "file", &json_string(file));
                push_field(out, "count", &count.to_string());
            }
            Self::DenyShellWrite { file, cmd } => {
                if let Some(file) = file.name() {
                    push_field(out, "file", &json_string(file));
                }
                push_field(out, "cmd", &json_string(cmd));
            }
            Self::DenyShellWriteVia { via, cmd } => push_via(out, *via, cmd.as_deref()),
            Self::DenyShellCopyScan { rules, cmd } => {
                push_field(out, "via", r#""shell-copy""#);
                push_rules(out, rules);
                push_field(out, "cmd", &json_string(cmd));
            }
            Self::ShellCopyAsk { count } => {
                push_field(out, "via", r#""shell-copy""#);
                push_field(out, "count", &count.to_string());
            }
            Self::DenyShellLockfile { cmd } => {
                push_field(out, "via", r#""shell""#);
                push_field(out, "cmd", &json_string(cmd));
            }
            Self::DenyBaselineRaise { flag, cmd } => {
                push_field(
                    out,
                    "via",
                    &json_string(&format!("baseline {}", flag.flag())),
                );
                push_field(out, "cmd", &json_string(cmd));
            }
            Self::DenyIgnoreRustVersion { cmd } => push_field(out, "cmd", &json_string(cmd)),
            Self::ShellAsk { via } => push_field(out, "via", &json_string(via.name())),
            Self::ProtectedAsk { change } => push_protected(out, change),
            Self::GateLost(lost) => {
                for (key, value) in lost.detail() {
                    push_field(out, key, &js_json(value));
                }
            }
            Self::SessionStart { source } => {
                if let Some(source) = source {
                    push_field(out, "source", source);
                }
            }
            Self::SubagentStart { agent_type } => push_field(out, "type", agent_type),
            Self::Handback {
                agent_type,
                reviewer,
            } => {
                push_field(out, "type", agent_type);
                push_field(out, "reviewer", js_bool(*reviewer));
            }
            Self::Verdict(row) => verdict::push_verdict(out, row),
            Self::VerdictInvalid { via, invalid } => {
                verdict::push_invalid(out, Some(*via), invalid)
            }
            Self::VerdictInvalidFinal(invalid) => verdict::push_invalid(out, None, invalid),
            Self::SubagentStop {
                agent_type,
                msg,
                transcript,
            } => {
                push_field(out, "type", agent_type);
                push_field(out, "msg", js_bool(*msg));
                push_field(out, "transcript", js_bool(*transcript));
            }
        }
    }
}

/// Appends a protected-configuration `ask` row's `via` and its file, if any.
fn push_protected(out: &mut String, change: &ProtectedChange) {
    let (via, file) = match change {
        ProtectedChange::Written(file) => ("shell-write-protected", Some(file)),
        ProtectedChange::Deleted(file) => ("shell-delete-protected", Some(file)),
        ProtectedChange::UnreadPatch => ("shell-patch-unread", None),
    };
    push_field(out, "via", &json_string(via));
    if let Some(file) = file {
        push_field(out, "file", &json_string(file));
    }
}

/// Appends `,"rules":[...]`.
fn push_rules(out: &mut String, rules: &[String]) {
    let rules: Vec<String> = rules.iter().map(|rule| json_string(rule)).collect();
    push_field(out, "rules", &format!("[{}]", rules.join(",")));
}

/// Appends a `deny-shell-write` row's `via`, a patch's `files` and the command, if any.
fn push_via(out: &mut String, via: ShellDenyVia, cmd: Option<&str>) {
    push_field(out, "via", &json_string(via.name()));
    if let ShellDenyVia::Patch { files } = via {
        push_field(out, "files", &files.to_string());
    }
    if let Some(cmd) = cmd {
        push_field(out, "cmd", &json_string(cmd));
    }
}

/// `JSON.stringify` of a boolean.
const fn js_bool(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

/// Appends `,"key":json`.
fn push_field(out: &mut String, key: &str, json: &str) {
    out.push_str(",\"");
    out.push_str(key);
    out.push_str("\":");
    out.push_str(json);
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
