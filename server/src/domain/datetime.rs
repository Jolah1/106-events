//! Human date/time formatting in an event's own timezone.
//!
//! Public pages are read by guests in Lagos, London and Houston alike, so every
//! instant is rendered in the *event's* timezone, never the viewer's.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

/// "Saturday, 21 November 2026"
pub fn day_label(instant: DateTime<Utc>, tz: Tz) -> String {
    instant.with_timezone(&tz).format("%A, %-d %B %Y").to_string()
}

/// "21 Nov 2026"
pub fn short_date(instant: DateTime<Utc>, tz: Tz) -> String {
    instant.with_timezone(&tz).format("%-d %b %Y").to_string()
}

/// "9:00 AM"
pub fn time_label(instant: DateTime<Utc>, tz: Tz) -> String {
    instant.with_timezone(&tz).format("%-l:%M %p").to_string()
}

/// "9:00 AM – 11:00 AM", or just the start when there's no end time.
pub fn time_range(starts_at: DateTime<Utc>, ends_at: Option<DateTime<Utc>>, tz: Tz) -> String {
    match ends_at {
        Some(ends_at) => format!("{} – {}", time_label(starts_at, tz), time_label(ends_at, tz)),
        None => time_label(starts_at, tz),
    }
}

/// A one-line date summary spanning every part of an event, for the page
/// subtitle and link previews: "Saturday, 21 November 2026" for a single day,
/// "20 – 21 Nov 2026" across days, "30 Nov – 2 Dec 2026" across months.
pub fn date_summary(first: DateTime<Utc>, last: DateTime<Utc>, tz: Tz) -> String {
    let (first, last) = if first <= last { (first, last) } else { (last, first) };
    let (a, b) = (first.with_timezone(&tz), last.with_timezone(&tz));

    if a.date_naive() == b.date_naive() {
        return day_label(first, tz);
    }
    if a.format("%Y").to_string() != b.format("%Y").to_string() {
        return format!("{} – {}", short_date(first, tz), short_date(last, tz));
    }
    if a.format("%m").to_string() != b.format("%m").to_string() {
        return format!("{} – {}", a.format("%-d %b"), b.format("%-d %b %Y"));
    }
    format!("{} – {}", a.format("%-d"), b.format("%-d %b %Y"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn renders_in_the_event_timezone_not_utc() {
        let lagos: Tz = "Africa/Lagos".parse().unwrap();
        // 23:30 UTC is already the next day in Lagos (UTC+1).
        let instant = utc("2026-11-20T23:30:00Z");
        assert_eq!(day_label(instant, lagos), "Saturday, 21 November 2026");
        assert_eq!(time_label(instant, lagos), "12:30 AM");
    }

    #[test]
    fn twelve_hour_clock_has_no_leading_zero() {
        let lagos: Tz = "Africa/Lagos".parse().unwrap();
        assert_eq!(time_label(utc("2026-11-21T08:00:00Z"), lagos), "9:00 AM");
        assert_eq!(time_label(utc("2026-11-21T12:00:00Z"), lagos), "1:00 PM");
        assert_eq!(time_label(utc("2026-11-21T11:00:00Z"), lagos), "12:00 PM");
        assert_eq!(time_label(utc("2026-11-20T23:00:00Z"), lagos), "12:00 AM");
    }

    #[test]
    fn time_range_falls_back_to_start_only() {
        let lagos: Tz = "Africa/Lagos".parse().unwrap();
        let start = utc("2026-11-21T08:00:00Z");
        assert_eq!(
            time_range(start, Some(utc("2026-11-21T10:00:00Z")), lagos),
            "9:00 AM – 11:00 AM"
        );
        assert_eq!(time_range(start, None, lagos), "9:00 AM");
    }

    #[test]
    fn date_summary_collapses_by_granularity() {
        let lagos: Tz = "Africa/Lagos".parse().unwrap();
        let day = utc("2026-11-21T08:00:00Z");
        assert_eq!(date_summary(day, day, lagos), "Saturday, 21 November 2026");
        assert_eq!(
            date_summary(utc("2026-11-20T08:00:00Z"), day, lagos),
            "20 – 21 Nov 2026"
        );
        assert_eq!(
            date_summary(utc("2026-11-30T08:00:00Z"), utc("2026-12-02T08:00:00Z"), lagos),
            "30 Nov – 2 Dec 2026"
        );
        assert_eq!(
            date_summary(utc("2026-12-31T08:00:00Z"), utc("2027-01-02T08:00:00Z"), lagos),
            "31 Dec 2026 – 2 Jan 2027"
        );
    }

    #[test]
    fn date_summary_tolerates_reversed_bounds() {
        let lagos: Tz = "Africa/Lagos".parse().unwrap();
        assert_eq!(
            date_summary(utc("2026-11-21T08:00:00Z"), utc("2026-11-20T08:00:00Z"), lagos),
            "20 – 21 Nov 2026"
        );
    }

    #[test]
    fn honours_non_lagos_timezones() {
        let ny: Tz = "America/New_York".parse().unwrap();
        // 01:00 UTC is still the previous evening in New York.
        assert_eq!(day_label(utc("2026-11-21T01:00:00Z"), ny), "Friday, 20 November 2026");
        assert_eq!(time_label(utc("2026-11-21T01:00:00Z"), ny), "8:00 PM");
    }
}
