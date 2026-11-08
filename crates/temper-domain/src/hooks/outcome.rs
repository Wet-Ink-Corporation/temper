//! What a hook answers, and its JSON on stdout (1.x `hooks.mjs` `ctx`, `deny`, `ask`, `notice`
//! and `none`).

use super::HookEvent;
use crate::js::json_string;

/// The event a context answer is addressed to (`hookSpecificOutput.hookEventName`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextEvent {
    /// `UserPromptSubmit`.
    UserPromptSubmit,
    /// `PreToolUse`.
    PreToolUse,
    /// `SessionStart`.
    SessionStart,
}

impl ContextEvent {
    const fn name(self) -> &'static str {
        match self {
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PreToolUse => "PreToolUse",
            Self::SessionStart => "SessionStart",
        }
    }
}

/// A hook's answer. The block shape arrives with `stop` (port Phase 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    /// No output: the action proceeds unremarked.
    Pass,
    /// Text for the model's context.
    Context {
        /// The event answered.
        event: ContextEvent,
        /// The text.
        text: String,
    },
    /// The tool call is refused (`PreToolUse`, `permissionDecision: "deny"`).
    Deny {
        /// Why, for the model.
        reason: String,
        /// More text for the model's context; `None` or empty leaves it out.
        context: Option<String>,
    },
    /// The human is asked to approve the tool call (`permissionDecision: "ask"`).
    Ask {
        /// What needs approval.
        reason: String,
        /// More text for the model's context; `None` or empty leaves it out.
        context: Option<String>,
    },
    /// A message for the human (`systemMessage`).
    Notice {
        /// The message.
        message: String,
    },
    /// Text for the model's context and a message for the human, as 1.x's
    /// `withRuntimeWarnings` spreads them: `hookSpecificOutput` first, then `systemMessage`.
    ContextWithNotice {
        /// The event answered.
        event: ContextEvent,
        /// The text.
        text: String,
        /// The message.
        notice: String,
    },
}

impl HookOutcome {
    /// 1.x's answer to hook input that is not JSON: nothing was checked.
    #[must_use]
    pub fn input_unparseable(event: HookEvent) -> Self {
        Self::Notice {
            message: format!(
                "temper hook {}: could not parse the hook input as JSON; nothing was checked",
                event.name()
            ),
        }
    }

    /// 1.x's answer when the handler fails: report it and let the action proceed.
    #[must_use]
    pub fn handler_failed(event: HookEvent, message: &str) -> Self {
        Self::Notice {
            message: format!("temper hook {} failed: {message}", event.name()),
        }
    }

    /// The compact JSON 1.x writes on stdout, without a newline; `None` for [`Self::Pass`].
    ///
    /// # Examples
    ///
    /// ```
    /// use temper_domain::HookOutcome;
    ///
    /// let notice = HookOutcome::Notice { message: "hi".to_owned() };
    /// assert_eq!(notice.to_json().as_deref(), Some(r#"{"systemMessage":"hi"}"#));
    /// assert_eq!(HookOutcome::Pass.to_json(), None);
    /// ```
    #[must_use]
    pub fn to_json(&self) -> Option<String> {
        match self {
            Self::Pass => None,
            Self::Context { event, text } => Some(format!(
                r#"{{"hookSpecificOutput":{{"hookEventName":"{}","additionalContext":{}}}}}"#,
                event.name(),
                json_string(text)
            )),
            Self::ContextWithNotice {
                event,
                text,
                notice,
            } => Some(format!(
                r#"{{"hookSpecificOutput":{{"hookEventName":"{}","additionalContext":{}}},"systemMessage":{}}}"#,
                event.name(),
                json_string(text),
                json_string(notice)
            )),
            Self::Deny { reason, context } => Some(decision("deny", reason, context.as_deref())),
            Self::Ask { reason, context } => Some(decision("ask", reason, context.as_deref())),
            Self::Notice { message } => {
                Some(format!(r#"{{"systemMessage":{}}}"#, json_string(message)))
            }
        }
    }
}

/// 1.x `deny` and `ask`: the decision, its reason, and the context only when it is not empty
/// (`...(extraContext ? { additionalContext } : {})`).
fn decision(kind: &str, reason: &str, context: Option<&str>) -> String {
    let context = match context {
        Some(text) if !text.is_empty() => format!(r#","additionalContext":{}"#, json_string(text)),
        Some(_) | None => String::new(),
    };
    format!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"{kind}","permissionDecisionReason":{}{context}}}}}"#,
        json_string(reason)
    )
}
