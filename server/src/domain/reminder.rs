//! The decisions behind an automated reminder, kept free of IO so they can be
//! tested directly: when a rung may fire, whether the hour is civil, and what
//! the message actually says.

use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};
use chrono_tz::Tz;

/// Nobody's phone should buzz at 3am because a rung came due at midnight.
/// Reminders are held until the local morning rather than dropped — the point
/// of the rung is the day, not the minute.
pub const QUIET_START_HOUR: u32 = 21;
pub const QUIET_END_HOUR: u32 = 8;

/// Whether a local time is inside sending hours (08:00–21:00 local).
pub fn is_sendable_hour(at: DateTime<Utc>, tz: Tz) -> bool {
    (QUIET_END_HOUR..QUIET_START_HOUR).contains(&at.with_timezone(&tz).hour())
}

/// The next moment at or after `at` that falls inside sending hours.
///
/// Used to report *when* a held reminder will go out, so the dashboard can say
/// "waiting until 8am" instead of looking stuck.
pub fn next_sendable(at: DateTime<Utc>, tz: Tz) -> DateTime<Utc> {
    if is_sendable_hour(at, tz) {
        return at;
    }
    let local = at.with_timezone(&tz);
    // Before 08:00 it's this morning; from 21:00 on it's tomorrow morning.
    let target_day = if local.hour() < QUIET_END_HOUR {
        local.date_naive()
    } else {
        local.date_naive().succ_opt().unwrap_or(local.date_naive())
    };
    let naive = target_day
        .and_hms_opt(QUIET_END_HOUR, 0, 0)
        .expect("08:00 is a valid time");
    // A DST gap would make 08:00 ambiguous or absent; take the later reading
    // rather than failing. Africa/Lagos has no DST, but events carry their own
    // timezone and this must not panic on one that does.
    tz.from_local_datetime(&naive)
        .latest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(at)
}

/// How far off the event is, in the words a person would use. Composed from the
/// real remaining time at send, never from the rung's label — so a reminder
/// delayed by downtime or quiet hours still tells the truth.
pub fn time_until(now: DateTime<Utc>, starts_at: DateTime<Utc>, tz: Tz) -> String {
    let remaining = starts_at - now;
    if remaining <= Duration::zero() {
        return "today".into();
    }
    // Compare local calendar days: an event at 9am tomorrow is "tomorrow", even
    // though it's under 24 hours away.
    let days = (starts_at.with_timezone(&tz).date_naive() - now.with_timezone(&tz).date_naive())
        .num_days();
    match days {
        0 => "today".into(),
        1 => "tomorrow".into(),
        2..=13 => format!("in {days} days"),
        _ => format!("in {} weeks", days / 7),
    }
}

/// What a guest who hasn't answered yet receives.
///
/// Deliberately short: this is a text message, it is going to a phone in Lagos,
/// and the only thing it needs to achieve is a tap on the link. It names the
/// event, says when, and asks the one question.
///
/// The guest is addressed by the name exactly as the organizer entered it. No
/// first-name extraction: "Aunt Ngozi", "Chief Adebayo" and "Dr. Emeka" are how
/// people are actually listed here, and taking the leading token turns every one
/// of them into a greeting addressed to a title.
pub fn compose(guest_name: &str, event_title: &str, when: &str, link: &str) -> String {
    format!(
        "Hi {guest_name}, {event_title} is {when} and we haven't heard from you yet. \
         Please let us know if you're coming: {link}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn lagos() -> Tz {
        chrono_tz::Africa::Lagos
    }

    /// Builds a UTC instant from a Lagos wall-clock time.
    fn at_lagos(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        lagos()
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn sending_hours_are_civil() {
        assert!(!is_sendable_hour(at_lagos(2026, 11, 1, 3, 0), lagos()), "3am");
        assert!(!is_sendable_hour(at_lagos(2026, 11, 1, 7, 59), lagos()));
        assert!(is_sendable_hour(at_lagos(2026, 11, 1, 8, 0), lagos()));
        assert!(is_sendable_hour(at_lagos(2026, 11, 1, 20, 59), lagos()));
        assert!(!is_sendable_hour(at_lagos(2026, 11, 1, 21, 0), lagos()), "9pm");
    }

    #[test]
    fn a_reminder_due_overnight_waits_for_morning_not_forever() {
        // Due at 2am: goes out at 8am the same day.
        let held = next_sendable(at_lagos(2026, 11, 1, 2, 0), lagos());
        assert_eq!(held, at_lagos(2026, 11, 1, 8, 0));

        // Due at 10pm: goes out at 8am tomorrow.
        let held = next_sendable(at_lagos(2026, 11, 1, 22, 0), lagos());
        assert_eq!(held, at_lagos(2026, 11, 2, 8, 0));

        // Due mid-morning: goes out immediately.
        let now = at_lagos(2026, 11, 1, 10, 0);
        assert_eq!(next_sendable(now, lagos()), now);
    }

    #[test]
    fn the_wording_counts_calendar_days_not_hours() {
        let tz = lagos();
        let event = at_lagos(2026, 11, 20, 10, 0);

        // 9pm the night before is "tomorrow", though it's 13 hours away.
        assert_eq!(time_until(at_lagos(2026, 11, 19, 21, 0), event, tz), "tomorrow");
        // 8am the same morning is "today", though the event is 2 hours off.
        assert_eq!(time_until(at_lagos(2026, 11, 20, 8, 0), event, tz), "today");
        assert_eq!(time_until(at_lagos(2026, 11, 17, 10, 0), event, tz), "in 3 days");
        // Days stay days up to a fortnight — "in 7 days" is as clear as "in a
        // week" and reads consistently next to "in 3 days".
        assert_eq!(time_until(at_lagos(2026, 11, 13, 10, 0), event, tz), "in 7 days");
        assert_eq!(time_until(at_lagos(2026, 11, 6, 10, 0), event, tz), "in 2 weeks");
    }

    #[test]
    fn a_late_reminder_still_tells_the_truth() {
        // The rung was "14 days before", but the send was held up. The wording
        // comes from the real remaining time, so it doesn't claim two weeks.
        let event = at_lagos(2026, 11, 20, 10, 0);
        assert_eq!(time_until(at_lagos(2026, 11, 18, 9, 0), event, lagos()), "in 2 days");
    }

    #[test]
    fn an_event_already_underway_reads_as_today() {
        let event = at_lagos(2026, 11, 20, 10, 0);
        assert_eq!(time_until(at_lagos(2026, 11, 20, 12, 0), event, lagos()), "today");
    }

    #[test]
    fn the_message_addresses_the_guest_and_carries_the_link() {
        let msg = compose(
            "Aunt Ngozi",
            "Ada & Tunde",
            "in 3 days",
            "https://106.events/r/abc",
        );
        assert!(msg.starts_with("Hi Aunt Ngozi,"), "{msg}");
        assert!(msg.contains("Ada & Tunde"));
        assert!(msg.contains("in 3 days"));
        assert!(msg.contains("https://106.events/r/abc"));
    }

    #[test]
    fn titles_and_honorifics_survive_the_greeting() {
        // Guests are listed the way the organizer refers to them. Reducing this
        // to a "first name" would greet a chief as "Hi Chief,".
        for name in ["Chief Adebayo", "Dr. Emeka", "Aunt Ngozi & family"] {
            let msg = compose(name, "Ada & Tunde", "today", "https://x/r/a");
            assert!(msg.starts_with(&format!("Hi {name},")), "{msg}");
        }
    }
}
