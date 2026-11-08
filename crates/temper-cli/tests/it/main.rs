//! End-to-end tests of the `temper` binary, compiled as one test binary.

mod cli;
mod hooks;
mod scan;
mod session_start;
mod status;
mod status_json;
#[cfg(test)]
mod support;
