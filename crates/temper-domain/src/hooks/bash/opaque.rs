//! What kind of command the shell reader cannot read. It lives apart from the reviewer's texts
//! because the shell reader, bound for `redkiln-guard` (wi-59cb6e), names it.

/// What kind of command temper cannot read (1.x `opaquePatch`'s `kind`), which picks the
/// reviewer's hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpaqueKind {
    /// A git subcommand temper cannot resolve, or one git does not have.
    Git,
    /// A command name that is not a literal word.
    Name,
    /// Interpreter code that runs a patch, built from an expansion, or read from where temper
    /// cannot.
    Code,
    /// A script a shell reads that temper cannot.
    Script,
    /// `git config` setting an alias, which could name `git apply` later.
    Alias,
}
