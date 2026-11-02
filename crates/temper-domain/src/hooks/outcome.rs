//! What a hook answers, and its JSON on stdout (1.x `hooks.mjs` `ctx`, `notice` and `none`).

use super::HookEvent;
use crate::js::json_string;

/// The event a context answer is addressed to (`hookSpecificOutput.hookEventName`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextEvent {
    /// `UserPromptSubmit`.
    UserPromptSubmit,
}

impl ContextEvent {
    const fn name(self) -> &'static str {
        match self {
            Self::UserPromptSubmit => "UserPromptSubmit",
        }
    }
}

/// A hook's answer. The deny, ask and block shapes arrive with the handlers that give them.
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
    /// A message for the human (`systemMessage`).
    Notice {
        /// The message.
        message: String,
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
            Self::Notice { message } => {
                Some(format!(r#"{{"systemMessage":{}}}"#, json_string(message)))
            }
        }
    }
}
