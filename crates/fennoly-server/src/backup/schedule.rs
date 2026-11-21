//! Backup schedules: "manual", "daily HH:MM" or "weekly <day> HH:MM", in the
//! server's local time (set `TZ` in Docker to your time zone).

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc, Weekday};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schedule {
    Manual,
    Daily(NaiveTime),
    Weekly(Weekday, NaiveTime),
}

fn parse_time(s: &str) -> AppResult<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").map_err(|_| AppError::bad("time must look like 02:00"))
}

fn parse_day(s: &str) -> AppResult<Weekday> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "mon" => Weekday::Mon,
        "tue" => Weekday::Tue,
        "wed" => Weekday::Wed,
        "thu" => Weekday::Thu,
        "fri" => Weekday::Fri,
        "sat" => Weekday::Sat,
        "sun" => Weekday::Sun,
        _ => return Err(AppError::bad("day must be mon, tue, wed, thu, fri, sat or sun")),
    })
}

fn day_name(d: Weekday) -> &'static str {
    match d {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}

impl Schedule {
    pub fn parse(raw: &str) -> AppResult<Self> {
        let parts: Vec<&str> = raw.split_whitespace().collect();
        match parts.as_slice() {
            ["manual"] => Ok(Schedule::Manual),
            ["daily", t] => Ok(Schedule::Daily(parse_time(t)?)),
            ["weekly", d, t] => Ok(Schedule::Weekly(parse_day(d)?, parse_time(t)?)),
            _ => Err(AppError::bad("schedule must be manual, 'daily HH:MM' or 'weekly day HH:MM'")),
        }
    }

    pub fn to_text(self) -> String {
        match self {
            Schedule::Manual => "manual".into(),
            Schedule::Daily(t) => format!("daily {}", t.format("%H:%M")),
            Schedule::Weekly(d, t) => format!("weekly {} {}", day_name(d), t.format("%H:%M")),
        }
    }

    /// The first scheduled moment strictly after `now` (in `now`'s time zone).
    pub fn next_after<Tz: TimeZone>(self, now: DateTime<Tz>) -> Option<DateTime<Utc>> {
        let tz = now.timezone();
        let (time, day) = match self {
            Schedule::Manual => return None,
            Schedule::Daily(t) => (t, None),
            Schedule::Weekly(d, t) => (t, Some(d)),
        };
        let today = now.date_naive();
        for offset in 0..=8 {
            let date = today + Duration::days(offset);
            if day.is_some_and(|d| date.weekday() != d) {
                continue;
            }
            // `earliest` handles DST gaps/overlaps (a skipped 02:30 runs at the next valid time).
            let Some(candidate) = tz.from_local_datetime(&date.and_time(time)).earliest() else {
                continue;
            };
            if candidate > now {
                return Some(candidate.with_timezone(&Utc));
            }
        }
        None
    }

    pub fn next_from_now(self) -> Option<DateTime<Utc>> {
        self.next_after(Local::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn parse_and_print() {
        for s in ["manual", "daily 02:00", "weekly sun 03:30"] {
            assert_eq!(Schedule::parse(s).unwrap().to_text(), s);
        }
        assert!(Schedule::parse("daily 25:00").is_err());
        assert!(Schedule::parse("weekly funday 02:00").is_err());
        assert!(Schedule::parse("hourly").is_err());
    }

    #[test]
    fn daily_runs_today_or_tomorrow() {
        let s = Schedule::parse("daily 02:00").unwrap();
        // 01:00 IST -> today 02:00 IST
        assert_eq!(s.next_after(at("2026-11-22T01:00:00+05:30")).unwrap(), at("2026-11-21T20:30:00Z"));
        // 02:00 exactly -> tomorrow
        assert_eq!(s.next_after(at("2026-11-22T02:00:00+05:30")).unwrap(), at("2026-11-22T20:30:00Z"));
    }

    #[test]
    fn weekly_finds_the_right_day() {
        let s = Schedule::parse("weekly sun 02:00").unwrap();
        // Sunday 22 Nov 2026, 10:00 -> next Sunday 29 Nov
        let next = s.next_after(at("2026-11-22T10:00:00+00:00")).unwrap();
        assert_eq!(next, at("2026-11-29T02:00:00Z"));
        assert!(Schedule::Manual.next_after(at("2026-11-22T10:00:00+00:00")).is_none());
    }
}
