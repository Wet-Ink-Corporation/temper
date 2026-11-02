//! The system clock.

use std::time::{SystemTime, UNIX_EPOCH};

use temper_app::Clock;
use temper_domain::Timestamp;

/// [`Clock`] on [`SystemTime`], in whole milliseconds as `Date.now()` reads it.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(after) => i64::try_from(after.as_millis()).unwrap_or(i64::MAX),
            Err(before) => i64::try_from(before.duration().as_millis()).map_or(i64::MIN, |ms| -ms),
        };
        // Past a JS `Date`'s range, the instant saturates at its end.
        Timestamp::from_unix_millis(millis)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use temper_app::Clock;

    use super::SystemClock;

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the system clock")]
    fn now_is_the_system_time_in_milliseconds() {
        let now = SystemClock.now();
        let system = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the test runs after 1970");
        let system = i64::try_from(system.as_millis()).expect("the time fits in an i64");

        // 2026-09-21, before this adapter was written: a clock read in seconds or nanoseconds
        // would be far outside it.
        assert!(
            (1_790_000_000_000..10_000_000_000_000).contains(&now.unix_millis()),
            "{now}"
        );
        assert!(
            system.abs_diff(now.unix_millis()) < 5_000,
            "{now} is within 5 s of the system time read beside it ({system} ms)"
        );
    }
}
