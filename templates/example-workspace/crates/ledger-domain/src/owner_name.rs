//! Account holder names.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Longest accepted name, in characters: room for a full legal name on one
/// line of output. The store validates names on read, so lowering it makes
/// streams that hold a longer name fail to load.
const MAX_CHARS: usize = 80;

/// The name of an account holder: trimmed, 1 to 80 characters, no control
/// characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OwnerName(String);

/// Why a string is not an [`OwnerName`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParseOwnerNameError {
    /// The name is empty or only whitespace.
    #[error("owner name is blank")]
    Blank,
    /// The trimmed name is longer than 80 characters.
    #[error("owner name is {chars} characters long; the limit is {max}", max = MAX_CHARS)]
    TooLong {
        /// Length of the trimmed name, in characters.
        chars: usize,
    },
    /// The name contains a control character such as a newline.
    #[error("owner name contains a control character")]
    ControlCharacter,
}

impl OwnerName {
    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for OwnerName {
    type Err = ParseOwnerNameError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let name = text.trim();
        if name.is_empty() {
            return Err(ParseOwnerNameError::Blank);
        }
        let chars = name.chars().count();
        if chars > MAX_CHARS {
            return Err(ParseOwnerNameError::TooLong { chars });
        }
        if name.chars().any(char::is_control) {
            return Err(ParseOwnerNameError::ControlCharacter);
        }
        Ok(Self(name.to_owned()))
    }
}

impl fmt::Display for OwnerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// Compile-time proof that these types can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OwnerName>();
    assert_send_sync::<ParseOwnerNameError>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_whitespace() {
        let name = "  Ada Lovelace \t".parse::<OwnerName>();

        assert_eq!(
            name.map(|name| name.to_string()),
            Ok("Ada Lovelace".to_owned())
        );
    }

    #[test]
    fn rejects_whitespace_only_name() {
        assert_eq!(" \t ".parse::<OwnerName>(), Err(ParseOwnerNameError::Blank));
    }

    #[test]
    fn counts_length_in_characters_not_bytes() {
        let at_limit = "é".repeat(80);
        let over_limit = "é".repeat(81);

        assert_eq!(
            at_limit.parse::<OwnerName>().map(|name| name.to_string()),
            Ok(at_limit)
        );
        assert_eq!(
            over_limit.parse::<OwnerName>(),
            Err(ParseOwnerNameError::TooLong { chars: 81 })
        );
    }

    #[test]
    fn rejects_embedded_newline() {
        assert_eq!(
            "Ada\nLovelace".parse::<OwnerName>(),
            Err(ParseOwnerNameError::ControlCharacter)
        );
    }
}
