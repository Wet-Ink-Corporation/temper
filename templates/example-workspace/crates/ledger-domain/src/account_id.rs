//! Account identifiers.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Longest accepted identifier, in bytes. Adapters embed ids in file names,
/// and 64 keeps `account-<id>.jsonl` far below the 255-byte name limit of
/// common file systems. Lowering it makes existing accounts unaddressable.
const MAX_LEN: usize = 64;

/// Identifies one account: 1 to 64 of `a-z`, `0-9` and `-`, starting with a
/// letter or digit.
///
/// The alphabet is deliberately narrow: no separators, dots or case
/// ambiguity, so an id can be embedded in a file name or URL segment without
/// escaping.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), ledger_domain::ParseAccountIdError> {
/// let id: ledger_domain::AccountId = "acc-42".parse()?;
/// assert_eq!(id.as_str(), "acc-42");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AccountId(String);

/// Why a string is not an [`AccountId`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParseAccountIdError {
    /// The id is the empty string.
    #[error("account id is empty")]
    Empty,
    /// The id is longer than 64 bytes.
    #[error("account id is {len} bytes long; the limit is {max}", max = MAX_LEN)]
    TooLong {
        /// Length of the rejected id, in bytes.
        len: usize,
    },
    /// The id starts with `-`.
    #[error("account id must start with a letter or digit")]
    LeadingHyphen,
    /// The id contains a character outside `a-z`, `0-9` and `-`.
    #[error("account id contains {found:?} at byte {index}; allowed are a-z, 0-9 and '-'")]
    InvalidChar {
        /// Byte offset of the first offending character.
        index: usize,
        /// The offending character.
        found: char,
    },
}

impl AccountId {
    /// The identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AccountId {
    type Err = ParseAccountIdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() {
            return Err(ParseAccountIdError::Empty);
        }
        if text.len() > MAX_LEN {
            return Err(ParseAccountIdError::TooLong { len: text.len() });
        }
        if text.starts_with('-') {
            return Err(ParseAccountIdError::LeadingHyphen);
        }
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if let Some((index, found)) = text.char_indices().find(|&(_, c)| !allowed(c)) {
            return Err(ParseAccountIdError::InvalidChar { index, found });
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// Compile-time proof that these types can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AccountId>();
    assert_send_sync::<ParseAccountIdError>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lowercase_digits_and_hyphens() {
        let id = "savings-2026".parse::<AccountId>();

        assert_eq!(id.map(|id| id.to_string()), Ok("savings-2026".to_owned()));
    }

    #[test]
    fn rejects_empty_id() {
        assert_eq!("".parse::<AccountId>(), Err(ParseAccountIdError::Empty));
    }

    #[test]
    fn rejects_id_over_64_bytes() {
        let long = "a".repeat(65);

        assert_eq!(
            long.parse::<AccountId>(),
            Err(ParseAccountIdError::TooLong { len: 65 })
        );
    }

    #[test]
    fn rejects_leading_hyphen() {
        assert_eq!(
            "-acc".parse::<AccountId>(),
            Err(ParseAccountIdError::LeadingHyphen)
        );
    }

    #[test]
    fn rejects_path_separators_with_their_position() {
        assert_eq!(
            "acc/../etc".parse::<AccountId>(),
            Err(ParseAccountIdError::InvalidChar {
                index: 3,
                found: '/'
            })
        );
    }
}
