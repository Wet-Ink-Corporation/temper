//! The denials `pre-edit` gives before it looks at the edit (1.x `hooks.mjs` `preEdit` and
//! `LOCKFILE_DENY`).

/// The refusal of an edit under `.git/` or the temper state dir.
pub const STATE_DIR_DENIAL: &str = "temper: files under .git (including temper gate state and review verdicts) are not edited directly. Verdicts come from the temper:rust-reviewer subagent; gate results come from the gate command.";

/// The refusal of a hand edit of the `Cargo.lock` at `rel` (PROC-13).
#[must_use]
pub fn lockfile_denial(rel: &str) -> String {
    format!(
        "temper: {rel} is maintained by Cargo and is never edited by hand (PROC-13). Change it with cargo: `cargo update -p <crate> --precise <version>` to pin or roll back one dependency, `cargo update --workspace` after a manifest edit, `cargo update --dry-run` to preview. For a merge conflict, check out one side's Cargo.lock and re-run a cargo command."
    )
}
