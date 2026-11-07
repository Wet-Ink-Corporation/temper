//! The tree fingerprint: the first 8 bytes of a SHA-256, written as 16 lowercase hex digits.

use std::fmt;
use std::str::FromStr;

mod hasher;

pub use hasher::{ContentSink, DiffSink, FingerprintHasher};

/// temper's config, relative to the project root (1.x `TEMPER_CONFIG_FILE`).
pub const TEMPER_CONFIG_FILE: &str = ".claude/temper.json";

/// A tree fingerprint, as 1.x writes it into gate, verdict and session files: 16 lowercase hex
/// digits, the first 8 bytes of a SHA-256.
///
/// # Examples
///
/// ```
/// use temper_domain::Fingerprint;
///
/// let fp: Fingerprint = "0123456789abcdef".parse()?;
/// assert_eq!(fp.to_string(), "0123456789abcdef");
/// assert!("0123456789ABCDEF".parse::<Fingerprint>().is_err());
/// # Ok::<(), temper_domain::FingerprintParseError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 8]);

impl Fingerprint {
    /// The fingerprint of a SHA-256 digest: its first 8 bytes, as 1.x's `digest('hex').slice(0, 16)`.
    pub(crate) fn from_sha256(digest: [u8; 32]) -> Self {
        let [b0, b1, b2, b3, b4, b5, b6, b7, ..] = digest;
        Self([b0, b1, b2, b3, b4, b5, b6, b7])
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// The text is not a fingerprint: 1.x's `/^[0-9a-f]{16}$/` refuses it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not a 16-hex fingerprint")]
pub struct FingerprintParseError {
    value: String,
}

impl FingerprintParseError {
    /// The text that was refused.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl FromStr for Fingerprint {
    type Err = FingerprintParseError;

    /// Exactly 16 lowercase hex digits; JS's `$` does not match before a trailing newline, so
    /// neither does this.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let refused = || FingerprintParseError {
            value: text.to_owned(),
        };
        let Ok(digits) = <&[u8; 16]>::try_from(text.as_bytes()) else {
            return Err(refused());
        };
        let mut bytes = [0_u8; 8];
        for (byte, pair) in bytes.iter_mut().zip(digits.as_chunks::<2>().0) {
            let [high, low] = *pair;
            *byte = (nibble(high).ok_or_else(refused)? << 4) | nibble(low).ok_or_else(refused)?;
        }
        Ok(Self(bytes))
    }
}

/// The value of a lowercase hex digit.
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
