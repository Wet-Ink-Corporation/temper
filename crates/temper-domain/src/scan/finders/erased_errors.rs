//! `erased-error`: public signatures that return an erased error (1.x `erasedErrors`).

use std::borrow::Cow;

use regex::Regex;

use super::Found;
use crate::scan::env::Env;
use crate::scan::scanner::{ScannerSetupError, compile};

/// The compiled patterns: the signature, then the four erased forms in 1.x `PUB_SIG_BAD` order,
/// each with its hit message.
#[derive(Debug)]
pub(crate) struct ErasedPatterns {
    sig: Regex,
    bad: [(Regex, &'static str); 4],
}

impl ErasedPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            sig: compile(
                "erased:sig",
                r"\bpub\s+(?:const\s+)?(?:async\s+)?fn\s+\w+[^{;]*",
            )?,
            bad: [
                (
                    compile("erased:string", r"\bResult\s*<[^;{]*,\s*String\s*>")?,
                    "public library signature returns Result<_, String>",
                ),
                (
                    compile(
                        "erased:box-dyn",
                        r"\bBox\s*<\s*dyn\s+(?:std\s*::\s*error\s*::\s*|core\s*::\s*error\s*::\s*)?Error\b",
                    )?,
                    "public library signature returns Box<dyn Error>",
                ),
                (
                    compile("erased:anyhow", r"\banyhow\s*::\s*(?:Result|Error)\b")?,
                    "public library signature returns anyhow",
                ),
                (
                    compile("erased:eyre", r"\beyre\s*::\s*(?:Result|Report)\b")?,
                    "public library signature returns eyre",
                ),
            ],
        })
    }
}

/// Pushes one hit per erased form each `pub fn` signature (up to its `{` or `;`) has.
pub(crate) fn find(env: &Env<'_>, p: &ErasedPatterns, out: &mut Vec<Found>) {
    for sig in p.sig.find_iter(env.code) {
        let found = p
            .bad
            .iter()
            .filter(|(re, _)| re.is_match(sig.as_str()))
            .map(|&(_, msg)| Found {
                msg: Some(Cow::Borrowed(msg)),
                ..Found::at(sig.start())
            });
        out.extend(found);
    }
}
