//! Compiling the hooks' built-in patterns: 1.x JS regex sources, translated once per hook run.

use regex::Regex;

use crate::js::{self, JsFlags, UnsupportedJs};

/// A built-in hook pattern did not compile. This is a bug in temper; a unit test guards it.
#[derive(Debug, thiserror::Error)]
#[error("hook pattern `{rule}` does not compile")]
pub struct PolicySetupError {
    rule: &'static str,
    #[source]
    source: SetupCause,
}

/// Why a built-in pattern failed: its JS source has no faithful translation, or `regex` rejects
/// the translation.
#[derive(Debug, thiserror::Error)]
enum SetupCause {
    #[error(transparent)]
    Js(#[from] UnsupportedJs),
    #[error(transparent)]
    Regex(#[from] regex::Error),
}

impl PolicySetupError {
    /// The name of the pattern that failed, such as `redact:url-userinfo`.
    #[must_use]
    pub fn rule(&self) -> &'static str {
        self.rule
    }
}

/// Compiles the JS regex source `js_source` with `flags`, named `rule` in the error.
pub(crate) fn compile(
    rule: &'static str,
    js_source: &str,
    flags: JsFlags,
) -> Result<Regex, PolicySetupError> {
    let compiled = js::translate(js_source, flags)
        .map_err(SetupCause::from)
        .and_then(|translated| Regex::new(&translated).map_err(SetupCause::from));
    compiled.map_err(|source| PolicySetupError { rule, source })
}
