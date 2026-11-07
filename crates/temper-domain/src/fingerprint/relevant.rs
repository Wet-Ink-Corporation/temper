//! Which paths are part of "the Rust change" and of the fingerprint (1.x `RUST_RELEVANT`,
//! `inFingerprint` and the pathspecs).

/// The committed lint baseline, relative to the project root (1.x `BASELINE_FILE`): part of the
/// fingerprint, but not a Rust change on its own.
pub const BASELINE_FILE: &str = ".claude/temper-baseline.json";

/// temper's config, relative to the project root (1.x `TEMPER_CONFIG_FILE`). It counts by what
/// the gate reads from it ([`gate_config`](crate::gate_config)), not by its bytes.
pub const TEMPER_CONFIG_FILE: &str = ".claude/temper.json";

/// The directories git's change lists leave out (1.x `EXCLUDE`), as `:(exclude)<dir>`.
pub const EXCLUDED_DIRS: [&str; 2] = [".temper", "target"];

/// 1.x `RELEVANT_PATHSPECS`: git's form of [`is_rust_relevant`].
pub const RELEVANT_PATHSPECS: [&str; 15] = [
    ":(glob)**/*.rs",
    ":(glob)**/*.snap",
    ":(glob)**/*.stderr",
    ":(glob)**/tests/**",
    ":(glob)**/Cargo.toml",
    ":(glob)**/Cargo.lock",
    ":(glob)**/rust-toolchain",
    ":(glob)**/rust-toolchain.toml",
    ":(glob)**/clippy.toml",
    ":(glob)**/.clippy.toml",
    ":(glob)**/rustfmt.toml",
    ":(glob)**/.rustfmt.toml",
    ":(glob)**/deny.toml",
    ":(glob)**/.cargo/config",
    ":(glob)**/.cargo/config.toml",
];

/// 1.x `FINGERPRINT_PATHSPECS`: [`RELEVANT_PATHSPECS`], the baseline and the crate charters.
pub const FINGERPRINT_PATHSPECS: [&str; 17] = [
    ":(glob)**/*.rs",
    ":(glob)**/*.snap",
    ":(glob)**/*.stderr",
    ":(glob)**/tests/**",
    ":(glob)**/Cargo.toml",
    ":(glob)**/Cargo.lock",
    ":(glob)**/rust-toolchain",
    ":(glob)**/rust-toolchain.toml",
    ":(glob)**/clippy.toml",
    ":(glob)**/.clippy.toml",
    ":(glob)**/rustfmt.toml",
    ":(glob)**/.rustfmt.toml",
    ":(glob)**/deny.toml",
    ":(glob)**/.cargo/config",
    ":(glob)**/.cargo/config.toml",
    ":(literal).claude/temper-baseline.json",
    ":(glob)**/AGENTS.md",
];

/// The last components that are Rust configuration wherever they sit.
const CONFIG_NAMES: [&str; 9] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain",
    "rust-toolchain.toml",
    "clippy.toml",
    ".clippy.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "deny.toml",
];

/// 1.x `RUST_RELEVANT.test(path)` on a `/`-separated path.
///
/// It takes sources, snapshots, trybuild `.stderr` files, anything under a `tests/` directory,
/// manifests, toolchain and lint config, and `.cargo/config(.toml)` in any directory. 1.x
/// applies it to relative paths and, in its walk outside git, to absolute ones, so a root under
/// a `tests/` directory takes every file.
///
/// # Examples
///
/// ```
/// use temper_domain::is_rust_relevant;
///
/// assert!(is_rust_relevant("crates/a/src/lib.rs"));
/// assert!(is_rust_relevant("a/.cargo/config.toml"));
/// assert!(!is_rust_relevant("README.md"));
/// ```
#[must_use]
pub fn is_rust_relevant(path: &str) -> bool {
    [".rs", ".snap", ".stderr"]
        .iter()
        .any(|end| path.ends_with(end))
        || path.starts_with("tests/")
        || path.contains("/tests/")
        || CONFIG_NAMES.contains(&last_component(path))
        || [".cargo/config", ".cargo/config.toml"]
            .iter()
            .any(|end| ends_with_component(path, end))
}

/// 1.x `inFingerprint(rel)`: Rust-relevant, the baseline, or a crate charter (`AGENTS.md`).
#[must_use]
pub fn in_fingerprint(rel: &str) -> bool {
    is_rust_relevant(rel) || rel == BASELINE_FILE || last_component(rel) == "AGENTS.md"
}

/// What follows the last `/`, or the whole path: the `X` of 1.x's `(^|\/)X$`.
fn last_component(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, last)| last)
}

/// 1.x's `(^|\/)<end>$`: `path` is `end`, or ends with `/` and `end`.
fn ends_with_component(path: &str, end: &str) -> bool {
    path.strip_suffix(end)
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('/'))
}

#[cfg(test)]
mod tests;
