//! Instants as 1.x records them: JS `Date` milliseconds, printed by `toISOString`.

use std::fmt;

/// The furthest a JS `Date` reaches from the epoch, in milliseconds (ECMA-262 `TimeClip`).
const MAX_MILLIS: i64 = 8_640_000_000_000_000;
const MILLIS_PER_DAY: i64 = 86_400_000;

/// An instant, in whole milliseconds since the Unix epoch, as `Date.now()` gives it.
///
/// `Display` is JS's `toISOString`: `2024-10-27T03:33:20.123Z`, with a signed six-digit year
/// outside 0000 to 9999.
///
/// # Examples
///
/// ```
/// use temper_domain::Timestamp;
///
/// let t = Timestamp::from_unix_millis(951_782_400_000);
/// assert_eq!(t.to_string(), "2000-02-29T00:00:00.000Z");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i64);

impl Timestamp {
    /// The instant `millis` milliseconds after the epoch (before it when negative), saturated
    /// to the range a JS `Date` holds: about 275,760 years either side of 1970.
    #[must_use]
    pub const fn from_unix_millis(millis: i64) -> Self {
        Self(if millis > MAX_MILLIS {
            MAX_MILLIS
        } else if millis < -MAX_MILLIS {
            -MAX_MILLIS
        } else {
            millis
        })
    }

    /// Milliseconds since the epoch.
    #[must_use]
    pub const fn unix_millis(self) -> i64 {
        self.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let days = self.0.div_euclid(MILLIS_PER_DAY);
        let ms = self.0.rem_euclid(MILLIS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        if (0..=9999).contains(&year) {
            write!(f, "{year:04}")?;
        } else {
            let sign = if year < 0 { '-' } else { '+' };
            write!(f, "{sign}{:06}", year.unsigned_abs())?;
        }
        write!(
            f,
            "-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000
        )
    }
}

/// The proleptic Gregorian `(year, month, day)` of the day `days` after 1970-01-01 (Howard
/// Hinnant's `civil_from_days`). The clamp keeps `days` within ±10^8, far from overflow.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests;
