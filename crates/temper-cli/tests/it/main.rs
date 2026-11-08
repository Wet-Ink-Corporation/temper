//! End-to-end tests of the `temper` binary, compiled as one test binary.

mod cli;
mod hooks;
mod pre_edit_trees;
mod pre_handback;
mod scan;
mod session_start;
mod status;
mod status_json;
mod subagent_start;
mod subagent_stop;
mod subagent_worktrees;
#[cfg(test)]
mod support;
