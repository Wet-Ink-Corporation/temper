//! The read-only reviewer's patch denials (1.x `hooks.mjs` `REVIEWER_PATCH_DENY`,
//! `REVIEWER_NAME_DENY`, `REVIEWER_NAME_HINT` and `REVIEWER_OPAQUE_HINT`, #140 and #151).

use super::opaque::OpaqueKind;

/// 1.x `REVIEWER_OPAQUE_HINT[kind]`: what the reviewer runs instead. A name has its own denial,
/// so its hint is never read here.
const fn hint(kind: OpaqueKind) -> &'static str {
    match kind {
        OpaqueKind::Git => {
            "Run one of git's own commands with its name written out; the reviewer runs no git alias."
        }
        OpaqueKind::Code => {
            "Run the git or shell command directly, not through an interpreter, so temper can read it."
        }
        OpaqueKind::Script | OpaqueKind::Name => {
            "Run the commands directly, not as a script piped or redirected into a shell, so temper can read them."
        }
        OpaqueKind::Alias => {
            "The reviewer sets no git alias, because an alias could run `git apply` or `git am` later."
        }
    }
}

/// 1.x `REVIEWER_PATCH_DENY`.
pub(super) const PATCH: &str = "temper: the reviewer is read-only, and `git apply`, `git am` and `patch` change files, so the command was not run. Do not retry it in any form. Review the change where it is (`git diff`, `git show`; `git apply --check` and `--stat` inspect a patch), and report what should change as a finding.";

/// 1.x `REVIEWER_NAME_DENY`.
const NAME: &str = "temper: the reviewer is read-only, and `git apply`, `git am` and `patch` change files, so a command whose name temper cannot read was not run.";

/// 1.x `REVIEWER_NAME_HINT`.
const NAME_HINT: &str = "Write the command name out, with no variable or `$(...)` in it (`cargo test`, `~/.cargo/bin/cargo test`, not `\"$CARGO\" test`, `\"$HOME/.cargo/bin/cargo\" test` or `$(which cargo) test`), so temper can tell that it applies no patch.";

/// The denial of a command temper cannot rule out applies a patch: `why` says what it holds,
/// and `hint`, when `opaquePatch` gave one, replaces the kind's.
pub(super) fn opaque(kind: OpaqueKind, why: &str, hint: Option<&str>) -> String {
    if kind == OpaqueKind::Name {
        return format!("{NAME} This command holds {why}. {NAME_HINT}");
    }
    let hint = hint.unwrap_or_else(|| self::hint(kind));
    format!(
        "{PATCH} This command holds {why}, so temper cannot tell that it applies no patch. {hint}"
    )
}
