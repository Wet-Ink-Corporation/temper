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
//! [`walk::MAX_DEPTH`] deep). On top of that, each value a line builds costs its length once:
//! an assignment copies its value, so `a=$a/b` repeated n times copies O(n^2) bytes, as in
//! 1.x. A directory is a parent handle plus the names a `cd` added ([`dir::Dir`]), so a `cd`
//! costs the value it reads and a line of `cd a` or `pushd a` holds each name once; the whole
//! path is built only where a rule resolves a name from it, and a rule builds at most
//! [`dir::Guard`]'s budget of that text per line, which also counts each join of a word's
//! text and each inner script it reads and walks (#202 re-review M2). The variables a subshell restores are
//! journaled once per name per scope, and its directories are shared handles; each string is
//! held to V8's longest (see [`walk::TooLong`]), and so is each path joined from them.

mod code;
mod deletes;
mod dir;
mod git;
mod lex;
mod opaque;
mod patch;
mod text;
mod walk;
mod words;
mod wrappers;
mod writes;

pub(crate) use code::Threw;
pub(crate) use deletes::command_deletes;
pub(crate) use opaque::opaque_patch;
pub(crate) use patch::{Patch, PatchCommand, PatchTool, patch_command, patch_files};
pub(crate) use walk::{Command, ParseOpts, Stdin, parse_commands};
pub(crate) use words::{
    Dir, Guard, PathCx, Word, js_len, msys_path, names_absolute, resolve_path, word_path,
};
pub(crate) use writes::{
    CopyDest, Target, Verb, command_writes, landing, node_basename, pathspec_pattern,
};

#[cfg(test)]
mod tests;
