//! Instants as 1.x records them: JS `Date` milliseconds, printed by `toISOString`.

mod parse;

use std::fmt;

use crate::js::number_from_text;

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

    /// Milliseconds since the epoch as the JS number `Date.now()` returns: exact, since a
    /// `Date`'s range is within 2^53.
    ///
    /// ```
    /// use temper_domain::Timestamp;
    ///
    /// assert_eq!(Timestamp::from_unix_millis(-8_640_000_000_000_000).js_millis(), -8.64e15);
    /// assert_eq!(Timestamp::from_unix_millis(1_730_000_000_123).js_millis(), 1_730_000_000_123.0);
    /// ```
    #[must_use]
    pub fn js_millis(self) -> f64 {
        number_from_text(&self.0.to_string())
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

/// A file's modification time as Node's `fs.statSync(p).mtimeMs` gives it.
///
/// That is a fractional count of milliseconds since the epoch, computed from the `timespec` as
/// `sec * 1e3 + nsec / 1e6` in doubles (Node's `msFromTimeSpec`), so it compares with
/// `Date.now()` as 1.x's does.
///
/// # Examples
///
/// ```
/// use temper_domain::ModTime;
///
/// let t = ModTime::from_unix_nanos(-1_500_000_000); // 1.5 s before the epoch
/// assert_eq!(t.millis(), -1500.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ModTime(f64);

impl ModTime {
    /// The time `nanos` nanoseconds after the epoch (before it when negative). The `timespec`
    /// holds whole seconds rounded down and the nanoseconds above them, as POSIX's does.
    #[must_use]
    pub fn from_unix_nanos(nanos: i128) -> Self {
        let sec = nanos.div_euclid(1_000_000_000);
        // `rem_euclid` is in 0..1e9, which always fits a `u32`.
        let nsec = u32::try_from(nanos.rem_euclid(1_000_000_000)).map_or(0.0, f64::from);
        // `static_cast<double>(sec)`: the nearest double, which the decimal text parses to.
        Self(number_from_text(&sec.to_string()) * 1e3 + nsec / 1e6)
    }

    /// Node's `mtimeMs`.
    #[must_use]
    pub const fn millis(self) -> f64 {
        self.0
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
