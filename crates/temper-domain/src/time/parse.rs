//! `Date.parse` for ECMA-262's date-time string format (§21.4.1.32), the form `toISOString`
//! writes and every time temper records takes.
//!
//! The accepted forms are a date (`YYYY`, `YYYY-MM` or `YYYY-MM-DD`, the year also as `+YYYYYY`
//! or `-YYYYYY`), alone (read as UTC) or followed by `THH:mm`, `THH:mm:ss` or `THH:mm:ss.s…`
//! and then `Z` or `±HH:mm`. V8 gives the same instant for each. A date-time without an offset
//! is local time in V8, and anything else goes to V8's legacy parser; both read as NaN here
//! (OC-3 A, wi-06100e), so a time that only V8's legacy reader takes is `None`.

use super::{MAX_MILLIS, MILLIS_PER_DAY, Timestamp};

impl Timestamp {
    /// `Date.parse(text)` for ECMA-262's date-time format, `None` where V8 gives `NaN` and for
    /// the local-time and legacy forms OC-3 A reads as `NaN`.
    ///
    /// As V8 does, a day of 29 to 31 past its month's end rolls into the next month, `24:00`
    /// is the next midnight, a fraction keeps its first three digits, and an instant outside
    /// a JS `Date`'s range is `NaN`.
    ///
    /// # Examples
    ///
    /// ```
    /// use temper_domain::Timestamp;
    ///
    /// let t = Timestamp::parse_js("2024-10-27T05:03:20.123+01:30");
    /// assert_eq!(t.map(Timestamp::unix_millis), Some(1_730_000_000_123));
    /// assert_eq!(Timestamp::parse_js("2024-10-27T03:33:20"), None); // local time (OC-3 A)
    /// ```
    #[must_use]
    pub fn parse_js(text: &str) -> Option<Self> {
        let mut cursor = Cursor(text.as_bytes());
        let year = cursor.year()?;
        let (month, day) = if cursor.eat(b'-') {
            let month = cursor.number(2).filter(|m| (1..=12).contains(m))?;
            let day = if cursor.eat(b'-') {
                cursor.number(2).filter(|d| (1..=31).contains(d))?
            } else {
                1
            };
            (month, day)
        } else {
            (1, 1)
        };
        let days = days_from_civil(year, month)?.checked_add(day - 1)?;
        let mut millis = days.checked_mul(MILLIS_PER_DAY)?;
        if !cursor.is_empty() {
            if !cursor.eat(b'T') {
                return None;
            }
            millis = millis.checked_add(cursor.time_of_day()?)?;
            millis = millis.checked_sub(cursor.offset()?)?;
            if !cursor.is_empty() {
                return None;
            }
        }
        (millis.unsigned_abs() <= MAX_MILLIS.unsigned_abs()).then_some(Self(millis))
    }
}

/// The days from 1970-01-01 to the first of `month` in `year` (proleptic Gregorian; Howard
/// Hinnant's `days_from_civil`). The year has at most six digits, so nothing overflows.
fn days_from_civil(year: i64, month: i64) -> Option<i64> {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era.checked_mul(146_097)?.checked_add(doe - 719_468)
}

/// A position in the text, which only ASCII ever matches.
struct Cursor<'a>(&'a [u8]);

impl Cursor<'_> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Consumes `byte` if it is next.
    fn eat(&mut self, byte: u8) -> bool {
        match self.0.split_first() {
            Some((&first, rest)) if first == byte => {
                self.0 = rest;
                true
            }
            _ => false,
        }
    }

    /// Exactly `len` ASCII digits as a number.
    fn number(&mut self, len: usize) -> Option<i64> {
        let (digits, rest) = self.0.split_at_checked(len)?;
        let mut n = 0_i64;
        for &d in digits {
            if !d.is_ascii_digit() {
                return None;
            }
            n = n * 10 + i64::from(d - b'0');
        }
        self.0 = rest;
        Some(n)
    }

    /// `YYYY`, `+YYYYYY` or `-YYYYYY`; `-000000` is not a year (ECMA-262).
    fn year(&mut self) -> Option<i64> {
        if self.eat(b'+') {
            self.number(6)
        } else if self.eat(b'-') {
            self.number(6).filter(|&y| y != 0).map(|y| -y)
        } else {
            self.number(4)
        }
    }

    /// `HH:mm`, `HH:mm:ss` or `HH:mm:ss.s…` as milliseconds into the day; `24:00` only with
    /// every later field zero, as V8 checks it.
    fn time_of_day(&mut self) -> Option<i64> {
        let hour = self.number(2).filter(|h| *h <= 24)?;
        if !self.eat(b':') {
            return None;
        }
        let minute = self.number(2).filter(|m| *m <= 59)?;
        let (second, millis, zero_fraction) = if self.eat(b':') {
            let second = self.number(2).filter(|s| *s <= 59)?;
            let (millis, zero) = if self.eat(b'.') {
                self.fraction()?
            } else {
                (0, true)
            };
            (second, millis, zero)
        } else {
            (0, 0, true)
        };
        if hour == 24 && (minute != 0 || second != 0 || !zero_fraction) {
            return None;
        }
        Some(((hour * 60 + minute) * 60 + second) * 1000 + millis)
    }

    /// One or more digits after `.`: the first three as milliseconds (a shorter fraction
    /// padded with zeros), the rest dropped, and whether every digit is `0`.
    fn fraction(&mut self) -> Option<(i64, bool)> {
        let len = self.0.iter().take_while(|d| d.is_ascii_digit()).count();
        let (digits, rest) = self.0.split_at_checked(len)?;
        if digits.is_empty() {
            return None;
        }
        let mut millis = 0_i64;
        for place in 0..3 {
            let digit = digits.get(place).map_or(0, |d| i64::from(d - b'0'));
            millis = millis * 10 + digit;
        }
        self.0 = rest;
        Some((millis, digits.iter().all(|d| *d == b'0')))
    }

    /// `Z` (zero) or `±HH:mm` as milliseconds east of UTC; anything else is local time, which
    /// OC-3 A reads as `NaN`.
    fn offset(&mut self) -> Option<i64> {
        if self.eat(b'Z') {
            return Some(0);
        }
        let sign = if self.eat(b'+') {
            1
        } else if self.eat(b'-') {
            -1
        } else {
            return None;
        };
        let hours = self.number(2).filter(|h| *h <= 23)?;
        if !self.eat(b':') {
            return None;
        }
        let minutes = self.number(2).filter(|m| *m <= 59)?;
        Some(sign * (hours * 60 + minutes) * 60_000)
    }
}
