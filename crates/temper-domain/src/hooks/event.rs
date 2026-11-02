//! The hook events `temper hook <event>` answers.

use std::str::FromStr;

/// A hook event, by its 1.x name (`scripts/temper.mjs` `HOOKS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    /// `session-start`.
    SessionStart,
    /// `user-prompt`.
    UserPrompt,
    /// `pre-edit`.
    PreEdit,
    /// `pre-bash`.
    PreBash,
    /// `pre-handback`.
    PreHandback,
    /// `post-tool`.
    PostTool,
    /// `stop`.
    Stop,
    /// `subagent-stop`.
    SubagentStop,
    /// `subagent-start`.
    SubagentStart,
}

impl HookEvent {
    /// The event's name on the command line.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SessionStart => "session-start",
            Self::UserPrompt => "user-prompt",
            Self::PreEdit => "pre-edit",
            Self::PreBash => "pre-bash",
            Self::PreHandback => "pre-handback",
            Self::PostTool => "post-tool",
            Self::Stop => "stop",
            Self::SubagentStop => "subagent-stop",
            Self::SubagentStart => "subagent-start",
        }
    }
}

impl FromStr for HookEvent {
    type Err = UnknownHookEvent;

    /// One of the nine names, exactly; a missing name reads as JS's `undefined`.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(match name {
            "session-start" => Self::SessionStart,
            "user-prompt" => Self::UserPrompt,
            "pre-edit" => Self::PreEdit,
            "pre-bash" => Self::PreBash,
            "pre-handback" => Self::PreHandback,
            "post-tool" => Self::PostTool,
            "stop" => Self::Stop,
            "subagent-stop" => Self::SubagentStop,
            "subagent-start" => Self::SubagentStart,
            other => return Err(UnknownHookEvent(other.to_owned())),
        })
    }
}

/// A name that is not a hook event; its `Display` is 1.x's stderr line, without the newline.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("temper: unknown hook event {0}")]
pub struct UnknownHookEvent(String);

impl UnknownHookEvent {
    /// The name as given.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }
}
