use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How far back a due check may look after sleep or a stall.
pub const CATCH_UP: Duration = Duration::hours(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    Once,
    Daily,
    Weekdays,
    Weekends,
}

impl Repeat {
    pub fn label(self) -> &'static str {
        match self {
            Repeat::Once => "Once",
            Repeat::Daily => "Every day",
            Repeat::Weekdays => "Weekdays",
            Repeat::Weekends => "Weekends",
        }
    }

    pub fn matches(self, weekday: Weekday) -> bool {
        match self {
            Repeat::Once | Repeat::Daily => true,
            Repeat::Weekdays => !matches!(weekday, Weekday::Sat | Weekday::Sun),
            Repeat::Weekends => matches!(weekday, Weekday::Sat | Weekday::Sun),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub id: Uuid,
    pub enabled: bool,
    pub hour: u32,
    pub minute: u32,
    pub label: String,
    /// Empty string means "use the built-in default sound".
    pub sound_path: String,
    pub snooze_minutes: u32,
    pub repeat: Repeat,
    /// When set, this one-off time overrides the regular schedule (used for snooze).
    pub snooze_until: Option<DateTime<Local>>,
    pub last_fired: Option<DateTime<Local>>,
}

impl Alarm {
    pub fn new(hour: u32, minute: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            enabled: true,
            hour,
            minute,
            label: String::new(),
            sound_path: String::new(),
            snooze_minutes: 5,
            repeat: Repeat::Once,
            snooze_until: None,
            last_fired: None,
        }
    }

    pub fn time_label_for(&self, use_12_hour: bool) -> String {
        format_hm(self.hour, self.minute, use_12_hour)
    }

    pub fn display_name(&self) -> String {
        let trimmed = self.label.trim();
        if trimmed.is_empty() {
            "Alarm".to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn occurrence_on(&self, date: NaiveDate) -> Option<DateTime<Local>> {
        if !self.repeat.matches(date.weekday()) {
            return None;
        }
        let naive = date.and_time(NaiveTime::from_hms_opt(self.hour, self.minute, 0)?);
        Local
            .from_local_datetime(&naive)
            .earliest()
            .or_else(|| Local.from_local_datetime(&naive).latest())
    }

    /// Next time this alarm should ring, if enabled.
    pub fn next_trigger(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        if !self.enabled {
            return None;
        }

        if let Some(until) = self.snooze_until {
            if until > now {
                return Some(until);
            }
        }

        for day_offset in 0..8 {
            let candidate_date = now.date_naive() + Duration::days(day_offset);
            if let Some(candidate) = self.occurrence_on(candidate_date) {
                let already = self.last_fired.map(|fired| fired >= candidate).unwrap_or(false);
                if already {
                    continue;
                }
                if candidate > now {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// True when a scheduled (or snooze) time falls in `(last_poll, now]` and within `CATCH_UP`.
    pub fn should_fire(&self, last_poll: DateTime<Local>, now: DateTime<Local>) -> bool {
        if !self.enabled {
            return false;
        }
        if now <= last_poll {
            return false;
        }

        let grace_start = now - CATCH_UP;
        let window_lo = if last_poll > grace_start { last_poll } else { grace_start };

        if let Some(until) = self.snooze_until {
            if until > now {
                return false;
            }
            let already = self.last_fired.map(|fired| fired >= until).unwrap_or(false);
            if !already && until > window_lo && until <= now {
                return true;
            }
            // Snooze already consumed or outside the window; do not also
            // fire the original clock time in the same pass.
            if !already {
                return false;
            }
        }

        let start_date = window_lo.date_naive() - Duration::days(1);
        let end_date = now.date_naive();
        let mut day = start_date;
        while day <= end_date {
            if let Some(scheduled) = self.occurrence_on(day) {
                let already = self.last_fired.map(|fired| fired >= scheduled).unwrap_or(false);
                if !already && scheduled > window_lo && scheduled <= now {
                    return true;
                }
            }
            day += Duration::days(1);
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlarmStore {
    pub alarms: Vec<Alarm>,
    pub default_snooze_minutes: u32,
    /// When true, clocks and editors use 12-hour time with AM/PM.
    #[serde(default)]
    pub use_12_hour: bool,
    /// Last time the running app sampled the clock. Used for catch-up after stalls.
    #[serde(default)]
    pub last_checked: Option<DateTime<Local>>,
}

impl AlarmStore {
    pub fn new() -> Self {
        Self {
            alarms: Vec::new(),
            default_snooze_minutes: 5,
            use_12_hour: false,
            last_checked: None,
        }
    }
}

/// Convert a 0–23 hour to 1–12 plus whether it is PM.
pub fn to_12_hour(hour24: u32) -> (u32, bool) {
    let is_pm = hour24 >= 12;
    let hour12 = hour24 % 12;
    (if hour12 == 0 { 12 } else { hour12 }, is_pm)
}

/// Convert a 1–12 hour plus AM/PM to 0–23.
pub fn to_24_hour(hour12: u32, is_pm: bool) -> u32 {
    let base = if hour12 == 12 { 0 } else { hour12 % 12 };
    if is_pm {
        base + 12
    } else {
        base
    }
}

pub fn format_hm(hour: u32, minute: u32, use_12_hour: bool) -> String {
    if use_12_hour {
        let (h, is_pm) = to_12_hour(hour);
        format!("{}:{:02} {}", h, minute, if is_pm { "PM" } else { "AM" })
    } else {
        format!("{:02}:{:02}", hour, minute)
    }
}

pub fn format_datetime(dt: DateTime<Local>, use_12_hour: bool) -> String {
    if use_12_hour {
        dt.format("%a %b %d   %I:%M:%S %p").to_string()
    } else {
        dt.format("%a %b %d   %H:%M:%S").to_string()
    }
}

pub fn format_next(dt: DateTime<Local>, use_12_hour: bool) -> String {
    if use_12_hour {
        dt.format("%a %I:%M %p").to_string()
    } else {
        dt.format("%a %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, second)
            .single()
            .or_else(|| Local.with_ymd_and_hms(year, month, day, hour, minute, second).earliest())
            .expect("valid local datetime")
    }

    fn alarm_at(hour: u32, minute: u32, repeat: Repeat) -> Alarm {
        let mut alarm = Alarm::new(hour, minute);
        alarm.repeat = repeat;
        alarm
    }

    #[test]
    fn twelve_hour_roundtrip() {
        assert_eq!(to_12_hour(0), (12, false));
        assert_eq!(to_12_hour(1), (1, false));
        assert_eq!(to_12_hour(12), (12, true));
        assert_eq!(to_12_hour(15), (3, true));
        assert_eq!(to_12_hour(23), (11, true));
        assert_eq!(to_24_hour(12, false), 0);
        assert_eq!(to_24_hour(12, true), 12);
        assert_eq!(to_24_hour(3, true), 15);
        assert_eq!(format_hm(15, 5, true), "3:05 PM");
        assert_eq!(format_hm(15, 5, false), "15:05");
    }

    #[test]
    fn fires_in_the_due_minute() {
        let alarm = alarm_at(8, 0, Repeat::Once);
        let last = at(2026, 9, 9, 7, 59, 50);
        let now = at(2026, 9, 9, 8, 0, 10);
        assert!(alarm.should_fire(last, now));
    }

    #[test]
    fn catches_up_after_sleep_across_the_minute() {
        let alarm = alarm_at(8, 0, Repeat::Daily);
        let last = at(2026, 9, 9, 7, 59, 0);
        let now = at(2026, 9, 9, 8, 1, 0);
        assert!(alarm.should_fire(last, now));
    }

    #[test]
    fn does_not_fire_outside_catch_up_window() {
        let alarm = alarm_at(8, 0, Repeat::Daily);
        let last = at(2026, 9, 8, 10, 0, 0);
        let now = at(2026, 9, 9, 12, 0, 0);
        assert!(!alarm.should_fire(last, now));
    }

    #[test]
    fn does_not_refire_same_occurrence() {
        let mut alarm = alarm_at(8, 0, Repeat::Daily);
        alarm.last_fired = Some(at(2026, 9, 9, 8, 0, 5));
        let last = at(2026, 9, 9, 8, 0, 10);
        let now = at(2026, 9, 9, 8, 0, 40);
        assert!(!alarm.should_fire(last, now));
    }

    #[test]
    fn weekday_alarm_skips_saturday() {
        let alarm = alarm_at(8, 0, Repeat::Weekdays);
        let last = at(2026, 9, 11, 23, 59, 0); // Friday night
        let now = at(2026, 9, 12, 8, 0, 30); // Saturday
        assert!(!alarm.should_fire(last, now));
        assert_eq!(
            alarm.next_trigger(now).map(|t| t.weekday()),
            Some(Weekday::Mon)
        );
    }

    #[test]
    fn weekend_alarm_skips_wednesday() {
        let alarm = alarm_at(9, 30, Repeat::Weekends);
        let last = at(2026, 9, 9, 9, 29, 0);
        let now = at(2026, 9, 9, 9, 30, 10);
        assert!(!alarm.should_fire(last, now));
    }

    #[test]
    fn midnight_boundary() {
        let alarm = alarm_at(0, 0, Repeat::Daily);
        let last = at(2026, 9, 9, 23, 59, 0);
        let now = at(2026, 9, 10, 0, 0, 15);
        assert!(alarm.should_fire(last, now));
    }

    #[test]
    fn snooze_due_uses_snooze_time_not_clock_time() {
        let mut alarm = alarm_at(8, 0, Repeat::Once);
        alarm.last_fired = Some(at(2026, 9, 9, 8, 0, 2));
        alarm.snooze_until = Some(at(2026, 9, 9, 8, 5, 0));
        let last = at(2026, 9, 9, 8, 4, 50);
        let now = at(2026, 9, 9, 8, 5, 2);
        assert!(alarm.should_fire(last, now));
    }

    #[test]
    fn snooze_in_the_future_does_not_fire() {
        let mut alarm = alarm_at(8, 0, Repeat::Once);
        alarm.snooze_until = Some(at(2026, 9, 9, 8, 10, 0));
        let last = at(2026, 9, 9, 8, 0, 0);
        let now = at(2026, 9, 9, 8, 1, 0);
        assert!(!alarm.should_fire(last, now));
        assert_eq!(
            alarm.next_trigger(now),
            Some(at(2026, 9, 9, 8, 10, 0))
        );
    }

    #[test]
    fn disabled_alarm_never_fires() {
        let mut alarm = alarm_at(8, 0, Repeat::Daily);
        alarm.enabled = false;
        let last = at(2026, 9, 9, 7, 59, 0);
        let now = at(2026, 9, 9, 8, 0, 10);
        assert!(!alarm.should_fire(last, now));
        assert!(alarm.next_trigger(now).is_none());
    }

    #[test]
    fn next_trigger_skips_consumed_today() {
        let mut alarm = alarm_at(8, 0, Repeat::Daily);
        alarm.last_fired = Some(at(2026, 9, 9, 8, 0, 1));
        let now = at(2026, 9, 9, 8, 1, 0);
        let next = alarm.next_trigger(now).expect("tomorrow");
        assert_eq!(next.date_naive(), NaiveDate::from_ymd_opt(2026, 9, 10).unwrap());
        assert_eq!(next.hour(), 8);
    }
}
