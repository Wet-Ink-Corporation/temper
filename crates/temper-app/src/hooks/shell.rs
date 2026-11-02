//! 1.x's reader of shell command lines (`scripts/lib/shell.mjs`), for the `pre-bash` rules
//! that look at what a command writes or deletes: the state-dir deny, the protected-config ask
//! and, from port Phase 2f, the scan of copies and patches onto Rust sources.
//!
//! It reads quotes, escapes, expansions, heredocs, operators, subshells, compound-command
//! keywords, wrappers (`sudo`, `env`, `xargs`, ...) and `cd`, and reports the files each
//! simple command writes. It never runs anything and reads no files: a rule that needs the
//! disk (a patch's text, whether a path is a directory) reads it itself. It lives here, beside
//! Node's path algebra, and is crate-private (port plan OC-1 B, wi-30e6a5).
//!
//! Each scan reads its input forward once per nesting level the walker follows (at most
//! [`walk::MAX_DEPTH`] deep), so a command's cost is linear in its length at each level; the
//! variables and directories a subshell restores are journaled, not copied.

mod deletes;
mod git;
mod lex;
mod patch;
mod walk;
mod words;
mod wrappers;
mod writes;

pub(crate) use deletes::command_deletes;
pub(crate) use patch::{Patch, PatchCommand, PatchTool, patch_command, patch_files};
pub(crate) use walk::{Command, ParseOpts, Stdin, parse_commands};
pub(crate) use words::{PathCx, Word, js_len, msys_path, resolve_path};
pub(crate) use writes::{CopyDest, Target, Verb, command_writes, landing, pathspec_pattern};

#[cfg(test)]
mod tests;
