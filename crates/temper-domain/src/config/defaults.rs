//! 1.x's `DEFAULT_CONFIG`.

/// Dependencies the standard itself relies on; adding them does not prompt (H-16 pre-approval).
pub(super) const PREAPPROVED_DEPENDENCIES: [&str; 12] = [
    "thiserror",
    "anyhow",
    "serde",
    "serde_json",
    "tracing",
    "tracing-subscriber",
    "proptest",
    "insta",
    "trybuild",
    "divan",
    "criterion",
    "tempfile",
];

/// Verification and agent configuration that edits and shell writes may not change unasked.
pub(super) const PROTECTED: [&str; 13] = [
    "**/clippy.toml",
    "**/.clippy.toml",
    "**/rustfmt.toml",
    "**/.rustfmt.toml",
    "**/deny.toml",
    "rust-toolchain",
    "rust-toolchain.toml",
    ".cargo/**",
    ".claude/**",
    ".github/**",
    ".githooks/**",
    "CLAUDE.md",
    "AGENTS.md",
];

/// The gate's per-step timeout, in seconds.
pub(super) const STEP_TIMEOUT_SEC: f64 = 900.0;

pub(super) fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}
