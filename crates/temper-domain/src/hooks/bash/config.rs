//! The protected-configuration ask of `pre-bash` (1.x `preBash`'s `configAsks`): what the
//! application's shell reader found a command writes, deletes or patches unread.

use super::super::{HookOutcome, PolicyDecision, ProtectedChange, TelemetryEvent};
use crate::js::truncate_utf16;

/// How much of an unread patch command the ask quotes, in UTF-16 units (1.x `slice(0, 120)`).
const PATCH_UNITS: usize = 120;

/// What a shell command changes of the protected configuration (`config.protected`), as the
/// application reads it (1.x `protectedShellWrites`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfigChanges<'a> {
    /// The project-relative files and globs it writes, in the order found.
    pub written: &'a [String],
    /// Those it deletes.
    pub deleted: &'a [String],
    /// The first patch command whose patch temper cannot read, as written.
    pub unread_patch: Option<&'a str>,
}

/// The protected-configuration ask, with its telemetry row. 1.x shows it before the dependency
/// or snapshot ask of the same command, in one ask.
///
/// # Examples
///
/// ```
/// use temper_domain::{ConfigAsk, ConfigChanges, HookOutcome};
///
/// let written = ["clippy.toml".to_owned()];
/// let changes = ConfigChanges { written: &written, ..ConfigChanges::default() };
/// let ask = ConfigAsk::new(changes).ok_or("a write asks")?;
/// let HookOutcome::Ask { reason, .. } = ask.into_outcome() else { return Err("an ask".into()) };
/// assert!(reason.contains("clippy.toml is verification/agent configuration"));
/// assert!(ConfigAsk::new(ConfigChanges::default()).is_none());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigAsk {
    reason: String,
    telemetry: TelemetryEvent,
}

impl ConfigAsk {
    /// The ask for `changes`, or `None` when they change nothing protected.
    #[must_use]
    pub fn new(changes: ConfigChanges<'_>) -> Option<Self> {
        let mut asks = Vec::new();
        if !changes.written.is_empty() {
            asks.push(changed("writes", "Changing", changes.written));
        }
        // Deleting protected configuration weakens the checks as much as rewriting it
        // (wi-2000a3).
        if !changes.deleted.is_empty() {
            asks.push(changed("deletes", "Deleting", changes.deleted));
        }
        // A patch from a pipe, a variable or a missing file may change protected
        // configuration too.
        if let Some(cmd) = changes.unread_patch {
            asks.push(format!(
                "temper: `{}`: temper cannot read the patch, so it cannot tell whether it changes protected configuration (H-05, H-12). Approve only if you know what the patch changes.",
                truncate_utf16(cmd, PATCH_UNITS)
            ));
        }
        // The row owns its file name; the changes are borrowed.
        let change = match (changes.written.first(), changes.deleted.first()) {
            (Some(file), _) => ProtectedChange::Written(file.clone()),
            (None, Some(file)) => ProtectedChange::Deleted(file.clone()),
            (None, None) => ProtectedChange::UnreadPatch,
        };
        (!asks.is_empty()).then(|| Self {
            reason: asks.join("\n"),
            telemetry: TelemetryEvent::ProtectedAsk { change },
        })
    }

    /// The row logged when the ask is made, before any later check's row.
    #[must_use]
    pub fn telemetry(&self) -> &TelemetryEvent {
        &self.telemetry
    }

    /// `decision` with this ask in front of its reason, when it is an ask (1.x `withConfig`);
    /// any other decision unchanged.
    #[must_use]
    pub fn before(self, decision: PolicyDecision) -> PolicyDecision {
        let outcome = match decision.outcome {
            HookOutcome::Ask { reason, context } => HookOutcome::Ask {
                reason: [self.reason, reason].join("\n"),
                context,
            },
            other => other,
        };
        PolicyDecision {
            outcome,
            telemetry: decision.telemetry,
        }
    }

    /// The ask on its own.
    #[must_use]
    pub fn into_outcome(self) -> HookOutcome {
        HookOutcome::Ask {
            reason: self.reason,
            context: None,
        }
    }
}

/// 1.x's write and delete asks: `files`, joined, with the verb, `is`/`are` and `it`/`them`.
fn changed(verb: &str, changing: &str, files: &[String]) -> String {
    let what = files.join(", ");
    let (is, it) = if files.len() > 1 {
        ("are", "them")
    } else {
        ("is", "it")
    };
    format!(
        "temper: this shell command {verb} {what}. {what} {is} verification/agent configuration. {changing} {it} can weaken the checks (H-05, H-12). Approve only if you asked for this change."
    )
}

#[cfg(test)]
mod tests {
    use super::{ConfigAsk, ConfigChanges};
    use crate::hooks::{HookOutcome, PolicyDecision, ProtectedChange, TelemetryEvent};

    fn reason(outcome: HookOutcome) -> String {
        match outcome {
            HookOutcome::Ask { reason, .. } => reason,
            other => format!("not an ask: {other:?}"),
        }
    }

    /// Each text is the one 1.x's `preBash` printed for the same changes.
    #[test]
    fn the_asks_join_in_1x_order_and_the_row_names_the_first_change() {
        let written = ["clippy.toml".to_owned(), ".cargo/**".to_owned()];
        let deleted = [".claude/settings.json".to_owned()];
        let cmd = format!("git apply {}x", "é".repeat(130));
        let all = ConfigAsk::new(ConfigChanges {
            written: &written,
            deleted: &deleted,
            unread_patch: Some(&cmd),
        });
        let all = all.map(|ask| (ask.telemetry().clone(), reason(ask.into_outcome())));
        let quoted = format!("git apply {}", "é".repeat(110));
        assert_eq!(
            all,
            Some((
                TelemetryEvent::ProtectedAsk {
                    change: ProtectedChange::Written("clippy.toml".to_owned())
                },
                [
                    "temper: this shell command writes clippy.toml, .cargo/**. clippy.toml, .cargo/** are verification/agent configuration. Changing them can weaken the checks (H-05, H-12). Approve only if you asked for this change.".to_owned(),
                    "temper: this shell command deletes .claude/settings.json. .claude/settings.json is verification/agent configuration. Deleting it can weaken the checks (H-05, H-12). Approve only if you asked for this change.".to_owned(),
                    format!("temper: `{quoted}`: temper cannot read the patch, so it cannot tell whether it changes protected configuration (H-05, H-12). Approve only if you know what the patch changes."),
                ]
                .join("\n")
            ))
        );
        let unread = ConfigAsk::new(ConfigChanges {
            unread_patch: Some("patch"),
            ..ConfigChanges::default()
        });
        assert_eq!(
            unread.map(|ask| ask.telemetry().clone()),
            Some(TelemetryEvent::ProtectedAsk {
                change: ProtectedChange::UnreadPatch
            })
        );
    }

    #[test]
    fn a_later_ask_follows_the_config_ask_and_keeps_its_row() {
        let deleted = ["deny.toml".to_owned()];
        let changes = ConfigChanges {
            deleted: &deleted,
            ..ConfigChanges::default()
        };
        let later = PolicyDecision {
            outcome: HookOutcome::Ask {
                reason: "later".to_owned(),
                context: None,
            },
            telemetry: None,
        };
        let joined = ConfigAsk::new(changes).map(|ask| reason(ask.before(later).outcome));
        assert!(joined.is_some_and(|r| r.ends_with("change.\nlater")));
    }
}
