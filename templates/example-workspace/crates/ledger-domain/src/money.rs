//! Money amounts.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Cents per currency unit.
const CENTS_PER_UNIT: u64 = 100;

/// A non-negative amount of money in cents.
///
/// Arithmetic is checked: an operation that would leave the `u64` range
/// returns `None` instead of wrapping. Text form is `units.cents`, for example
/// `12.50`.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), ledger_domain::ParseCentsError> {
/// use ledger_domain::Cents;
///
/// let price: Cents = "12.5".parse()?;
/// assert_eq!(price, Cents::new(1250));
/// assert_eq!(price.to_string(), "12.50");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cents(u64);

impl Cents {
    /// No money.
    pub const ZERO: Self = Self(0);

    /// An amount of `cents` cents.
    #[must_use]
    pub const fn new(cents: u64) -> Self {
        Self(cents)
    }

    /// The amount in cents.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Whether this is no money at all.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// `self + other`, or `None` if the sum does not fit in `u64` cents.
    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(sum) => Some(Self(sum)),
            None => None,
        }
    }

    /// `self - other`, or `None` if `other` is larger than `self`.
    #[must_use]
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(difference) => Some(Self(difference)),
            None => None,
        }
    }
}

impl fmt::Display for Cents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{:02}",
            self.0 / CENTS_PER_UNIT,
            self.0 % CENTS_PER_UNIT
        )
    }
}

/// Why a string is not an amount of [`Cents`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParseCentsError {
    /// Not of the form `units` or `units.c` or `units.cc` with ASCII digits.
    #[error("amount {text:?} is not of the form 12, 12.5 or 12.50")]
    Malformed {
        /// The rejected text.
        text: String,
    },
    /// The amount does not fit in `u64` cents.
    #[error("amount {text:?} is too large")]
    TooLarge {
        /// The rejected text.
        text: String,
    },
}

impl FromStr for Cents {
    type Err = ParseCentsError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (units, fraction) = text.split_once('.').unwrap_or((text, ""));
        let is_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        let fraction_ok = fraction.len() <= 2 && is_digits(fraction);
        if units.is_empty() || !is_digits(units) || !fraction_ok || text.ends_with('.') {
            return Err(ParseCentsError::Malformed {
                text: text.to_owned(),
            });
        }
        // "12.5" means 12.50: pad the fraction to two digits before reading it.
        let padded_fraction = format!("{fraction:0<2}");
        decimal_value(units)
            .and_then(|units| units.checked_mul(CENTS_PER_UNIT))
            .and_then(|cents| cents.checked_add(decimal_value(&padded_fraction)?))
            .map(Self)
            .ok_or_else(|| ParseCentsError::TooLarge {
                text: text.to_owned(),
            })
    }
}

/// Value of a string of ASCII digits, or `None` if it overflows `u64`.
fn decimal_value(digits: &str) -> Option<u64> {
    digits.chars().try_fold(0_u64, |value, digit| {
        value
            .checked_mul(10)?
            .checked_add(u64::from(digit.to_digit(10)?))
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn displays_units_and_two_digit_cents() {
        assert_eq!(Cents::new(1205).to_string(), "12.05");
    }

    #[test]
    fn parses_whole_units_and_one_or_two_decimals() {
        let parsed = ["7", "7.5", "7.05"].map(str::parse::<Cents>);

        assert_eq!(
            parsed,
            [
                Ok(Cents::new(700)),
                Ok(Cents::new(750)),
                Ok(Cents::new(705))
            ]
        );
    }

    #[test]
    fn rejects_signs_separators_and_extra_decimals() {
        for text in ["", "-1", "+1", "1,00", "1.234", "1.", ".5", "1.5x"] {
            assert_eq!(
                text.parse::<Cents>(),
                Err(ParseCentsError::Malformed {
                    text: text.to_owned()
                }),
                "input {text:?}"
            );
        }
    }

    #[test]
    fn rejects_amount_beyond_u64_cents() {
        let text = "184467440737095516.16";

        assert_eq!(
            text.parse::<Cents>(),
            Err(ParseCentsError::TooLarge {
                text: text.to_owned()
            })
        );
    }

    #[test]
    fn checked_sub_refuses_to_go_below_zero() {
        assert_eq!(Cents::new(5).checked_sub(Cents::new(6)), None);
    }

    proptest! {
        #[test]
        fn parsing_inverts_display(cents in any::<u64>()) {
            prop_assert_eq!(Cents::new(cents).to_string().parse(), Ok(Cents::new(cents)));
        }
    }
}
