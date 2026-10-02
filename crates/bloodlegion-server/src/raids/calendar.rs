//! The raid calendar: when Forever's raids open, and the weekly reset that numbers the guild's
//! raiding weeks.
//!
//! The raids open at [`RELEASE`], a New York wall-clock time, kept as such so the instant follows
//! New York's own offset (EST in December). The lockouts reset every Tuesday at 15:00 UTC, fixed in
//! UTC, so in local time the reset moves by an hour when daylight saving starts or ends. Week 1
//! runs from the release to the first reset after it; every reset starts the next week.
//!
//! Raids are stored as instants (UTC); their week is derived from those here, never stored.

use std::sync::LazyLock;

use jiff::civil;
use time::{Duration, OffsetDateTime, Time, Weekday};

/// The raids' release, as New York's own clock reads it.
pub const RELEASE: civil::DateTime = civil::date(2026, 12, 9).at(18, 0, 0, 0);
pub const RELEASE_TIME_ZONE: &str = "America/New_York";

/// The weekly reset, in UTC.
pub const RESET_WEEKDAY: Weekday = Weekday::Tuesday;
pub const RESET_TIME: Time = time::macros::time!(15:00);

/// [`RELEASE`] in [`RELEASE_TIME_ZONE`], as an instant. A wall-clock time can be skipped or
/// repeated around a daylight-saving change; jiff's default resolves either to the earlier
/// reading, which a 6 PM release never meets.
static RELEASE_AT: LazyLock<OffsetDateTime> = LazyLock::new(|| {
    let zoned = RELEASE
        .in_tz(RELEASE_TIME_ZONE)
        .expect("the release time zone is in the bundled database");
    OffsetDateTime::from_unix_timestamp(zoned.timestamp().as_second())
        .expect("the release is a representable instant")
});

pub fn release_at() -> OffsetDateTime {
    *RELEASE_AT
}

/// The first reset after the release, which ends week 1.
pub fn first_reset() -> OffsetDateTime {
    let release = release_at().to_offset(time::UtcOffset::UTC);
    let days = (RESET_WEEKDAY.number_days_from_monday() as i64
        - release.weekday().number_days_from_monday() as i64)
        .rem_euclid(7);
    let candidate = release
        .date()
        .midnight()
        .assume_utc()
        .replace_time(RESET_TIME)
        + Duration::days(days);
    if candidate > release {
        candidate
    } else {
        candidate + Duration::weeks(1)
    }
}

/// One raiding week: `[starts_at, ends_at)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Week {
    pub number: i64,
    #[serde(with = "time::serde::iso8601")]
    pub starts_at: OffsetDateTime,
    #[serde(with = "time::serde::iso8601")]
    pub ends_at: OffsetDateTime,
}

/// Week `number` (from 1), or `None` for a number below 1.
pub fn week(number: i64) -> Option<Week> {
    let first_reset = first_reset();
    match number {
        ..1 => None,
        1 => Some(Week {
            number,
            starts_at: release_at(),
            ends_at: first_reset,
        }),
        _ => {
            let starts_at = first_reset + Duration::weeks(number - 2);
            Some(Week {
                number,
                starts_at,
                ends_at: starts_at + Duration::weeks(1),
            })
        }
    }
}

/// The week `at` falls in, or `None` before the release.
pub fn week_of(at: OffsetDateTime) -> Option<Week> {
    if at < release_at() {
        return None;
    }
    let first_reset = first_reset();
    if at < first_reset {
        return week(1);
    }
    let since = at - first_reset;
    week(2 + since.whole_weeks())
}

/// The calendar as the site shows it: the release (its wall-clock reading and its zone, and the
/// instant), the reset rule, and every week so far.
#[derive(Debug, serde::Serialize)]
pub struct Calendar {
    /// `2026-12-09T18:00:00`, in `release_time_zone`.
    pub release_local: String,
    pub release_time_zone: &'static str,
    #[serde(with = "time::serde::iso8601")]
    pub release_at: OffsetDateTime,
    /// `tuesday`.
    pub reset_weekday: String,
    /// `15:00`, UTC.
    pub reset_time_utc: String,
    /// The current week, `None` before the release.
    pub current_week: Option<Week>,
    /// Weeks 1 through the current one; empty before the release.
    pub weeks: Vec<Week>,
}

pub fn calendar(now: OffsetDateTime) -> Calendar {
    let current_week = week_of(now);
    Calendar {
        release_local: RELEASE.to_string(),
        release_time_zone: RELEASE_TIME_ZONE,
        release_at: release_at(),
        reset_weekday: RESET_WEEKDAY.to_string().to_lowercase(),
        reset_time_utc: format!("{:02}:{:02}", RESET_TIME.hour(), RESET_TIME.minute()),
        weeks: (1..=current_week.map_or(0, |w| w.number))
            .filter_map(week)
            .collect(),
        current_week,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn the_release_is_six_in_the_evening_in_new_york() {
        // December is EST (UTC-5).
        assert_eq!(release_at(), datetime!(2026-12-09 23:00 UTC));
        assert_eq!(release_at().weekday(), Weekday::Wednesday);
    }

    #[test]
    fn week_one_runs_from_the_release_to_the_first_reset() {
        assert_eq!(first_reset(), datetime!(2026-12-15 15:00 UTC));
        assert_eq!(week_of(datetime!(2026-12-09 22:59 UTC)), None);
        let one = week_of(release_at()).unwrap();
        assert_eq!(
            (one.number, one.starts_at, one.ends_at),
            (1, release_at(), first_reset())
        );
        assert_eq!(week_of(datetime!(2026-12-15 14:59 UTC)).unwrap().number, 1);

        let two = week_of(datetime!(2026-12-15 15:00 UTC)).unwrap();
        assert_eq!(
            (two.number, two.ends_at),
            (2, datetime!(2026-12-22 15:00 UTC))
        );
        assert_eq!(
            week_of(datetime!(2026-12-22 14:59:59 UTC)).unwrap().number,
            2
        );
        assert_eq!(week_of(datetime!(2026-12-22 15:00 UTC)).unwrap().number, 3);
    }

    #[test]
    fn the_reset_stays_at_fifteen_utc_across_daylight_saving() {
        // US daylight saving starts 2027-03-14: the reset is still 15:00 UTC (11 AM in New York
        // instead of 10), and the weeks stay seven days long.
        let before = week_of(datetime!(2027-03-09 15:00 UTC)).unwrap();
        let after = week_of(datetime!(2027-03-16 15:00 UTC)).unwrap();
        assert_eq!(after.number, before.number + 1);
        assert_eq!(after.starts_at, datetime!(2027-03-16 15:00 UTC));
        assert_eq!(week(after.number), Some(after));
    }

    #[test]
    fn the_calendar_lists_the_weeks_so_far() {
        let before = calendar(datetime!(2026-12-01 00:00 UTC));
        assert!(before.current_week.is_none() && before.weeks.is_empty());
        assert_eq!(before.release_local, "2026-12-09T18:00:00");
        assert_eq!(before.reset_weekday, "tuesday");
        assert_eq!(before.reset_time_utc, "15:00");

        let third = calendar(datetime!(2026-12-23 12:00 UTC));
        assert_eq!(third.current_week.unwrap().number, 3);
        assert_eq!(
            third.weeks.iter().map(|w| w.number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }
}
