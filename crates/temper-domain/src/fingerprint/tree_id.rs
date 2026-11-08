//! The id of a checkout in the names of the session's tree records (1.x `hooks.mjs` `treeId`).

use std::fmt;

use sha2::{Digest, Sha256};

/// A checkout's id, written as 12 lowercase hex digits.
///
/// It is the first 6 bytes of the SHA-256 of the resolved root with `\` written `/`. Every worktree of a repository shares one state dir, so
/// the session records each checkout it changes as `tree-<id>-<agent>.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TreeId([u8; 6]);

impl fmt::Display for TreeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// 1.x `treeId(root)` for a root already resolved (`path.resolve(root)`): it hashes the root
/// with each `\` written `/`.
///
/// # Examples
///
/// ```
/// use temper_domain::tree_id;
///
/// assert_eq!(tree_id("/w").to_string(), "d1bb8ca35467");
/// assert_eq!(tree_id(r"C:\x\y").to_string(), tree_id("C:/x/y").to_string());
/// ```
#[must_use]
pub fn tree_id(resolved_root: &str) -> TreeId {
    let digest: [u8; 32] = Sha256::digest(resolved_root.replace('\\', "/")).into();
    let [b0, b1, b2, b3, b4, b5, ..] = digest;
    TreeId([b0, b1, b2, b3, b4, b5])
}

#[cfg(test)]
mod tests {
    use super::tree_id;

    /// Each id printed by Node 22's `createHash('sha256').update(norm(path.resolve(r)))`.
    #[test]
    fn the_id_is_1x_s_on_posix_and_windows_roots() {
        assert_eq!(
            ["/w", "/a/c", r"C:\x\y", "/tmp/\u{e9}"].map(|root| tree_id(root).to_string()),
            [
                "d1bb8ca35467",
                "61f97427d954",
                "457c4f96f476",
                "ed2fd1a3eec8"
            ],
            "1.x treeId"
        );
    }
}
