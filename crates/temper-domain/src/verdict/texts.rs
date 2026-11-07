//! The texts that refuse an invalid verdict (1.x `preHandback` and `subagentStop`).

use super::VerdictProblem;

/// The problems as 1.x lists them: `- ` before each, one per line.
fn listed(problems: &[VerdictProblem]) -> String {
    let messages: Vec<&str> = problems.iter().map(VerdictProblem::message).collect();
    messages.join("\n- ")
}

/// 1.x `preHandback`'s denial of a hand-back whose verdict is invalid.
#[must_use]
pub fn handback_denial(problems: &[VerdictProblem]) -> String {
    format!(
        "temper: the review verdict is invalid, so the report was not delivered:\n- {}\nFix the verdict block and hand back again.",
        listed(problems)
    )
}

/// 1.x `subagentStop`'s block of a reviewer whose verdict is invalid.
#[must_use]
pub fn verdict_block(problems: &[VerdictProblem]) -> String {
    format!(
        "temper: your review verdict is invalid:\n- {}\nEnd your final report with a corrected ```json verdict block.",
        listed(problems)
    )
}
